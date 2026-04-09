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

この repo には利用者が 2 種類います。

- Bot を動かす運用者
- 監査や開発を行う contributor / CI

Nix は後者向けの任意導入です。Bot の利用者や運用者へ、Nix のインストールを必須にはしません。

### Bot を動かすだけの最短手順

1. `.env` を作成

```bash
cp env.example .env
```

2. 生成ステップ

```bash
cargo xtask generate
```

3. Bot 実行

```bash
cargo run
```

### 開発・監査ツールを入れたい場合

この repo では `cargo-audit` / `cargo-deny` / `semgrep` などの追加ツールを使います。ここは Bot 本体の実行には不要です。

選択肢は次の 2 つです。

- Nix を使う
  - 再現性と供給網保護を重視する contributor / CI 向け
  - Nix を入れた上で、必要な監査ツールを profile や dev shell で配布する想定
- Nix を使わない
  - 既存の Rust / Python 環境だけで進めたい開発者向け
  - `./scripts/bootstrap_security_tools.sh` で `cargo install` / `pipx` ベースのツール導入を行う

現在の `bootstrap_security_tools.sh` は、非 Nix 環境向けの互換導線です。Nix dev shell を使う場合でも、エンドユーザー向けではなく contributor / CI 向けの導線として扱います。

### Nix を使う場合

この repo には [flake.nix](flake.nix) を追加しています。Nix を使う場合の基本操作は次のとおりです。

1. 開発シェルに入る

```bash
nix develop
```

2. 生成と軽量確認

```bash
cargo xtask generate
cargo test --workspace --all-features
```

3. 監査込みのローカル CI 相当

```bash
just ci-local
```

必要なら、shell に入らずツール群だけ profile へ入れることもできます。

```bash
nix profile install .#security-tools
```

必要な CLI は dev shell に同梱しています。

- Rust toolchain: `cargo`, `rustc`, `rustfmt`, `clippy`, `rust-analyzer`
- quality/security: `cargo-audit`, `cargo-deny`, `cargo-nextest`, `cargo-llvm-cov`, `cargo-geiger`, `cargo-hack`, `cargo-fuzz`, `cargo-mutants`, `cargo-machete`, `cargo-udeps`, `semgrep`
- build support: `just`, `pkg-config`, `openssl`

「Nix を常用したくないが、一時的に監査だけ再現したい」という場合は、shell に入るだけで十分です。ユーザーのグローバル環境へ各ツールを個別 install する必要はありません。

### Nix を使わない場合

非 Nix 環境では従来どおり次を使えます。

```bash
./scripts/bootstrap_security_tools.sh
```

## ローカル再現テスト (Discord API 不要)

### replay harness

```bash
cargo run --bin replay -- tests/fixtures/replay
```

governed replay は trusted core + sandbox を通します。

現在の replay CLI は test harness と同じ基準で動きます。

- fixture ごとに `TrustedCore` を再構築し、通常は状態を持ち越しません
- `runtime.preserve_state: true` を付けた fixture だけ、直前ケースの duplicate / breaker / sandbox failure 状態を引き継ぎます
- replay 専用の baseline runtime は cooldown を 0、breaker threshold を 2 にしてあり、fixture の期待値と一致するよう固定されています
- `[[sandbox_trap]]` / `[[sandbox_timeout]]` を含む入力は、replay harness 内で sandbox error を決定的に注入します

このため、`cargo run --bin replay -- tests/fixtures/replay` と
`cargo test --test replay_harness --test replay_suppress_reason_regression --all-features`
の結果は、同じ fixture に対して整合する前提です。

### 全 CI 相当チェック

```bash
just ci-local
```

監査ツールが未導入の環境では、まず追加ツールなしで次の軽量確認から始めると安全です。

```bash
cargo test --workspace --all-features
cargo run --bin replay -- tests/fixtures/replay
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
- `.github/dependabot.yml`
	- cargo / fuzz / xtask / GitHub Actions の依存更新 PR
	- Dependabot alerts / security updates と併用する継続監視導線
- `.github/workflows/security.yml`
	- miri (heavy)
	- fuzz smoke (heavy, analyze_message/sandbox_abi/replay_parser)
	- mutation test (heavy)
	- benchmark sanity (heavy, pure vs sandbox/governor)

## セキュリティ運用メモ

- `DISCORD_TOKEN` をログへ出力しない設計です。
- `.env` はローカル開発向け。運用では Secret Manager を推奨。
- 起動時に token 形式の軽量バリデーションを実施します。
- 整数設定は実装上の型幅で厳密に parse します。
  - 例: `OO_GLOBAL_RATE_BURST` は `u32`
  - 例: `OO_MAX_ACTIONS_PER_MESSAGE` は `u8`
  - 例: `OO_SESSION_BUDGET_*` の total / remaining / low watermark は `u32`
- `OO_SESSION_BUDGET_REMAINING > OO_SESSION_BUDGET_TOTAL` の場合は起動失敗します。
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

設定の読み込み規則:

- 不正な数値・真偽値・mode 文字列は `StartupError::InvalidEnv` で起動失敗します
- comma-separated ID list は空要素を無視し、各要素は `u64` として parse されます
- `OO_SESSION_BUDGET_REMAINING` は `OO_SESSION_BUDGET_TOTAL` 以下である必要があります

詳細は以下を参照してください。

- `docs/reference/env-reference.md`
- `docs/reference/config-reference.md`
- `docs/operations/troubleshooting.md`

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
