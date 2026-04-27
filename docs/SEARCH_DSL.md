# Search DSL 仕様

Progest の検索エンジン（`core::search`）が解釈する DSL の正規仕様。parser / planner / executor 実装はこの文書と bit-for-bit 一致させる。

最終更新: 2026-04-25
対象: v1.0 MVP（macOS 先行）

関連:
- [REQUIREMENTS.md §3.7 検索](./REQUIREMENTS.md) — ハイレベル意思決定（本文書の上位）
- [REQUIREMENTS.md §3.8 ビュー](./REQUIREMENTS.md) — 保存ビュー / コマンドパレットの位置づけ
- [IMPLEMENTATION_PLAN.md §5 M3](./IMPLEMENTATION_PLAN.md) — モジュール完了条件
- [M3_HANDOFF.md](./M3_HANDOFF.md) — parser 着手前の kickoff メモ
- [NAMING_RULES_DSL.md](./NAMING_RULES_DSL.md) — 命名規則 DSL（本仕様書の参照モデル）

---

## 0. 位置付け

- 検索 DSL は GitHub / Linear 風の `key:value` 句 + 自由テキスト + ブール演算子からなる。1 行で書ける文字列。
- 検索結果は `core::index`（SQLite + FTS5）に対する単一 SELECT に reduce される。
- 自由テキストは FTS5 仮想テーブル、`key:value` は通常テーブル + index に当てる。両者は INNER JOIN で合成する。
- DSL が扱うのは「インデックス可能な属性に対する検索」。FS 状態の動的判定（例: 「未保存のロック中ファイル」）は対象外。
- 検索結果はキャッシュ可能。同一 query + 同一 index version → 同一結果（決定的）。
- 規則評価（lint）と異なり、検索 DSL は副作用なし（read-only）。
- v1 スコープ外（v1.x 以降）: `sort:` 句、`limit:` 句、ファセット集計、近接検索（`"foo bar"~3`）、編集距離、リレーション辿り（`parent:`/`children:`）、保存ビューの `extends`、結果スコアでの ranking、lindera 形態素解析。

---

## 1. ファイル配置

### 1.1 関連ファイル

| パス | git | 用途 |
| --- | --- | --- |
| `.progest/views.toml` | ✅ 共有 | プロジェクト共有の保存済みビュー定義 |
| `.progest/local/history.json` | ❌ ローカル | 個人の検索履歴（最新 N 件、retention 100 固定） |
| `.progest/index.db`（FTS5 含む） | ❌ ローカル | クエリ実行対象。`schema.toml` のカスタムフィールドを反映 |
| `.progest/schema.toml` | ✅ 共有 | カスタムフィールドの定義（§6 参照） |

### 1.2 `views.toml` 最小スケルトン

```toml
# .progest/views.toml
schema_version = 1

[[views]]
id = "violations"
name = "Violations"
query = "is:violation"
description = "全ての lint 違反"

[[views]]
id = "psd-shots"
name = "PSD Shot files"
query = 'type:psd path:"./assets/shots/**"'
group_by = "scene"   # §11.3 参照（v1 サポート）

[[views]]
id = "stale"
name = "Stale assets"
query = "type:psd updated:..2025-10-01"
sort = "updated:asc"  # v1 ではロード時 warning（§11.4 参照）
```

### 1.3 `history.json` フォーマット

```json
{
  "schema_version": 1,
  "entries": [
    { "ts": "2026-04-25T10:30:00Z", "query": "is:violation", "result_count": 12 },
    { "ts": "2026-04-25T10:31:42Z", "query": "tag:wip type:psd", "result_count": 7 }
  ]
}
```

- `entries` は降順（新しいものが先頭）、retention は **100 件**固定。M2 `core::history` とは別管理（用途が違う）。
- `result_count` は実行時の結果件数スナップショット。再実行時に再計算される。

### 1.4 スキーマバージョン / forward-compat

- `views.toml` / `history.json` の `schema_version` は **必須**、v1 は `1` 固定。
- 未知フィールドの扱いは [`NAMING_RULES_DSL.md §1.3`](./NAMING_RULES_DSL.md) と同じ:
  - 同バージョン: typo warning（stderr / `--format json` の `warnings`）
  - 新しい: `extra: Table` に保持、warning なし
  - 古い: 未定義（migration を将来提供）

---

## 2. 文法

### 2.1 字句（lexical）

| トークン | 構文 | 例 |
| --- | --- | --- |
| `BAREWORD` | `[A-Za-z0-9_./*?\-+]+`、ただし先頭 `-` は NOT として字句解析（§5.1） | `assets`、`shot_v01`、`*.psd` |
| `STRING` | `"..."`、`\"` `\\` をエスケープ。改行を含めない | `"foo bar"`、`"path with \"quote\""` |
| `KEY` | `[a-z][a-z0-9_]{0,31}`、`:` が直後に必ず続く | `tag:`、`type:`、`scene:` |
| `LPAREN` / `RPAREN` | `(` / `)`、グルーピング | |
| `OR` | キーワード `OR`（大文字、両側空白）。`or` `Or` は ungrouped term として扱う | `tag:a OR tag:b` |
| `WS` | スペース・タブ。複数連続は 1 個と等価。改行は禁止（1 行 query） | |

エスケープが必要な特殊文字: `"` `\`。それ以外（`(` `)` `:` `-` 等）は `STRING` で囲めば literal。

### 2.2 構文（EBNF）

```
query        = expr ;
expr         = or_expr ;
or_expr      = and_expr ( "OR" and_expr )* ;
and_expr     = term ( WS term )* ;          /* 暗黙 AND（空白区切り） */
term         = neg_term | atom ;
neg_term     = "-" atom ;
atom         = group | clause | freetext ;
group        = "(" expr ")" ;
clause       = KEY value ;
value        = BAREWORD | STRING | range ;
range        = value? ".." value? ;          /* 半開可 */
freetext     = BAREWORD | STRING ;
```

- **暗黙 AND**: 空白区切りの term は AND（GitHub 同様）。`tag:foo type:psd` は両方を満たすファイル。
- **OR は明示**: `tag:a OR tag:b`。優先度は AND > OR、`(...)` でグループ。
- **NOT は単項前置**: `-tag:foo`、`-is:violation`、`-(tag:a tag:b)`。`--` の二重否定はパースエラー（typo 抑止）。

### 2.3 評価優先度

```
NOT  >  AND (暗黙)  >  OR
```

`a OR b c` は `a OR (b AND c)`。意図を明確にしたい時はグループを推奨。

### 2.4 空クエリ

- 空文字列 / 空白のみ: 全件マッチ（`SELECT * FROM files`）。CLI は `--allow-empty` フラグなしでは exit 2 で拒否（暴発防止）。UI は空クエリで現在ディレクトリの全件を表示する用途を許容。

---

## 3. 自由テキスト

### 3.1 マッチ対象

自由テキスト term（`KEY` を持たない単独 `BAREWORD` / `STRING`）は以下の 2 列に対する FTS5 MATCH。

| 列 | 由来 |
| --- | --- |
| `name` | ファイル basename（拡張子含む、最後のドット以前） |
| `notes` | `.meta` の `notes` フィールド |

OR 結合: `tokenized(name) MATCH q OR tokenized(notes) MATCH q`。複数自由テキストは AND（暗黙）。

### 3.2 トークナイザ

- v1: FTS5 `tokenize='trigram'`（CJK 含む）。3-gram で indexing、クエリ側も 3-gram 化して MATCH。
- 短語（< 3 文字）の扱い: literal 完全一致でフォールバック（FTS5 は本来 trigram 向け、短語のヒットを保証する）。
- v1.x で lindera による形態素解析オプションを追加予定（schema_version は据え置き、`tokenize` の選択を `schema.toml` で持たせる）。

### 3.3 制限

- ワイルドカード（FTS5 の `foo*` prefix match）は v1 では未提供。`name:foo*` を使うか、3-gram 完全一致に倒す。
- 近接検索 `"foo bar"~N` は v1 未提供（v1.x 候補）。
- スコアでのランキング非対応。結果順は §10.4 のとおり決定的にソートする。

### 3.4 引用

- `"`-quoted は **literal フレーズ**として trigram 連結マッチに使う。GitHub 風の片寄せ動作。
- 例: `"forest night"` は trigram 列 `for ore res est st_ ..._nig igh ght` をすべて満たす行を検索。

---

## 4. 予約キー

予約キーは parser がハードコードで認識する。`schema_version = 1` の v1 では以下のセットが正規。これ以外の `KEY:` はカスタムフィールド（§6）として扱う。

### 4.1 全体一覧

| キー | 値型 | 多重 | 否定 | 説明 |
| --- | --- | --- | --- | --- |
| `tag` | string | ✅ AND | `-tag:` | M2 `tag` テーブルへの EXISTS |
| `type` | extension（dot なし） | ✅ AND | `-type:` | basename 末尾セグメント、複合拡張子は §4.4 |
| `kind` | enum `asset\|directory\|derived` | ❌ | `-kind:` | `core::index` の `kind` 列 |
| `is` | enum `violation\|orphan\|duplicate\|misplaced` | ✅ AND | `-is:` | 派生フラグ（§4.5） |
| `name` | glob | ✅ AND | `-name:` | basename への glob（§4.6） |
| `path` | glob | ✅ AND | `-path:` | プロジェクトルート相対のフルパスへの glob |
| `created` | date / datetime / range | ❌ | `-created:` | `.meta.created_at` |
| `updated` | date / datetime / range | ❌ | `-updated:` | `.meta.updated_at` |

### 4.2 `tag:`

- 値は `[a-zA-Z0-9_-]+`（`schema.toml` の tag 制約に従う）。
- `tag:foo` = ファイルが tag `foo` を持つ。
- `tag:foo tag:bar` = 両方を持つ（INNER JOIN 相当、AND）。
- `-tag:foo` = `foo` を持たない。`-tag:foo -tag:bar` は両方を持たない。

### 4.3 `kind:`

```
kind:asset      # 通常のファイル（.meta あり）
kind:directory  # ディレクトリ
kind:derived    # サムネイル / プロキシ等の派生物（M4）
```

v1 では `derived` を生成するモジュールが M4 まで存在しないので、実質 `asset` / `directory` のみ意味を持つ。

### 4.4 `type:`

- 値は拡張子 1 つ（dot なし、`schema.toml` で正規化された小文字）。
- 複合拡張子は M2 `[extension_compounds]` の最長一致を踏襲（例: `.tar.gz` を 1 単位として扱う場合、`type:tar.gz`）。loader は `schema.toml` を参照する。
- **コンマ区切りリスト**（§4.10）と **`::alias`**（§4.11）をサポート。`type:psd,tif` で 2 種を OR、`type::image` で alias 展開。
- `type:psd type:tif` = AND ではなく **OR を強制したい場合は明示**: `type:psd,tif`（推奨）、または `type:psd OR type:tif`（同等）。同一ファイルは複数 type を持たないので AND は実質「結果 0 件」。
  - **parser warning**: 同一 `type:` の暗黙 AND（複数 `type:` を空白区切り）は実質 0 件になる。loader は `warning: type-and-multi` を出す（lint と同様、stderr / json `warnings`）。コンマリスト 1 個は同 warning 対象外（1 clause として数える）。

### 4.5 `is:`

| 値 | 意味 |
| --- | --- |
| `violation` | naming / placement / sequence のいずれかで `severity ∈ {strict, warn}` の違反を持つ |
| `orphan` | `.meta` ファイルだけ存在し、対応するアセットファイルがない |
| `duplicate` | 同一 `file_id` が複数 path に存在（M2 `core::identity`） |
| `misplaced` | placement 違反（accepts に対する不適合）を持つ。`is:violation` の subset |

`is:violation` は `severity ∈ {strict, warn}` のみ。`hint` / `off` は除外。

### 4.6 `name:` / `path:`

- 値は glob。`globset` crate 互換、メタ文字は [`NAMING_RULES_DSL.md §3.2`](./NAMING_RULES_DSL.md) と同じ（`*` `**` `?` `[abc]` `[!abc]` `\`）。
- `name:` は basename 全体に対するマッチ（`name:*.psd`、`name:ch???_*`）。
- `path:` はプロジェクトルート相対のフルパスに対するマッチ（`path:./assets/shots/**`、先頭の `./` は省略可）。
- 両者ともケース感度はファイルシステムに従う（macOS デフォルト APFS は CI、CS の場合は CS で評価）。
- 引用が必要な値（スペース・特殊文字）は `path:"./My Project/**"`。

### 4.7 `created:` / `updated:`

- 単一日付: `created:2026-04-25`
- 単一日時: `created:2026-04-25T12:30:00Z`（タイムゾーン付き ISO 8601）
- 範囲: `created:2026-01-01..2026-04-30`（両端 inclusive）
- 半開: `created:2026-01-01..` / `created:..2026-04-30`
- 解像度: 日付指定は UTC の `00:00:00.000` 〜 `23:59:59.999` として展開（範囲右端の場合）。
- パース失敗時はクエリ全体が parse error（exit 2）。
- 集合演算: `created:` 句は 1 クエリに 1 つだけ（複数あれば parse error）。範囲を組合せたい場合は v1.x で対応予定（候補: `created:2024..2025 -created:2024-06`）。

### 4.8 タイムゾーン

- 内部表現は UTC ISO 8601（`.meta` の `created_at` / `updated_at` は UTC で記録される M1 仕様）。
- クエリ側で TZ 指定（`+09:00` 等）は parser が受け入れて UTC 変換してから比較する。
- `today` / `now` 等の相対表現は v1 未対応（lint と同じく決定性優先）。

### 4.10 コンマ区切り値リスト

- 適用対象キー: `tag:` / `type:` / `kind:` / カスタム値全部（`scene:1,2,3` 等）。範囲・glob・enum を取る `name:` / `path:` / `is:` / `created:` / `updated:` は対象外（コンマがリテラルの一部になりうるため）。
- 意味論: コンマで区切られた token 群を **OR** として展開。`type:png,jpg` は `type:png OR type:jpg` と等価。
- `-` 否定との組合せ: `-type:png,jpg` は de Morgan 的に `NOT (type:png OR type:jpg)` ＝ `-type:png AND -type:jpg`。
- 引用値はリテラル: `tag:"a,b"` は「`a,b` という名前の tag を 1 つ」探す（コンマでの分割は走らない）。
- 空 token は `warning: empty_list_item` を出してスキップ（`type:psd,,jpg` は psd + jpg 2 個と扱う）。全 token が空・無効なら clause 全体が `AlwaysFalse`。
- カスタム文字列フィールドにコンマを含めたい場合は引用必須（`scene:"a,b"` でコンマ含む文字列マッチ）。

### 4.11 `::alias` 展開

- 構文: `<key>::<alias-name>`。`type::image`、`type::psd-family` 等。
- v1 では **`type:` のみ** alias 展開をサポート。`tag::group` のような他キーへの `::name` は `warning: unsupported_alias` を出してスキップ。
- 解決元: `core::accepts` の AliasCatalog（builtin + プロジェクトの `.progest/schema.toml [alias.<name>]`）。`ACCEPTS_ALIASES.md` 参照。例: `type::image` → png/jpg/psd/... に展開。
- 不明 alias は `warning: unknown_alias` を出して 0 値を寄与（clause が空になれば AlwaysFalse）。
- コンマリスト内に混在可: `type::image,raw` は image + raw alias の和集合、`type::image,svg` は image set ∪ {svg}。

---

## 5. ブール演算子

### 5.1 NOT (`-`)

- `-tag:foo` / `-is:violation` / `-(tag:a tag:b)`。
- 単項前置のみ。`a -tag:foo` は `a AND NOT tag:foo`。
- 自由テキストの否定 `-foo` は **`name`/`notes` のいずれにも `foo` がマッチしない**。
- `--tag:foo` は parse error（typo 抑止）。

### 5.2 AND (暗黙)

- 空白区切りの term は AND。`tag:foo type:psd` = `tag:foo AND type:psd`。
- 明示的な `AND` キーワードは **未対応**（parse error）。GitHub と同じく暗黙のみ。

### 5.3 OR

- `tag:foo OR tag:bar`。`OR` は大文字・前後空白必須。
- `tag:foo or tag:bar` の `or` は通常 BAREWORD（free text の "or" として扱う）。
- 結合性: 左結合。`a OR b OR c` = `(a OR b) OR c`。

### 5.4 グループ `( ... )`

- 任意のサブ式を括れる。`(tag:a OR tag:b) -is:violation`。
- 空グループ `()` は parse error。

### 5.5 演算子の優先度

```
NOT  >  AND  >  OR
```

例: `tag:wip OR tag:review type:psd` は `tag:wip OR (tag:review AND type:psd)`。意図を明示したいときは `(tag:wip OR tag:review) type:psd`。

---

## 6. カスタムフィールド

### 6.1 定義

- `schema.toml` の `[custom_fields.<name>]` セクションで定義（M2 `core::accepts` と同居）。
- v1 でサポートする型: `string` / `int` / `enum`。

```toml
# .progest/schema.toml
schema_version = 1

[custom_fields.scene]
type = "int"

[custom_fields.shot]
type = "int"

[custom_fields.status]
type = "enum"
values = ["wip", "review", "approved", "delivered"]
```

### 6.2 クエリ構文

| 型 | 構文 | 例 |
| --- | --- | --- |
| `string` | `<key>:<value>` または `<key>:"<value>"` | `status:approved` |
| `int` | `<key>:<int>` または範囲 `<key>:<lo>..<hi>` | `scene:10`、`shot:1..50` |
| `enum` | `<key>:<value>`（define された値以外は parse warning + 0 件） | `status:wip` |

### 6.3 未定義キー

- `schema.toml` に存在しないキーが query に出現したとき:
  - **parse 自体は通す**（parser は予約キーセット + custom_fields を loader 起動時に渡される）
  - planner で 0 件にショートサーキット
  - `warnings: ["unknown_key:foobar"]` を返す
- これによりタイポを早期検出しつつ、CLI は exit 0 で続行（lint の strict と異なり、search は read-only ゆえ非破壊）。

### 6.4 インデックス

- `core::index` に `custom_fields(file_id, key, value_text, value_int)` テーブルを追加（M2 で予約済み、M3 で実体化）。
- `value_text` / `value_int` のどちらかが NULL（型に応じて）。
- カラム index: `(key, value_int)` と `(key, value_text)` の 2 本。範囲クエリは int の方を使う。

---

## 7. 評価フロー

### 7.1 query plan

```
1. parse (str) -> AST
2. validate     (予約キー / カスタムフィールドの型一致を検査、warning 集約)
3. plan         (AST -> SQLite SELECT。FTS5 サブクエリ + WHERE 句)
4. execute      (SQLite 実行、結果を Vec<SearchHit>)
5. project      (Hit に必要な join を遅延適用 — name, kind, tag, severity 等)
```

### 7.2 SQL 生成戦略

- **FTS5 副選択**: 自由テキスト + free text の合成は `SELECT file_id FROM files_fts WHERE files_fts MATCH ?`。
- **WHERE 句**: 予約キー句は `EXISTS (...)` / 直接列比較。
- **NOT**: `WHERE NOT EXISTS` か、`AND ... NOT IN`。
- **OR**: SQL の `OR` に直接マップ。
- 計画は **immutable AST → SQL string + params** で純関数。同 AST → 同 SQL（ゴールデンテスト可能）。

### 7.3 性能契約

| データ規模 | 目標 |
| --- | --- |
| 10k ファイル / 100 タグ / 5 custom field | p95 50ms |
| 100k ファイル / 1k タグ / 10 custom field | p95 100ms |

ベンチは `tests/bench/search_smoke.rs` に M3 で追加。100k は v1 上限の参考値で、超えた場合の挙動は未保証（v1.x の課題）。

### 7.4 結果順序

- デフォルト: `path ASC`（決定的、セッション間で一貫）
- v1 では sort 句なし。views.toml `sort = ...` はロード時 warning + 黙視（§11.4）。
- 同 path 重複は理論上発生しないが、念のため `(path ASC, file_id ASC)` で tie-break。

---

## 8. 結果スキーマ

### 8.1 CLI `progest search` の `--format json`

```json
{
  "query": "tag:wip type:psd",
  "result_count": 12,
  "elapsed_ms": 23,
  "warnings": [],
  "hits": [
    {
      "file_id": "0193f4c3-...",
      "path": "./assets/shots/ch010/ch010_001_bg.psd",
      "name": "ch010_001_bg.psd",
      "kind": "asset",
      "type": "psd",
      "tags": ["wip"],
      "violations": { "naming": 0, "placement": 1, "sequence": 0 },
      "custom_fields": { "scene": 10, "shot": 1 }
    }
  ]
}
```

- `violations` は M2 `LintReport.summary` と同じ集計を file 単位で持つ。`is:violation` の判定はこの数を使う。
- `custom_fields` は `schema.toml` で定義済みのフィールドのみ含む（未定義の `.meta.[custom].xxx` は出力しない）。

### 8.2 CLI text 形式

```
ch010_001_bg.psd  tags:wip  scene:10  shot:1  ★placement
ch010_002_bg.psd  tags:wip  scene:10  shot:2
```

- 1 行 1 ヒット、列は `path` / `tags` / カスタムフィールド / `★` 記号で違反種別（M2 lint text 形式と統一）。

### 8.3 終了コード

- `0`: 成功（結果 0 件でも 0、`is:violation` でヒットしても 0）
- `1`: **未使用**（lint との対称性は崩す。search はクエリ実行であって検査ではない）
- `2`: parse error / unknown key（user error）
- `3`: 内部エラー（SQLite / FTS5 失敗）

REQUIREMENTS §3.9 の終了コードに準拠。

---

## 9. エラー

### 9.1 parse error の表示

```
$ progest search "tag:foo --bar"
error: unexpected '--' at column 9
  tag:foo --bar
          ^^
help: '-' は単項否定。'--' は parse error として扱われます（typo 抑止）
exit 2
```

- `--format json` でも同じ情報を JSON で返す:

```json
{
  "ok": false,
  "error": {
    "kind": "parse",
    "message": "unexpected '--' at column 9",
    "column": 9,
    "hint": "'-' は単項否定。'--' は parse error として扱われます"
  }
}
```

### 9.2 警告

- 0 件にショートサーキットされる原因は `warnings` に集約（exit 0 / parse OK）:
  - `unknown_key:<name>` — schema.toml に存在しないキー
  - `type_mismatch:<key>` — int フィールドに非数値、enum に未定義値
  - `type-and-multi` — 同一 `type:` の暗黙 AND
  - `unknown_alias:<key>=:<alias>` — `type::name` で参照した alias が AliasCatalog に無い（§4.11）
  - `unsupported_alias:<key>=:<alias>` — `::name` を `type:` 以外に使った（§4.11）
  - `empty_list_item:<key>` — コンマリスト内の空 token、例 `type:psd,,jpg`（§4.10）
- `warnings` が空でない json は `--format json` でも出力（CI スクリプトが拾える）。

---

## 10. Worked examples

### 10.1 自由テキスト + 予約キー

```
forest tag:wip type:psd
```

- AST: `(freetext "forest") AND (tag:wip) AND (type:psd)`
- SQL（概念）:
```sql
SELECT f.file_id, f.path FROM files f
JOIN files_fts fts ON fts.file_id = f.file_id
JOIN tags t ON t.file_id = f.file_id
WHERE files_fts MATCH 'forest' AND t.name = 'wip' AND f.ext = 'psd'
ORDER BY f.path;
```

### 10.2 OR / グループ

```
(tag:wip OR tag:review) -is:violation
```

- AST: `(tag:wip OR tag:review) AND NOT is:violation`
- 「`wip` または `review` を持ち、違反のないファイル」

### 10.3 範囲

```
type:psd updated:2026-04-01..2026-04-25
```

- AST: `type:psd AND updated:[2026-04-01T00:00:00Z .. 2026-04-25T23:59:59.999Z]`
- 半開: `updated:2026-04-01..` は `>= 2026-04-01T00:00:00Z`、`updated:..2026-04-30` は `<= 2026-04-30T23:59:59.999Z`。

### 10.4 カスタムフィールド + 範囲

```
scene:10 shot:1..20 status:wip
```

- AST: `scene = 10 AND shot ∈ [1, 20] AND status = 'wip'`
- planner: int は `value_int`、enum は `value_text` を引く。

### 10.5 NOT で除外

```
type:psd -path:"./assets/shots/archive/**"
```

- 「PSD だが archive 以下にないもの」

### 10.6 引用 / グロブのエスケープ

```
name:"My File*.psd"  path:"./プロジェクト x/**"
```

- スペースや非 ASCII を含む値は `"..."` で括る。glob メタ文字は `"..."` の中でも有効（`*` は wildcard）。
- リテラル `*` を含めたい場合は `\*`（v1 では `name:` / `path:` の glob 構文上のみ。`STRING` トークンとしては未対応）。

### 10.7 暗黙 AND の落とし穴

```
type:psd type:tif
```

- AST 上は AND。同一ファイルが両方の type を持つことはなく結果 0 件。
- `warnings: ["type-and-multi"]` を返す。
- 意図したのは OR: `type:psd OR type:tif`。

### 10.8 saved view 参照（query 単独 / 拡張）

views.toml で定義した `id = "violations"` の query は、CLI / UI から `view:violations` で参照する **予約キー風シンタックス**ではなく、別フラグで指定する:

```
$ progest search --view violations
$ progest search --view violations -is:misplaced
```

- 後者は **保存ビューの query AND 追加 query** で評価。views.toml `query` の前置展開。
- v1 では query 文字列内に `view:<id>` のような自己参照は許さない（無限再帰防止）。

---

## 11. 保存済みビュー

### 11.1 ライフサイクル

- ロード時 `views.toml` を読み、id 一意性を確認。重複は load error。
- view の query は parse → validate を loader 時に実行し、構文エラーは load error（views.toml 全体を拒否）。
- runtime（GUI / CLI）では view を参照するだけ、編集は `progest view save <id> <query>` / `progest view delete <id>` の CLI または UI から。

### 11.2 フィールド

| フィールド | 型 | 必須 | 意味 |
| --- | --- | --- | --- |
| `id` | `^[a-z][a-z0-9_-]{0,63}$` | ✅ | 識別子 |
| `name` | string | ✅ | UI 表示名 |
| `query` | string | ✅ | DSL 本体 |
| `description` | string | ❌ | UI 説明 |
| `group_by` | string (key) | ❌ | フラットビュー時のグルーピングキー（§11.3） |
| `sort` | string | ❌ | v1 では warning（§11.4） |

### 11.3 `group_by`

- v1 サポート対象キー: `tag`（複数タグなら最初のもの）/ `type` / `kind` / カスタムフィールド名 / `parent_dir`（path の親ディレクトリ）。
- 上記以外を指定したらロード時 warning（黙視 / null 扱い）。
- GUI のフラットビューで `group_by` キーで区切って表示。CLI 出力には影響しない。

### 11.4 `sort` は v1 未対応

- views.toml `sort` フィールドの値はロード時 warning に集約、評価には使わない。
- v1.x で `sort:<key>:<asc|desc>` の DSL clause として再導入予定。

---

## 12. 検索履歴（個人）

### 12.1 書込タイミング

- CLI / UI で query が **正常実行**（exit 0、parse OK）した時点で 1 entry 追加。
- parse error / cancel は履歴に残さない。
- retention 100 件、超過分は古い順に drop（M2 `core::history` と同じく FIFO）。

### 12.2 用途

- UI の検索ボックス / コマンドパレットの recent 候補。
- CLI 補完（v1.x、`progest search --history` などの subcommand 候補）。

---

## 13. 性能ベンチ

`tests/bench/search_smoke.rs`:

| シナリオ | データ | 目標 p95 |
| --- | --- | --- |
| `is:violation`（全違反列挙） | 100k files / 5% violation | 100ms |
| `tag:foo type:psd path:./assets/**` | 100k files | 50ms |
| `forest tag:wip` (FTS5 + tag) | 100k files / 1k tags | 100ms |
| `created:2026-01-01..2026-04-30` | 100k files | 50ms |

ベンチは M3 で導入。CI gate にはせず、回帰検出用の参考値として `cargo bench` で確認する。

---

## 14. 実装メモ

### 14.1 Crate 構成

- `progest_core::search::{ast, lex, parse, validate, plan, execute}`
- `progest_core::index::fts5` — schema migration（M3 で virtual table 追加）
- `progest_core::index::custom_fields` — `(file_id, key, value_text, value_int)` テーブル
- `progest_cli::cmd::search` — CLI driver
- `progest_cli::cmd::tag` — `tag add|remove|list`
- `progest_tauri::commands::search` — Tauri IPC

### 14.2 SQLite / FTS5 設定

- FTS5 virtual table:
```sql
CREATE VIRTUAL TABLE files_fts USING fts5(
  file_id UNINDEXED,
  name,
  notes,
  tokenize = 'trigram'
);
```
- `name` / `notes` 更新時にトリガで `files_fts` を upsert。
- SQLite は `rusqlite` の `bundled` feature（FTS5 含む）。

### 14.3 順序

実装順は [`M3_HANDOFF.md §2`](./M3_HANDOFF.md) を参照。本 DSL 仕様書の確定 → `core::search` parser/planner → FTS5 + custom_fields → CLI → UI。

---

## 15. 既知の v1.x 候補

- `sort:<key>:<asc|desc>` の DSL clause + views.toml `sort` 復活
- `limit:N` / `offset:N` ページング
- `parent:<id>` / `children:<id>` のリレーション辿り
- 近接検索 `"foo bar"~3`、編集距離
- ファセット集計（`progest search --facets tag,type`）
- ranking スコア（FTS5 bm25）に基づく結果順
- lindera 形態素解析オプション（`schema.toml` の `[search] tokenize = "lindera"`、schema_version 据え置き）
- `extends`（views.toml の他 view を継承）
- relative date（`updated:>1w`、`created:<1mo`）
- query 内 `view:<id>` 展開（無限再帰防止のための深さ制限つき）

---

## 16. 変更履歴

- 2026-04-25: 初版（feat/m3-search-dsl-spec、M3 kickoff）
