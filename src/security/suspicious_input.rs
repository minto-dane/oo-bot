use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspicionLevel {
    None,
    Soft,
    Hard,
}

#[derive(Debug, Clone)]
pub struct SuspiciousInputConfig {
    pub soft_char_limit: usize,
    pub hard_char_limit: usize,
    pub repetition_threshold: usize,
}

impl Default for SuspiciousInputConfig {
    fn default() -> Self {
        Self { soft_char_limit: 2_000, hard_char_limit: 8_000, repetition_threshold: 256 }
    }
}

pub fn classify_suspicious_input(content: &str, cfg: &SuspiciousInputConfig) -> SuspicionLevel {
    let char_count = content.chars().count();
    if char_count >= cfg.hard_char_limit {
        return SuspicionLevel::Hard;
    }

    if char_count >= cfg.soft_char_limit {
        return SuspicionLevel::Soft;
    }

    if has_bidi_control(content) || has_excessive_repetition(content, cfg.repetition_threshold) {
        return SuspicionLevel::Soft;
    }

    let normalized = content.nfkc().collect::<String>();
    if normalized.chars().count() > char_count.saturating_mul(2) {
        return SuspicionLevel::Soft;
    }

    SuspicionLevel::None
}

fn has_bidi_control(s: &str) -> bool {
    s.chars().any(|ch| {
        matches!(
            ch,
            '\u{202A}'
                | '\u{202B}'
                | '\u{202C}'
                | '\u{202D}'
                | '\u{202E}'
                | '\u{2066}'
                | '\u{2067}'
                | '\u{2068}'
                | '\u{2069}'
        )
    })
}

fn has_excessive_repetition(s: &str, threshold: usize) -> bool {
    let mut prev = None;
    let mut streak = 0usize;

    for ch in s.chars() {
        if Some(ch) == prev {
            streak += 1;
            if streak >= threshold {
                return true;
            }
        } else {
            prev = Some(ch);
            streak = 1;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{classify_suspicious_input, SuspicionLevel, SuspiciousInputConfig};

    #[test]
    fn hard_limit_works() {
        let cfg = SuspiciousInputConfig { hard_char_limit: 10, ..SuspiciousInputConfig::default() };
        assert_eq!(classify_suspicious_input("a".repeat(10).as_str(), &cfg), SuspicionLevel::Hard);
    }

    #[test]
    fn bidi_is_soft_suspicious() {
        let cfg = SuspiciousInputConfig::default();
        assert_eq!(classify_suspicious_input("abc\u{202e}", &cfg), SuspicionLevel::Soft);
    }
}
