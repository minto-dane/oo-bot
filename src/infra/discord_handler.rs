use std::sync::{Arc, Mutex};

use serenity::{
    all::{EmojiId, ReactionType},
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
    Error as SerenityError,
};
use tracing::{error, info, warn};

use crate::{
    app::analyze_message::BotAction,
    security::core_governor::{ActionDecision, MessageContext, TrustedCore},
};

#[derive(Clone)]
pub struct Handler {
    pub core: Arc<Mutex<TrustedCore>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
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
                channel_id = msg.channel_id.get(),
                message_id = msg.id.get(),
                author_id = msg.author.id.get(),
                content_len = msg.content.chars().count(),
                error = %err,
                "failed to apply bot action"
            );
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!(user = %ready.user.name, "bot is connected");
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
        guild_id = msg.guild_id.map(|id| id.get()),
        channel_id = msg.channel_id.get(),
        message_id = msg.id.get(),
        author_id = msg.author.id.get(),
        content_len = msg.content.chars().count(),
        analyzer_result = ?decision.proposal,
        final_action = ?decision.action,
        suppress_reason = ?decision.suppress_reason,
        mode = ?decision.mode,
        suspicion = ?decision.suspicion,
        "governor decision"
    );
}
