# Assumptions and Non-goals

## Assumptions

- Discord Gateway/REST が提供される
- token は運用基盤で安全に管理される
- KANJIDIC2 ライセンス要件が満たされる
- replay/fault-injection が release 前に実行される

## Non-goals

- 熟語解析/NLP
- confusable 完全検出
- Discord 実サーバ必須の E2E を CI 常設
- プロセス隔離をアプリ内で代替すること

## 補足

self-protection はアプリ内 capability 分離と governor に主軸を置く。
OS/infra hardening は補助的対策として扱う。
