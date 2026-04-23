# M2 引き継ぎメモ

M1 完了後に M2 を進めるための、次セッション向けハンドオフ。
**M2 に関わる実装に入る前に必ず目を通す。** 進捗に合わせて更新していく。

最終更新: 2026-04-24（`core::rename` + `core::sequence` + CLI `rename` / `clean --apply` landed、残るは CLI `lint` / `undo` / `redo`）

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
- 次: CLI `lint`（既存 `core::rules` / `core::accepts` の Violation を集約）→ `progest undo` / `progest redo`（`history::Store` 配線、各 op kind の replay は呼び出し側 — rename op は `Rename::apply` を再呼び）→ M2 完了。

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
| 6 | CLI `lint` / `undo` / `redo` | 上記 | 残り。入力 format は json|text、exit code 割り当ては 4 モードに合わせる。placement 違反は `rules` と同じ Violation 形状で出るので一体で出力可能。`undo` / `redo` は `history::Store` の最新 entry を取って op kind ごとに reverse — rename op は `Rename::apply` を inverse 入力で再呼。 |

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

## 5. 次モジュール（CLI `lint` / `undo` / `redo`）kickoff 質問テンプレ

CLI `lint` / `undo` / `redo` 着手時に AskUserQuestion で確認する想定:

1. `lint` 出力構造: `core::rules` の `Violation` を category（naming / placement）ごとにグルーピングして出すか、フラット配列か。`--explain` で trace 全件を出すかどうか。json shape を CLI 安定 wire と捉えて固定してよいか
2. `lint` の exit code: strict 違反が 1 件でもあれば exit 1（DSL §8.2 準拠）。`evaluation_error` の重み、`--format json` 時の挙動（exit 0/1 関係なく JSON は流す）の確認
3. `undo` / `redo` のインタラクション: `history::Store::head()` から op を取って kind ごとに dispatcher を呼ぶ。rename op は `Rename::apply` を inverse 入力で再呼、tag / meta_edit / import op はそれぞれ `core::meta` / `core::index` を触る。`history.undo()` の呼び出しタイミング（apply 後 / apply 前）と失敗時の対応
4. group_id-aware undo: `progest undo --group <id>` で当該 group のすべての entry を順次 undo する affordance を入れるか、`progest undo` を 1 回ずつ呼ぶ前提か
5. `progest doctor` の staging cleanup: rename failure 時に `.progest/local/staging/<uuid>/` に残骸が残る場合がある（rollback 失敗時など）。doctor が古い staging dir を検出して掃除する機能を入れるか、ユーザー手動か

旧 `core::rules` / `core::accepts` / `core::naming` / `core::history` / `core::rename` kickoff 質問は履歴参照。
6. `FillMode::Prompt` の TTY 実装場所: `core::rename` 側で interactive resolver を受け取り、core は trait だけ定義する形で良いか
7. 1 PR 粒度: 単一 feat branch で preview + apply + history 連携 + CLI + golden まで一括か、段階分割するか
8. failure injection テスト: FS rename 中の クラッシュ / permission denied / ディスク満杯など、property / failure-injection テストを最初から入れるか（既往 kickoff §3.4 で合意）

旧 `core::rules` / `core::accepts` / `core::naming` / `core::history` kickoff 質問は履歴参照。

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
- 2026-04-24: `core::rename` + `core::sequence` + `naming::HolePrompter` + CLI `rename` / `clean --apply` 一括 landed（feat/m2-core-rename）。`fs::FaultyFileSystem` decorator + proptest を最初に入れて apply の rollback 不変条件を property test で固定（5-op × 20 fault placements）。`Rename::apply` は 2-phase staging（`.progest/local/staging/<uuid>/`）+ rollback / FS rename と `.meta` rename を一体に / index update post-commit best-effort（`IndexWarning`）/ history `Operation::Rename` append と bulk auto group_id（per-op 優先、`outcome.group_id` が caller group を round-trip）。`core::sequence` は同 parent + stem + sep + padding + ext で group・gap 許容・min 2 members、`requests_from_sequence(seq, new_stem)` で stem 置換 RenameRequest 群を生成。`StdinHolePrompter` は generic `Read + Send`/`Write + Send` で stderr→prompt / stdin→入力（JSON pipe を壊さない）。CLI は `--mode preview|apply` / `--format text|json` / `--fill-mode skip|placeholder|prompt` / `--sequence-stem STEM` / `--from-stdin`、`clean --apply` は同じ apply path。core 39 + cli 5 smoke + 6 prompter test + property test 全 green。M2_HANDOFF §1 / §2 / §5 を CLI `lint` / `undo` / `redo` 着手向けに更新
