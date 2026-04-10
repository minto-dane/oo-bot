# oo-bot

serenity を使った Rust 製 Discord Bot です。  
メッセージ中の oo 系シーケンスと、Lindera 形態素解析で得た読みを使ってヒット数を算出し、リアクションまたはスタンプ送信を行います。

今回の実装では、既存の判定仕様を維持したまま self-protection / runtime protection を追加しています。

## ドキュメント導線

- 全体索引: [docs/index.md](docs/index.md)
- docs ポータル: [docs/README.md](docs/README.md)
- TUI リファレンス: [docs/reference/tui-reference.md](docs/reference/tui-reference.md)
- 稼働 / 停止 / systemd: [docs/operations/service-control.md](docs/operations/service-control.md)

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

### 2) 形態素読みカウント仕様

- `morphological_reading` backend のみサポートします。
- Lindera (`embedded://ipadic`) で token 化し、`details()[7]` / `details()[8]` / surface を正規化して評価します。
- target readings (`おお`, `オオ`, `oo`) を含む token を hit として数えます。
- literal sequence (`おお`, `オオ`, `oo`) と重複する token hit は二重加算しません。
- 漢字語も読みが取得できる場合は hit します (例: `大きい`)。

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

## データソース

- 解析辞書は Lindera の埋め込み IPA 辞書 (`embedded://ipadic`) を利用します。
- ランタイムは外部ネットワークを使いません。

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

2. strict config を配置

```bash
cp config/oo-bot.yaml config/oo-bot.local.yaml
export OO_CONFIG_PATH=config/oo-bot.local.yaml
```

3. Bot 実行

```bash
cargo run --bin oo-bot -- run
```

4. 別シェルから状態確認 / TUI / 停止

```bash
cargo run --bin oo-bot -- control status
cargo run --bin oo-bot -- tui
cargo run --bin oo-bot -- control stop
```

`tui` は bot 本体とは別プロセスです。TUI を終了しても bot は継続します。

### audit CLI / TUI

```bash
cargo run --bin oo-bot -- tui
cargo run --bin oo-bot -- tui --page setup
cargo run --bin oo-bot -- audit tail --limit 100
cargo run --bin oo-bot -- audit stats
cargo run --bin oo-bot -- audit inspect 42
cargo run --bin oo-bot -- audit verify
cargo run --bin oo-bot -- audit export --format jsonl --out /tmp/audit.jsonl
cargo run --bin oo-bot -- audit tui
```

### runtime control

```bash
cargo run --bin oo-bot -- control status
cargo run --bin oo-bot -- control stop
```

`Ctrl+C` は operator TUI を閉じるためには使えますが、bot 本体の停止には `control stop` または systemd stop を使う前提です。

### hardening build 検証

```bash
cargo build --release --bin oo-bot
./scripts/verify_hardening.sh target/release/oo-bot stable

./scripts/build_hardened_x64.sh
./scripts/verify_hardening.sh target/x86_64-unknown-linux-gnu/release/oo-bot hardened-x64
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

2. 軽量確認

```bash
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
	- yaml config drift check
	- docs build
- `.github/dependabot.yml`
	- cargo / fuzz / GitHub Actions の依存更新 PR
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

設定は `config/oo-bot.yaml` を source of truth とする strict YAML です。

- detector / bot / runtime / audit / diagnostics / integrity を YAML で定義します。
- unknown key は拒否され、起動失敗します。
- 既定値は `config/oo-bot.yaml` の sample YAML から導出されます。
- 必要な環境変数は `DISCORD_TOKEN` と `OO_CONFIG_PATH`（任意上書き）です。
- `OO_PSEUDO_ID_HMAC_KEY` は pseudo-id を有効にする場合のみ必要です。

設定の読み込み規則:

- 不正な YAML / unknown key / テンプレートプレースホルダ不整合は起動失敗します。
- detached signature を設定した場合、検証失敗は起動失敗します。

詳細は以下を参照してください。

- `docs/reference/config-reference.md`
- `docs/operations/troubleshooting.md`

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
