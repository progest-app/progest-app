# Refactor Backlog

実装に直接関わらないが、整理しておきたい候補の置き場。**やる前に確認、やったら消す、新しく見つけたら追記**。

着手前に [`CLAUDE.md`](../CLAUDE.md) の「不明点の扱い」と「破壊的操作は必ず preview → confirm」を念押し。

最終更新: 2026-04-25

---

## 凡例

- **コスト**: small / medium / large（実装行数 + テスト書き換え量の合算ざっくり感）
- **タイミング**: now / M3-併合 (search/views) / M4-併合 (import) / future（事象が起きてから）

---

## A. CLI 構造 (`crates/progest-cli/`)

### A-1. clap enum 命名揺れの最終整理 (1-3)

- 残り: `RenameMode { Preview, Apply }` だけが per-command。揺れ感は低い。
- コスト: small
- タイミング: future（追加コマンドが揺れを増やしたタイミング）

### A-2. exit code 規約の集約 (1-4)

- `main.rs::to_exit_code` と subcommand の `Result<i32>` / `Result<ExitCode>` 混在。`doctor` だけ `ExitCode` 直返し。
- 案: `CommandExit { Success, Strict, EvalError(message) }` enum で集約。
- コスト: small
- タイミング: M3 で search コマンド追加時、または `doctor` のリッチ化時

### A-3. CLI Args 構造の共通化

- `LintArgs` / `CleanArgs` / `RenameArgs` / `UndoRedoArgs` がそれぞれ `paths` / `format` を持つ。
- 案: clap の `#[command(flatten)]` で `CommonArgs { paths, format }` を共有、または trait で getter を共通化。
- コスト: small
- タイミング: future（パターンが 5+ コマンドに増えたら）

### A-4. CLI module の細分化 (1-6)

- `clean.rs` / `rename.rs` が単一ファイル内で flag / args / preview / apply / emit を全部抱える（300〜500 行）。
- 案: `cmd/{clean,rename}/{mod, args, output, apply}.rs` 風に分割。
- コスト: medium
- タイミング: M3 で search が入って同パターンを再現する前に

---

## B. core wire types (`crates/progest-core/`)

### B-1. Violation の serde 整理 (2-2)

- `Violation` に `placement_details: Option<PlacementDetails>` があるが naming/sequence では常に None。
- 案: `NameViolation` / `PlacementViolation` / `SequenceViolation` の enum 化。または `#[serde(flatten)]` で variant 別に出す。
- コスト: medium（wire format 変更）
- タイミング: M4 import の Violation 拡張と合わせて (B-3 と一緒にやるのが理想)

### B-2. RuleSource ↔ AcceptsSource の階層統一 (2-3)

- `RuleSource { Own, Inherited, ProjectWide }` と `AcceptsSource { Own, Inherited }` が同じ概念で別名。
- 案: `Source { layer: Layer, distance: Option<u16> }` 共通型。internal API 限定なら wire 互換は保てる。
- コスト: medium
- タイミング: M4-併合（accepts の `suggested_destinations` 実装時に触る、import kickoff と同時）

### B-3. Conflict ↔ Warning 語彙整理 (2-4)

- すでに [`docs/M2_HANDOFF.md §5.5.7`](./M2_HANDOFF.md) で M4 持ち越し決定。詳細はそちらに。
- `ApplyOutcome` の `{ conflicts, warnings }` 構造化 / `Issue` enum 化を含めて core::import の Conflict variants と一緒に再設計。
- コスト: medium
- タイミング: M4 `core::import` kickoff

### B-4. ApplyWarning に ImportWarning variant (B-3 の前段階)

- すでに [`docs/M2_HANDOFF.md §5.5.5`](./M2_HANDOFF.md) で M4 持ち越し決定。
- `ApplyWarning` enum (現 `IndexUpdate` / `HistoryAppend`) に `ImportFailed { src, dest, message }` を追加。
- コスト: small
- タイミング: M4 `core::import` 着手時

### B-5. Module re-export 整理 (2-5)

- `accepts/mod.rs` / `rules/mod.rs` / `rename/mod.rs` の `pub use` ネスト。`naming::Hole` (top) vs `naming::types::Hole` (内部) の不揃いは別途修正済 (post-M2 refactor)。
- 案: 各 mod.rs の re-export を一段整理、internal は `__private::` などで隔離。
- コスト: small
- タイミング: future（実害なし、掃除レベル）

### B-6. Severity ↔ Mode 双方向変換 (2-6)

- 現在 `Mode::violation_severity()` のみ。逆方向 `Severity::from_mode` は未実装。
- 案: 双方向変換を 1 箇所に集約。
- コスト: small
- タイミング: future（逆方向の need が出てから）

---

## C. テスト周り

### C-1. reconcile_flow の Harness を共通化

- post-M2 で見送ったが、second user が出れば promote 候補。
- 候補: M4 import の integration test、watch_flow の Harness 化など。
- コスト: small
- タイミング: future（second consumer 出現時）

### C-2. golden fixture 命名規則の明文化 (3-4)

- audit で agent が `tests/FIXTURE_CONVENTIONS.md` 作成を提案。実害は出ていない。
- コスト: small
- タイミング: future（fixture 数が倍増したら）

---

## D. 軽微なクリーンアップ (post-M2 で実施したものを除く)

ここに溜まったら 1 commit でまとめて掃除する。

(現時点では空)

---

## 履歴

- 2026-04-25: 初版作成。post-M2 リファクタ (PR #26) 完了時に audit 残り + 今回気づいた候補を集約。
- 2026-04-25: M3 スコープ整合に合わせて B-2 / B-3 / B-4 / C-1 のタイミング表記と凡例の `M3-併合` を `M4-併合` に修正（IMPLEMENTATION_PLAN §5 が「M3 = 検索とビュー / M4 = import + thumbnail + AI + template」のため）。
