use std::sync::{Arc, Mutex};
use std::time::Instant;

use serenity::{
    all::{EmojiId, ReactionType},
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
    Error as SerenityError,
};
use tracing::{error, info, warn};
use unicode_normalization::UnicodeNormalization;

use crate::{
    app::analyze_message::BotAction,
    audit::{AuditEventInput, AuditEventType, AuditStore},
    security::core_governor::{ActionDecision, MessageContext, TrustedCore},
};

#[derive(Debug, Clone)]
pub struct HandlerRuntimeMeta {
    pub binary_version: String,
    pub config_fingerprint: String,
    pub active_lsm: String,
    pub hardening_status: String,
}

#[derive(Clone)]
pub struct Handler {
    pub core: Arc<Mutex<TrustedCore>>,
    pub audit: Option<Arc<Mutex<AuditStore>>>,
    pub runtime_meta: HandlerRuntimeMeta,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        let started = Instant::now();
        let message_ctx = MessageContext {
            message_id: msg.id.get(),
            author_id: msg.author.id.get(),
            channel_id: msg.channel_id.get(),
            guild_id: msg.guild_id.map(|id| id.get()),
            author_is_bot: msg.author.bot,
        };

        let decision = {
            let mut guard = match self.core.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.decide_message(message_ctx, &msg.content)
        };

        log_decision(&msg, &decision);

        self.record_decision_audit(
            message_ctx,
            &msg,
            &decision,
            started.elapsed().as_millis() as u64,
        );

        if matches!(decision.action, BotAction::Noop) {
            return;
        }

        if let Err(err) = apply_action(&ctx, &msg, decision.action).await {
            if let Some(status) = status_from_serenity_error(&err) {
                let mut guard = match self.core.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                guard.record_http_status(status);
            }
            warn!(
                content_len = msg.content.chars().count(),
                error = %err,
                "failed to apply bot action"
            );
        } else {
            self.record_action_sent_audit(message_ctx, &msg, started.elapsed().as_millis() as u64);
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!(user = %ready.user.name, "bot is connected");
    }
}

impl Handler {
    fn record_decision_audit(
        &self,
        message_ctx: MessageContext,
        msg: &Message,
        decision: &ActionDecision,
        processing_time_ms: u64,
    ) {
        let Some(audit) = &self.audit else {
            return;
        };

        let detection = {
            let guard = match self.core.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.last_detection()
        };

        let event = AuditEventInput {
            event_type: if matches!(decision.action, BotAction::Noop) {
                AuditEventType::ActionSuppressed
            } else {
                AuditEventType::ResponseCompiled
            },
            binary_version: self.runtime_meta.binary_version.clone(),
            config_fingerprint: self.runtime_meta.config_fingerprint.clone(),
            detector_backend: detection
                .as_ref()
                .map(|report| report.matched_backend.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            matched_readings: detection
                .as_ref()
                .map(|report| report.matched_readings.clone())
                .unwrap_or_default(),
            sequence_hits: detection.as_ref().map(|report| report.sequence_hits).unwrap_or(0),
            kanji_hits: detection.as_ref().map(|report| report.kanji_hits).unwrap_or(0),
            total_count: detection.as_ref().map(|report| report.total_count).unwrap_or(0),
            special_phrase_hit: detection
                .as_ref()
                .map(|report| report.special_phrase_hit)
                .unwrap_or(false),
            selected_action: format!("{:?}", decision.action),
            suppressed_reason: decision.suppress_reason.map(|reason| format!("{:?}", reason)),
            mode: format!("{:?}", decision.mode),
            active_lsm: self.runtime_meta.active_lsm.clone(),
            hardening_status: self.runtime_meta.hardening_status.clone(),
            processing_time_ms,
            message_length: msg.content.chars().count(),
            normalized_length: msg.content.nfkc().count(),
            token_count: detection.as_ref().map(|report| report.token_count).unwrap_or(0),
            suspicious_flags: vec![format!("{:?}", decision.suspicion)],
            truncated_flag: matches!(decision.action, BotAction::SendMessage { ref content } if content.chars().count() >= 1900),
            guild_id: message_ctx.guild_id,
            channel_id: Some(message_ctx.channel_id),
            user_id: Some(message_ctx.author_id),
            message_id: Some(message_ctx.message_id),
        };

        let mut guard = match audit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Err(err) = guard.record_event(&event) {
            warn!(error = %err, "failed to record decision audit event");
        }
    }

    fn record_action_sent_audit(
        &self,
        message_ctx: MessageContext,
        msg: &Message,
        processing_time_ms: u64,
    ) {
        let Some(audit) = &self.audit else {
            return;
        };

        let event = AuditEventInput {
            event_type: AuditEventType::ActionSent,
            binary_version: self.runtime_meta.binary_version.clone(),
            config_fingerprint: self.runtime_meta.config_fingerprint.clone(),
            active_lsm: self.runtime_meta.active_lsm.clone(),
            hardening_status: self.runtime_meta.hardening_status.clone(),
            processing_time_ms,
            message_length: msg.content.chars().count(),
            normalized_length: msg.content.nfkc().count(),
            guild_id: message_ctx.guild_id,
            channel_id: Some(message_ctx.channel_id),
            user_id: Some(message_ctx.author_id),
            message_id: Some(message_ctx.message_id),
            ..AuditEventInput::default()
        };

        let mut guard = match audit.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Err(err) = guard.record_event(&event) {
            warn!(error = %err, "failed to record action_sent audit event");
        }
    }
}

async fn apply_action(ctx: &Context, msg: &Message, action: BotAction) -> serenity::Result<()> {
    match action {
        BotAction::Noop => Ok(()),
        BotAction::React { emoji_id, emoji_name, animated } => {
            let emoji = ReactionType::Custom {
                animated,
                id: EmojiId::new(emoji_id),
                name: Some(emoji_name),
            };
            msg.react(&ctx.http, emoji).await.map(|_| ())
        }
        BotAction::SendMessage { content } => {
            msg.channel_id.say(&ctx.http, content).await.map(|_| ())
        }
    }
    .inspect_err(|err| {
        error!(error = %err, "discord API request failed");
    })
}

fn status_from_serenity_error(err: &SerenityError) -> Option<u16> {
    match err {
        SerenityError::Http(http_err) => http_err.status_code().map(|code| code.as_u16()),
        _ => None,
    }
}

fn log_decision(msg: &Message, decision: &ActionDecision) {
    info!(
        content_len = msg.content.chars().count(),
        analyzer_result = ?decision.proposal,
        final_action = ?decision.action,
        suppress_reason = ?decision.suppress_reason,
        mode = ?decision.mode,
        suspicion = ?decision.suspicion,
        "governor decision"
    );
}
