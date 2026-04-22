# M2 引き継ぎメモ

M1 完了後に M2 を進めるための、次セッション向けハンドオフ。
**M2 に関わる実装に入る前に必ず目を通す。** 進捗に合わせて更新していく。

最終更新: 2026-04-22（M2 opener = `core::meta` 残タスク landed、`core::rules` 未着手）

---

## 1. 現在地

- M1 Core data layer 完了。全モジュール + CLI `init`/`scan`/`doctor` + 10k scan ベンチ（実測 ~82 ms）landed。
- M2 opener: `core::meta` の残タスク（pending queue + `.dirmeta.toml` loader）landed（PR #12）。
- 次: `core::rules` 本体。DSL パーサ → eval → template の順。

詳細は [`CLAUDE.md` 現在の開発ステージ](../CLAUDE.md#現在の開発ステージ) と [`docs/IMPLEMENTATION_PLAN.md` §5 M2](./IMPLEMENTATION_PLAN.md) を参照。

---

## 2. 次に着手する順序（推奨）

| # | モジュール | 依存 | メモ |
| --- | --- | --- | --- |
| 1 | `core::rules` | なし | M2 の中核。大きめ。DSL パーサ / 制約 eval / テンプレート生成 / rule_id trace を段階的に。**最初のスライスで DSL 構文を確定させる PR を切るかどうか kickoff で要相談**。 |
| 2 | `core::accepts` | `.dirmeta.toml` loader | `document.extra.get("accepts")` を typed にして effective_accepts 計算 / placement lint / インポート先ランキング。組み込みエイリアス（`:image`, `:video`, ...）の構成拡張子をここで確定し `docs/` に記載。 |
| 3 | `core::history` | なし | 操作ログ（rename/tag/meta_edit/import）、inverse 生成、undo/redo スタック。JSON Lines を `.progest/local/history.json` にappend 予定（要確認）。 |
| 4 | `core::rename` | rules + history | preview → apply、原子トランザクション、undo 連携。preview モデル（Vec<RenameOp>）と apply 時の `.meta` 同時更新 / FS rename の十字結びが肝。 |
| 5 | CLI `lint` / `rename --preview|--apply` / `undo` / `redo` | 上記 | 入力 format は json|text、exit code 割り当ては 4 モードに合わせる。 |

---

## 3. ユーザーが「丁寧に見たい」と事前指定した領域（focus areas）

kickoff 時に必ず AskUserQuestion で個別に詰める。

### 3.1 DSL 構文の確定（template / constraints の実格子）

- REQUIREMENTS §3.4 に `{prefix}_{seq:03d}` 等の例はあるが、
  - 許容されるプレースホルダーの完全集合（`seq` / `prefix` / `ext` / タグ / カスタムフィールド参照）
  - フォーマット指定子（`:03d`, `:lower`, ...）
  - 制約 DSL の演算子（`charset=`, `casing=`, `max_len=`, `forbidden=`, ...）
  - ルール継承順と上書き可否
  - 複数ルールが同じ要素に当たった時の勝敗判定
- いずれも明文化待ち。**`core::rules` の parser に手を付ける前に、仕様を `docs/NAMING_RULES_DSL.md` として user 承認つきで起こす PR を先行させたい**。

### 3.2 rule_id trace の実装方針

- 「どの規則が勝ったか」「継承チェーンは何か」を常に返す
- 表現候補:
  - 1 評価 = `Vec<RuleHit>`（rule_id, source=own|inherited, decision=match|reject, explanation）
  - printable tree / JSON 両対応
- lint レポートにも `progest lint --explain` にも載る
- 疑問点: 大量ファイルで trace を全件保持するメモリ量。lint 全実行で `Vec<Trace>` を作ると OOM 懸念 → streaming iterator か、違反のあるファイルだけ詳細 trace か。

### 3.3 4 モード（strict / warn / hint / off）の振る舞い

- 仕様:
  - `strict`: 違反は常にエラー、CLI exit 1、UI ブロック
  - `warn` (default): 違反は警告、CLI exit 0 だが stderr、UI バッジ
  - `hint`: UI にだけ出る、CLI レポートには出ない
  - `off`: 評価しない
- 要確認:
  - CLI exit code: `strict=1 / warn=0` の他、placement モードを `warn` と別のカテゴリとしてどう混ぜるか（naming=strict かつ placement=warn の時の総合 exit はどうすべきか）
  - `--format json` 時に 4 モードをどう露出するか（`severity: "strict"` とか）
  - `.progest/rules.toml` 側でのモード指定文法

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

- **ドキュメント更新**。各 PR 完了時に CLAUDE.md の M2 進捗チェックボックス、学び・はまりどころ、古くなった記述の削除までセット。
- **`docs/IMPLEMENTATION_PLAN.md` の M2 完了条件**: 「fixture プロジェクトに対して lint が naming / placement 両方の違反を期待通り検出、rename preview と apply が動く、undo で戻せる」。M2 終わりに改めて確認。
- **`core::meta` の schema_version**: M2 で新フィールドを足す場合は基本 `extra: Table` に載せて SCHEMA_VERSION は据え置く。構造的な互換破壊時のみ bump。
- **破壊的操作は必ず preview → confirm**: rename, bulk apply, merge resolution 全て。undo history を N 件残す（デフォルト 20）— M2 の仕様にも書かれているので `core::history` 実装時に担保。
- **progest-merge**（`.meta` 用 git merge driver）は M2 範疇ではない（M4）。Aware しておくが今は触らない。

---

## 5. 次セッション housekeeping — CLAUDE.md の分割

`core::rules` に入る前に、CLAUDE.md（約 420 行）をスリムにしておきたい。合意済みの方針:

- **CLAUDE.md に残す（薄い Claude 用マニュアル）**: プロジェクト概要 / 不明点の扱い / 重要な設計原則（要約） / 避けるべきこと / 作業パターン / 参照すべきドキュメント
- **切り出し先**:
  - `docs/ARCHITECTURE.md` ← モノレポ構成 / モジュール役割 / プラットフォーム優先度
  - `docs/CODING_STYLE.md` ← Rust / TypeScript / コミット・PR 規約
  - `docs/WORKFLOW.md` ← mise / `mise run check` 必須ルール / よく使うコマンド
  - `docs/LESSONS.md` ← 学び・はまりどころ（一番肥大、モジュール別に再構成推奨）
- **進捗トラッカーは `IMPLEMENTATION_PLAN.md` に寄せる**。`STATUS.md` は作らない
- ブランチ名: `docs/claude-md-restructure`
- 各切り出しを 1 コミットずつ、最後に CLAUDE.md を薄くする 1 コミット
- 切り出し時にリンク切れが出やすいので、grep で `CLAUDE.md#<アンカー>` と `学び・はまりどころ` 等の固有フレーズを確認してから push

このハウスクリーニングが終わってから `core::rules` の kickoff に入る。

---

## 6. M2 着手時の最初の kickoff 質問テンプレ

次に `core::rules` を始める時、kickoff でこれを聞く想定:

1. スコープ: DSL 構文確定 docs 先行 / parser 先行 / eval + template まで一気通貫
2. 粒度: サブ commit 数、PR 数
3. DSL 構文ドラフトは `docs/NAMING_RULES_DSL.md` に書き起こしてユーザー承認を取るか
4. 既往 PR と同じ feat branch + squash merge で良いか

本ドキュメントが参考資料として kickoff で引用されることを想定している。更新漏れは即 kickoff の質問から確認する。

---

## 7. 履歴

- 2026-04-22: 初版作成（M1 完了 / M2 opener landed のタイミング）
- 2026-04-22: §5 に CLAUDE.md 分割の次セッション housekeeping を追加
