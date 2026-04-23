# M2 引き継ぎメモ

M1 完了後に M2 を進めるための、次セッション向けハンドオフ。
**M2 に関わる実装に入る前に必ず目を通す。** 進捗に合わせて更新していく。

最終更新: 2026-04-23（DSL 仕様書 `docs/NAMING_RULES_DSL.md` landed、`core::rules` parser 着手可）

---

## 1. 現在地

- M1 Core data layer 完了。全モジュール + CLI `init`/`scan`/`doctor` + 10k scan ベンチ（実測 ~82 ms）landed。
- M2 opener: `core::meta` の残タスク（pending queue + `.dirmeta.toml` loader）landed（PR #12）。
- DSL 仕様書: [`docs/NAMING_RULES_DSL.md`](./NAMING_RULES_DSL.md) landed。プレースホルダー集合 / フォーマット指定子 / 制約フィールド / 継承・override / specificity / 4 モード × exit code / AND 合成 / worked examples まで規定済み。parser / evaluator はこの文書と bit-for-bit 一致させる。
- 次: `core::rules` 本体。DSL パーサ → eval → template の順。

詳細は [`docs/IMPLEMENTATION_PLAN.md §0`](./IMPLEMENTATION_PLAN.md)（進捗スナップショット）と [`§5 M2`](./IMPLEMENTATION_PLAN.md) を参照。

---

## 2. 次に着手する順序（推奨）

| # | モジュール | 依存 | メモ |
| --- | --- | --- | --- |
| 1 | `core::rules` | [NAMING_RULES_DSL.md](./NAMING_RULES_DSL.md) | M2 の中核。DSL 仕様書に沿って parser → eval → template → rule_id trace を段階的に。仕様書の §10 worked examples を golden test fixture と 1:1 対応させる。 |
| 2 | `core::accepts` | `.dirmeta.toml` loader | `document.extra.get("accepts")` を typed にして effective_accepts 計算 / placement lint / インポート先ランキング。組み込みエイリアス（`:image`, `:video`, ...）の構成拡張子をここで確定し `docs/` に記載。 |
| 3 | `core::history` | なし | 操作ログ（rename/tag/meta_edit/import）、inverse 生成、undo/redo スタック。JSON Lines を `.progest/local/history.json` にappend 予定（要確認）。 |
| 4 | `core::rename` | rules + history | preview → apply、原子トランザクション、undo 連携。preview モデル（Vec<RenameOp>）と apply 時の `.meta` 同時更新 / FS rename の十字結びが肝。 |
| 5 | CLI `lint` / `rename --preview|--apply` / `undo` / `redo` | 上記 | 入力 format は json|text、exit code 割り当ては 4 モードに合わせる。 |

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

## 5. M2 着手時の最初の kickoff 質問テンプレ

次に `core::rules` を始める時、kickoff でこれを聞く想定:

1. スコープ: parser だけ / parser + eval まで / テンプレート suggest まで一気通貫
2. 粒度: サブ commit 数、PR 数。DSL 仕様書 §4/5/7/8 の章単位での段階 PR 切りが想定候補
3. golden test fixture の配置（`crates/progest-core/tests/rules_golden/` 直下に章ごとか、worked example ごとに 1 dir か）
4. 既往 PR と同じ feat branch + squash merge で良いか

本ドキュメントが参考資料として kickoff で引用されることを想定している。更新漏れは即 kickoff の質問から確認する。

---

## 6. 履歴

- 2026-04-22: 初版作成（M1 完了 / M2 opener landed のタイミング）
- 2026-04-22: CLAUDE.md 分割完了（`docs/ARCHITECTURE.md` / `CODING_STYLE.md` / `WORKFLOW.md` / `LESSONS.md` / 進捗スナップショットを `IMPLEMENTATION_PLAN.md §0` へ）。該当セクションは本ドキュメントから削除
- 2026-04-23: DSL 仕様書 `docs/NAMING_RULES_DSL.md` landed。§3.1（DSL 構文未確定）/ §3.2（trace 方針）/ §3.3（4 モード）/ §4（undo 件数）を仕様確定版に差し替え、kickoff テンプレから DSL 確定論点を外した
