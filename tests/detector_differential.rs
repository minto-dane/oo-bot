use discord_oo_bot::domain::detector::{build_detector, DetectorBackendKind, DetectorPolicy};

#[test]
fn morphological_backend_handles_kanji_tokens() {
    let policy = DetectorPolicy::default();
    let morph = build_detector(DetectorBackendKind::MorphologicalReading, policy)
        .expect("morph backend should initialize");

    let report = morph.detect("大きい");
    assert_eq!(report.backend, DetectorBackendKind::MorphologicalReading);
    assert!(report.total_count >= 1);
}

#[test]
fn morphological_backend_handles_unreadable_tokens_safely() {
    let morph =
        build_detector(DetectorBackendKind::MorphologicalReading, DetectorPolicy::default())
            .expect("morph backend should initialize");

    let report = morph.detect("😀😀😀");
    assert_eq!(report.backend, DetectorBackendKind::MorphologicalReading);
    assert!(report.total_count <= report.token_count.saturating_add(report.sequence_hits));
}
