use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Normal,
    ObserveOnly,
    ReactOnly,
    AuditOnly,
    FullDisable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeTrigger {
    OperatorOverride,
    EmergencyKillSwitch,
    InvalidToken,
    CircuitBreakerOpen,
    SandboxFailureSpike,
    SessionBudgetLow,
    Recovery,
}

#[derive(Debug, Clone)]
pub struct ModeState {
    mode: RuntimeMode,
    changed_at: Instant,
    last_trigger: ModeTrigger,
}

impl ModeState {
    pub fn new(now: Instant) -> Self {
        Self { mode: RuntimeMode::Normal, changed_at: now, last_trigger: ModeTrigger::Recovery }
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    pub fn last_trigger(&self) -> ModeTrigger {
        self.last_trigger
    }

    pub fn transition(&mut self, mode: RuntimeMode, trigger: ModeTrigger, now: Instant) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        self.last_trigger = trigger;
        self.changed_at = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{ModeState, ModeTrigger, RuntimeMode};

    #[test]
    fn transitions_are_recorded() {
        let now = Instant::now();
        let mut s = ModeState::new(now);
        assert_eq!(s.mode(), RuntimeMode::Normal);

        let changed = s.transition(RuntimeMode::ObserveOnly, ModeTrigger::CircuitBreakerOpen, now);
        assert!(changed);
        assert_eq!(s.mode(), RuntimeMode::ObserveOnly);
        assert_eq!(s.last_trigger(), ModeTrigger::CircuitBreakerOpen);
    }
}
