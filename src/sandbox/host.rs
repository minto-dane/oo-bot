use wasmtime::{
    Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
};

use crate::sandbox::abi::{
    ActionProposal, AnalyzerError, AnalyzerRequest, ProposalAnalyzer, SANDBOX_ABI_VERSION,
};

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub fuel_limit: u64,
    pub memory_limit_bytes: usize,
    pub table_elements_limit: usize,
    pub store_instance_limit: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            fuel_limit: 50_000,
            memory_limit_bytes: 65_536,
            table_elements_limit: 64,
            store_instance_limit: 4,
        }
    }
}

pub struct WasmtimeSandboxAnalyzer {
    engine: Engine,
    module: Module,
    cfg: SandboxConfig,
    abi_version: u32,
}

impl WasmtimeSandboxAnalyzer {
    pub fn new(cfg: SandboxConfig) -> Result<Self, String> {
        let mut config = Config::new();
        // This bot prioritizes deterministic fail-safe behavior over max throughput.
        config.consume_fuel(true);

        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
        let wasm = wat::parse_str(GUEST_WAT).map_err(|e| e.to_string())?;
        let module = Module::new(&engine, wasm).map_err(|e| e.to_string())?;

        let mut instance = Self { engine, module, cfg, abi_version: 0 };
        let actual = instance.fetch_abi_version()?;
        instance.abi_version = actual;
        Ok(instance)
    }

    fn fetch_abi_version(&self) -> Result<u32, String> {
        let mut store = self.new_store()?;
        let instance = Instance::new(&mut store, &self.module, &[]).map_err(|e| e.to_string())?;
        let abi_fn: TypedFunc<(), i32> =
            instance.get_typed_func(&mut store, "abi_version").map_err(|e| e.to_string())?;
        let abi = abi_fn.call(&mut store, ()).map_err(|e| e.to_string())?;
        Ok(abi as u32)
    }

    fn new_store(&self) -> Result<Store<SandboxStoreState>, String> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.cfg.memory_limit_bytes)
            .table_elements(self.cfg.table_elements_limit)
            .instances(self.cfg.store_instance_limit)
            .tables(self.cfg.store_instance_limit)
            .memories(1)
            .trap_on_grow_failure(true)
            .build();

        let mut store = Store::new(&self.engine, SandboxStoreState { limits });
        store.limiter(|s| &mut s.limits);
        store.set_fuel(self.cfg.fuel_limit).map_err(|e| e.to_string())?;
        Ok(store)
    }
}

impl ProposalAnalyzer for WasmtimeSandboxAnalyzer {
    fn abi_version(&self) -> u32 {
        self.abi_version
    }

    fn propose(&mut self, req: &AnalyzerRequest<'_>) -> Result<ActionProposal, AnalyzerError> {
        if self.abi_version != SANDBOX_ABI_VERSION {
            return Err(AnalyzerError::AbiMismatch {
                expected: SANDBOX_ABI_VERSION,
                actual: self.abi_version,
            });
        }

        let bytes = req.content.as_bytes();
        if bytes.len() > self.cfg.memory_limit_bytes {
            return Err(AnalyzerError::ResourceLimit(
                "message exceeds sandbox linear memory budget".to_string(),
            ));
        }

        let mut store = self
            .new_store()
            .map_err(|e| AnalyzerError::Trap(format!("sandbox setup failed: {e}")))?;
        let instance = Instance::new(&mut store, &self.module, &[])
            .map_err(|e| AnalyzerError::Trap(e.to_string()))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| AnalyzerError::Trap("guest memory export missing".to_string()))?;

        memory
            .write(&mut store, 0, bytes)
            .map_err(|e| AnalyzerError::ResourceLimit(e.to_string()))?;

        let analyze: TypedFunc<(i32, i32, i32, i32), i64> = instance
            .get_typed_func(&mut store, "analyze")
            .map_err(|e| AnalyzerError::Trap(e.to_string()))?;

        let wire = analyze
            .call(
                &mut store,
                (
                    0,
                    bytes.len() as i32,
                    req.kanji_count.min(i32::MAX as usize) as i32,
                    i32::from(req.special_phrase_hit),
                ),
            )
            .map_err(|err| {
                let msg = err.to_string();
                if msg.contains("all fuel consumed") {
                    AnalyzerError::Timeout
                } else {
                    AnalyzerError::Trap(msg)
                }
            })?;

        ActionProposal::decode_wire(wire).map_err(|_| AnalyzerError::InvalidWire(wire))
    }
}

#[derive(Debug)]
struct SandboxStoreState {
    limits: StoreLimits,
}

const GUEST_WAT: &str = r#"
(module
  (memory (export "memory") 1 1)

  (func (export "abi_version") (result i32)
    i32.const 1
  )

  (func $is_ascii_o (param $b i32) (result i32)
    local.get $b
    i32.const 111
    i32.eq
    local.get $b
    i32.const 79
    i32.eq
    i32.or
  )

  (func $byte (param $addr i32) (result i32)
    local.get $addr
    i32.load8_u
  )

  (func (export "analyze") (param $ptr i32) (param $len i32) (param $kanji_count i32) (param $special_hit i32) (result i64)
    (local $i i32)
    (local $count i32)
    (local $b0 i32)
    (local $b1 i32)

    ;; suspicious sentinel for extreme payloads
    local.get $len
    i32.const 32768
    i32.gt_s
    if
      i64.const 17179869184 ;; SuspiciousInput
      return
    end

    block $done
      loop $loop
        local.get $i
        local.get $len
        i32.const 1
        i32.sub
        i32.ge_s
        br_if $done

        local.get $ptr
        local.get $i
        i32.add
        call $byte
        local.set $b0

        local.get $ptr
        local.get $i
        i32.add
        i32.const 1
        i32.add
        call $byte
        local.set $b1

        ;; ASCII oo / oO / Oo / OO (non-overlapping)
        local.get $b0
        call $is_ascii_o
        local.get $b1
        call $is_ascii_o
        i32.and
        if
          local.get $count
          i32.const 1
          i32.add
          local.set $count

          local.get $i
          i32.const 2
          i32.add
          local.set $i
          br $loop
        end

        ;; hiragana おお in UTF-8: E3 81 8A E3 81 8A
        local.get $i
        i32.const 5
        i32.add
        local.get $len
        i32.lt_s
        if
          local.get $b0
          i32.const 227
          i32.eq
          local.get $b1
          i32.const 129
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 2
          i32.add
          call $byte
          i32.const 138
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 3
          i32.add
          call $byte
          i32.const 227
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 4
          i32.add
          call $byte
          i32.const 129
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 5
          i32.add
          call $byte
          i32.const 138
          i32.eq
          i32.and

          if
            local.get $count
            i32.const 1
            i32.add
            local.set $count

            local.get $i
            i32.const 6
            i32.add
            local.set $i
            br $loop
          end
        end

        ;; katakana オオ in UTF-8: E3 82 AA E3 82 AA
        local.get $i
        i32.const 5
        i32.add
        local.get $len
        i32.lt_s
        if
          local.get $b0
          i32.const 227
          i32.eq
          local.get $b1
          i32.const 130
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 2
          i32.add
          call $byte
          i32.const 170
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 3
          i32.add
          call $byte
          i32.const 227
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 4
          i32.add
          call $byte
          i32.const 130
          i32.eq
          i32.and

          local.get $ptr
          local.get $i
          i32.add
          i32.const 5
          i32.add
          call $byte
          i32.const 170
          i32.eq
          i32.and

          if
            local.get $count
            i32.const 1
            i32.add
            local.set $count

            local.get $i
            i32.const 6
            i32.add
            local.set $i
            br $loop
          end
        end

        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $loop
      end
    end

    local.get $count
    local.get $kanji_count
    i32.add
    local.set $count

    local.get $special_hit
    i32.const 0
    i32.ne
    if
      i64.const 12884901889 ;; SpecialPhrase + aux=1
      return
    end

    local.get $count
    i32.const 0
    i32.le_s
    if
      i64.const 0
      return
    end

    local.get $count
    i32.const 1
    i32.eq
    if
      i64.const 4294967297 ;; ReactOnce + aux=1
      return
    end

    local.get $count
    i32.const 255
    i32.gt_s
    if
      i32.const 255
      local.set $count
    end

    i64.const 8589934592 ;; SendStamped
    local.get $count
    i64.extend_i32_u
    i64.or
  )
)
"#;

#[cfg(test)]
mod tests {
    use crate::sandbox::abi::{ActionProposal, AnalyzerRequest, ProposalAnalyzer};

    use super::{SandboxConfig, WasmtimeSandboxAnalyzer};

    #[test]
    fn simple_proposals_work() {
        let mut analyzer = match WasmtimeSandboxAnalyzer::new(SandboxConfig::default()) {
            Ok(analyzer) => analyzer,
            Err(err) => panic!("sandbox init should succeed: {err}"),
        };

        let proposal = match analyzer.propose(&AnalyzerRequest {
            content: "oo",
            kanji_count: 0,
            special_phrase_hit: false,
        }) {
            Ok(proposal) => proposal,
            Err(err) => panic!("proposal should succeed: {err:?}"),
        };
        assert_eq!(proposal, ActionProposal::ReactOnce);

        let proposal = match analyzer.propose(&AnalyzerRequest {
            content: "oooo",
            kanji_count: 1,
            special_phrase_hit: false,
        }) {
            Ok(proposal) => proposal,
            Err(err) => panic!("proposal should succeed: {err:?}"),
        };
        assert_eq!(proposal, ActionProposal::SendStamped { count: 3 });

        let proposal = match analyzer.propose(&AnalyzerRequest {
            content: "whatever",
            kanji_count: 0,
            special_phrase_hit: true,
        }) {
            Ok(proposal) => proposal,
            Err(err) => panic!("proposal should succeed: {err:?}"),
        };
        assert_eq!(proposal, ActionProposal::SpecialPhrase);
    }

    #[test]
    fn timeout_by_low_fuel_is_handled() {
        let cfg = SandboxConfig { fuel_limit: 8, ..SandboxConfig::default() };
        let mut analyzer = match WasmtimeSandboxAnalyzer::new(cfg) {
            Ok(analyzer) => analyzer,
            Err(err) => panic!("sandbox init should succeed: {err}"),
        };
        let payload = "a".repeat(4000);

        let result = analyzer.propose(&AnalyzerRequest {
            content: &payload,
            kanji_count: 0,
            special_phrase_hit: false,
        });

        assert!(result.is_err(), "very low fuel should fail safely");
    }
}
