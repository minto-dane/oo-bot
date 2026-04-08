# Incident Response

## 目的

runtime 保護異常、token 事故、送信暴走疑いに対する標準対応を定義します。

## 重大インシデント定義

- 意図しない大量送信
- token 漏えい疑い
- breaker open が長時間継続
- mode が意図せず `full_disable` 固定

## 初動

1. `OO_EMERGENCY_KILL_SWITCH=true` で即時停止
2. 直近ログの `suppress_reason` / `mode` / `error` を確認
3. Discord 側 401/403/429 の有無を確認

## 分類別対応

### A. Token 漏えい疑い

1. Discord Developer Portal で token 再発行
2. Secret Manager を更新
3. 旧 token を無効化
4. Bot 再起動
5. 監査記録を残す

### B. 送信暴走疑い

1. kill switch 有効
2. replay fixture で再現確認
3. `runtime_protection` テスト群を再実行
4. suppress_reason の欠落有無を確認

### C. breaker open 継続

1. Discord API 障害有無を確認
2. allow/deny と権限設定を確認
3. 必要時 `observe_only` で継続運転

## 復旧判定

- `cargo test --workspace --all-features`
- `cargo test --test replay_harness --test replay_suppress_reason_regression --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`

## 事後対応

- 事象を [appendices/migration-notes.md](../appendices/migration-notes.md) に追記
- 必要なら ADR を追加
