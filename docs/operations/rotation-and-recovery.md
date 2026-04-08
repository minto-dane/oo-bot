# Rotation and Recovery

## 目的

token・設定・生成物不整合に対する復旧手順を定義します。

## Token Rotation

1. Discord 側で token 再発行
2. Secret Manager 更新
3. 旧 token を無効化
4. プロセス再起動
5. 起動ログ確認 (`bot is connected`)

## Config Corruption Recovery

1. `env.example` と差分比較
2. 破損キーをデフォルトまたは正値へ復旧
3. 起動前に `cargo run` で型検証

## Generated DB Recovery

1. `cargo xtask generate`
2. `cargo xtask verify`
3. `git diff -- src/generated/kanji_oo_db.rs data/generated/kanji_oo_db_meta.json`
4. replay smoke 実施

## Degraded Mode Recovery

- observe_only/audit_only から戻らない場合:
  1. 401/403/429 と sandbox error を確認
  2. 原因解消後に再起動
  3. mode が normal へ戻ることを確認

## 事故後の証跡

- 発生時刻
- suppress_reason と mode の推移
- 実施した復旧コマンド
- 再発防止策
