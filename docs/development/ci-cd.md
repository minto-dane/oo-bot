# CI/CD

## 目的

CI の責務と job 意味を運用・開発で共通理解するための文書です。

## Workflow 一覧

- main: [.github/workflows/ci.yml](../../.github/workflows/ci.yml)
- heavy: [.github/workflows/security.yml](../../.github/workflows/security.yml)

## ci.yml job

- format-check
- clippy
- unit-and-integration-tests
- runtime-protection
- nextest
- coverage
- audit
- deny
- geiger
- semgrep
- feature-matrix
- deterministic-db
- docs-and-readme

## runtime-protection job の固定回帰

以下を必須実行:

- `runtime_protection_integration`
- `replay_harness`
- `replay_suppress_reason_regression`
- `fault_injection`

## ローカル同等実行

```bash
just ci-local
```

## 失敗時対応

- clippy/msrv: API 利用互換を見直す
- deterministic-db: 生成物再作成と差分レビュー
- replay 系: fixture expected/action/mode/suppress_reason を確認
