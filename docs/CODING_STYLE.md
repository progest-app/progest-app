# Coding Style

Progest のコード規約。このリポジトリに commit する全ての diff はここに従う。PR レビューで最初に当たる観点でもある。

---

## Rust

- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` 必須
- アプリケーション層（CLI、Tauri glue）は `anyhow`、ライブラリ（core）は `thiserror`
- ロギングは `tracing`
- public API には doc comment 必須
- **テストは必ず書く。** 新規ロジックに対応するテストなしで PR を出さない。規則評価はゴールデンテスト、FS 操作は tempdir を使う統合テスト、パーサ類はプロパティテスト検討。バグ修正時は「失敗を再現するテスト」を先に書いてから修正する
- IO は trait 越し（`FileSystem`, `MetaStore`, `Index`）、差し替え可能性を保つ

---

## TypeScript

- `pnpm` workspace
- shadcn/ui コンポーネント起点、独自 UI 部品は最小限
- 状態管理: Tauri IPC を軸にしつつ、ローカル UI state は zustand、非同期は TanStack Query（`@tanstack/react-query`）
- IPC は型付きラッパー経由（手書きの `invoke` 禁止）
- emoji を UI に入れる場合はユーザーの明示許可があるときのみ

---

## コミット / PR

- **1 論理単位ごとに必ずコミットする。** 作業完了時にまとめてコミットしない。ロジック追加・リファクタ・テスト追加・スタイル修正はそれぞれ別コミット。「仕様変更 + テスト追加 + 無関係な typo 修正」が 1 コミットに混ざるのは禁止
- コミットせずに複数論理変更を積み上げない。次の変更に進む前にコミット
- **コミットメッセージと PR（タイトル・本文・レビューコメント）は英語で書く。** ユーザーとのチャットは日本語で構わないが、リポジトリに残る成果物（commit message, PR description, code comment）は原則英語。Conventional Commits 推奨（`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`）
- コミット本文は「なぜ」を書く。「何を」は diff で分かる
- 例外的にまとめたい場合（相互依存で段階分割不可 等）はユーザーに事前確認
