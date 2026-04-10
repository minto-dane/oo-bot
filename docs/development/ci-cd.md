# CI/CD

## 目的

CI の責務と job 意味を運用・開発で共通理解するための文書です。

## Workflow 一覧

- main: [.github/workflows/ci.yml](../../.github/workflows/ci.yml)
- heavy: [.github/workflows/security.yml](../../.github/workflows/security.yml)
- dependency updates: [.github/dependabot.yml](../../.github/dependabot.yml)

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
- canonical-config-and-artifacts
- docs-and-readme

## Dependency monitoring

依存関係の継続監視は 2 層で行います。

- CI 実行時の検知
  - `cargo audit`
  - `cargo deny check`
- GitHub 上の継続監視
  - Dependabot version updates
  - Dependabot alerts
  - Dependabot security updates

役割分担:

- CI
  - その commit / PR に含まれる lockfile と manifest を検査する
- Dependabot
  - GitHub 側で advisory を継続監視し、修正 PR や alert を出す

この repo では Cargo workspace root、`fuzz`、および GitHub Actions を [dependabot.yml](../../.github/dependabot.yml) で監視します。

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
- canonical-config-and-artifacts: `config/oo-bot.yaml` が bootstrap/render 経路と整合するか確認
- replay 系: fixture expected/action/mode/suppress_reason を確認
