use std::collections::BTreeSet;
use std::sync::Mutex;

use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::domain::reading_normalizer::normalize_reading;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetectorBackendKind {
    #[default]
    MorphologicalReading,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectorPolicy {
    pub target_readings: Vec<String>,
    pub literal_sequence_patterns: Vec<String>,
    pub special_phrases: Vec<String>,
}

impl Default for DetectorPolicy {
    fn default() -> Self {
        crate::config::canonical_detector_policy()
    }
}

impl DetectorPolicy {
    pub fn normalized_target_readings(&self) -> Vec<String> {
        self.target_readings
            .iter()
            .map(|value| normalize_reading(value))
            .filter(|value| !value.is_empty())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionReport {
    pub backend: DetectorBackendKind,
    pub matched_backend: &'static str,
    pub matched_readings: Vec<String>,
    pub sequence_hits: usize,
    pub kanji_hits: usize,
    pub total_count: usize,
    pub special_phrase_hit: bool,
    pub token_count: usize,
}

impl DetectionReport {
    #[must_use]
    pub fn backend_count_for_sandbox(&self) -> usize {
        self.total_count
    }
}

pub trait MessageDetector {
    fn backend_kind(&self) -> DetectorBackendKind;
    fn detect(&self, content: &str) -> DetectionReport;
}

pub fn build_detector(
    backend: DetectorBackendKind,
    policy: DetectorPolicy,
) -> Result<Box<dyn MessageDetector + Send + Sync>, String> {
    match backend {
        DetectorBackendKind::MorphologicalReading => {
            Ok(Box::new(MorphologicalReadingDetector::new(policy)?))
        }
        DetectorBackendKind::Fallback => {
            Err("fallback backend is internal-only and cannot be selected in config".to_string())
        }
    }
}

pub struct MorphologicalReadingDetector {
    policy: DetectorPolicy,
    normalized_target_readings: Vec<String>,
    tokenizer: Mutex<Tokenizer>,
}

impl MorphologicalReadingDetector {
    pub fn new(policy: DetectorPolicy) -> Result<Self, String> {
        let dictionary = load_dictionary("embedded://ipadic").map_err(|err| err.to_string())?;
        let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
        let tokenizer = Tokenizer::new(segmenter);

        Ok(Self {
            normalized_target_readings: policy.normalized_target_readings(),
            policy,
            tokenizer: Mutex::new(tokenizer),
        })
    }
}

impl MessageDetector for MorphologicalReadingDetector {
    fn backend_kind(&self) -> DetectorBackendKind {
        DetectorBackendKind::MorphologicalReading
    }

    fn detect(&self, content: &str) -> DetectionReport {
        let sequence_hits = count_literal_patterns(content, &self.policy.literal_sequence_patterns);
        let special_phrase_hit = self
            .policy
            .special_phrases
            .iter()
            .any(|phrase| !phrase.is_empty() && content.contains(phrase));

        let mut matched_readings = BTreeSet::new();
        let mut token_hits = 0usize;
        let mut token_count = 0usize;

        let tokenizer = match self.tokenizer.lock() {
            Ok(tokenizer) => tokenizer,
            Err(_) => {
                warn!("morphological tokenizer lock is poisoned; returning safe non-hit detection");
                return DetectionReport {
                    backend: DetectorBackendKind::MorphologicalReading,
                    matched_backend: "morphological_reading",
                    matched_readings: vec![],
                    sequence_hits,
                    kanji_hits: 0,
                    total_count: sequence_hits,
                    special_phrase_hit,
                    token_count: 0,
                };
            }
        };

        if let Ok(mut tokens) = tokenizer.tokenize(content) {
            token_count = tokens.len();
            for token in &mut tokens {
                let normalized_surface = normalize_reading(&token.surface);
                let mut candidates = Vec::with_capacity(3);
                if let Some(reading) = token.get_detail(7) {
                    candidates.push(reading.to_string());
                }
                if let Some(pronunciation) = token.get_detail(8) {
                    candidates.push(pronunciation.to_string());
                }
                candidates.push(token.surface.to_string());

                let normalized_candidates: Vec<String> = candidates
                    .into_iter()
                    .map(|value| normalize_reading(&value))
                    .filter(|value| !value.is_empty())
                    .collect();

                for normalized in &normalized_candidates {
                    for target in &self.normalized_target_readings {
                        if !normalized_surface.is_empty() && normalized_surface.contains(target) {
                            continue;
                        }
                        if !target.is_empty() && normalized.contains(target) {
                            token_hits = token_hits.saturating_add(1);
                            let _ = matched_readings.insert(target.clone());
                            break;
                        }
                    }
                }
            }
        }

        DetectionReport {
            backend: DetectorBackendKind::MorphologicalReading,
            matched_backend: "morphological_reading",
            matched_readings: matched_readings.into_iter().collect(),
            sequence_hits,
            kanji_hits: token_hits,
            total_count: sequence_hits.saturating_add(token_hits),
            special_phrase_hit,
            token_count,
        }
    }
}

fn count_literal_patterns(content: &str, patterns: &[String]) -> usize {
    patterns
        .iter()
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| count_non_overlapping(content, pattern))
        .sum()
}

fn count_non_overlapping(content: &str, pattern: &str) -> usize {
    if pattern.is_empty() {
        return 0;
    }

    let (haystack, needle) = if pattern.is_ascii() {
        (content.to_ascii_lowercase(), pattern.to_ascii_lowercase())
    } else {
        (content.to_string(), pattern.to_string())
    };

    let mut count = 0usize;
    let mut cursor = 0usize;
    while let Some(found) = haystack[cursor..].find(&needle) {
        count = count.saturating_add(1);
        cursor = cursor.saturating_add(found).saturating_add(needle.len());
        if cursor >= haystack.len() {
            break;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::{build_detector, DetectorBackendKind, DetectorPolicy};

    #[test]
    fn morphological_backend_builds_with_embedded_ipadic() {
        let detector =
            build_detector(DetectorBackendKind::MorphologicalReading, DetectorPolicy::default());
        assert!(detector.is_ok());
    }

    #[test]
    fn morphological_backend_detects_kana_and_kanji_words() {
        let detector =
            build_detector(DetectorBackendKind::MorphologicalReading, DetectorPolicy::default())
                .expect("backend init");

        let kana = detector.detect("おお");
        assert!(kana.total_count >= 1);

        let kanji_word = detector.detect("大きい");
        assert!(kanji_word.total_count >= 1);
    }
}
