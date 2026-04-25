# M3 引き継ぎメモ

M2 完了後に M3 を進めるための、次セッション向けハンドオフ。
**M3 に関わる実装に入る前に必ず目を通す。** 進捗に合わせて更新していく。

最終更新: 2026-04-25（M3 #2 landed: `core::search` parser + AST + validate + planner + 8 golden fixtures、chrono を core 依存に追加）

---

## 1. 現在地

- M2 Naming rules engine + accepts 完了。`core::rules` / `core::accepts` / `core::naming` / `core::history` / `core::rename` / `core::sequence` / `core::sequence::drift` / `core::lint` 全コア + CLI `init`/`scan`/`doctor`/`clean`/`rename`/`lint`/`undo`/`redo` + history retention 50 が landed（IMPLEMENTATION_PLAN §0）。
- post-M2 リファクタ landed（PR #26）: CLI 共通化（`crate::output` / `crate::context` / `crate::walk`）、テストハーネス、`ApplyWarning` enum 統合。
- M3 スコープ整合（PR #27）: §0/§5/M2_HANDOFF が「M3 = import」と「M3 = 検索とビュー」で乖離していたのを §5 を正として揃え直し。**M3 = 検索とビュー**、**M4 = import + thumbnail + AI + template** が確定。
- M3 kickoff 議論で確定:
  - 順序: DSL 仕様書 → `core::search` → FTS5 → CLI search/tag → UI
  - PR 粒度: M2 同様フェーズ単位（7〜9 PR 想定）
  - スコープ追加: **Tauri IPC 層を M3 で同時に係る**、**lindera は v1.x defer を IMPLEMENTATION_PLAN に明示**
  - DSL 詰め方: M2 NAMING_RULES_DSL と同粒度の `docs/SEARCH_DSL.md` を最初に landed
- DSL 仕様書 [`docs/SEARCH_DSL.md`](./SEARCH_DSL.md) landed（feat/m3-search-dsl-spec）: 文法 EBNF / 予約キー全 8 種 / 自由テキスト FTS5 trigram / カスタムフィールド / 性能契約 (10k=50ms / 100k=100ms p95) / Worked examples 8 / v1.x defer 候補 / 実装メモ。lindera は §3.2 / §15 で v1.x defer を明示。
- `core::search` parser + AST + validate + planner landed（feat/m3-core-search-parser）: `progest_core::search::{ast, lex, parse, validate, plan, mod}`。二状態（Expr/Value）字句解析 + 再帰下降パーサ（OR > AND > NOT、`-` 単項、`( )` グループ、改行禁止）+ `Warning` 列挙 + `AlwaysFalse` 短絡 + `BindValue` パラメータ化 SQL 出力。`chrono` を workspace + core 依存に追加（datetime parse + UTC 正規化）。unit 72 + golden 8（§10 worked examples 1:1）= 全 80 テスト pass。executor + FTS5 schema + custom_fields テーブルは次 PR（M3 #3 + #4）。

---

## 2. 着手順序（推奨）

| # | モジュール | 依存 | メモ |
| --- | --- | --- | --- |
| 1 | `docs/SEARCH_DSL.md` | なし | **landed**（feat/m3-search-dsl-spec、PR #28） |
| 2 | `core::search` parser + AST + validate + planner | DSL 仕様書 | **landed**（feat/m3-core-search-parser）。lex / parse / validate / plan の純関数 4 段。`AlwaysFalse` 短絡 + `Warning` 列挙、`BindValue` 化 SQL 出力。72 unit + 8 golden test。chrono 依存追加。 |
| 3 | `core::search` executor | planner + index migration | SQLite に `PlannedQuery.sql` を流して `Vec<SearchHit>` を返す。M3 #4 と同 PR で landed させる予定（FTS5 schema が居ないと executor は空回り）。 |
| 4 | `core::index::fts5` + `custom_fields` | M1 `core::index` | FTS5 virtual table（`name` + `notes`、`tokenize='trigram'`）+ `custom_fields(file_id, key, value_text, value_int)` テーブル。M1 `core::index` の migration に追記、startup で create-if-not-exists |
| 5 | CLI `progest search` / `progest tag` | search executor + custom_fields | `progest search <query> [--format json\|text] [--view <id>]`、`progest tag {add\|remove\|list}`。`crate::output::OutputFormat` を流用 |
| 6 | shadcn/ui 初期化 | Vite + React 19 が居る | `pnpm dlx shadcn@latest init --preset b1D0dy4m --template vite --pointer`（`docs/IMPLEMENTATION_PLAN.md §5 M0` に固定）。`components.json` を生成して以降の UI フェーズの土台にする |
| 7 | コマンドパレット UI | shadcn / Tauri IPC search | shadcn `Command` + `Dialog`。Cmd+K で起動。検索 DSL 入力 + recent history（`local/history.json`）から候補表示。検索結果クリックでファイル選択 |
| 8 | tree view + flat view + `views.toml` | コマンドパレット | shadcn ベースのカラム/ツリー両表示。flat view はクエリ結果を group_by 含めてレンダ。views.toml を loader / saver で I/O。`progest view` CLI（`save`/`delete`/`list`）も同 PR |
| 9 | ディレクトリインスペクター + placement バッジ | tree/flat view | accepts 編集フォーム（chip input + inherit checkbox + mode セレクタ）→ `.dirmeta.toml` 書込。flat / tree 上で placement 違反バッジ（naming とは別色）。`is:misplaced` クエリ動作確認も同 PR |

各 PR で `progest-tauri` の IPC コマンド（search / tag / view CRUD / accepts edit）も合流させる。

---

## 3. ユーザーが「丁寧に見たい」と事前指定した領域（focus areas）

M3 着手中の節目（§2 #2, #3, #4, #6, #9）で AskUserQuestion で個別に詰める。

### 3.1 DSL 構文（確定済み）

[`docs/SEARCH_DSL.md`](./SEARCH_DSL.md) で以下を規定済み。parser 着手時は該当章を参照:

- §2 文法 EBNF（NOT > AND > OR、`OR` キーワード大文字、`-` 単項否定、`( )` グループ、改行禁止）
- §3 自由テキスト（FTS5 trigram、`name` + `notes` 列）
- §4 予約キー（tag/type/kind/is/name/path/created/updated）と多重 / 否定 / 範囲の意味論
- §6 カスタムフィールド（schema.toml 参照、未定義キーは parse OK + warning + 0 件）
- §7 query plan（AST → SQL の純関数、決定的、ゴールデンテスト可能）
- §8 結果スキーマ（CLI text / json）
- §9 エラー（parse error は exit 2、unknown_key は warning 集約）
- §10 Worked examples（golden fixture と 1:1 対応想定）

v1.x に送った項目（§15）: `sort:` / `limit:` / `parent:` `children:` / 近接検索 / facet / ranking / lindera / `extends` / relative date / `view:<id>` 展開。

### 3.2 FTS5 + trigram の動作確認

- `sqlite3` の `--features bundled` で FTS5 が確実に効く環境を確認（M1 で前提済み、改めて手動確認推奨）
- 短語（< 3 文字）の literal フォールバック挙動を golden test で固定
- CJK でのトリグラム生成境界（半角 / 全角混在、結合文字、絵文字）の corner case を CLI smoke で 1 件以上カバー

### 3.3 ビュースコープ（GUI / CLI 役割分担）

- `views.toml` の I/O は core で（read/write 両方）。CLI と Tauri は薄い
- `views.toml` を編集できる窓口:
  - CLI `progest view save <id> --query <q>` / `progest view delete <id>` / `progest view list`（M3 同時実装）
  - GUI 「ビューを保存」ダイアログ → IPC で core に渡す
- `local/history.json` も同様に core で I/O。retention 100 を core 側で担保

### 3.4 Tauri IPC の境界（M3 で確定させたい）

- 検索: `search.execute(query: string, view_id?: string) -> SearchResult`
- 検索履歴: `search.history.list() -> [HistoryEntry]` / `search.history.clear()`
- 保存ビュー: `view.list()` / `view.save(view: View)` / `view.delete(id)`
- ファイル一覧（tree / flat）: `files.list_tree(path)` / `files.list_flat(query)` — 後者は search.execute と統一できるか kickoff 時に再検討
- accepts 編集: `accepts.read(dir)` / `accepts.write(dir, accepts)` — `.dirmeta.toml` の書込は M2 既存 API を経由
- IPC 型は `progest-tauri/src/commands/*.rs` で `serde::Serialize` ↔ `core::*` の wire 構造を直接共有（M2 lint / clean / rename と同じ pattern）

### 3.5 shadcn 初期化の確認事項

- `--preset b1D0dy4m` の中身（カラーパレット / フォント等）が今のプロジェクトデザインと整合するか
- `--template vite` で `app/` の Vite 設定と衝突しないか（`vite.config.ts` の merge 戦略）
- `components.json` を `app/` 配下に置くか / リポジトリルートに置くか

shadcn skill が利用可能なので、init 着手時は `Skill: shadcn` を呼んでガイドに沿って進める。

### 3.6 placement バッジの色 / 位置

- naming 違反は M2 で既に lint UI（CLI text）に色付き、UI 側ではまだ未提示
- placement 違反バッジは naming とは別色（要件書 §3.13.6 のバッジ仕様を実装時に再確認）
- アイコン / 色は M5 アイコン差し替え時にも整合する規則で（仮: naming = amber、placement = sky、sequence = violet）

---

## 4. 横断的に忘れてはいけないこと

- **ドキュメント更新**: 各 PR 完了時に `docs/IMPLEMENTATION_PLAN.md §0` の進捗スナップショット、本ドキュメント（M3_HANDOFF）の履歴、新しく見つけた落とし穴は `docs/LESSONS.md` に追記、古くなった記述の削除まで含めてセット。
- **`docs/IMPLEMENTATION_PLAN.md` の M3 完了条件**: 「UI で `tag:foo type:psd is:violation` / `is:misplaced` 相当が 100ms 以下で返る、保存済みビューが永続化される、ディレクトリインスペクターで accepts を編集して `.dirmeta.toml` に反映される」（§5 M3）。M3 終わりに改めて確認。
- **性能ベンチ**: `docs/SEARCH_DSL.md §13` の規模・p95 目標を `tests/bench/search_smoke.rs` に固定。CI gate にはしない（参考値）。
- **破壊的操作**: search は read-only なので破壊的ではないが、`view save` / `view delete` / accepts 編集は preview → confirm を踏襲。
- **Tauri IPC は M3 で wire を確定**: M4 import / M5 thumbnail で IPC 表面が増える前に、search/view/accepts の IPC pattern を kickoff 時に固定する。
- **progest-merge**（`.meta` 用 git merge driver）は M3 範疇ではない（M4）。
- **lindera defer**: REQUIREMENTS §3.7 に既記載だが IMPLEMENTATION_PLAN §5 M3 にも明示で書く（PR #27 同時更新）。

---

## 5. 履歴

- 2026-04-25: 初版作成。M3 kickoff（feat/m3-search-dsl-spec）。`docs/SEARCH_DSL.md` 初版 landed と同 PR で本ドキュメントを起こす。スコープは §5 M3 のリスト + Tauri IPC 同時 + lindera defer 明示。順序は DSL 仕様書 → core::search → FTS5 → CLI → UI、PR 粒度はフェーズ単位 7〜9 PR 想定。M2_HANDOFF はそのまま M2 完了アーカイブとして残す。
- 2026-04-25: M3 #2 landed（feat/m3-core-search-parser）。`progest_core::search::{ast, lex, parse, validate, plan}` の 4 段 pure pipeline。二状態字句解析、再帰下降パーサ（OR > AND > NOT）、`Warning` 列挙、`AlwaysFalse` で unknown_key / type_mismatch / kind 値不正 / glob 不正 / datetime 不正 / `created`/`updated` 重複 / `type:` AND 多重 を全部短絡で吸収（parse は通る）、planner が決定的 SQL + `BindValue` を生成。72 unit + 8 golden（§10 worked examples 1:1）= 全 80 テスト pass。chrono を workspace + core に追加（`Z` / `±HH:MM` 両対応、UTC 正規化）。executor + FTS5 + custom_fields テーブルは M3 #3+#4 で合流予定。
