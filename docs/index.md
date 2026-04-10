# Documentation Index

この文書は docs 全体の索引です。機能説明ではなく、どこに何が書いてあるかを定義します。

## 全体構成

- 概要
  - [overview/architecture.md](overview/architecture.md)
  - [overview/data-flow.md](overview/data-flow.md)
  - [overview/glossary.md](overview/glossary.md)
- プロダクト仕様
  - [product/behavior-spec.md](product/behavior-spec.md)
  - [product/message-analysis-spec.md](product/message-analysis-spec.md)
  - [product/unicode-normalization-spec.md](product/unicode-normalization-spec.md)
- アーキテクチャ
  - [architecture/module-boundaries.md](architecture/module-boundaries.md)
  - [architecture/runtime-protection.md](architecture/runtime-protection.md)
  - [architecture/sandboxing.md](architecture/sandboxing.md)
  - [architecture/governor-and-guardrails.md](architecture/governor-and-guardrails.md)
  - [architecture/replay-harness.md](architecture/replay-harness.md)
  - [architecture/config-model.md](architecture/config-model.md)
- セキュリティ
  - [security/threat-model.md](security/threat-model.md)
  - [security/security-controls.md](security/security-controls.md)
  - [security/secrets-handling.md](security/secrets-handling.md)
  - [security/logging-and-redaction.md](security/logging-and-redaction.md)
  - [security/abuse-resistance.md](security/abuse-resistance.md)
  - [security/residual-risks.md](security/residual-risks.md)
  - [security/incident-response.md](security/incident-response.md)
- 運用
  - [operations/deployment.md](operations/deployment.md)
  - [operations/hardening-and-lsm.md](operations/hardening-and-lsm.md)
  - [operations/runtime-modes.md](operations/runtime-modes.md)
  - [operations/observability.md](operations/observability.md)
  - [operations/troubleshooting.md](operations/troubleshooting.md)
  - [operations/maintenance.md](operations/maintenance.md)
  - [operations/rotation-and-recovery.md](operations/rotation-and-recovery.md)
- 開発
  - [development/local-setup.md](development/local-setup.md)
  - [development/nix-dev-shell.md](development/nix-dev-shell.md)
  - [development/test-strategy.md](development/test-strategy.md)
  - [development/ci-cd.md](development/ci-cd.md)
  - [development/fuzzing.md](development/fuzzing.md)
  - [development/contributing.md](development/contributing.md)
- リファレンス
  - [reference/config-reference.md](reference/config-reference.md)
  - [reference/env-reference.md](reference/env-reference.md)
  - [reference/cli-reference.md](reference/cli-reference.md)
  - [reference/replay-fixture-format.md](reference/replay-fixture-format.md)
  - [reference/metrics-reference.md](reference/metrics-reference.md)
  - [reference/error-reference.md](reference/error-reference.md)
  - [reference/docs-conventions.md](reference/docs-conventions.md)
- ADR
  - [decisions/adr-0001-docs-structure.md](decisions/adr-0001-docs-structure.md)
  - [decisions/adr-0003-runtime-protection-boundary.md](decisions/adr-0003-runtime-protection-boundary.md)
  - [decisions/adr-0004-sandbox-choice.md](decisions/adr-0004-sandbox-choice.md)
- 付録
  - [appendices/compatibility-matrix.md](appendices/compatibility-matrix.md)
  - [appendices/assumptions-and-non-goals.md](appendices/assumptions-and-non-goals.md)
  - [appendices/migration-notes.md](appendices/migration-notes.md)

## 読み分けガイド

- 実装者: overview -> product -> architecture -> development
- SRE/運用: overview -> architecture/runtime-protection -> operations
- セキュリティレビュー: architecture/sandboxing -> security/* -> reference/config-reference
- 監査: index -> decisions/* -> security/* -> operations/*

## 文書更新規約

- 命名規約とテンプレートは [reference/docs-conventions.md](reference/docs-conventions.md)
- 破壊的な仕様変更時は ADR を追加
- docs 更新はコード変更と同一 PR で行う
