# Fuzzing

## 目的

解析器/ABI/fixture parser のロバスト性検証を継続的に実施します。

## fuzz target

- analyze_message
  - [fuzz/fuzz_targets/analyze_message.rs](../../fuzz/fuzz_targets/analyze_message.rs)
- sandbox_abi
  - [fuzz/fuzz_targets/sandbox_abi.rs](../../fuzz/fuzz_targets/sandbox_abi.rs)
- replay_parser
  - [fuzz/fuzz_targets/replay_parser.rs](../../fuzz/fuzz_targets/replay_parser.rs)

## 実行

```bash
cargo fuzz run analyze_message -- -max_total_time=10
cargo fuzz run sandbox_abi -- -max_total_time=10
cargo fuzz run replay_parser -- -max_total_time=10
```

または:

```bash
just fuzz-smoke
```

## 運用

- CI heavy workflow で定期実行
- crash input は corpus 化して回帰に取り込む
