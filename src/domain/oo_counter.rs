/// Counts non-overlapping occurrences of:
/// - hiragana "おお"
/// - katakana "オオ"
/// - ASCII case-insensitive "oo"
pub fn count_oo_sequences(text: &str) -> usize {
    let mut count = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(current) = chars.next() {
        let Some(&next) = chars.peek() else {
            break;
        };

        let hit = (current == 'お' && next == 'お')
            || (current == 'オ' && next == 'オ')
            || (is_ascii_o(current) && is_ascii_o(next));

        if hit {
            count += 1;
            let _ = chars.next();
        }
    }

    count
}

#[inline]
fn is_ascii_o(ch: char) -> bool {
    matches!(ch, 'o' | 'O')
}

#[cfg(test)]
mod tests {
    use super::count_oo_sequences;

    #[test]
    fn counts_non_overlapping_hiragana() {
        assert_eq!(count_oo_sequences("おおおお"), 2);
        assert_eq!(count_oo_sequences("おおお"), 1);
    }

    #[test]
    fn counts_non_overlapping_ascii_case_insensitive() {
        assert_eq!(count_oo_sequences("oooo"), 2);
        assert_eq!(count_oo_sequences("OoOO"), 2);
        assert_eq!(count_oo_sequences("oOo"), 1);
    }

    #[test]
    fn mixed_inputs_work() {
        assert_eq!(count_oo_sequences("おおooオオ"), 3);
        assert_eq!(count_oo_sequences("abc"), 0);
    }
}
