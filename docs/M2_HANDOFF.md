# M2 引き継ぎメモ

M1 完了後に M2 を進めるための、次セッション向けハンドオフ。
**M2 に関わる実装に入る前に必ず目を通す。** 進捗に合わせて更新していく。

最終更新: 2026-04-25（post-M2 リファクタ landed: CLI 共通化 / テストハーネス / `ApplyWarning` enum 統合）

---

## 1. 現在地

- M1 Core data layer 完了。全モジュール + CLI `init`/`scan`/`doctor` + 10k scan ベンチ（実測 ~82 ms）landed。
- M2 opener: `core::meta` の残タスク（pending queue + `.dirmeta.toml` loader）landed（PR #12）。
- DSL 仕様書: [`docs/NAMING_RULES_DSL.md`](./NAMING_RULES_DSL.md) landed。
- `core::rules` 本体 landed（feat/m2-core-rules）: loader（forward-compat schema gate）/ applies_to（glob + specificity）/ template parser & matcher / constraint evaluator / inheritance（full-replace override）/ evaluate + `RuleHit` trace。unit tests ~260 + §10 golden + Codex 指摘 5 件のホットフィックス + regression golden。未着手拡張（suggested_names / §6 seq 採番 / trace `NotApplicable` / Regex::new キャッシュ化 / §1.3・§4・§5・§7・§9 の golden 拡充）は follow-up issue。
- `core::accepts` 本体 landed（feat/m2-core-accepts）: [accepts] TOML 抽出 / builtin alias catalog（v1 の 7 種、`docs/ACCEPTS_ALIASES.md` で拡張子確定）/ project alias loader (`.progest/schema.toml`) / effective_accepts 計算（inherit union + own/inherited provenance）/ placement lint（`category=placement`, `rule_id=placement`, `PlacementDetails` 充填）/ 7 シナリオ × 12 golden。Codex レビューで `:image` → svg/psb/dds、`:video` から prores 削除、`:text` から svg 移動などが反映済み。`suggested_destinations` は follow-up で充填。
- `core::naming` 本体 landed（feat/m2-core-naming）: `NameCandidate`/`Segment::{Literal,Hole}`/`Hole{origin,kind,pos}`、`⟨cjk-N⟩` sentinel、pipeline 3 段（stage1: ` (N)` / ` - Copy [ (N)]` / ` のコピー [ N]` 3 種を tail-only で剥ぐ、stage2: 連続 CJK ラン → 単一 Hole、stage3: heck-backed case）、`[cleanup]` loader（flat bool + `convert_case` 文字列）、`FillMode::{Skip, Placeholder(String), Prompt}` と `resolve`（`Prompt` は core 層で `UnresolvedHoleError::PromptUnavailable`）、`suggest::fill_suggested_names` が naming 系 Violation にのみ hole-free 候補を充填、CLI `progest clean`（preview のみ、`--case`/`--strip-cjk`/`--strip-suffix`/`--fill-mode`/`--format text|json`/`[PATH]...`）、`core::rules::template::apply_string_specs` は `naming::case::rules_format_spec` 経由に切替。`FillMode::Prompt` の実装と `progest clean --apply` は `core::rename` と同時実装。
- `core::history` 本体 landed（feat/m2-core-history）: SQLite `.progest/local/history.db`、`entries(id, ts, op_kind, payload_json, inverse_json, consumed, group_id)` 1 テーブル + `meta(key, value)` の `pointer` row、5 op kind（`rename`/`tag_add`/`tag_remove`/`meta_edit`/`import`）、`invert()` を純粋関数で実装（double-inverse 恒等）、`Store::append` が redo branch を erase して retention 50 を tail 削除で維持（pointer stale 化は最新 non-consumed に reconciliation）、`undo()`/`redo()` は `consumed` フラグの反転 + pointer 遷移、33 単体 + 5 integration。apply 側（FS+meta+index 原子性）は各 op の呼び出し側の責務、history は「記録だけ」。CLI 配線と rename/import の呼び出しは `core::rename` 着手時に合流。
- `core::rename` 本体 landed（feat/m2-core-rename）: `RenameOp`/`Conflict`/`ConflictKind` の pub serde wire type、`build_preview` / `build_preview_with_prompter`（4 種 conflict 検出 — Identity / TargetExists / DuplicateTarget / Unresolved、chain `foo→bar→baz` は許容）、`Rename::apply` の 2-phase atomic（`.progest/local/staging/<uuid>/` 経由 stage→commit→rollback、FS rename + `.meta` sidecar rename を一体に扱う）、index update は post-commit best-effort（`IndexWarning` で記録、reconcile が回復）、`history::Store` 連携（bulk ops で auto group_id、per-op `group_id` 優先、`HistoryWarning`）、`fs::FaultyFileSystem` decorator + property test（5-op × 20 fault placements で all-or-nothing 不変条件確認、`(file, sidecar)` ペアの一致もチェック）。
- `core::sequence` 本体 landed（feat/m2-core-rename）: 同 parent + stem prefix + separator + padding + extension で group、min 2 members、gap 許容、決定的出力（`(parent, stem, sep, padding, ext)` でソート、members は index 昇順、singletons は lex sort）。`requests_from_sequence(seq, new_stem)` で stem 置換 `RenameRequest` 群を生成、`seq-{uuid}` group_id 共有で undo がバッチ単位。`Rename::apply` 側で `outcome.group_id` が caller-supplied per-op group を round-trip するように調整。
- `naming::HolePrompter` trait + `PromptError` + `resolve_with_prompter` landed（feat/m2-core-rename）: core が trait 定義のみ、CLI 側 `StdinHolePrompter`（generic `Read + Send` / `Write + Send`、prompt は stderr / 入力は stdin で JSON pipe を壊さない）。
- CLI `progest rename` / `progest clean --apply` landed（feat/m2-core-rename）: `--mode preview|apply`、`--format text|json`、`--fill-mode skip|placeholder|prompt`、`--sequence-stem STEM`、`--from-stdin` (RenameOp[] / RenamePreview JSON)、`--sequence-stem` と `--from-stdin` は mutually exclusive。`clean --apply` は core::rename と同じ apply path 経由、Identity / Unresolved (`from == to`) ops は filter で落とす。core 39 + cli 5 smoke + 6 prompter test + property test 全 green。
- `core::sequence::drift` landed（feat/m2-cli-lint-undo-redo）: 同 parent + 正規化 stem + ext の sequence 群で separator / padding / stem-case 差を検出、majority canonical で非カノニカル sibling の全 member に suggested_name 付きで DriftViolation を発行。lint に `Category::Sequence` / `rule_id=sequence-drift` として流れる。
- `core::lint` orchestrator landed（feat/m2-cli-lint-undo-redo）: `lint_paths(fs, meta_store, paths, opts)` が `CompiledRuleSet` + `AliasCatalog` + `CleanupConfig` + `compound_exts` を受け取り、rules + accepts + drift を 1 パスで集約。dirmeta chain は parent 単位でキャッシュ、`explain=false` の時は非 Winner trace を trim（DSL §9.3）、`fill_suggested_names` は naming のみ充填。
- CLI `progest lint` landed（feat/m2-cli-lint-undo-redo）: `[PATH]...` / `--format text|json` / `--explain` / exit code DSL §8.2 準拠。grouped JSON `{naming, placement, sequence, summary}`、text は 3 セクション見出し + summary。`rules.toml` / `schema.toml` は optional、`ProjectRoot::{rules_toml,schema_toml}()` accessor + filename const を追加。6 smoke test。
- `progest clean` sequence-aware preview landed（feat/m2-cli-lint-undo-redo）: `detect_sequences` を walker 結果に走らせ、各 member に `seq-{uuid}` を割当。JSON `sequence_group`、text 横タグ、apply 時は `RenameRequest.group_id` として送出 → history に同じ group で記録 → undo で一括。2 smoke test。
- CLI `progest undo` / `progest redo` landed（feat/m2-cli-lint-undo-redo）: head (undo) / 次 consumed (redo) を peek、default で同 group の contiguous entry を driver で replay、`--entry` で 1 件のみ。Rename は `Rename::new_without_history` で FS+index を触る（history は `Store::undo/redo` で `consumed` flip のみ）。tag/meta_edit/import は "not yet wired" エラーで明示停止。5 smoke test。
- **M2 完了**。`core::rename` + `core::sequence` + `core::lint` + CLI `lint` / `clean` / `rename` / `undo` / `redo` + undo history 50 + DSL §8.2 exit code が全て landed し、M2 完了条件「fixture に lint が naming / placement 両方の違反を期待通り検出、rename preview と apply が動く、undo で戻せる」を満たす。

詳細は [`docs/IMPLEMENTATION_PLAN.md §0`](./IMPLEMENTATION_PLAN.md)（進捗スナップショット）と [`§5 M2`](./IMPLEMENTATION_PLAN.md) を参照。

---

## 2. 次に着手する順序（推奨）

| # | モジュール | 依存 | メモ |
| --- | --- | --- | --- |
| 1 | `core::rules` | [NAMING_RULES_DSL.md](./NAMING_RULES_DSL.md) | **landed**（feat/m2-core-rules）。Codex レビューで挙がった 5 件の仕様乖離は同 PR 内で修正済。未着手の拡張（suggested_names / §6 seq 採番 / trace 強化 / perf）は follow-up issue に切り出した。 |
| 2 | `core::accepts` | `.dirmeta.toml` loader + rules | **landed**（feat/m2-core-accepts）。`[accepts]` 抽出 + alias catalog + effective_accepts + placement lint。`suggested_destinations` ランキングと `[extension_compounds]` project loader は follow-up issue。 |
| 3 | `core::naming` | heck crate 追加 / rules template との連携 | **landed**（feat/m2-core-naming）。pipeline 3 段 / NameCandidate + Hole / `[cleanup]` loader / `FillMode` / rules::template 経由の case 切替 / `suggest::fill_suggested_names` / CLI `progest clean` (preview)。残タスク: `FillMode::Prompt` 実装、`progest clean --apply`（共に `core::rename` 着手時に同時実装）。 |
| 4 | `core::history` | なし | **landed**（feat/m2-core-history）。SQLite `.progest/local/history.db`、entries + meta(pointer)、5 op kind、`invert()` pure、undo/redo + retention 50、apply 原子性は呼び出し側責務。CLI 配線は `core::rename` 合流時。 |
| 5 | `core::rename` | rules + naming + history | **landed**（feat/m2-core-rename）。`RenameOp` pub serde / `build_preview` + `build_preview_with_prompter` / `Rename::apply` の 2-phase staging（`.progest/local/staging/<uuid>/`）+ rollback / FS rename と `.meta` rename を一体に / index update post-commit best-effort / `history::Store` 連携 with bulk auto-group_id / `FaultyFileSystem` decorator + 5-op × 20 fault property test。 |
| 5b | `core::sequence` | rename | **landed**（feat/m2-core-rename）。同 parent + stem + sep + padding + ext で group、min 2 members、gap 許容。`requests_from_sequence(seq, new_stem)` で stem 置換。 |
| 5c | `naming::HolePrompter` + `StdinHolePrompter` | naming | **landed**（feat/m2-core-rename）。core が trait 定義、CLI が TTY impl（prompts → stderr、入力 → stdin）。 |
| 5d | CLI `progest rename` / `progest clean --apply` | 上記 | **landed**（feat/m2-core-rename）。path 引数 + lint stdin パイプ + `--sequence-stem STEM`、`--mode preview|apply`、`--format text|json`、`--fill-mode skip|placeholder|prompt`。 |
| 6 | CLI `lint` / `undo` / `redo` + sequence 横展開 | 上記 | **landed**（feat/m2-cli-lint-undo-redo）。`core::sequence::drift` / `core::lint` orchestrator / `progest lint` / sequence-aware `progest clean` / `progest undo` / `progest redo` / `Rename::new_without_history` constructor + `Rename.history` を `Option` 化。M2 完了。

---

## 3. ユーザーが「丁寧に見たい」と事前指定した領域（focus areas）

kickoff 時に必ず AskUserQuestion で個別に詰める。

### 3.1 DSL 構文（確定済み）

[`docs/NAMING_RULES_DSL.md`](./NAMING_RULES_DSL.md) で以下を規定済み。parser 着手時は該当章を参照:

- §4 テンプレート規則（プレースホルダー / フォーマット指定子 / open-ended slot の 1 個制限 / 複合拡張子最長一致 / `{field:<name>}` `{date:<fmt>}` の具体値展開による literal 比較）
- §5 制約規則（charset / casing / forbidden_chars / forbidden_patterns / reserved_words のトークン化定義 / max_length・min_length の NFC + grapheme cluster 計測）
- §7 継承・override（full replace、kind 変更時のみ override 必須）と specificity（literal segments → literal chars → source hierarchy → rule_id 辞書順）
- §8 評価フロー（template は specificity winner 1 本で fallback なし、constraint は AND 合成）と CLI exit code
- §9 rule_id trace（違反ファイルのみ trace 保持、`--explain` で全件）
- §10 worked examples（golden fixture と 1:1 対応を想定）

v1.x に送った項目（§12）: `{today:}` / include・extends / `pack_gaps = true` / `--explain=verbose` / brace expansion。

### 3.2 rule_id trace の実装方針

- 「どの規則が勝ったか」「継承チェーンは何か」を常に返す
- 表現: DSL 仕様書 §9.2 の `RuleHit` 構造（rule_id / kind / source / decision / specificity_score / explanation）
- メモリ予算: 違反ファイルのみ trace 全件保持、非違反は winner rule_id のみ（§9.3）。`--explain` 指定時のみ全件保持

### 3.3 4 モード（strict / warn / hint / off）の振る舞い

DSL 仕様書 §8.2 で確定済み:
- `strict`: 違反は保存・rename を拒否、lint で exit 1
- `warn` (default): lint レポート出力、exit 影響なし
- `hint`: lint に出さず、UI の rename suggest でのみ使用
- `off`: 評価しない
- 総合 exit: naming / placement いずれかに strict 違反 1 件でもあれば exit 1。`evaluation_error`（参照プレースホルダーの値欠落等）も strict と同等の重み。`--format json` は exit 0/1 に関わらず JSON を stdout に流す

### 3.4 rename の原子トランザクション

- preview: `Vec<RenameOp>` を返す。各 op は (old_path, new_path, rule_id, conflicts?)
- apply: for each op、FS rename + `.meta` rename + index upsert を全部成功させる。途中失敗したら既完了分を rollback。
- 実装の勘所:
  - rollback 戦略: shadow copy を作ってから apply、失敗時は shadow を戻す。temp dir 経由で十字結びを回避
  - index と FS の順序（どっち先？失敗時の復帰順）
  - watch イベントで apply 中の rename がループに入り込まないか（apply 自身を quiet window として扱う必要あり）
- 事故った時に CI で再現しにくい領域なので、まず property test / 人為的 failure injection の枠組みを入れてから実装に入るべき

---

## 4. 横断的に忘れてはいけないこと

- **ドキュメント更新**。各 PR 完了時に `docs/IMPLEMENTATION_PLAN.md §0` の進捗スナップショット、本ドキュメント（M2_HANDOFF）の履歴、新しく見つけた落とし穴は `docs/LESSONS.md` に追記、古くなった記述の削除まで含めてセット。
- **`docs/IMPLEMENTATION_PLAN.md` の M2 完了条件**: 「fixture プロジェクトに対して lint が naming / placement 両方の違反を期待通り検出、rename preview と apply が動く、undo で戻せる」。M2 終わりに改めて確認。
- **`core::meta` の schema_version**: M2 で新フィールドを足す場合は基本 `extra: Table` に載せて SCHEMA_VERSION は据え置く。構造的な互換破壊時のみ bump。
- **破壊的操作は必ず preview → confirm**: rename, bulk apply, merge resolution 全て。undo history を N 件残す（デフォルト 50、REQUIREMENTS §3.4 準拠）— `core::history` 実装時に担保。
- **progest-merge**（`.meta` 用 git merge driver）は M2 範疇ではない（M4）。Aware しておくが今は触らない。

---

## 5. M3 着手時の論点 — `core::import` と sequence の統合

M2 で `core::sequence` + `core::sequence::drift` が landed し、`progest clean` / `rename --sequence-stem` / `lint` が既に sequence を消費している。M3 で着手する `core::import`（IMPLEMENTATION_PLAN §5 で位置づけ）では sequence を以下のように活用できる。着手時に AskUserQuestion で詰める想定。

### 5.1 drop list → sequence 集約（基本フロー）

1. ユーザーが D&D / CLI `progest import <paths...>` でファイル群を投入
2. `sequence::detect_sequences(paths)` で sequence / singletons 分離
3. sequence 群: まとめて 1 UI 行として提示し「N files · `frame_*.exr` (4-padded)」のような単一ヘッダ + 代表 member を見せる
4. `accepts::suggested_destinations` ランキングは sequence 全体で 1 回だけ走らせ、全 member に同じ候補配置を適用（extension 統一されているので ext ベースで OK）
5. 命名調整が要る場合は `rename --sequence-stem` 相当のフローで stem 置換を 1 度入力 → 全 member に適用
6. 原子 commit: `[copy-or-move, ..., meta 生成, index 登録]` を 1 batch で記録、sequence は同じ history `group_id` (`seq-{uuid}`) で入れる → `progest undo` が 1 回で全体を戻す

### 5.2 accepts ランキング + sequence の噛み合い

- `core::accepts::suggested_destinations`（follow-up 未実装）は dir ごとのスコアを返す設計。sequence 全体で 1 回計算して全 member に流用する
- sequence 内の member の ext が万一バラけていたら（`.exr` / `.jpg` の混在）、sequence detection が別グループ扱いにするので問題なし（§5 検出仕様）
- inherit + own の優先度は既存仕様通り（REQUIREMENTS §3.13.2）

### 5.3 sequence drift と import

- import 時に drift が見つかった場合（既存 sequence に pad 違いで追加しようとした等）: `lint` がやるのと同じ `Category::Sequence` Violation として扱い、UI は「既存の 0001 形式に合わせて 0042 に正規化」選択肢を出す
- `suggested_name` は既存のカノニカル shape で pre-render されている → 命名調整ステップはスキップ可能

### 5.4 API 変更ポイント候補

- `core::import` module 新設（M3）: `core::rename` と同じく `stage→commit→rollback` 2-phase、history `Operation::Import` append
- `Rename::apply` / `Rename::new_without_history` の構造は M3 でも再利用可。import は (rename + meta 新規生成) の拡張で書けるはず
- `core::sequence` へ `requests_from_sequence_import(seq, dest_dir)` 的な helper を追加するか検討（stem 置換は必要ないが配置先 dir を渡して新 path を合成する分）

### 5.5 新規 op kind が要るか

`Operation::Import` は既に `core::history::types` に予約されているが、現行 CLI では未発行。M3 で発行側を実装する。inverse は `is_inverse = true` の Import（= 削除）で定義済。

### 5.5.5 ApplyWarning に ImportWarning variant を追加

post-M2 リファクタで `IndexWarning` + `HistoryWarning` を `ApplyWarning enum { IndexUpdate, HistoryAppend }` に統合済 (`crates/progest-core/src/rename/apply.rs`)。`core::import` 着手時に 3 つ目の variant `ImportWarning::ImportFailed { src, dest, message }` を足す方針（rename と同じ Vec に乗せるか別 ApplyOutcome を使うかは設計時に再検討）。

### 5.5.7 Conflict ↔ Warning の語彙整理 (post-M2 refactor 2-4)

post-M2 audit で挙がった候補 2-4（ApplyOutcome の `{ conflicts, warnings }` 構造化 / `Conflict` ↔ `Warning` の体系再検討）は import の payload 設計と合流させるため M3 kickoff に持ち越し。論点:

- `Conflict` は preview phase（apply をブロックする条件）、`Warning` は apply post-commit（FS 完了済みで repair 可能）の意味的差があり、安易に `Issue` enum へ統合するのは情報を捨てる
- import が新規 conflict 種を追加するか（例: dest already exists、source missing）で語彙が変わる
- M3 で import の Conflict variants を確定させてから一気に整理

### 5.6 doctor staging cleanup（M2 から follow-up）

rename / import 共通の `.progest/local/staging/<uuid>/` に残骸が残り得る（rollback 失敗時）。`progest doctor --clean-staging` (age > 1 日の uuid dir を GC) を M3 着手時に同時実装するのが望ましい。IMPLEMENTATION_PLAN §0 の rename follow-up 項目に既記載。

旧 kickoff 質問（`core::rules` / `core::accepts` / `core::naming` / `core::history` / `core::rename` / CLI `lint`/`undo`/`redo`）は §6 履歴から辿れる。

---

## 6. 履歴

- 2026-04-22: 初版作成（M1 完了 / M2 opener landed のタイミング）
- 2026-04-22: CLAUDE.md 分割完了（`docs/ARCHITECTURE.md` / `CODING_STYLE.md` / `WORKFLOW.md` / `LESSONS.md` / 進捗スナップショットを `IMPLEMENTATION_PLAN.md §0` へ）。該当セクションは本ドキュメントから削除
- 2026-04-23: DSL 仕様書 `docs/NAMING_RULES_DSL.md` landed。§3.1（DSL 構文未確定）/ §3.2（trace 方針）/ §3.3（4 モード）/ §4（undo 件数）を仕様確定版に差し替え、kickoff テンプレから DSL 確定論点を外した
- 2026-04-23: `core::rules` 本体 landed（feat/m2-core-rules）。Codex レビューで検出した 5 件の仕様乖離を同 PR で修正（loader forward-compat / `{field:}` spec 検証 / `{ext}` compound 最長一致 / `required_prefix` を stem 判定 / §5.7 NFC）。残タスクは follow-up issue に切り出し済み。M2_HANDOFF §1 / §2 / §5 を `core::accepts` 着手向けに更新
- 2026-04-23: `core::accepts` 本体 landed（feat/m2-core-accepts）。`docs/ACCEPTS_ALIASES.md` 初版で builtin alias の拡張子集合を確定（Codex レビューで `svg` を `:image` へ移動、`prores`/`fcpbundle` 除外、`psb`/`dds`/`vdb` 追加など）。`Violation` に `placement_details` を追加して naming と一体で運べる形に。`suggested_destinations` / import ランキング / `[extension_compounds]` project loader は follow-up issue。M2_HANDOFF §1 / §2 / §5 を `core::history` 着手向けに更新
- 2026-04-23: `core::naming`（AI 非依存の機械的命名整理）を Phase 2.5 として M2 へ挿入。`heck` crate 差し替え、pipeline（remove_copy_suffix → remove_cjk → convert_case、固定正規順序・stage ごと on/off）、NameCandidate（literal + 穴）モデル、fill-mode（`prompt`/`placeholder[:STR]`/`skip`、穴付き文字列がディスクへ出ないことを保証）、`.progest/project.toml [cleanup]` loader、violation.suggested_names[] 機械的充填、CLI `progest clean` の設計合意。REQUIREMENTS §3.5.5 / IMPLEMENTATION_PLAN §0・§5 / M2_HANDOFF §1・§2・§5 を naming 着手向けに更新。実装は別ブランチで後日
- 2026-04-23: `core::naming` 本体 landed（feat/m2-core-naming）。pipeline 3 段実装（strip は tail-only、3 種の OS copy-suffix のみ対応）、連続 CJK ランを単一 Hole に収斂、`heck` 導入で `PascalCase → snake_case` の正しい境界検出、`core::rules::template::apply_string_specs` を `naming::case::rules_format_spec` へ委譲、`suggest::fill_suggested_names` は placement を除外して hole-free 候補のみ充填、CLI `progest clean` は preview 限定で JSON/text 出力、integration + smoke テスト整備。`FillMode::Prompt` と `progest clean --apply` は `core::rename` 着手時に同時実装予定。M2_HANDOFF §1 / §2 / §5 を `core::history` 着手向けに更新
- 2026-04-24: `core::history` 本体 landed（feat/m2-core-history）。SQLite 単一テーブル `entries` + `meta(pointer)` 構成で `.progest/local/history.db`、5 op kind（`rename`/`tag_add`/`tag_remove`/`meta_edit`/`import`）、`invert()` は純粋関数で double-inverse 恒等、`append` が redo branch を自動 erase、retention 50 で tail 削除 + pointer stale は最新 non-consumed に reconciliation、undo/redo は `consumed` フラグの反転 + pointer 遷移。history は「記録だけ」で FS/meta/index 原子性は呼び出し側の責務とする契約（REQUIREMENTS §3.4 準拠）。 `ProjectRoot::history_db()` accessor と `HISTORY_DB_FILENAME` constant を追加、`.progest/local/` は既に gitignore 対象。M2_HANDOFF §1 / §2 / §5 を `core::rename` 着手向けに更新
- 2026-04-24: **M2 完了**。CLI `lint` / `undo` / `redo` + sequence 横展開を `feat/m2-cli-lint-undo-redo` ブランチで一括 landed。`core::sequence::drift`（inter-sequence 差分検出、majority canonical、`DriftViolation` + `suggested_name`）/ `core::lint` orchestrator（rules + accepts + drift 集約、dirmeta chain cache、DSL §9.3 trace trim）/ `progest lint`（grouped JSON `{naming, placement, sequence, summary}` + text + exit code DSL §8.2）/ sequence-aware `progest clean`（`sequence_group` タグ + apply 時 group_id 共有）/ `progest undo` / `progest redo`（default group 単位 / `--entry` 単発、`Rename::new_without_history` で FS+index のみ replay、history は `Store::undo/redo` で consumed flip）/ `Rename.history` を `Option` 化。core lint 4 + drift 9 + cli lint 6 + clean 2 新規 + undo/redo 5 smoke test、既存 rename/clean test 全 pass。`Category::Sequence` 追加、`ProjectRoot::{rules_toml,schema_toml}()` accessor + filename const 追加。§5 は M3 `core::import` kickoff 向けに差し替え。
- 2026-04-24: `core::rename` + `core::sequence` + `naming::HolePrompter` + CLI `rename` / `clean --apply` 一括 landed（feat/m2-core-rename）。`fs::FaultyFileSystem` decorator + proptest を最初に入れて apply の rollback 不変条件を property test で固定（5-op × 20 fault placements）。`Rename::apply` は 2-phase staging（`.progest/local/staging/<uuid>/`）+ rollback / FS rename と `.meta` rename を一体に / index update post-commit best-effort（`IndexWarning`）/ history `Operation::Rename` append と bulk auto group_id（per-op 優先、`outcome.group_id` が caller group を round-trip）。`core::sequence` は同 parent + stem + sep + padding + ext で group・gap 許容・min 2 members、`requests_from_sequence(seq, new_stem)` で stem 置換 RenameRequest 群を生成。`StdinHolePrompter` は generic `Read + Send`/`Write + Send` で stderr→prompt / stdin→入力（JSON pipe を壊さない）。CLI は `--mode preview|apply` / `--format text|json` / `--fill-mode skip|placeholder|prompt` / `--sequence-stem STEM` / `--from-stdin`、`clean --apply` は同じ apply path。core 39 + cli 5 smoke + 6 prompter test + property test 全 green。M2_HANDOFF §1 / §2 / §5 を CLI `lint` / `undo` / `redo` 着手向けに更新
- 2026-04-25: post-M2 リファクタ landed（refactor/post-m2-cleanup、7 commits）。M3 着手前の整理として、agent audit に基づき (1) CLI 共通化 — `crate::output::OutputFormat`（lint/clean/undo の `FormatFlag` × 3 を統合）+ `crate::context::{discover_root, load_ruleset, load_alias_catalog_from_root, load_cleanup_config, open_index, open_history}`（5 sub command の ProjectRoot 解決 + 設定ローダー + DB open boilerplate を集約、`CleanupOverrides` で flag → cfg の差し込みを 1 箇所に）+ `crate::walk::collect_entries`（lint/clean/rename の 3 重コピーを単一実装に）/ `CaseFlag::to_style` を `pub(crate)` 化して rename 側の重複削除、(2) テストハーネス — `progest-cli/tests/support/mod.rs`（binary_path / init_project / touch / write_file / run、5 smoke ファイルから抽出）+ `progest-core/tests/support/mod.rs`（p / sample_fingerprint / sample_doc）、(3) `core::rename` Warning 統合 — `IndexWarning` + `HistoryWarning` を `ApplyWarning enum { IndexUpdate, HistoryAppend }` に統合、`ApplyOutcome.warnings: Vec<ApplyWarning>` + `index_warnings()` / `history_warnings()` iterator helpers + `from()` / `to()` / `message()` accessors。JSON wire は `{"kind":"index_update", ...}` 形式に変更（smoke test は warning フィールドに assert していなかったので fixture 更新不要）。Conflict ↔ Warning の語彙整理 (audit 2-4) は M3 import の Conflict variants を確定させてから合流するため defer。reconcile_flow の `Harness` は単一消費者なのでまだ promote しない判断。
