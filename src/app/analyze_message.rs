use serde::{Deserialize, Serialize};
use tracing::error;

use crate::domain::detector::{
    DetectionReport, DetectorPolicy, MessageDetector, MorphologicalReadingDetector,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotAction {
    Noop,
    React { emoji_id: u64, emoji_name: String, animated: bool },
    SendMessage { content: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionPolicy {
    ReactOrSend,
    ReactOnly,
    NoOutbound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReactionConfig {
    pub emoji_id: u64,
    pub emoji_name: String,
    pub animated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotConfig {
    pub special_phrase: String,
    pub stamp_text: String,
    pub reaction: ReactionConfig,
    pub max_count_cap: usize,
    pub max_send_chars: usize,
    pub send_template: String,
    pub action_policy: ActionPolicy,
}

impl Default for BotConfig {
    fn default() -> Self {
        crate::config::canonical_bot_config()
    }
}

#[must_use]
pub fn analyze_message(
    content: &str,
    author_is_bot: bool,
    config: &BotConfig,
) -> BotAction {
    match MorphologicalReadingDetector::new(DetectorPolicy::default()) {
        Ok(detector) => analyze_message_with_detector(content, author_is_bot, config, &detector),
        Err(err) => {
            error!(error = %err, "failed to initialize MorphologicalReadingDetector; falling back to BotAction::Noop");
            BotAction::Noop
        }
    }
}

#[must_use]
pub fn analyze_message_with_detector(
    content: &str,
    author_is_bot: bool,
    config: &BotConfig,
    detector: &dyn MessageDetector,
) -> BotAction {
    if author_is_bot {
        return BotAction::Noop;
    }

    let report = detector.detect(content);

    if report.special_phrase_hit || content.contains(&config.special_phrase) {
        return BotAction::SendMessage { content: config.stamp_text.clone() };
    }

    let total = report.total_count;
    if total == 0 {
        return BotAction::Noop;
    }

    let capped = total.min(config.max_count_cap.max(1));
    if capped == 1 {
        return BotAction::React {
            emoji_id: config.reaction.emoji_id,
            emoji_name: config.reaction.emoji_name.clone(),
            animated: config.reaction.animated,
        };
    }

    let max_repeats_by_len = max_repeats_for_len(&config.stamp_text, config.max_send_chars).max(1);
    let repeats = capped.min(max_repeats_by_len);

    let mut action = BotAction::SendMessage {
        content: render_send_template(config, repeats, &report)
            .unwrap_or_else(|| join_repeated(&config.stamp_text, repeats)),
    };

    action = enforce_action_policy(action, config);
    action
}

#[inline]
fn max_repeats_for_len(stamp: &str, max_send_chars: usize) -> usize {
    if stamp.is_empty() {
        return 0;
    }
    let unit = stamp.chars().count();
    let separator = 1usize;
    (max_send_chars + separator) / (unit + separator)
}

fn join_repeated(stamp: &str, repeats: usize) -> String {
    let mut out =
        String::with_capacity(stamp.len().saturating_mul(repeats).saturating_add(repeats));
    for idx in 0..repeats {
        if idx > 0 {
            out.push(' ');
        }
        out.push_str(stamp);
    }
    out
}

fn enforce_action_policy(action: BotAction, config: &BotConfig) -> BotAction {
    match config.action_policy {
        ActionPolicy::ReactOrSend => action,
        ActionPolicy::ReactOnly => match action {
            BotAction::SendMessage { .. } => BotAction::React {
                emoji_id: config.reaction.emoji_id,
                emoji_name: config.reaction.emoji_name.clone(),
                animated: config.reaction.animated,
            },
            other => other,
        },
        ActionPolicy::NoOutbound => BotAction::Noop,
    }
}

fn render_send_template(config: &BotConfig, count: usize, report: &DetectionReport) -> Option<String> {
    let mut output = config.send_template.clone();

    let placeholders = [
        ("${count}", sanitize_template_value(&count.to_string())),
        ("${stamp}", sanitize_template_value(&config.stamp_text)),
        ("${matched_backend}", sanitize_template_value(report.matched_backend)),
        (
            "${matched_reading}",
            sanitize_template_value(report.matched_readings.first().map(String::as_str).unwrap_or_default()),
        ),
        ("${action_kind}", sanitize_template_value("send_message")),
    ];

    for (needle, value) in placeholders {
        output = output.replace(needle, &value);
    }

    if output.contains("${") {
        return None;
    }

    let expanded = if output == config.stamp_text {
        join_repeated(&output, count)
    } else {
        output
    };

    let capped: String = expanded.chars().take(config.max_send_chars).collect();
    if capped.is_empty() {
        None
    } else {
        Some(capped)
    }
}

fn sanitize_template_value(value: &str) -> String {
    value
        .replace("${", "")
        .replace('}', "")
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{analyze_message, ActionPolicy, BotAction, BotConfig};

    #[test]
    fn special_phrase_has_priority() {
        let cfg = BotConfig::default();
        let action = analyze_message("これはおお でも他は無視", false, &cfg);
        assert_eq!(action, BotAction::SendMessage { content: cfg.stamp_text });
    }

    #[test]
    fn one_hit_reacts() {
        let cfg = BotConfig::default();
        let action = analyze_message("おお", false, &cfg);
        assert!(matches!(action, BotAction::React { .. }));
    }

    #[test]
    fn multiple_hits_send_message() {
        let cfg = BotConfig::default();
        let action = analyze_message("おおoo", false, &cfg);
        assert!(matches!(action, BotAction::SendMessage { .. }));
    }

    #[test]
    fn bot_message_is_ignored() {
        let cfg = BotConfig::default();
        let action = analyze_message("おお", true, &cfg);
        assert_eq!(action, BotAction::Noop);
    }

    #[test]
    fn react_only_policy_downgrades_send() {
        let cfg = BotConfig {
            action_policy: ActionPolicy::ReactOnly,
            ..BotConfig::default()
        };
        let action = analyze_message("おおoo", false, &cfg);
        assert!(matches!(action, BotAction::React { .. }));
    }
}
