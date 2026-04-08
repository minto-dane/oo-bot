# oo-bot

serenity を使った Rust 製 Discord Bot です。  
メッセージ中の oo 系シーケンスと、「読みのどこかに おお を含む単漢字」をカウントし、リアクションまたはスタンプ送信を行います。

今回の実装では、既存の判定仕様を維持したまま self-protection / runtime protection を追加しています。

## ドキュメント導線

- 全体索引: [docs/index.md](docs/index.md)
- docs ポータル: [docs/README.md](docs/README.md)

## 仕様

### 1) oo 系のカウント仕様

次を non-overlapping でカウントします。

- ひらがな: `おお`
- カタカナ: `オオ`
- ASCII: `oo`, `oO`, `Oo`, `OO`

例:

- `おおおお` -> `2`
- `oooo` -> `2`
- `おおお` -> `1`
- `OoOO` -> `2`
- `おおoo大` -> `3` (最後の `大` が辞書判定対象の場合)

### 2) 漢字カウント仕様

- 1文字単位で判定します。
- 単漢字が持つ読みのうち、正規化後に `おお` を含むものが1つでもあれば、その漢字1文字につき `+1`。
- 対象読み: `ja_kun`, `nanori`, `ja_on` (デフォルトで有効。生成時フラグで無効化可能)
- `ja_on` はカタカナをひらがなへ正規化して判定します。
- 読み記号として `.` `-` `・` などは除去して判定します。
- Unicode 正規化は NFKC を適用します。
- 熟語読みは対象外。KANJIDIC2 の単漢字読みのみを対象にします。

### 3) special phrase 優先仕様

- メッセージが `これはおお` を含む場合は、通常カウントより優先してスタンプ1件送信。

### 4) BotAction 仕様

解析結果は次の pure な列挙型で返します。

- `Noop`
- `React { emoji_id, emoji_name, animated }`
- `SendMessage { content }`

このため Discord API なしで挙動テストできます。

## Trusted Core + Sandbox

- trusted core
	- `DISCORD_TOKEN` を保持する唯一の領域
	- Discord Gateway/REST 送信
	- duplicate suppression / cooldown / token bucket / circuit breaker
	- mode/state 管理 (`Normal`, `ObserveOnly`, `ReactOnly`, `AuditOnly`, `FullDisable`)
	- allowlist/denylist・kill switch・session budget guard
	- final action 決定 (`ActionProposal` -> `BotAction`)
- sandboxed analyzer
	- Wasmtime 上で動作する WebAssembly guest
	- token / serenity 型 / file / network / env / wall-clock へのアクセスなし
	- 入力: message text + kanji_count + special_phrase_hit
	- 出力: 制限された `ActionProposal`

主な配置:

- `src/security/core_governor.rs`: trusted core
- `src/security/mode.rs`: mode state machine
- `src/security/rate_limiter.rs`: global token bucket
- `src/security/duplicate_guard.rs`: duplicate suppression
- `src/security/circuit_breaker.rs`: 401/403/429 breaker
- `src/security/session_budget.rs`: gateway/session budget guard
- `src/security/suspicious_input.rs`: suspicious input classifier
- `src/sandbox/abi.rs`: sandbox ABI (`ActionProposal` wire format)
- `src/sandbox/host.rs`: Wasmtime host + guest module
- `src/infra/discord_handler.rs`: very thin serenity adapter
- `src/app/replay.rs`: pure replay + governed replay

## なぜ Wasm Sandbox か

- Rust モジュール分離だけでは、同一アドレス空間で capability を強制できません。
- Wasmtime により guest のメモリ・実行予算を明示制限できます。
- guest へ host imports を与えないため、token・network・filesystem への経路を構造的に持てません。

この実装では fuel を採用しています。

- 理由: 同一入力・同一初期状態で timeout/trap 判定を決定的に再現しやすいため
- 参考: Wasmtime docs は fuel を deterministic な停止条件向け、epoch を高速寄りとして説明

## Runtime Protection 仕様

- self-trigger prevention: bot authored message を常に drop
- duplicate suppression: message id + TTL キャッシュ
- cooldown: per-user/per-channel/per-guild/global
- global outbound governor: token bucket
- per-message caps: max actions / max send chars
- suspicious input: soft/hard 長さ上限, bidi control, repetition spike
- invalid request protection: 401/403/429 を breaker へ集約
- circuit breaker: 開放中は outbound 抑止 (`ObserveOnly`)
- mode transitions:
	- kill switch -> `FullDisable`
	- breaker open -> `ObserveOnly`
	- session budget low -> `ReactOnly`
	- sandbox failure spike -> `AuditOnly`
	- recovery -> `Normal`

## Mode Table

| Mode | 送信 | 典型トリガ |
|---|---|---|
| `Normal` | react/send 許可 | 通常運転 |
| `ObserveOnly` | 全 outbound 抑止 | breaker open, invalid request surge |
| `ReactOnly` | react のみ許可 | session budget low |
| `AuditOnly` | 全 outbound 抑止 | sandbox trap/timeout 連発 |
| `FullDisable` | 全 outbound 抑止 | kill switch / invalid token 相当 |

## データソースとライセンス

- 主データ: KANJIDIC2 (`data/vendor/kanjidic2.xml.gz`)
- 由来: Electronic Dictionary Research and Development Group (EDRDG)
- ライセンス表記・更新手順: `data/vendor/README.md`

ランタイムでは外部ネットワークを使いません。  
辞書は `cargo xtask generate` で静的 Rust ソースに変換され、`src/generated/kanji_oo_db.rs` を参照します。

## セットアップ

1. `.env` を作成

```bash
cp env.example .env
```

2. 依存ツール導入

```bash
./scripts/bootstrap_security_tools.sh
```

3. 生成ステップ

```bash
cargo xtask generate
```

4. Bot 実行

```bash
cargo run
```

## ローカル再現テスト (Discord API 不要)

### replay harness

```bash
cargo run --bin replay -- tests/fixtures/replay
```

governed replay は trusted core + sandbox を通します。

### 全 CI 相当チェック

```bash
just ci-local
```

### runtime protection smoke

```bash
just runtime-smoke
```

### fault injection

```bash
just fault-inject
```

### fuzz smoke

```bash
just fuzz-smoke
```

## CI

- `.github/workflows/ci.yml`
	- format check
	- clippy
	- unit + integration tests
	- runtime protection tests
	- fault injection tests
	- nextest
	- coverage
	- cargo-audit
	- cargo-deny
	- cargo-geiger
	- semgrep
	- feature matrix
	- 辞書生成 deterministic check
	- docs build
- `.github/workflows/security.yml`
	- miri (heavy)
	- fuzz smoke (heavy, analyze_message/sandbox_abi/replay_parser)
	- mutation test (heavy)
	- benchmark sanity (heavy, pure vs sandbox/governor)

## セキュリティ運用メモ

- `DISCORD_TOKEN` をログへ出力しない設計です。
- `.env` はローカル開発向け。運用では Secret Manager を推奨。
- 起動時に token 形式の軽量バリデーションを実施します。
- Gateway Intent は `GUILD_MESSAGES`, `DIRECT_MESSAGES`, `MESSAGE_CONTENT` のみ。
- `MESSAGE_CONTENT` はメッセージ解析のため必須です。
- 入力全文ログを避け、監査向けメタ情報のみ記録します。
- 送信量制御: cooldown + bucket + breaker + per-message cap。
- invalid request 抑止: 401/403/429 を蓄積して自動縮退。
- Discord rate-limit docs を前提に、固定ハードコードではなく応答を見て抑止します。

### Token の存在範囲

- ある場所: `main` から起動される trusted core / serenity client
- ない場所: sandbox guest, replay fixtures, generator, domain pure logic

## 設定

`env.example` に runtime protection 用設定を追加しています。

- mode/kill switch: `OO_MODE_OVERRIDE`, `OO_EMERGENCY_KILL_SWITCH`
- allow/deny: `OO_ALLOW_*`, `OO_DENY_*`
- cooldown/bucket: `OO_COOLDOWN_*`, `OO_GLOBAL_RATE_*`
- suspicious thresholds: `OO_LONG_MESSAGE_*`, `OO_SUSPICIOUS_REPETITION_THRESHOLD`
- breaker: `OO_BREAKER_*`
- sandbox budget: `OO_SANDBOX_*`
- session budget: `OO_SESSION_BUDGET_*`

## 辞書更新・再生成手順

1. `data/vendor/kanjidic2.xml.gz` を更新
2. `cargo xtask generate`
3. `cargo xtask verify`
4. `cargo test --workspace --all-features`

## 非目標

- 熟語単位の読み推定
- 文脈依存の同形異義判定
- confusable 文字の完全な同一視
- Discord 実サーバー接続を前提にした E2E

## 残留リスク

- Discord API の外部障害やプラットフォーム全体障害は bot 単体では解決不可
- confusable を完全正規化しないため、一部の見た目攻撃は検知漏れの可能性
- Wasmtime/依存ライブラリの脆弱性は supply-chain 管理に依存

## Self-protection と OS/Infra 依存の境界

- self-protection (このリポジトリ内): governor, breaker, cooldown, sandbox budget, mode degrade
- OS/infra 側: process isolation, cgroup, host firewall, secret manager

本実装は OS hardening を主防御にせず、アプリ内 capability 分離を主防御にしています。
