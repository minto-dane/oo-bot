# Migration Notes

## 2026-04-07

### docs 再編

- 旧 root docs
  - runtime_protection.md
  - security_review.md
  - spec_boundaries.md
  - test_strategy.md
- 新階層へ統合
  - architecture/
  - security/
  - development/
  - appendices/

### suppress_reason 固定回帰

- replay schema に `expected_suppress_reason` を追加
- replay harness で mode/action に加えて suppress_reason も照合
- CI runtime-protection job に `replay_suppress_reason_regression` を追加

### 既知の継続課題

- external metrics exporter は未実装
- reconnect backoff/jitter は現行コードでは未実装
