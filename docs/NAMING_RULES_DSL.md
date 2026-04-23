# Naming Rules DSL 仕様

Progest の命名規則エンジン（`core::rules`）が解釈する DSL の正規仕様。parser / evaluator 実装はこの文書と bit-for-bit 一致させる。

最終更新: 2026-04-23
対象: v1.0 MVP（macOS 先行）

関連:
- [REQUIREMENTS.md §3.4 命名規則エンジン](./REQUIREMENTS.md) — ハイレベル意思決定（本文書の上位）
- [IMPLEMENTATION_PLAN.md §5 M2](./IMPLEMENTATION_PLAN.md) — モジュール完了条件
- [M2_HANDOFF.md](./M2_HANDOFF.md) — parser 着手前の kickoff メモ

---

## 0. 位置付け

- 命名規則は **テンプレート規則** と **制約規則** の 2 層。どちらも `.progest/rules.toml` に `[[rules]]` エントリとして宣言する。
- 評価は必ず rule_id trace を返す（勝利規則・継承チェーン・理由）。
- 規則定義はチーム共有対象（git 管理下）。評価結果は再計算可能なためキャッシュ扱い。
- DSL が扱うのは「名前の文字列・パス」まで。配置規則（accepts）は別 DSL（`.dirmeta.toml` の `[accepts]`）として分離する。
- v1 スコープ外（v1.x 以降）: ルールファイルの `include`、`[[rules]] extends = ...`（規則継承）、`{today:}` プレースホルダー、`pack_gaps = true`（欠番詰め）、`--explain=verbose`。

---

## 1. ファイル配置と基本レイアウト

### 1.1 ファイル

- プロジェクトルート共有: `.progest/rules.toml`（git commit される）
- ディレクトリローカル上書き: `<dir>/.dirmeta.toml` の `[[rules]]` セクション

### 1.2 `rules.toml` 最小スケルトン

```toml
# .progest/rules.toml
schema_version = 1

# テンプレート規則（厳密パターン、basename を丸ごと規定）
[[rules]]
id = "shot-assets-v1"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
mode = "warn"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"

# 制約規則（緩い制約）— AND 合成されるので目的別に細かく切って良い
[[rules]]
id = "ascii-only"
kind = "constraint"
applies_to = ["./assets/**", "./references/**"]  # 配列可、OR 評価
mode = "warn"
charset = "ascii"
forbidden_chars = [" ", "　"]

[[rules]]
id = "length-cap"
kind = "constraint"
applies_to = "./**"
max_length = 96
```

### 1.3 スキーマバージョン / forward-compat

- `schema_version` は `rules.toml` / `.dirmeta.toml` 両方の最上位で **必須**。
- v1 は `schema_version = 1` 固定。
- ロード時の未知フィールドの扱い:
  - **`schema_version` がロード側と同じ**（= 1）: 未知キーは **typo 疑いとして warning**（stderr、ロード自体は成功）。lint の `--format json` 出力には `warnings: [...]` として載せる。
  - **`schema_version` がロード側より新しい**（= 2+ を v1 が読む）: 新フィールドとして `extra: Table` に保持、warning なし。
  - **`schema_version` がロード側より古い**: 現時点（v1）では発生しないので未定義。将来 migration path を提供する。
- 将来の互換破壊時のみ `schema_version` を bump。新フィールドの**互換追加**は `schema_version` 据え置きで OK。

---

## 2. エントリ共通フィールド

全 `[[rules]]` エントリで共通のフィールド。

| フィールド | 型 | 必須 | 意味 |
| --- | --- | --- | --- |
| `id` | string | ✅ | 規則識別子。`^[a-z][a-z0-9_-]{0,63}$` に制限。**同一定義ファイル内でユニーク**（§7.2 も参照） |
| `kind` | `"template"` \| `"constraint"` | ✅ | 2 層のどちらか |
| `applies_to` | string \| string[] | ✅ | §3 で規定。配列の場合は OR 評価 |
| `mode` | `"strict"` \| `"warn"` \| `"hint"` \| `"off"` | ❌ | 省略時 `"warn"` |
| `description` | string | ❌ | UI / lint メッセージに載せる自由記述 |
| `override` | bool | ❌ | §7.2。子レイヤで親を置換する意図の明示フラグ |

`id` は **同一定義ファイル内でユニーク**。異なるレイヤ（`.progest/rules.toml` / 各 `.dirmeta.toml`）では同一 `id` を許容し、子レイヤで同一 `id` を再定義した場合は §7.2 の override 解決が発動する。

---

## 3. `applies_to` — glob 構文

### 3.1 基準パス

- `.progest/rules.toml` 内の `applies_to` は **プロジェクトルート相対**。`./` で始まる。
- `<dir>/.dirmeta.toml` 内の `applies_to` は **その `<dir>` 相対**。`./` で始まる。
- loader は `.dirmeta.toml` 側の glob を **プロジェクトルート相対に正規化** した上で保持・評価する。
- セパレータは `/` 固定（Windows 移植時も正規化は `core::fs::ProjectPath` が吸収）。
- 対象は **ファイル名を含むフルパス**（例: `./assets/shots/ch010/ch010_001_bg_forest_v03.psd`）。
- `applies_to` が配列のときは、いずれかの glob にマッチすればルールの適用対象。

### 3.2 メタ文字

`globset` crate 互換。

| 記号 | 意味 |
| --- | --- |
| `*` | `/` を除く任意文字列（0 文字以上） |
| `**` | 任意深さのディレクトリ。セグメント単位でしか使えない（`/**/`、先頭 `**/`、末尾 `/**`） |
| `?` | `/` を除く任意 1 文字 |
| `[abc]` | 文字クラス |
| `[!abc]` | 否定文字クラス |
| `\` | エスケープ |

regex 非対応。brace expansion（`./{a,b}/**`）非対応 — 複数 dir を当てたい場合は `applies_to` を配列にする。

### 3.3 具体例

```toml
applies_to = "./assets/**"
applies_to = "./assets/shots/**/*.psd"
applies_to = ["./assets/shots/**/*.psd", "./assets/shots/**/*.tif"]
applies_to = "./assets/shots/ch???/**"
```

---

## 4. テンプレート規則（`kind = "template"`）

### 4.1 役割

ファイル名（basename）を **厳密なパターン** で規定する。テンプレート winner が basename に合致しない場合は違反となり、suggested_names が生成される。

テンプレートは basename のみを対象とし、親ディレクトリ名には触れない（ディレクトリリネームは v1 範囲外）。

### 4.2 必須 / オプションフィールド

共通フィールド（§2）に加え:

| フィールド | 型 | 必須 | 意味 |
| --- | --- | --- | --- |
| `template` | string (non-empty) | ✅ | basename 全体を規定するパターン。空文字列は load error |

### 4.3 プレースホルダー

#### 4.3.1 組込みプレースホルダー

| プレースホルダー | 型 | 意味 |
| --- | --- | --- |
| `{prefix}` | 自由部分（open-ended） | 先頭の任意文字列スロット |
| `{desc}` | 自由部分（open-ended） | 自由記述スロット（AI 命名支援の対象） |
| `{seq}` | integer | 連番（§6） |
| `{version}` | integer | バージョン番号 |
| `{ext}` | string | 拡張子（先頭ドットなし、小文字正規化。複合拡張子は §4.8） |

#### 4.3.2 参照プレースホルダー

| プレースホルダー | 参照元 | 例 |
| --- | --- | --- |
| `{field:<name>}` | `.meta` の `[custom].<name>` | `{field:scene:03d}` → `custom.scene = 20` なら `"020"` |
| `{date:<fmt>}` | `.meta` の `[file].created_at` | `{date:YYYYMMDD}` → `"20260420"` |

参照プレースホルダーは **評価時に `.meta` を読み、整形後の literal 文字列に展開して比較する**（§4.6 参照）。generic regex としては扱わない。

**日付フォーマットトークン**:

| トークン | 意味 |
| --- | --- |
| `YYYY` | 西暦 4 桁 |
| `YY` | 西暦下 2 桁 |
| `MM` | 月 2 桁 0 埋め |
| `DD` | 日 2 桁 0 埋め |
| `HH` | 時 2 桁 0 埋め（24 時制） |
| `mm` | 分 2 桁 0 埋め |
| `ss` | 秒 2 桁 0 埋め |

区切り文字はリテラル（`{date:YYYY-MM-DD}` も可）。時刻は評価時の UTC で整形する（v1 は TZ 指定を持たない）。

**`{today:<fmt>}` は v1 では提供しない**（lint が実行時刻に依存すると golden test が安定しない / 同じファイルが翌日違反化するため）。「今日の日付を付けて rename したい」ユースケースは v1.x で rename 側の suggest 機構として別途検討。

#### 4.3.3 リテラルエスケープ

- `{{` → リテラル `{`
- `}}` → リテラル `}`
- それ以外の `{` `}` は必ずプレースホルダー境界としてパースされる。

### 4.4 フォーマット指定子

プレースホルダー名の後に `:<spec>` を付与。`:<spec1>:<spec2>:...` と連結可能。左から右へ順に適用する。

#### 4.4.1 数値指定子

対象: `{seq}` / `{version}`、および `{field:<name>}` が `.meta.[custom].<name>` で整数型として取得できた場合。

| 指定子 | 意味 | 例 |
| --- | --- | --- |
| `:0Nd` | N 桁 0 埋め | `{seq:03d}` → `042` |
| `:d` | 0 埋めなし | `{version:d}` → `3` |

#### 4.4.2 文字列指定子

対象: `{prefix}` / `{desc}` / `{ext}`、および `{field:<name>}` が文字列型として取得できた場合。

| 指定子 | 意味 | 例 |
| --- | --- | --- |
| `:snake` | snake_case | `Forest Night` → `forest_night` |
| `:kebab` | kebab-case | `Forest Night` → `forest-night` |
| `:camel` | camelCase | `Forest Night` → `forestNight` |
| `:pascal` | PascalCase | `Forest Night` → `ForestNight` |
| `:lower` | 小文字化 | `FOO` → `foo` |
| `:upper` | 大文字化 | `foo` → `FOO` |
| `:slug` | URL-safe スラグ（空白・記号を `-` に、連続 `-` 畳み込み、両端トリム、最終的に小文字化） | `Ch 10 / Sc 20` → `ch-10-sc-20` |

#### 4.4.3 連結の評価順

左から右へ、前段の出力を次段の入力にパイプ。例:

```
{desc:snake:lower}  "Forest Night"
  → snake: "forest_night"
  → lower: "forest_night"  (no-op)

{prefix:upper:slug}  "Ch 10"
  → upper: "CH 10"
  → slug:  "ch-10"         (slug は最終的に小文字化を内包)
```

**重複する指定子はロード時エラー**（例: `:snake:snake`）。数値指定子と文字列指定子の混在もエラー（例: `:snake:03d`）。

### 4.5 名前空間（`@` 構文）

- `{seq@<key>}` — `.meta.[custom].<key>` の値単位で連番を独立採番
- `<key>` は 1 層のキー名のみ（ドット区切りのネストは v1 では未対応）
- 名前空間は `{seq}` 系でのみ使える（`{prefix@...}` 等はロード時エラー）

詳細な採番アルゴリズムは §6。

### 4.6 マッチの意味論

テンプレート文字列はロード時に内部 regex へコンパイルされる。各プレースホルダーは以下の扱い:

| 種類 | マッチ時の処理 |
| --- | --- |
| `{prefix}` / `{desc}`（open-ended、spec なし） | `[^/]+` 相当（ただし §4.7 の 1 個制限あり） |
| `{prefix:snake}` 等（casing spec 付き） | 対応する casing を満たす文字集合（`[a-z0-9]+(_[a-z0-9]+)*` 等） |
| `{prefix:slug}` | `[a-z0-9]+(-[a-z0-9]+)*` |
| `{seq:03d}` | `\d{3}`、capture 後に数値として検査 |
| `{seq:d}` | `\d+` |
| `{version:02d}` / `{version:d}` | 同上 |
| `{ext}` | `§4.8` に従い最長一致で決定後、小文字比較 |
| `{field:<name>[:spec]}` | 評価時に `.meta.[custom].<name>` を読み、spec に従って整形し、**literal 文字列として比較**する |
| `{date:<fmt>}` | 評価時に `.meta.[file].created_at` を読み、フォーマットトークンで整形し、**literal 文字列として比較**する |

参照プレースホルダーの値取得に失敗したときは `template_mismatch` ではなく **`evaluation_error`** として扱う（§8.3 の severity にかかわらず必ず lint レポートに出力、lint の exit code には strict と同じ重みで影響）。失敗条件:
- `.meta.[custom].<name>` が欠落
- 型が spec と不整合（文字列 spec に int / 数値 spec に string 等）
- `{date:}` に対応する `[file].created_at` が欠落 / パース不能
- `{seq}` が spec の桁数に収まらない数値

### 4.7 open-ended placeholder の個数制限

spec なしの `{prefix}` / `{desc}`、および文字列 spec を持たない `{field:<name>}` は **1 つの template につき最大 1 個**。2 個以上ある場合はロード時エラー。

理由: deterministic な regex capture を保証するため。`{prefix}_{desc}` のような例は open-ended が 2 つで曖昧になる。代替として片方に casing spec を付ける（`{prefix}_{desc:snake}` → prefix だけ open-ended）。

### 4.8 `{ext}` と複合拡張子

- ファイル名の末尾から、後述 builtin セットと `.progest/schema.toml` の `[extension_compounds]` に登録されたトークンを **最長一致** で 1 つだけ拾って `{ext}` として扱う。
- 比較は小文字正規化後に行う。
- v1 の builtin: `tar.gz`, `blend1`（それぞれ先頭ドットは含まない文字列として保持）。
- プロジェクトで追加したい場合は `.progest/schema.toml`:

```toml
[extension_compounds]
items = ["tar.gz", "blend1", "psd.bak"]  # 項目はプロジェクト側で自由に定義
```

- `[extension_compounds]` は **accepts のカテゴリエイリアス（`:image` 等）とは別テーブル**（概念が違うため名前空間を分離）。

---

## 5. 制約規則（`kind = "constraint"`）

### 5.1 役割

ファイル名を厳密に規定せず、文字集合・ケース・長さ等の制約を課す。同じディレクトリに複数 hit した場合は **§8.1 の通り AND 合成** で評価されるため、目的別に細かくルールを切って良い。

### 5.2 フィールド

| フィールド | 型 | 既定 | 意味 |
| --- | --- | --- | --- |
| `charset` | `"ascii"` \| `"utf8"` \| `"no_cjk"` | `"utf8"` | 許容文字集合（§5.3） |
| `casing` | `"any"` \| `"snake"` \| `"kebab"` \| `"camel"` \| `"pascal"` | `"any"` | basename（拡張子除く）の書き味（§5.4） |
| `forbidden_chars` | `string[]` | `[]` | 明示的に禁止する文字。各要素は 1 codepoint |
| `forbidden_patterns` | `string[]` | `[]` | 禁止 regex（§5.5） |
| `reserved_words` | `string[]` | `[]` | 予約語（§5.6） |
| `max_length` | integer | `255` | basename の grapheme cluster 数上限（拡張子含む、§5.7） |
| `min_length` | integer | `1` | 同上、下限 |
| `required_prefix` | string | `""` | basename 先頭に必須の literal（拡張子除く部分の先頭） |
| `required_suffix` | string | `""` | 拡張子を除いた basename の末尾に必須の literal |

**フィールドはすべて AND 評価**（全制約を満たさないと違反）。

### 5.3 `charset` の定義

- `ascii`: 印字可能 ASCII `[\x20-\x7E]` のみ
- `utf8`: UTF-8 として valid であれば OK（既定）
- `no_cjk`: UTF-8 かつ、以下の Unicode ブロックを含まない
  - CJK Unified Ideographs (`U+4E00..U+9FFF`)
  - CJK Unified Ideographs Extension A (`U+3400..U+4DBF`)
  - CJK Unified Ideographs Extension B〜（`U+20000..U+2FA1F`）
  - Hiragana (`U+3040..U+309F`)
  - Katakana (`U+30A0..U+30FF`)
  - Katakana Phonetic Extensions (`U+31F0..U+31FF`)
  - Hangul Syllables (`U+AC00..U+D7AF`)
  - Hangul Jamo (`U+1100..U+11FF`, `U+A960..U+A97F`, `U+D7B0..U+D7FF`)
  - CJK Symbols and Punctuation (`U+3000..U+303F`)
  - Halfwidth and Fullwidth Forms (`U+FF00..U+FFEF`)
  - 絵文字・拡張記号（Emoji blocks, Mathematical Symbols 等）は許容

実装は `unicode-ucd` 相当の静的表で判定。

### 5.4 `casing` の定義

basename から拡張子を除いた部分に対して、以下の regex で full match 判定:

| 値 | 許容 regex |
| --- | --- |
| `any` | 制限なし |
| `snake` | `^[a-z0-9]+(_[a-z0-9]+)*$` |
| `kebab` | `^[a-z0-9]+(-[a-z0-9]+)*$` |
| `camel` | `^[a-z][a-zA-Z0-9]*$` |
| `pascal` | `^[A-Z][a-zA-Z0-9]*$` |

**casing と charset の関係**: casing が `any` 以外なら casing 側が charset より狭い範囲を要求するので、両方成り立つことが求められる。矛盾して両方満たせないルール設定（`charset = "ascii"` + `casing = "pascal"` + `forbidden_patterns = ["^[A-Z]"]` 等）はロード時エラー扱いはせず、評価時に全ファイル違反として出す（ルール設計のバグはユーザー側の責務）。

### 5.5 `forbidden_patterns`

- Rust `regex` crate の構文。`(?i)` 等のフラグは有効。lookaround / backreference は **非対応**（crate が非サポート）。
- Unicode モード ON がデフォルト。
- anchor（`^$`）は basename の **拡張子を除いた部分** に対して効く。
- コンパイル失敗時はロード時エラー。

### 5.6 `reserved_words`

- 比較対象: basename から拡張子を除いた部分
- トークン分割: basename を regex `[_\-. ]+` で split（セパレータが連続しても空トークンは出さない）
- 各トークンと `reserved_words` の各要素を **大小文字非感知で完全一致**。いずれかが一致したら違反

例: `reserved_words = ["final", "copy"]` + basename `my_shot_final_v02` → `["my", "shot", "final", "v02"]` に split、`"final"` が一致 → 違反。

### 5.7 `max_length` / `min_length`

- 数え方: NFC 正規化後の Unicode grapheme cluster 数（`unicode-segmentation` crate の `graphemes(true)` を使用）
- 対象: basename 全体（拡張子ドット・拡張子本体を含む）

---

## 6. 連番（`{seq}`）の採番

### 6.1 スコープ

- 既定: **ディレクトリローカル**（`{seq}` を含む template が適用されるそのディレクトリ内での集計）
- 名前空間: `{seq@<key>}` で `.meta.[custom].<key>` の値単位に独立カウント

### 6.2 既存解析

- ファイル評価時、同ディレクトリの兄弟ファイル名から `{seq}` プレースホルダー位置の数値を抽出して既存集合を得る
- suggested_names 生成時は「既存集合の最大値 + 1」を次の番号として提案
- 欠番は **詰めない**（`005` → `007` → 次は `008`）— これは v1 固定挙動。`pack_gaps = true` による詰めモードは v1.x 以降

### 6.3 衝突

- 複数人が同時に同じ番号を取った場合は reconcile 時に検出
- UI は「両採用リネーム提案」を提示（実装は `core::rename`、本 DSL の範囲外）

---

## 7. スコープと継承

### 7.1 ソースの階層

1. `<dir>/.dirmeta.toml` の `[[rules]]`（own）
2. 祖先 dir の `.dirmeta.toml` の `[[rules]]`（inherited、最近接祖先が優先）
3. `.progest/rules.toml`（project-wide）

上位（own に近い側）が優先する CSS カスケード式。

### 7.2 Override（ルール単位の full replace）

- 子レイヤで **同一 `id`** の `[[rules]]` を再定義すると、親レイヤの同 id ルールを **丸ごと置換**。フィールド単位の部分マージはしない。
- `kind` が親と同じ場合は `override = true` の明示は **任意**（推奨だが必須ではない）。parser は `override` 無しで子が親を置換するケースを stderr warning として報告する。
- `kind` を親と違うものに変える置換は **`override = true` を必須**（意図しない kind 置換を防ぐため）。
- 子で `id` を変えて別ルールを追加するのは通常追加（置換ではない）。

### 7.3 Applies-to 解決

ファイル `F` を評価する時:

1. `F` の親ディレクトリ鎖を root まで辿り、全レイヤから `[[rules]]` を収集
2. 同一 `id` の重複は §7.2 で解決し、残った集合を候補とする
3. 各候補ルールの `applies_to` を `F` のフルパスに対して match
4. match したルールのみ残し、§8 の評価フローへ

### 7.4 Specificity（同一 kind 内で複数ルールが hit した時）

同一 `kind` 内での **template winner 選定** に使う。constraint では全候補を AND 合成するので specificity は使わない。

計算規則（上から順に比較、tie なら次段へ）:

1. **リテラル segment 数**: `applies_to`（配列のときは match したパターン）の `/` 区切りセグメントのうち、メタ文字 `* ** ? [ ] \` を含まないセグメントの数
2. **リテラル文字数**: 上記セグメントの文字数合計（メタ文字除く literal 長）
3. **階層ソース**: own > inherited（近 → 遠）> project-wide
4. **rule_id 辞書順**: 安定ソートのためのフォールバック

例:
- `./assets/shots/ch010/**` vs `./assets/shots/**` vs `./assets/**` → literal segments で勝敗
- `./assets/shots/**/*.psd` と `./assets/shots/**/*.tif` は applies_to が互いに素なので同一ファイルでの tie は発生しない
- 完全同一 glob が 2 ルール出た場合 → own > inherited でまず決まり、それも同じなら rule_id 辞書順

### 7.5 Template と Constraint の関係

Template winner と Constraint の全候補は **独立に評価**し、違反はマージして報告する。§8 に続く。

---

## 8. 評価フロー

### 8.1 ファイル単位の評価ステップ

1. **収集**: §7.3 の通り候補 `[[rules]]` を集める（override 解決済み）
2. **Template winner 選定**: template kind の候補を §7.4 の specificity で並べ、**最上位 1 本を winner** とする
3. **Template 評価**: winner の template と basename を match 判定
   - 一致 → テンプレート層は違反なし
   - 不一致 → 違反として記録（`kind = "template"`、suggested_names を生成）
   - **winner が不一致でも specificity 次点の template にはフォールバックしない**（trace が安定する / 違反の根拠が一意になる）
4. **Constraint 評価**: constraint kind の候補すべてを **AND 合成**で評価。各ルールが独立に判定され、違反したルールごとに 1 エントリの Violation を生成（`kind = "constraint"`）
5. **マージ**: テンプレート層の Violation + 制約層の Violation を単一の `Vec<Violation>` として返す

**Template も Constraint も該当ルール無し** → naming 評価 skip（違反 0）。

### 8.2 Mode の解釈

| Mode | lint 出力 | rename apply | 違反の exit 影響 |
| --- | --- | --- | --- |
| `strict` | 違反をエラーとして出力 | 新規作成・rename を **拒否** | あれば exit 1 |
| `warn` | 違反を警告として出力 | 許可（UI バッジ表示） | exit への影響なし |
| `hint` | lint レポートには出さない | UI で候補提示のみ | なし |
| `off` | 評価しない | 影響なし | なし |

CLI exit code:
- `progest lint` は naming / placement いずれかで **`strict` 違反が 1 件でもあれば exit 1**、それ以外は exit 0
- `evaluation_error`（§4.6）は strict と同等の重みで exit 1 を引く
- `--format json` は exit 0/1 に関わらず JSON を stdout に流す
- `--explain` は trace を拡張出力（§9）

### 8.3 違反レポートのスキーマ

既存 `violations` テーブル（[IMPLEMENTATION_PLAN.md §4](./IMPLEMENTATION_PLAN.md)）への insert 形:

```rust
enum Category  { Naming, Placement }        // placement は別エンジン
enum RuleKind  { Template, Constraint }
enum Severity  { Strict, Warn, Hint, EvaluationError }

struct Violation {
    file_id: FileId,
    path: ProjectPath,
    rule_id: String,
    category: Category,             // "naming" — placement は core::accepts 側
    kind: RuleKind,                 // template | constraint
    severity: Severity,
    reason: String,                 // 人間可読（例: "casing expected snake, got PascalCase"）
    trace: Vec<RuleHit>,            // §9
    suggested_names: Vec<String>,   // 最大 3 件。template 不一致時が主用途
    detected_at: Iso8601,
}
```

suggested_names 生成:
- テンプレート不一致時: `{desc}` を AI 提案 or 既存名からスラグ化で埋めた候補を最大 3 件
- 制約違反時: 違反箇所の最小修正を 1 件まで（charset fix / casing fix / forbidden_char 置換）

---

## 9. rule_id trace

### 9.1 目的

「なぜこの違反が出たか / なぜこのルールが勝ったか」を user に説明する。`progest lint --explain` / GUI の違反詳細で表示される。

### 9.2 RuleHit 形式

```rust
enum RuleSource { Own, Inherited { distance: u16 }, ProjectWide }

enum Decision {
    Winner,          // template の specificity 勝者、または constraint の AND 合成参加者
    Shadowed,        // template の specificity 次点以下（候補には残ったが評価されず）
    NotApplicable,   // applies_to で弾かれた（--explain 時のみ出力）
}

struct RuleHit {
    rule_id: String,
    kind: RuleKind,
    source: RuleSource,
    decision: Decision,
    specificity_score: (u32, u32),  // (literal_segments, literal_chars)
    explanation: String,            // 例: "winner by literal-segment count (3 vs 2)"
}
```

### 9.3 メモリ予算

M2_HANDOFF §3.2 の懸念（10 万ファイルで trace 全件保持が OOM）への方針:

- lint 実行時はデフォルトで **違反のあるファイルのみ `trace: Vec<RuleHit>` を保持**。違反ゼロのファイルは winner の rule_id だけ覚える。
- `--explain` 指定時のみ、非違反ファイルでも trace を全件保持する。
- `--explain` レベル区分（verbose 等）は v1 では提供しない。必要なら v1.x で検討。

---

## 10. Worked examples

本セクションの各 case は `crates/progest-core/tests/rules_golden/` の fixture と 1:1 で対応する想定（golden は YAML 形式、§11 参照）。

### 10.1 Shot assets template — pass / fail ペア

`.progest/rules.toml`:

```toml
schema_version = 1

[[rules]]
id = "shot-assets-v1"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
mode = "warn"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"

[[rules]]
id = "shot-charset"
kind = "constraint"
applies_to = "./assets/shots/**"
mode = "strict"
charset = "ascii"
forbidden_chars = [" ", "　"]
```

#### Case A — pass

```
path:    ./assets/shots/ch010/ch010_001_bg_forest_v03.psd
custom:  {}
expect:
  template winner: shot-assets-v1 → match ✓
  constraints:
    shot-charset: ascii / no forbidden char → pass
  violations: []
```

#### Case B — template violation

```
path:    ./assets/shots/ch010/ch010_bg_forest_v03.psd
custom:  {}
expect:
  template winner: shot-assets-v1 → match ✗
    reason: missing `{seq:03d}` segment before `{desc:snake}`
  constraints:
    shot-charset: pass
  violations:
    - rule_id: shot-assets-v1
      kind: template
      severity: warn
      reason: "missing seq segment"
      suggested_names:
        - ch010_001_bg_forest_v03.psd
```

#### Case C — constraint violation (AND 合成)

```
path:    ./assets/shots/ch010/ch010 001 bg forest v03.psd
custom:  {}
expect:
  template winner: shot-assets-v1 → match ✗（空白で snake にならない）
  constraints:
    shot-charset:
      charset=ascii ✓
      forbidden_chars=[" ", "　"] → violation (basename contains " ")
  violations:
    - rule_id: shot-assets-v1 (template, warn)
    - rule_id: shot-charset (constraint, strict)
  exit_code: 1   # shot-charset が strict 違反
```

### 10.2 Namespace seq with field placeholder

```toml
[[rules]]
id = "scene-seq"
kind = "template"
applies_to = "./assets/scenes/**/*.tif"
template = "sc{field:scene:03d}_{seq@scene:03d}_{desc:slug}.{ext}"
```

```
path:    ./assets/scenes/ch010/sc020_007_forest-night.tif
custom:  { scene = 20 }
expect:
  template winner: scene-seq → match ✓
    - field:scene = 20 → "020" literal と "020" を比較 → pass
    - seq@scene では scene=20 のファイル群内で独立採番（兄弟の最大値参照）
  violations: []
```

反例:

```
path:    ./assets/scenes/ch010/sc999_007_forest-night.tif
custom:  { scene = 20 }
expect:
  template winner: scene-seq → match ✗
    reason: field:scene expanded to "020" but path has "999"
  violations:
    - rule_id: scene-seq (template, warn)
      reason: "literal mismatch at {field:scene}"
```

### 10.3 Constraint の AND 合成

```toml
# .progest/rules.toml
schema_version = 1

[[rules]]
id = "ascii-only"
kind = "constraint"
applies_to = "./**"
charset = "ascii"

[[rules]]
id = "length-cap"
kind = "constraint"
applies_to = "./**"
max_length = 40
```

```
path:    ./assets/shots/ch010/very_long_basename_that_exceeds_forty_chars.psd  (44 graphemes)
expect:
  template: 該当なし
  constraints:
    ascii-only: pass
    length-cap: violation (44 > 40)
  violations:
    - rule_id: length-cap (constraint, warn)
```

現行仕様（勝者 1 本）なら片方しか評価されないが、AND 合成なので両方独立に評価される。

### 10.4 Override（full replace）

`.progest/rules.toml`:
```toml
[[rules]]
id = "project-default"
kind = "constraint"
applies_to = "./**"
casing = "snake"
```

`./references/.dirmeta.toml`（この dir 相対の `applies_to`、loader がプロジェクト相対 `./references/**` に正規化）:
```toml
schema_version = 1

[[rules]]
id = "project-default"       # same id, same kind → full replace
applies_to = "./**"          # ← この dir（= references）から見た相対
kind = "constraint"
casing = "any"
charset = "utf8"
# override は同 kind なので省略可。書けば明示的
```

```
path:    ./references/reference_doc.pdf
expect:
  constraints:
    project-default (dirmeta 版): casing=any, charset=utf8 → pass
    ※ project-wide の project-default は full replace で消えている
  violations: []

path:    ./assets/shots/ch010/ForestNight.psd
expect:
  constraints:
    project-default (rules.toml 版): casing=snake → violation
  violations:
    - rule_id: project-default (constraint, warn)
```

### 10.5 Specificity（Template winner 選定）

```toml
[[rules]]
id = "general"
kind = "template"
applies_to = "./assets/**"
template = "{desc:snake}_v{version:02d}.{ext}"

[[rules]]
id = "shots-specific"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"
```

```
path:    ./assets/shots/ch010/ch010_001_bg_forest_v03.psd
expect:
  template candidates:
    general:        specificity=(1, 6)   # 1 literal segment "assets"
    shots-specific: specificity=(2, 12)  # 2 literal segments "assets","shots"
  winner: shots-specific → match ✓
  trace:
    - { rule_id: shots-specific, decision: winner }
    - { rule_id: general,        decision: shadowed }
  violations: []
```

### 10.6 Applies-to 配列化

```toml
[[rules]]
id = "no-japanese"
kind = "constraint"
applies_to = ["./assets/**", "./references/**"]
mode = "warn"
charset = "no_cjk"
```

```
path A:  ./assets/shots/ch010/forest_night.psd      → applies (assets 配下) → evaluated
path B:  ./references/日本語メモ.pdf                 → applies (references 配下) → violation (no_cjk)
path C:  ./docs/memo.md                              → does not apply → not evaluated
```

---

## 11. 実装メモ

- テンプレート文字列の regex コンパイルはロード時 1 回、以後キャッシュ
- specificity score は glob の静的解析で求まるのでロード時計算
- `{field:<name>}` / `{date:}` の値取得は評価時に `.meta` ロードと join
- 全 DSL 仕様は **golden test suite** として `crates/progest-core/tests/rules_golden/` に、**入力 = `rules.toml` + fixture paths + per-file `custom` / `[file].created_at`**、**期待 = violations YAML** の形で固定する。YAML フォーマットは [IMPLEMENTATION_PLAN.md §5 M2](./IMPLEMENTATION_PLAN.md) の方針と一致させる
- 仕様変更時は golden を更新し、その PR で本 docs と同期する

---

## 12. 既知の v1.x 候補

本 docs で一旦 v1 スコープ外とした項目:

1. `{today:<fmt>}` — lint 実行時刻依存で不安定。rename suggest 側の拡張として再検討
2. rules ファイルの `include` / `[[rules]] extends`
3. `pack_gaps = true`（seq 欠番詰めモード）
4. `--explain=verbose` 等のレベル区分
5. brace expansion の `applies_to` 構文（`./{a,b}/**`）

---

## 13. 変更履歴

- 2026-04-23: 初版（PR `docs/m2-naming-rules-dsl`）。Round 1/2/3 で合意した DSL スコープを反映。
