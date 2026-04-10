# Threat Model

## 目的

このシステムの防御対象・攻撃面・信頼境界を明示します。

## 保護対象資産

- Discord bot token
- outbound action capability
- runtime mode/state
- yaml config integrity
- audit/log evidence

## 攻撃者像

- 任意メッセージを投稿できる一般ユーザー
- Bot へ高負荷入力を送る abuse actor
- 設定値を誤設定する運用者（事故）
- 依存関係脆弱性の影響を受ける supply-chain リスク

## 攻撃面

- Discord message content
- REST 失敗連発を誘発する入力パターン
- replayed/duplicated events
- 極端長文/Unicode 異常入力
- 設定改ざん

## 信頼境界

- trusted: core + serenity client + token
- semi-trusted: runtime config
- untrusted: message content, sandbox result, external API status

## 主要脅威と対策

| 脅威 | 対策 |
|---|---|
| token 漏えい | token 非ログ化、sandbox 非伝搬、env からのみ読込 |
| message spam で連投 | duplicate + cooldown + token bucket + caps |
| analyzer 暴走 | Wasmtime fuel + store limits + fail-safe Noop |
| invalid request surge | breaker + observe_only 遷移 |
| 誤送信 | allow/deny と mode gate |
| 設定改ざん | アクセス制御（最小権限、RBAC）+ 変更監査ログ（変更履歴の記録・アラート）+ 署名検証またはハッシュによる設定整合性チェック + 変更承認ワークフロー |
| yaml 設定ドリフト | canonical-config-and-artifacts CI check |

## 参照

- [architecture/runtime-protection.md](../architecture/runtime-protection.md)
- [security/security-controls.md](security-controls.md)
- [security/residual-risks.md](residual-risks.md)
