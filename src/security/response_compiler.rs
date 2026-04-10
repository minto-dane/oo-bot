use crate::app::analyze_message::{ActionPolicy, BotAction, BotConfig};
use crate::domain::detector::DetectionReport;
use crate::sandbox::abi::ActionProposal;
use crate::security::mode::RuntimeMode;
use crate::security::suspicious_input::SuspicionLevel;

pub struct CompileContext<'a> {
    pub proposal: &'a ActionProposal,
    pub mode: RuntimeMode,
    pub suspicion: SuspicionLevel,
    pub bot: &'a BotConfig,
    pub max_send_chars: usize,
    pub matched_backend: &'a str,
    pub matched_reading: Option<&'a str>,
    pub count_cap: usize,
    pub detector_total_count: usize,
}

#[must_use]
pub fn compile_response(ctx: &CompileContext<'_>) -> BotAction {
    let capped_count = ctx.detector_total_count.min(ctx.count_cap.max(1));

    let proposed = proposal_to_action(
        ctx.proposal,
        ctx.bot,
        capped_count,
        ctx.matched_backend,
        ctx.matched_reading,
    );

    let softened = if ctx.suspicion == SuspicionLevel::Soft
        && matches!(proposed, BotAction::SendMessage { .. })
    {
        as_react(ctx.bot)
    } else {
        proposed
    };

    let gated = apply_mode_gate(ctx.mode, softened, ctx.bot);
    let policy_applied = apply_action_policy(gated, ctx.bot);
    let capped = enforce_send_cap(policy_applied, ctx.max_send_chars);

    if is_invalid_action(&capped) {
        BotAction::Noop
    } else {
        capped
    }
}

#[must_use]
pub fn compile_response_from_detection(
    proposal: &ActionProposal,
    mode: RuntimeMode,
    suspicion: SuspicionLevel,
    bot: &BotConfig,
    max_send_chars: usize,
    count_cap: usize,
    detection: Option<&DetectionReport>,
) -> BotAction {
    let backend = detection.map_or("morphological_reading", |d| d.matched_backend);
    let matched = detection.and_then(|d| d.matched_readings.first().map(String::as_str));
    let total = detection.map_or(0usize, |d| d.total_count);

    compile_response(&CompileContext {
        proposal,
        mode,
        suspicion,
        bot,
        max_send_chars,
        matched_backend: backend,
        matched_reading: matched,
        count_cap,
        detector_total_count: total,
    })
}

fn proposal_to_action(
    proposal: &ActionProposal,
    bot: &BotConfig,
    detector_total_count: usize,
    matched_backend: &str,
    matched_reading: Option<&str>,
) -> BotAction {
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
                let repeats = (*count as usize).min(detector_total_count.max(1));
                let rendered = render_template(
                    &bot.send_template,
                    repeats,
                    &bot.stamp_text,
                    matched_backend,
                    matched_reading.unwrap_or_default(),
                    "send_message",
                );
                match rendered {
                    Some(content) if !content.trim().is_empty() => {
                        BotAction::SendMessage { content }
                    }
                    _ => BotAction::Noop,
                }
            }
        }
    }
}

fn render_template(
    template: &str,
    count: usize,
    stamp: &str,
    matched_backend: &str,
    matched_reading: &str,
    action_kind: &str,
) -> Option<String> {
    let mut output = template.to_string();
    let replacements = [
        ("${count}", sanitize_template_value(&count.to_string())),
        ("${stamp}", sanitize_template_value(stamp)),
        ("${matched_backend}", sanitize_template_value(matched_backend)),
        ("${matched_reading}", sanitize_template_value(matched_reading)),
        ("${action_kind}", sanitize_template_value(action_kind)),
    ];

    for (needle, value) in replacements {
        output = output.replace(needle, &value);
    }

    if output.contains("${") {
        return None;
    }

    if output == stamp {
        return Some(repeat_stamp(stamp, count));
    }

    Some(output)
}

fn sanitize_template_value(value: &str) -> String {
    value
        .replace("${", "")
        .replace('}', "")
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect()
}

fn apply_action_policy(action: BotAction, bot: &BotConfig) -> BotAction {
    match bot.action_policy {
        ActionPolicy::ReactOrSend => action,
        ActionPolicy::ReactOnly => match action {
            BotAction::SendMessage { .. } => as_react(bot),
            other => other,
        },
        ActionPolicy::NoOutbound => BotAction::Noop,
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

fn enforce_send_cap(action: BotAction, max_send_chars: usize) -> BotAction {
    match action {
        BotAction::SendMessage { content } => {
            let capped: String = content.chars().take(max_send_chars).collect();
            if capped.trim().is_empty() {
                BotAction::Noop
            } else {
                BotAction::SendMessage { content: capped }
            }
        }
        other => other,
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
    use crate::app::analyze_message::{BotAction, BotConfig};
    use crate::sandbox::abi::ActionProposal;
    use crate::security::mode::RuntimeMode;
    use crate::security::response_compiler::{compile_response, CompileContext};
    use crate::security::suspicious_input::SuspicionLevel;

    #[test]
    fn rejects_undefined_placeholder() {
        let cfg = BotConfig { send_template: "${unknown}".to_string(), ..BotConfig::default() };

        let action = compile_response(&CompileContext {
            proposal: &ActionProposal::SendStamped { count: 3 },
            mode: RuntimeMode::Normal,
            suspicion: SuspicionLevel::None,
            bot: &cfg,
            max_send_chars: 300,
            matched_backend: "morphological_reading",
            matched_reading: Some("おお"),
            count_cap: cfg.max_count_cap,
            detector_total_count: 3,
        });

        assert_eq!(action, BotAction::Noop);
    }
}
