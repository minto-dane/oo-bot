use unicode_normalization::UnicodeNormalization;

/// Normalizes dictionary readings for stable matching.
///
/// Applied rules:
/// - NFKC normalization
/// - trim both ends
/// - strip separators and punctuation used in dictionary reading notation
/// - convert katakana to hiragana
pub fn normalize_reading(input: &str) -> String {
    let normalized = input.nfkc().collect::<String>();
    let trimmed = normalized.trim();

    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if should_drop(ch) {
            continue;
        }
        out.push(katakana_to_hiragana(ch));
    }
    out.nfkc().collect()
}

#[inline]
fn should_drop(ch: char) -> bool {
    matches!(
        ch,
        '.' | '-' | '‐' | '‑' | '–' | '—' | '―' | '・' | '･' | ' ' | '　' | '\t' | '\n' | '\r'
    )
}

#[inline]
fn katakana_to_hiragana(ch: char) -> char {
    let code = ch as u32;
    if (0x30A1..=0x30F6).contains(&code) {
        // Katakana and hiragana are aligned by this offset in Unicode.
        char::from_u32(code - 0x60).unwrap_or(ch)
    } else {
        ch
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_reading;

    #[test]
    fn strips_kanjidic_markers() {
        assert_eq!(normalize_reading(" おお.- "), "おお");
    }

    #[test]
    fn converts_katakana_to_hiragana() {
        assert_eq!(normalize_reading("オオ"), "おお");
    }

    #[test]
    fn handles_nfkc_for_half_width_katakana() {
        assert_eq!(normalize_reading("ｵｵ"), "おお");
    }
}
