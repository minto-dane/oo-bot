use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    app::analyze_message::{BotAction, BotConfig},
    domain::kanji_matcher::{count_oo_kanji, KanjiOoDb},
    sandbox::abi::{ActionProposal, AnalyzerError, AnalyzerRequest, ProposalAnalyzer},
    security::{
        circuit_breaker::HttpCircuitBreaker,
        duplicate_guard::DuplicateGuard,
        mode::{ModeState, ModeTrigger, RuntimeMode},
        rate_limiter::TokenBucket,
        session_budget::SessionBudget,
        suspicious_input::{classify_suspicious_input, SuspicionLevel, SuspiciousInputConfig},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProtectionConfig {
    pub duplicate_ttl_ms: u64,
    pub duplicate_cache_cap: usize,
    pub per_user_cooldown_ms: u64,
    pub per_channel_cooldown_ms: u64,
    pub per_guild_cooldown_ms: u64,
    pub global_cooldown_ms: u64,
    pub global_rate_per_sec: f64,
    pub global_rate_burst: u32,
    pub max_actions_per_message: u8,
    pub max_send_chars: usize,
    pub long_message_soft_chars: usize,
    pub long_message_hard_chars: usize,
    pub suspicious_repetition_threshold: usize,
    pub breaker_window_ms: u64,
    pub breaker_threshold: usize,
    pub breaker_open_ms: u64,
    pub sandbox_failure_window_ms: u64,
    pub sandbox_failure_threshold: usize,
    pub allow_guild_ids: Vec<u64>,
    pub deny_guild_ids: Vec<u64>,
    pub allow_channel_ids: Vec<u64>,
    pub deny_channel_ids: Vec<u64>,
    pub mode_override: Option<RuntimeMode>,
    pub emergency_kill_switch: bool,
    pub session_budget_low_watermark: u32,
}

impl Default for RuntimeProtectionConfig {
    fn default() -> Self {
        Self {
            duplicate_ttl_ms: 180_000,
            duplicate_cache_cap: 8192,
            per_user_cooldown_ms: 900,
            per_channel_cooldown_ms: 400,
            per_guild_cooldown_ms: 250,
            global_cooldown_ms: 100,
            global_rate_per_sec: 20.0,
            global_rate_burst: 30,
            max_actions_per_message: 1,
            max_send_chars: 1_900,
            long_message_soft_chars: 2_000,
            long_message_hard_chars: 8_000,
            suspicious_repetition_threshold: 256,
            breaker_window_ms: 60_000,
            breaker_threshold: 64,
            breaker_open_ms: 120_000,
            sandbox_failure_window_ms: 30_000,
            sandbox_failure_threshold: 10,
            allow_guild_ids: vec![],
            deny_guild_ids: vec![],
            allow_channel_ids: vec![],
            deny_channel_ids: vec![],
            mode_override: None,
            emergency_kill_switch: false,
            session_budget_low_watermark: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageContext {
    pub message_id: u64,
    pub author_id: u64,
    pub channel_id: u64,
    pub guild_id: Option<u64>,
    pub author_is_bot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressReason {
    SelfTrigger,
    Duplicate,
    Cooldown,
    RateLimit,
    CircuitOpen,
    ChannelDenied,
    GuildDenied,
    ModeRestricted,
    Suspicious,
    InvalidAction,
}

#[derive(Debug, Clone)]
pub struct ActionDecision {
    pub action: BotAction,
    pub mode: RuntimeMode,
    pub proposal: ActionProposal,
    pub suspicion: SuspicionLevel,
    pub suppress_reason: Option<SuppressReason>,
}

#[derive(Debug, Default, Clone)]
pub struct RuntimeMetrics {
    pub analyzer_calls_total: u64,
    pub analyzer_traps_total: u64,
    pub analyzer_timeout_total: u64,
    pub messages_dropped_total: u64,
    pub duplicate_suppressed_total: u64,
    pub outbound_suppressed_total: u64,
    pub cooldown_hits_total: u64,
    pub invalid_request_prevented_total: u64,
    pub session_budget_low_total: u64,
    pub reconnect_attempts_total: u64,
    pub resume_failures_total: u64,
    pub mode_transitions_total: u64,
}

pub struct TrustedCore {
    analyzer: Box<dyn ProposalAnalyzer + Send>,
    bot_config: BotConfig,
    runtime: RuntimeProtectionConfig,
    db: &'static KanjiOoDb,
    mode: ModeState,
    duplicate_guard: DuplicateGuard,
    breaker: HttpCircuitBreaker,
    outbound_bucket: TokenBucket,
    user_cooldown: CooldownMap,
    channel_cooldown: CooldownMap,
    guild_cooldown: CooldownMap,
    global_cooldown: CooldownMap,
    suspicious_cfg: SuspiciousInputConfig,
    session_budget: SessionBudget,
    sandbox_failures: VecDeque<Instant>,
    metrics: RuntimeMetrics,
}

impl TrustedCore {
    pub fn new(
        analyzer: Box<dyn ProposalAnalyzer + Send>,
        bot_config: BotConfig,
        runtime: RuntimeProtectionConfig,
        db: &'static KanjiOoDb,
    ) -> Self {
        let now = Instant::now();

        Self {
            analyzer,
            bot_config,
            suspicious_cfg: SuspiciousInputConfig {
                soft_char_limit: runtime.long_message_soft_chars,
                hard_char_limit: runtime.long_message_hard_chars,
                repetition_threshold: runtime.suspicious_repetition_threshold,
            },
            db,
            mode: ModeState::new(now),
            duplicate_guard: DuplicateGuard::new(
                Duration::from_millis(runtime.duplicate_ttl_ms),
                runtime.duplicate_cache_cap,
            ),
            breaker: HttpCircuitBreaker::new(
                Duration::from_millis(runtime.breaker_window_ms),
                runtime.breaker_threshold,
                Duration::from_millis(runtime.breaker_open_ms),
            ),
            outbound_bucket: TokenBucket::new(
                runtime.global_rate_burst,
                runtime.global_rate_per_sec,
                now,
            ),
            user_cooldown: CooldownMap::new(Duration::from_millis(runtime.per_user_cooldown_ms)),
            channel_cooldown: CooldownMap::new(Duration::from_millis(
                runtime.per_channel_cooldown_ms,
            )),
            guild_cooldown: CooldownMap::new(Duration::from_millis(runtime.per_guild_cooldown_ms)),
            global_cooldown: CooldownMap::new(Duration::from_millis(runtime.global_cooldown_ms)),
            session_budget: SessionBudget::new(
                1000,
                1000,
                24 * 60 * 60,
                runtime.session_budget_low_watermark,
            ),
            sandbox_failures: VecDeque::new(),
            metrics: RuntimeMetrics::default(),
            runtime,
        }
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode.mode()
    }

    pub fn metrics(&self) -> RuntimeMetrics {
        self.metrics.clone()
    }

    pub fn set_mode_override(&mut self, mode: Option<RuntimeMode>) {
        self.runtime.mode_override = mode;
    }

    pub fn set_emergency_kill_switch(&mut self, enabled: bool) {
        self.runtime.emergency_kill_switch = enabled;
    }

    pub fn set_access_lists(
        &mut self,
        allow_guild_ids: Vec<u64>,
        deny_guild_ids: Vec<u64>,
        allow_channel_ids: Vec<u64>,
        deny_channel_ids: Vec<u64>,
    ) {
        self.runtime.allow_guild_ids = allow_guild_ids;
        self.runtime.deny_guild_ids = deny_guild_ids;
        self.runtime.allow_channel_ids = allow_channel_ids;
        self.runtime.deny_channel_ids = deny_channel_ids;
    }

    pub fn set_suspicious_thresholds(
        &mut self,
        soft_chars: usize,
        hard_chars: usize,
        repetition: usize,
    ) {
        self.suspicious_cfg.soft_char_limit = soft_chars;
        self.suspicious_cfg.hard_char_limit = hard_chars;
        self.suspicious_cfg.repetition_threshold = repetition.max(1);
    }

    pub fn reset_suspicious_thresholds(&mut self) {
        self.suspicious_cfg.soft_char_limit = self.runtime.long_message_soft_chars;
        self.suspicious_cfg.hard_char_limit = self.runtime.long_message_hard_chars;
        self.suspicious_cfg.repetition_threshold = self.runtime.suspicious_repetition_threshold.max(1);
    }

    pub fn update_session_budget(&mut self, total: u32, remaining: u32, reset_after_secs: u64) {
        self.session_budget = SessionBudget::new(
            total,
            remaining,
            reset_after_secs,
            self.runtime.session_budget_low_watermark,
        );
    }

    pub fn record_http_status(&mut self, status: u16) {
        let now = Instant::now();
        if matches!(status, 401 | 403 | 429) {
            self.metrics.invalid_request_prevented_total =
                self.metrics.invalid_request_prevented_total.saturating_add(1);
        }
        self.breaker.record_status(status, now);
    }

    pub fn record_reconnect_attempt(&mut self) {
        self.metrics.reconnect_attempts_total =
            self.metrics.reconnect_attempts_total.saturating_add(1);
    }

    pub fn record_resume_failure(&mut self) {
        self.metrics.resume_failures_total = self.metrics.resume_failures_total.saturating_add(1);
    }

    pub fn session_budget_low(&self) -> bool {
        self.session_budget.is_low()
    }

    pub fn decide_message(&mut self, ctx: MessageContext, content: &str) -> ActionDecision {
        let now = Instant::now();
        let mode = self.recompute_mode(now);

        if mode == RuntimeMode::FullDisable {
            return self.drop(
                mode,
                ActionProposal::Noop,
                SuspicionLevel::None,
                SuppressReason::ModeRestricted,
            );
        }

        if ctx.author_is_bot {
            return self.drop(
                mode,
                ActionProposal::Noop,
                SuspicionLevel::None,
                SuppressReason::SelfTrigger,
            );
        }

        if !self.allow_guild(ctx.guild_id) {
            return self.drop(
                mode,
                ActionProposal::Noop,
                SuspicionLevel::None,
                SuppressReason::GuildDenied,
            );
        }

        if !self.allow_channel(ctx.channel_id) {
            return self.drop(
                mode,
                ActionProposal::Noop,
                SuspicionLevel::None,
                SuppressReason::ChannelDenied,
            );
        }

        if self.duplicate_guard.is_duplicate_and_mark(ctx.message_id, now) {
            self.metrics.duplicate_suppressed_total =
                self.metrics.duplicate_suppressed_total.saturating_add(1);
            return self.drop(
                mode,
                ActionProposal::Noop,
                SuspicionLevel::None,
                SuppressReason::Duplicate,
            );
        }

        let suspicion = classify_suspicious_input(content, &self.suspicious_cfg);

        let proposal = if suspicion == SuspicionLevel::Hard {
            ActionProposal::SuspiciousInput
        } else {
            self.metrics.analyzer_calls_total = self.metrics.analyzer_calls_total.saturating_add(1);
            let req = AnalyzerRequest {
                content,
                kanji_count: count_oo_kanji(content, self.db),
                special_phrase_hit: content.contains(&self.bot_config.special_phrase),
            };
            match self.analyzer.propose(&req) {
                Ok(p) => p,
                Err(err) => {
                    self.record_analyzer_error(now, &err);
                    ActionProposal::Defer
                }
            }
        };

        self.finalize(ctx, proposal, suspicion, mode, now)
    }

    fn finalize(
        &mut self,
        ctx: MessageContext,
        proposal: ActionProposal,
        suspicion: SuspicionLevel,
        mode: RuntimeMode,
        now: Instant,
    ) -> ActionDecision {
        let mut proposed_action = proposal_to_action(&proposal, &self.bot_config);

        if suspicion == SuspicionLevel::Soft
            && matches!(proposed_action, BotAction::SendMessage { .. })
        {
            proposed_action = as_react(&self.bot_config);
        }

        let mut action = apply_mode_gate(mode, proposed_action.clone(), &self.bot_config);

        if matches!(action, BotAction::Noop) {
            if !matches!(proposed_action, BotAction::Noop) {
                return self.drop(mode, proposal, suspicion, SuppressReason::ModeRestricted);
            }
            if matches!(proposal, ActionProposal::SuspiciousInput) {
                return self.drop(mode, proposal, suspicion, SuppressReason::Suspicious);
            }
            return ActionDecision { action, mode, proposal, suspicion, suppress_reason: None };
        }

        if !self.breaker.allows_outbound(now) {
            return self.drop(mode, proposal, suspicion, SuppressReason::CircuitOpen);
        }

        if !self.outbound_bucket.try_take(1, now) {
            return self.drop(mode, proposal, suspicion, SuppressReason::RateLimit);
        }

        if self.runtime.max_actions_per_message == 0 {
            return self.drop(mode, proposal, suspicion, SuppressReason::InvalidAction);
        }

        if !self.cooldown_allows(ctx, now) {
            self.metrics.cooldown_hits_total = self.metrics.cooldown_hits_total.saturating_add(1);
            return self.drop(mode, proposal, suspicion, SuppressReason::Cooldown);
        }

        action = enforce_action_caps(action, &self.bot_config, self.runtime.max_send_chars);
        if is_invalid_action(&action) {
            return self.drop(mode, proposal, suspicion, SuppressReason::InvalidAction);
        }

        ActionDecision { action, mode, proposal, suspicion, suppress_reason: None }
    }

    fn cooldown_allows(&mut self, ctx: MessageContext, now: Instant) -> bool {
        if self.user_cooldown.is_cooling(ctx.author_id, now) {
            return false;
        }
        if self.channel_cooldown.is_cooling(ctx.channel_id, now) {
            return false;
        }
        if let Some(guild_id) = ctx.guild_id {
            if self.guild_cooldown.is_cooling(guild_id, now) {
                return false;
            }
            self.guild_cooldown.mark(guild_id, now);
        }

        if self.global_cooldown.is_cooling(0, now) {
            return false;
        }

        self.user_cooldown.mark(ctx.author_id, now);
        self.channel_cooldown.mark(ctx.channel_id, now);
        self.global_cooldown.mark(0, now);
        true
    }

    fn record_analyzer_error(&mut self, now: Instant, err: &AnalyzerError) {
        match err {
            AnalyzerError::Timeout => {
                self.metrics.analyzer_timeout_total =
                    self.metrics.analyzer_timeout_total.saturating_add(1)
            }
            _ => {
                self.metrics.analyzer_traps_total =
                    self.metrics.analyzer_traps_total.saturating_add(1)
            }
        }
        self.sandbox_failures.push_back(now);
        self.trim_sandbox_failures(now);
    }

    fn trim_sandbox_failures(&mut self, now: Instant) {
        let window = Duration::from_millis(self.runtime.sandbox_failure_window_ms);
        while let Some(front) = self.sandbox_failures.front().copied() {
            if now.duration_since(front) > window {
                let _ = self.sandbox_failures.pop_front();
            } else {
                break;
            }
        }
    }

    fn recompute_mode(&mut self, now: Instant) -> RuntimeMode {
        self.trim_sandbox_failures(now);

        let target = if self.runtime.emergency_kill_switch {
            (RuntimeMode::FullDisable, ModeTrigger::EmergencyKillSwitch)
        } else if let Some(override_mode) = self.runtime.mode_override {
            (override_mode, ModeTrigger::OperatorOverride)
        } else if self.session_budget.is_low() {
            self.metrics.session_budget_low_total =
                self.metrics.session_budget_low_total.saturating_add(1);
            (RuntimeMode::ReactOnly, ModeTrigger::SessionBudgetLow)
        } else if self.breaker.is_open(now) {
            (RuntimeMode::ObserveOnly, ModeTrigger::CircuitBreakerOpen)
        } else if self.sandbox_failures.len() >= self.runtime.sandbox_failure_threshold {
            (RuntimeMode::AuditOnly, ModeTrigger::SandboxFailureSpike)
        } else {
            (RuntimeMode::Normal, ModeTrigger::Recovery)
        };

        if self.mode.transition(target.0, target.1, now) {
            self.metrics.mode_transitions_total =
                self.metrics.mode_transitions_total.saturating_add(1);
        }

        self.mode.mode()
    }

    fn drop(
        &mut self,
        mode: RuntimeMode,
        proposal: ActionProposal,
        suspicion: SuspicionLevel,
        reason: SuppressReason,
    ) -> ActionDecision {
        self.metrics.messages_dropped_total = self.metrics.messages_dropped_total.saturating_add(1);
        self.metrics.outbound_suppressed_total =
            self.metrics.outbound_suppressed_total.saturating_add(1);
        ActionDecision {
            action: BotAction::Noop,
            mode,
            proposal,
            suspicion,
            suppress_reason: Some(reason),
        }
    }

    fn allow_channel(&self, channel_id: u64) -> bool {
        if self.runtime.deny_channel_ids.contains(&channel_id) {
            return false;
        }
        if self.runtime.allow_channel_ids.is_empty() {
            return true;
        }
        self.runtime.allow_channel_ids.contains(&channel_id)
    }

    fn allow_guild(&self, guild_id: Option<u64>) -> bool {
        let Some(gid) = guild_id else {
            return true;
        };

        if self.runtime.deny_guild_ids.contains(&gid) {
            return false;
        }
        if self.runtime.allow_guild_ids.is_empty() {
            return true;
        }
        self.runtime.allow_guild_ids.contains(&gid)
    }
}

#[derive(Debug, Clone)]
struct CooldownMap {
    ttl: Duration,
    next_allowed: HashMap<u64, Instant>,
}

impl CooldownMap {
    fn new(ttl: Duration) -> Self {
        Self { ttl, next_allowed: HashMap::new() }
    }

    fn is_cooling(&self, key: u64, now: Instant) -> bool {
        self.next_allowed.get(&key).is_some_and(|next| now < *next)
    }

    fn mark(&mut self, key: u64, now: Instant) {
        self.next_allowed.insert(key, now + self.ttl);
    }
}

fn proposal_to_action(proposal: &ActionProposal, bot: &BotConfig) -> BotAction {
    match proposal {
        ActionProposal::Noop | ActionProposal::Defer | ActionProposal::Reject { .. } => {
            BotAction::Noop
        }
        ActionProposal::SuspiciousInput => BotAction::Noop,
        ActionProposal::SpecialPhrase => BotAction::SendMessage { content: bot.stamp_text.clone() },
        ActionProposal::ReactOnce => as_react(bot),
        ActionProposal::SendStamped { count } => {
            if *count <= 1 {
                as_react(bot)
            } else {
                BotAction::SendMessage { content: repeat_stamp(&bot.stamp_text, *count as usize) }
            }
        }
    }
}

fn as_react(bot: &BotConfig) -> BotAction {
    BotAction::React {
        emoji_id: bot.reaction.emoji_id,
        emoji_name: bot.reaction.emoji_name.clone(),
        animated: bot.reaction.animated,
    }
}

fn repeat_stamp(stamp: &str, n: usize) -> String {
    let mut out = String::with_capacity(stamp.len().saturating_mul(n).saturating_add(n));
    for idx in 0..n {
        if idx > 0 {
            out.push(' ');
        }
        out.push_str(stamp);
    }
    out
}

fn apply_mode_gate(mode: RuntimeMode, action: BotAction, bot: &BotConfig) -> BotAction {
    match mode {
        RuntimeMode::Normal => action,
        RuntimeMode::ObserveOnly | RuntimeMode::AuditOnly | RuntimeMode::FullDisable => {
            BotAction::Noop
        }
        RuntimeMode::ReactOnly => match action {
            BotAction::SendMessage { .. } => as_react(bot),
            other => other,
        },
    }
}

fn enforce_action_caps(action: BotAction, _bot: &BotConfig, max_send_chars: usize) -> BotAction {
    match action {
        BotAction::SendMessage { content } => {
            let capped: String = content.chars().take(max_send_chars).collect();
            if capped.is_empty() {
                BotAction::Noop
            } else {
                BotAction::SendMessage { content: capped }
            }
        }
        BotAction::React { emoji_id, emoji_name, animated } => {
            if emoji_id == 0 || emoji_name.is_empty() {
                BotAction::Noop
            } else {
                BotAction::React { emoji_id, emoji_name, animated }
            }
        }
        BotAction::Noop => BotAction::Noop,
    }
}

fn is_invalid_action(action: &BotAction) -> bool {
    match action {
        BotAction::Noop => false,
        BotAction::React { emoji_id, emoji_name, .. } => *emoji_id == 0 || emoji_name.is_empty(),
        BotAction::SendMessage { content } => content.trim().is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        app::analyze_message::{BotAction, BotConfig},
        domain::kanji_matcher::{KanjiOoDb, KanjiOoDbMetadata},
        sandbox::abi::{
            ActionProposal, AnalyzerError, AnalyzerRequest, ProposalAnalyzer, SANDBOX_ABI_VERSION,
        },
        security::mode::RuntimeMode,
    };

    use super::{MessageContext, RuntimeProtectionConfig, TrustedCore};

    const TEST_DB: KanjiOoDb = KanjiOoDb::new(
        &['大' as u32],
        KanjiOoDbMetadata {
            source_name: "test",
            source_sha256: "test",
            total_chars: 1,
            ja_kun_hits: 1,
            nanori_hits: 0,
            ja_on_hits: 0,
        },
    );

    struct FixedAnalyzer {
        proposal: ActionProposal,
    }

    impl ProposalAnalyzer for FixedAnalyzer {
        fn abi_version(&self) -> u32 {
            SANDBOX_ABI_VERSION
        }

        fn propose(&mut self, _req: &AnalyzerRequest<'_>) -> Result<ActionProposal, AnalyzerError> {
            Ok(self.proposal.clone())
        }
    }

    #[test]
    fn duplicate_is_suppressed() {
        let cfg = RuntimeProtectionConfig::default();
        let mut core = TrustedCore::new(
            Box::new(FixedAnalyzer { proposal: ActionProposal::ReactOnce }),
            BotConfig::default(),
            cfg,
            &TEST_DB,
        );

        let ctx = MessageContext {
            message_id: 1,
            author_id: 2,
            channel_id: 3,
            guild_id: Some(4),
            author_is_bot: false,
        };

        let first = core.decide_message(ctx, "oo");
        assert!(matches!(first.action, BotAction::React { .. }));

        let second = core.decide_message(ctx, "oo");
        assert_eq!(second.action, BotAction::Noop);
        assert!(second.suppress_reason.is_some());
    }

    #[test]
    fn react_only_mode_converts_send() {
        let cfg = RuntimeProtectionConfig {
            mode_override: Some(RuntimeMode::ReactOnly),
            ..RuntimeProtectionConfig::default()
        };
        let mut core = TrustedCore::new(
            Box::new(FixedAnalyzer { proposal: ActionProposal::SendStamped { count: 3 } }),
            BotConfig::default(),
            cfg,
            &TEST_DB,
        );

        let ctx = MessageContext {
            message_id: 42,
            author_id: 10,
            channel_id: 11,
            guild_id: Some(12),
            author_is_bot: false,
        };

        let decision = core.decide_message(ctx, "oooo");
        assert!(matches!(decision.action, BotAction::React { .. }));
        assert_eq!(decision.mode, RuntimeMode::ReactOnly);
    }
}
