use serde::{Deserialize, Serialize};

use crate::domain::kanji_matcher::{count_oo_kanji, KanjiOoDb};
use crate::domain::oo_counter::count_oo_sequences;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotAction {
    Noop,
    React { emoji_id: u64, emoji_name: String, animated: bool },
    SendMessage { content: String },
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
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            special_phrase: "これはおお".to_string(),
            stamp_text: "<:Omilfy:1489695886773587978>".to_string(),
            reaction: ReactionConfig {
                emoji_id: 1489695886773587978,
                emoji_name: "Omilfy".to_string(),
                animated: false,
            },
            max_count_cap: 48,
            max_send_chars: 1_900,
        }
    }
}

#[must_use]
pub fn analyze_message(
    content: &str,
    author_is_bot: bool,
    config: &BotConfig,
    db: &KanjiOoDb,
) -> BotAction {
    if author_is_bot {
        return BotAction::Noop;
    }

    if content.contains(&config.special_phrase) {
        return BotAction::SendMessage { content: config.stamp_text.clone() };
    }

    let total = count_oo_sequences(content).saturating_add(count_oo_kanji(content, db));
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

    BotAction::SendMessage { content: join_repeated(&config.stamp_text, repeats) }
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

#[cfg(test)]
mod tests {
    use super::{analyze_message, BotAction, BotConfig};
    use crate::domain::kanji_matcher::{KanjiOoDb, KanjiOoDbMetadata};

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

    #[test]
    fn special_phrase_has_priority() {
        let cfg = BotConfig::default();
        let action = analyze_message("これはおお でも他は無視", false, &cfg, &TEST_DB);
        assert_eq!(action, BotAction::SendMessage { content: cfg.stamp_text });
    }

    #[test]
    fn one_hit_reacts() {
        let cfg = BotConfig::default();
        let action = analyze_message("おお", false, &cfg, &TEST_DB);
        assert!(matches!(action, BotAction::React { .. }));
    }

    #[test]
    fn multiple_hits_send_message() {
        let cfg = BotConfig::default();
        let action = analyze_message("おおoo", false, &cfg, &TEST_DB);
        assert!(matches!(action, BotAction::SendMessage { .. }));
    }

    #[test]
    fn bot_message_is_ignored() {
        let cfg = BotConfig::default();
        let action = analyze_message("おお", true, &cfg, &TEST_DB);
        assert_eq!(action, BotAction::Noop);
    }
}
