# Message Analysis Specification

## 目的

解析ロジック（pure analyzer と sandbox analyzer）の期待挙動を定義します。

## カウント対象

- ひらがな `おお`
- カタカナ `オオ`
- ASCII `oo`, `oO`, `Oo`, `OO`
- 単漢字（generated DB に存在する文字）

## non-overlapping 条件

`count_oo_sequences` はヒット時に 2 文字進めます。

例:

- `oooo` -> 2
- `oOo` -> 1
- `おおお` -> 1

実装: [src/domain/oo_counter.rs](../../src/domain/oo_counter.rs)

## special phrase 優先

`content.contains(special_phrase)` が true の場合、他判定より優先して stamp 1件。

実装: [src/app/analyze_message.rs](../../src/app/analyze_message.rs)

## 漢字カウント

`count_oo_kanji` は message の各 `char` が DB に含まれるかを二分探索で判定します。

実装: [src/domain/kanji_matcher.rs](../../src/domain/kanji_matcher.rs)

## sandbox analyzer の役割

sandbox は以下のみを入力として受けます。

- content bytes
- `kanji_count`
- `special_phrase_hit`

sandbox 出力は `ActionProposal` のみです。Discord API 呼び出しはできません。

実装: [src/sandbox/abi.rs](../../src/sandbox/abi.rs), [src/sandbox/host.rs](../../src/sandbox/host.rs)

## 境界条件

- stamp 文字列が空の場合、`max_repeats_for_len` は 0
- send 文字数は `OO_MAX_SEND_CHARS` で切り詰め
- analyzer trap/timeout は proposal を `Defer` として扱う

## 検証

- unit: [src/app/analyze_message.rs](../../src/app/analyze_message.rs)
- integration: [tests/analyze_message_integration.rs](../../tests/analyze_message_integration.rs)
- property: [tests/property_oo.rs](../../tests/property_oo.rs)
- fuzz: [fuzz/fuzz_targets/analyze_message.rs](../../fuzz/fuzz_targets/analyze_message.rs)
