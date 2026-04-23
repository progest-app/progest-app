# CLAUDE.md

Claude Code 向けのプロジェクト作業指示。このリポジトリで作業する前に必ず通読すること。

---

## プロジェクト概要

Progest は、映像・ゲーム・3DCG・VFX 等のパイプライン系クリエイター向けプロジェクト・ファイル管理ツール。命名規則ファースト設計、sidecar メタデータ（`.meta`）、高速検索をローカル完結で提供する。対象は個人〜小規模スタジオ（5〜30人）。

正確な定義・機能スコープ・非機能要件は [docs/REQUIREMENTS.md](./docs/REQUIREMENTS.md)、実装計画は [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) を参照。

現在のフェーズ: **M2 Naming rules engine + accepts 進行中**。M1 Core data layer 完了、`core::rules`（DSL loader / template / constraint / inheritance / evaluate）と `core::accepts`（`[accepts]` / alias catalog / effective_accepts / placement lint）が landed。次は `core::naming`（機械的命名整理 pipeline / `progest clean` 基盤、REQUIREMENTS §3.5.5）→ `core::history`（操作ログ / undo/redo スタック）→ `core::rename`（preview / apply、原子トランザクション）→ CLI `lint` / `rename` / `undo` / `redo` / `clean`。進捗詳細は `docs/IMPLEMENTATION_PLAN.md §0`、次モジュールの kickoff 質問テンプレは `docs/M2_HANDOFF.md §5` 参照。

---

## 不明点の扱い（最重要）

**このリポジトリで作業する時、仕様・設計・UX の判断で少しでも迷ったら、実装を進める前に必ずユーザーに確認すること。**

理由:
- Progest はドメイン固有性が非常に高い（パイプライン現場のワークフロー、命名規則文化、meta 衝突の運用、DCC 連携の微妙な挙動）
- コード上の正しさだけで判断するとユーザーの期待から外れる
- 要件書はハイレベルな意思決定の記録。細部の UX や挙動は書ききれていない

確認すべき具体的な場面:
- 要件書・実装計画書に明示記載のない挙動
- 「こうあるべき」と思っても明示されていない UX 選択
- 既存の決定事項と衝突する設計課題が出た時
- 技術選定で複数候補があり一本化されていない時
- MVP スコープの解釈が分かれる時
- 命名規則の評価結果、merge driver の競合解決、watch の挙動など、ユーザーが見て驚きうる挙動
- ファイルを破壊的に扱う操作（rename, delete, move, .meta 書換）の条件

原則: **確認せずに「たぶんこうだろう」で進めない。短い問い合わせを頻繁に、が正解。**

ユーザーから「毎回聞かなくていい」と明示された領域のみ、自律的に判断してよい。

**確認時は `AskUserQuestion` ツールを優先使用する。** 選択肢を提示する形式の問いは平文より `AskUserQuestion` の方がユーザーが判断しやすい。自由記述が必要な時のみ平文で聞く。推奨案がある場合は最初の選択肢に `(Recommended)` を付ける。

---

## アーキテクチャ

モノレポ構成の at-a-glance とプラットフォーム優先度は [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) にまとめてある。詳細は [`docs/IMPLEMENTATION_PLAN.md`](./docs/IMPLEMENTATION_PLAN.md)。

**原則: ビジネスロジックをフロントエンド層に書かない。** 全てのロジックは `core` に集約し、CLI / Tauri / 将来の Lua 拡張で共有する。

---

## 重要な設計原則

### `.meta` が真実源
- SQLite index（`.progest/index.db`）は再構築可能なキャッシュ。整合性の基準は常に `.meta`
- index 破損時は削除して startup scan で再構築

### watch を信頼しない
- 三段構え: `startup full scan` + `OS watch (notify)` + `periodic reconcile (5分)` 
- watch は最速反映のヒントにすぎない
- watch 単独でインデックス更新しない。必ず reconcile で事後補正

### UUID はコピーで継承しない
- ファイル複製時は必ず新規 UUID 発行 + `source_file_id` に元 UUID を記録
- 同一 `file_id` が複数パスに現れたら即 conflict、UI で解決肢を提示

### 規則評価は説明可能であること
- どの規則が勝ったか（勝利 rule_id、継承チェーン）を常にトレース
- lint レポート・違反 UI に必ず表示

### 破壊的操作は必ず preview → confirm
- rename、bulk apply、merge resolution 全て
- undo history を N 件残す（デフォルト 20）

### `.meta` 書込は原子的
- temp file（`foo.psd.meta.tmp`）→ rename
- 失敗時は `.progest/local/pending/` にキュー、バックオフ再試行

### ignore は厳格に
- デフォルト: `.git/`, `node_modules/`, `.DS_Store`, `Thumbs.db`, `*.tmp`, DCC autosave 等
- `.progest/index.db`, `.progest/thumbs/`, `.progest/local/` は必ず gitignore
- `.progest/project.toml`, `rules.toml`, `schema.toml`, `views.toml`, `ignore` は git 共有

### パスは抽象化して扱う
- `std::path::Path` を直接使わず、`progest_core::fs::ProjectPath` 経由
- Windows 移植時（v1.1）の長パス・大小文字・UNC 対応をこの層で吸収

---

## コード規約

詳細は [`docs/CODING_STYLE.md`](./docs/CODING_STYLE.md) にある（Rust / TypeScript / コミット・PR 規約）。最低限の要点:

- `mise run check` グリーン必須（rustfmt + clippy `-D warnings` + tsc）
- **1 論理単位 = 1 コミット**。仕様変更 + テスト追加 + 無関係な修正を混ぜない
- commit message / PR は英語、Conventional Commits 推奨（`feat:` `fix:` `refactor:` `test:` `docs:` `chore:`）
- 新規ロジックには必ずテストを付ける。バグ修正時は失敗を再現するテストを先に書く
- IO は trait 越し（`FileSystem` / `MetaStore` / `Index`）、差し替え可能性を保つ

---

## ワークフロー

`mise` 前提。コマンド一覧とコミット前の check ルールは [`docs/WORKFLOW.md`](./docs/WORKFLOW.md) にまとめてある。

**非交渉: 全コミット前に `mise run check` グリーンを確認する。** 失敗状態で commit しない。

---

## 避けるべきこと

- **フロントエンドにビジネスロジックを書く**（全て core へ）
- **`.meta` の直接編集**（必ず `core::meta` API 経由、原子書込を壊さない）
- **watch イベントを真実源として扱う**（reconcile で補正する前提）
- **feature creep**（Lua、クラウド同期、Windows 固有層、lindera 等を v1 で先回り実装）
- **`.progest/index.db` を git 管理下に置く**
- **`.meta` にタイムスタンプ以外の衝突しやすいフィールドを追加する**
- **絶対パスの保存**（全てプロジェクトルート相対）
- **「たぶんこうだろう」で仕様外の判断をする**（ユーザーに確認）

---

## 学び・はまりどころ

過去セッションで踏んだ落とし穴と解決策は [`docs/LESSONS.md`](./docs/LESSONS.md) にモジュール・テーマ別に整理してある。**実装前に該当セクションを一読する。** 新しく気づいた落とし穴も LESSONS.md に追記する（CLAUDE.md には直接追記しない）。

---

## 現在の開発ステージ

進捗スナップショットは [`docs/IMPLEMENTATION_PLAN.md §0`](./docs/IMPLEMENTATION_PLAN.md)、M2 進行中の詳細な引き継ぎは [`docs/M2_HANDOFF.md`](./docs/M2_HANDOFF.md)。作業一区切りごとに両方を更新する。

---

## 参照すべきドキュメント

**仕様・計画**（作業前に必ず目を通す）:
- [docs/REQUIREMENTS.md](./docs/REQUIREMENTS.md) — 要件定義書（日本語）
- [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) — 実装計画・進捗スナップショット・マイルストーン・スキーマ
- [docs/M2_HANDOFF.md](./docs/M2_HANDOFF.md) — **M2 進行中の引き継ぎメモ**（次に触るモジュール順、focus areas、kickoff 質問テンプレ）

**リファレンス**（該当作業に入る時に参照）:
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — モノレポ構成 / プラットフォーム優先度
- [docs/CODING_STYLE.md](./docs/CODING_STYLE.md) — Rust / TypeScript / コミット・PR 規約
- [docs/WORKFLOW.md](./docs/WORKFLOW.md) — mise コマンド一覧 / コミット前の check 必須ルール
- [docs/LESSONS.md](./docs/LESSONS.md) — 学び・はまりどころ（モジュール別）
- [docs/_DRAFT.md](./docs/_DRAFT.md) — 初期ドラフト（歴史参考）

**ユーザー向け**:
- [README.md](./README.md) — 英語版 README
- [README.ja.md](./README.ja.md) — 日本語版 README

---

## 作業パターン

1. 着手前に該当セクションを REQUIREMENTS / IMPLEMENTATION_PLAN で確認
2. **仕様に書かれていない判断が必要なら、手を動かす前にユーザーに確認**
3. 小さく変更、先にテスト、**1 論理単位ごとに必ずコミット**
4. 破壊的操作（rename、delete、移動、git の force 系）は必ずユーザー確認
5. 終了時は変更内容と次にやるべきことを 1〜2 文で報告
6. **作業が一段落したらドキュメントの更新・整理を必ず行う。** 対象:
   - `docs/IMPLEMENTATION_PLAN.md §0` の進捗スナップショット
   - `docs/M2_HANDOFF.md` の進行中メモ（該当フェーズのみ）
   - 新しい落とし穴を `docs/LESSONS.md` の該当モジュールセクションに追記
   - 古くなった記述の削除

   本当に更新の必要がないと判断した場合（tiny な typo 修正、内部的な名前変更で影響なし等）はスキップ可 — その場合は「更新不要と判断した」と一言添える
