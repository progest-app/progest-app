# M2 引き継ぎメモ

M1 完了後に M2 を進めるための、次セッション向けハンドオフ。
**M2 に関わる実装に入る前に必ず目を通す。** 進捗に合わせて更新していく。

最終更新: 2026-04-23（`core::accepts` 本体 landed、次は `core::naming`）

---

## 1. 現在地

- M1 Core data layer 完了。全モジュール + CLI `init`/`scan`/`doctor` + 10k scan ベンチ（実測 ~82 ms）landed。
- M2 opener: `core::meta` の残タスク（pending queue + `.dirmeta.toml` loader）landed（PR #12）。
- DSL 仕様書: [`docs/NAMING_RULES_DSL.md`](./NAMING_RULES_DSL.md) landed。
- `core::rules` 本体 landed（feat/m2-core-rules）: loader（forward-compat schema gate）/ applies_to（glob + specificity）/ template parser & matcher / constraint evaluator / inheritance（full-replace override）/ evaluate + `RuleHit` trace。unit tests ~260 + §10 golden + Codex 指摘 5 件のホットフィックス + regression golden。未着手拡張（suggested_names / §6 seq 採番 / trace `NotApplicable` / Regex::new キャッシュ化 / §1.3・§4・§5・§7・§9 の golden 拡充）は follow-up issue。
- `core::accepts` 本体 landed（feat/m2-core-accepts）: [accepts] TOML 抽出 / builtin alias catalog（v1 の 7 種、`docs/ACCEPTS_ALIASES.md` で拡張子確定）/ project alias loader (`.progest/schema.toml`) / effective_accepts 計算（inherit union + own/inherited provenance）/ placement lint（`category=placement`, `rule_id=placement`, `PlacementDetails` 充填）/ 7 シナリオ × 12 golden。Codex レビューで `:image` → svg/psb/dds、`:video` から prores 削除、`:text` から svg 移動などが反映済み。`suggested_destinations` は follow-up で充填。
- 次: `core::naming`。AI 非依存の機械的命名整理 pipeline（`remove_copy_suffix` → `remove_cjk` → `convert_case`）。`heck` crate 追加、`core::rules::template` の private case fn を `core::naming::case` へ移管（PascalCase→snake_case 挙動を修正する旨の golden 更新含む）、NameCandidate（literal + 穴）モデル、fill-mode（prompt/placeholder/skip）、`.progest/project.toml [cleanup]` loader、violation.suggested_names[] の機械的充填、CLI `progest clean`。設計合意は REQUIREMENTS §3.5.5。

詳細は [`docs/IMPLEMENTATION_PLAN.md §0`](./IMPLEMENTATION_PLAN.md)（進捗スナップショット）と [`§5 M2`](./IMPLEMENTATION_PLAN.md) を参照。

---

## 2. 次に着手する順序（推奨）

| # | モジュール | 依存 | メモ |
| --- | --- | --- | --- |
| 1 | `core::rules` | [NAMING_RULES_DSL.md](./NAMING_RULES_DSL.md) | **landed**（feat/m2-core-rules）。Codex レビューで挙がった 5 件の仕様乖離は同 PR 内で修正済。未着手の拡張（suggested_names / §6 seq 採番 / trace 強化 / perf）は follow-up issue に切り出した。 |
| 2 | `core::accepts` | `.dirmeta.toml` loader + rules | **landed**（feat/m2-core-accepts）。`[accepts]` 抽出 + alias catalog + effective_accepts + placement lint。`suggested_destinations` ランキングと `[extension_compounds]` project loader は follow-up issue。 |
| 3 | `core::naming` | heck crate 追加 / rules template との連携 | AI 非依存の機械的命名整理。pipeline（remove_copy_suffix → remove_cjk → convert_case）、NameCandidate（literal + 穴）モデル、fill-mode、`[cleanup]` loader、rules の `{field:snake}` 等も `core::naming::case` 経由に切り替え（PascalCase 境界を含む正しい変換）、violation.suggested_names[] を機械的に充填。`core::rename` の「candidate 入力」口を規定する。 |
| 4 | `core::history` | なし | 操作ログ（rename/tag/meta_edit/import）、inverse 生成、undo/redo スタック。JSON Lines を `.progest/local/history.json` にappend 予定（要確認）。 |
| 5 | `core::rename` | rules + naming + history | preview → apply、原子トランザクション、undo 連携。preview モデル（Vec<RenameOp>）と apply 時の `.meta` 同時更新 / FS rename の十字結びが肝。候補名は naming の NameCandidate から供給、fill-mode（prompt/placeholder/skip）で穴を解消してから apply。 |
| 6 | CLI `lint` / `rename --preview|--apply` / `undo` / `redo` / `clean` | 上記 | 入力 format は json|text、exit code 割り当ては 4 モードに合わせる。placement 違反は `rules` と同じ Violation 形状で出るので一体で出力可能。`progest clean` は naming を直接呼び、単体 rename（rule 不在でも走る）として機能する。 |

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

## 5. 次モジュール（`core::naming`）kickoff 質問テンプレ

`core::naming` 着手時に AskUserQuestion で確認する想定:

1. NameCandidate のデータ構造: `enum Segment { Literal(String), Hole { kind, pos, origin } }` を `Vec<Segment>` で持つ案で良いか。Hole に「元の日本語文字列」をメタ情報として保持するか（UI 表示用）
2. `heck` 依存追加で十分か、Progest 固有の post-processing（バージョン token `v01` 保護、NFC 正規化、`.meta` stem とファイル名 stem の整合）を同時に入れるか
3. `.progest/project.toml [cleanup]` の key 設計。案: `convert_case = "snake"` / `remove_cjk = false` / `remove_copy_suffix = false`。disable を明示する key にするか、enum にするか
4. `progest clean` の CLI フラグ詳細: `--case` / `--strip-cjk` / `--strip-suffix` の個別 toggle で十分か、preset（`--preset strict|light|none`）も切るか。`--apply` の破壊的実行（preview の対義）を今回実装するか、`progest rename` に委ねるか
5. 穴の sentinel 表記。case 変換後の見栄え（`output_⟨cjk-1⟩_v01.png` は読みにくくならないか）を確認
6. `core::rules::template` の `{field:snake}` 等を `core::naming::case` 経由に切り替える際、既存 golden の更新範囲（PascalCase 入力を含むテストがあれば挙動変更）と regression 対策
7. 1 PR 粒度: 単一 feat branch で lib + CLI + golden + doc 更新まで一括か、lib / CLI を分けるか

旧 `core::rules` / `core::accepts` kickoff 質問は履歴参照。`core::history` / `core::rename` の kickoff テンプレは naming landed 後に差し替える。

---

## 6. 履歴

- 2026-04-22: 初版作成（M1 完了 / M2 opener landed のタイミング）
- 2026-04-22: CLAUDE.md 分割完了（`docs/ARCHITECTURE.md` / `CODING_STYLE.md` / `WORKFLOW.md` / `LESSONS.md` / 進捗スナップショットを `IMPLEMENTATION_PLAN.md §0` へ）。該当セクションは本ドキュメントから削除
- 2026-04-23: DSL 仕様書 `docs/NAMING_RULES_DSL.md` landed。§3.1（DSL 構文未確定）/ §3.2（trace 方針）/ §3.3（4 モード）/ §4（undo 件数）を仕様確定版に差し替え、kickoff テンプレから DSL 確定論点を外した
- 2026-04-23: `core::rules` 本体 landed（feat/m2-core-rules）。Codex レビューで検出した 5 件の仕様乖離を同 PR で修正（loader forward-compat / `{field:}` spec 検証 / `{ext}` compound 最長一致 / `required_prefix` を stem 判定 / §5.7 NFC）。残タスクは follow-up issue に切り出し済み。M2_HANDOFF §1 / §2 / §5 を `core::accepts` 着手向けに更新
- 2026-04-23: `core::accepts` 本体 landed（feat/m2-core-accepts）。`docs/ACCEPTS_ALIASES.md` 初版で builtin alias の拡張子集合を確定（Codex レビューで `svg` を `:image` へ移動、`prores`/`fcpbundle` 除外、`psb`/`dds`/`vdb` 追加など）。`Violation` に `placement_details` を追加して naming と一体で運べる形に。`suggested_destinations` / import ランキング / `[extension_compounds]` project loader は follow-up issue。M2_HANDOFF §1 / §2 / §5 を `core::history` 着手向けに更新
- 2026-04-23: `core::naming`（AI 非依存の機械的命名整理）を Phase 2.5 として M2 へ挿入。`heck` crate 差し替え、pipeline（remove_copy_suffix → remove_cjk → convert_case、固定正規順序・stage ごと on/off）、NameCandidate（literal + 穴）モデル、fill-mode（`prompt`/`placeholder[:STR]`/`skip`、穴付き文字列がディスクへ出ないことを保証）、`.progest/project.toml [cleanup]` loader、violation.suggested_names[] 機械的充填、CLI `progest clean` の設計合意。REQUIREMENTS §3.5.5 / IMPLEMENTATION_PLAN §0・§5 / M2_HANDOFF §1・§2・§5 を naming 着手向けに更新。実装は別ブランチで後日
