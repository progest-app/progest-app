# Development Workflow

Progest のローカル開発フロー。**このリポジトリは mise を前提とする。** ツールチェーン（Rust / Node / pnpm）は `mise.toml` に固定、開発タスクも全て mise 経由で実行。素の `cargo` / `pnpm` も動くが、mise 経由にすると CI と完全同一の挙動になるので、迷ったら mise を使う。

---

## よく使うコマンド

```bash
mise install                    # 初回のみ（ツールチェーン導入）

mise run check                  # rustfmt --check + clippy -D warnings + tsc
mise run test                   # cargo test --workspace
mise run bench                  # cargo bench --workspace（例: M1 の 10k scan ゲート）
mise run build                  # cargo build + vite build
mise run fmt                    # cargo fmt --all

mise run dev                    # Vite だけ起動（フロント反復用）
mise run tauri-dev              # デスクトップアプリ起動（Vite + Tauri）
mise run tauri-build            # リリースバンドル

mise run cli -- <args>          # progest CLI 実行（例: -- scan）
```

---

## コミット前に必ず通すこと

**全てのコミットの前に `mise run check` を実行し、グリーンであることを確認する。** これは CI と同じタスクを実行するローカルゲート。失敗している状態でコミットしない。

- fmt 違反 → `mise run fmt` で整形してから再 check
- clippy warning → 警告原因を修正する（`#[allow]` でごまかさない、本当に必要なら理由を doc comment で添えて局所適用）
- typecheck error → `any` でごまかさない、型を整える
- test が新規ロジックに対して存在しないときは check が通っても PR に進まない（テスト必須ルール）

check を通すのは成果物の最低条件であって品質保証ではない。通っているからといって設計判断・破壊性・仕様遵守の確認は省略しない。

---

## 参考

- 学び・はまりどころ（実装時の注意点）: [`docs/LESSONS.md`](./LESSONS.md)
- コード規約: [`docs/CODING_STYLE.md`](./CODING_STYLE.md)
