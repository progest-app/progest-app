# CLAUDE.md

Claude Code 向けのプロジェクト作業指示。このリポジトリで作業する前に必ず通読すること。

---

## プロジェクト概要

Progest は、映像・ゲーム・3DCG・VFX 等のパイプライン系クリエイター向けプロジェクト・ファイル管理ツール。命名規則ファースト設計、sidecar メタデータ（`.meta`）、高速検索をローカル完結で提供する。対象は個人〜小規模スタジオ（5〜30人）。

正確な定義・機能スコープ・非機能要件は [docs/REQUIREMENTS.md](./docs/REQUIREMENTS.md)、実装計画は [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) を参照。

現在のフェーズ: **M1 Core data layer 進行中**。M0 Skeleton 完了、`progest-core::fs` / `progest-core::identity` / `progest-core::meta`（TOML I/O + 原子書込）/ `progest-core::index`（SQLite schema + migration + files/tags CRUD）が landed（PR #3/#4/#5 merged、PR #6 in review）。`core::meta` の残タスク（`.progest/local/pending/` queue、`.dirmeta.toml` loader）は別 PR。次は `core::reconcile` が本線。

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

モノレポ構成:

| パッケージ | 役割 |
| --- | --- |
| `crates/progest-core` | ドメインロジック全て（meta I/O、FS、規則エンジン、index、search、watch、reconcile、thumbnail、template、AI クライアント、rename） |
| `crates/progest-cli` | CLI バイナリ。core を直接使用 |
| `crates/progest-merge` | `.meta` 用 git merge driver（単機能バイナリ） |
| `crates/progest-tauri` | Tauri IPC glue。薄層、core を呼ぶだけ |
| `app/` | React + shadcn/ui フロントエンド。Tauri IPC 経由で core にアクセス |

**ビジネスロジックをフロントエンド層に書かない。** UI は描画とユーザー入力の受け流しのみ。全てのロジックは core に集約する。理由: CLI、Lua 拡張（v2+）、将来のヘッドレス利用で同じロジックが使われるため。

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

## プラットフォーム優先度

| OS | v1.0 | 備考 |
| --- | --- | --- |
| macOS | 主対象 | Darwin 11+、FSEvents 経由 notify、notarization 必須 |
| Windows | 対象外（v1.1） | 長パス、ロック、rename 複数イベント、OneDrive Placeholder 対応を後で |
| Linux | ベストエフォート（v2+） | inotify 上限対応が必要 |

v1.0 は macOS だけを対象にビルド・テストする。ただし core のパス抽象・FS trait はクロスプラ前提で設計する。

---

## コード規約

### Rust
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` 必須
- アプリケーション層（CLI、Tauri glue）は `anyhow`、ライブラリ（core）は `thiserror`
- ロギングは `tracing`
- public API には doc comment 必須
- **テストは必ず書く。** 新規ロジックに対応するテストなしで PR を出さない。規則評価はゴールデンテスト、FS 操作は tempdir を使う統合テスト、パーサ類はプロパティテスト検討。バグ修正時は「失敗を再現するテスト」を先に書いてから修正する
- IO は trait 越し（`FileSystem`, `MetaStore`, `Index`）、差し替え可能性を保つ

### TypeScript
- `pnpm` workspace
- shadcn/ui コンポーネント起点、独自 UI 部品は最小限
- 状態管理: Tauri IPC を軸にしつつ、ローカル UI state は zustand、非同期は TanStack Query（`@tanstack/react-query`）
- IPC は型付きラッパー経由（手書きの `invoke` 禁止）
- emoji を UI に入れる場合はユーザーの明示許可があるときのみ

### コミット / PR
- **1 論理単位ごとに必ずコミットする。** 作業完了時にまとめてコミットしない。ロジック追加・リファクタ・テスト追加・スタイル修正はそれぞれ別コミット。「仕様変更 + テスト追加 + 無関係な typo 修正」が 1 コミットに混ざるのは禁止
- コミットせずに複数論理変更を積み上げない。次の変更に進む前にコミット
- **コミットメッセージと PR（タイトル・本文・レビューコメント）は英語で書く。** ユーザーとのチャットは日本語で構わないが、リポジトリに残る成果物（commit message, PR description, code comment）は原則英語。Conventional Commits 推奨（`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`）
- コミット本文は「なぜ」を書く。「何を」は diff で分かる
- 例外的にまとめたい場合（相互依存で段階分割不可 等）はユーザーに事前確認

---

## よく使うコマンド

**このリポジトリは mise を前提とする。** ツールチェーン（Rust / Node / pnpm）は `mise.toml` に固定、開発タスクも全て mise 経由で実行。素の `cargo` / `pnpm` も動くが、mise 経由にすると CI と完全同一の挙動になるので、迷ったら mise を使う。

```bash
mise install                    # 初回のみ（ツールチェーン導入）

mise run check                  # rustfmt --check + clippy -D warnings + tsc
mise run test                   # cargo test --workspace
mise run build                  # cargo build + vite build
mise run fmt                    # cargo fmt --all

mise run dev                    # Vite だけ起動（フロント反復用）
mise run tauri-dev              # デスクトップアプリ起動（Vite + Tauri）
mise run tauri-build            # リリースバンドル

mise run cli -- <args>          # progest CLI 実行（例: -- scan）
```

### コミット前に必ず通すこと

**全てのコミットの前に `mise run check` を実行し、グリーンであることを確認する。** これは CI と同じタスクを実行するローカルゲート。失敗している状態でコミットしない。

- fmt 違反 → `mise run fmt` で整形してから再 check
- clippy warning → 警告原因を修正する（`#[allow]` でごまかさない、本当に必要なら理由を doc comment で添えて局所適用）
- typecheck error → `any` でごまかさない、型を整える
- test が新規ロジックに対して存在しないときは check が通っても PR に進まない（テスト必須ルール）

check を通すのは成果物の最低条件であって品質保証ではない。通っているからといって設計判断・破壊性・仕様遵守の確認は省略しない。

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

## 学び・はまりどころ（過去セッションから）

足場作成中に踏んだ落とし穴と解決策。同じ穴を踏まないための備忘録。

### Tauri アイコンは実データが 128×128 RGBA を満たす必要
- `generate_context!()` は**ビルド時に**アイコンを埋め込み、tauri ランタイムが**起動時に**それを RGBA に decode する
- 「PNG ヘッダ上は 128×128 だが IDAT の実ピクセル数が足りない」placeholder を置くと起動時に `invalid icon: dimensions 128x128 don't match rgba pixel count` で panic
- 解決: `magick -size 128x128 canvas:'#xxx' PNG32:icon.png` のように実データが満たされる PNG を使う
- 場所: `crates/progest-tauri/icons/icon.png`

### Tauri 開発起動は必ず tauri CLI 経由
- `cargo run -p progest-tauri --bin progest-desktop` は **Vite 未起動のまま WebView が devUrl を叩きに行き真っ白画面**になる
- `tauri dev` が `beforeDevCommand`（Vite 起動）を実行 → port 1420 を待機 → アプリ起動、の三段を面倒見る
- 実行経路: `mise run tauri-dev` → `pnpm tauri:dev` → `tauri dev -c crates/progest-tauri/tauri.conf.json`

### tauri.conf.json の配置が非標準（`crates/progest-tauri/`）
- 通常の tauri プロジェクトは `src-tauri/tauri.conf.json` を自動検出
- 本プロジェクトはモノレポ都合で `crates/progest-tauri/` 配下
- 結果: すべての tauri CLI 呼び出しで `-c crates/progest-tauri/tauri.conf.json` が必須
- root `package.json` の `tauri`/`tauri:dev`/`tauri:build` スクリプトが config パスをラップしているので、手で tauri CLI を叩くときはこれらを経由する

### lefthook.yml の `{N}` プレースホルダはクォート必須
- `run: grep -qE "^Signed-off-by: " {1}` は YAML パーサに `{1}` が object 開始扱いされエラー
- 解決: `run: 'grep -qE "^Signed-off-by: " {1}'` のように run 値全体を single quote で囲む

### mise.toml と rustup の関係
- `rust-toolchain.toml` を置くだけでは、mise 未活性のシェルでは rustup のデフォルト（古いバージョン）が拾われる
- ローカル実行時は `mise exec -- cargo ...` を必ず挟む or mise activate でシェル統合する
- CI は `jdx/mise-action` が mise.toml を読むので気にしなくて良い

### pnpm の postinstall スキップ
- pnpm v10 はセキュリティ上 `esbuild` や `lefthook` の postinstall を**デフォルトでスキップ**する（"Ignored build scripts" 警告）
- 多くの場合は `pnpm exec <tool>` で動作するので approve 不要
- どうしても必要な場面（例: esbuild の native binary 切替）で `pnpm approve-builds` を検討

### Tauri v2 の Linux ビルド依存
- CI の ubuntu ランナーで tauri crate を clippy/build するには `libwebkit2gtk-4.1-dev`, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev` が必要
- `.github/workflows/ci.yml` にインストールステップが入っている
- ローカル Linux では mise では提供されないので apt で入れる

### `ignore` crate の `Gitignore::matched` はディレクトリ除外をカスケードしない
- `matched(path, is_dir)` は渡されたパス自身しか判定しない。例えば `.progest/` パターンが有効でも `.progest/index.db` に対して `matched` は `None` を返す
- ディレクトリ除外を子孫まで効かせるには `matched_path_or_any_parents(path, is_dir)` を使う
- 場所: `crates/progest-core/src/fs/ignore.rs::IgnoreRules::is_ignored`

### `std::path::Path::components()` は `foo//bar` を正規化する
- 多重スラッシュは `Path::components()` で自動的に 1 つに潰される。結果、`ProjectPath::new("foo//bar")` を `Path::components()` ベースだけで検証しても空セグメント違反を検出できない
- 文字列入力ベースの `new` 側で `raw.contains("//")` を事前チェックする
- 場所: `crates/progest-core/src/fs/path.rs::ProjectPath::new`

### clippy pedantic の doc_markdown は頭字語を「項目」扱いする
- doc コメントに `UUIDv7`, `RFC9562`, `DCC` 等の裸の頭字語を書くと `item in documentation is missing backticks` でエラー
- バッククオートで囲む（`` `UUIDv7` ``）か、英単語化する
- 通常の proper noun（`Tauri`、`macOS`）はセーフ、完全大文字トークンで要注意

### `io::Error::new(ErrorKind::Other, ...)` は clippy pedantic で禁止
- Rust 1.74+ の `io::Error::other(msg)` に置換される。clippy の `io_other_error` が `-D warnings` で拒否する
- `ErrorKind::Other` に特定の意味を持たせる他用途では `io::Error::new` で別の kind を指定する

### TOML round-trip で未知フィールドを保持する実装パターン
- `.meta` は git 同期でバージョン差のある teammate 間を行き来するので、現行ビルドが知らないキーを save で落とさないことが要件
- 解決: 既知フィールドを typed struct で宣言しつつ、各 struct に `#[serde(flatten)] pub extra: toml::Table` を足す。top-level だけでなく `[core]` 等のセクション内の未知キーまで拾える
- `toml` crate 0.8 では `flatten` + `toml::Table` の組み合わせが round-trip で期待通り動く（`toml_edit` の重量級 API を入れずに済む）
- 場所: `crates/progest-core/src/meta/document.rs::MetaDocument` と `CoreSection` / `NamingSection` / `TagsSection` / `NotesSection`

### serde の `with` module は `serialize(value: &T, ...)` を要求、clippy `ref_option` に引っかかる
- `#[serde(with = "my_mod")]` を `Option<FileId>` に当てると、`my_mod::serialize` の第一引数は仕様上 `&Option<FileId>` 固定
- clippy pedantic の `ref_option` は `Option<&T>` への書換えを要求してくるが、serde 契約と競合して書換え不可
- 妥協: `serialize` 関数だけに `#[allow(clippy::ref_option)]` をローカル付与し、理由コメント（「serde's `with` contract requires `&Option<T>`」）を添える
- 場所: `crates/progest-core/src/meta/document.rs::source_file_id_serde::serialize`

### lefthook の hook install はタイミング依存
- lefthook は `pnpm install` の postinstall で入るが、pnpm v10 は postinstall をスキップするので、明示的に `pnpm exec lefthook install` を走らせるまで `.git/hooks/` は空
- `mise run check` が都度 `lefthook install` を呼ぶので、check を一度も通していない状態で commit すると **DCO 署名チェックも pre-commit の fmt もすり抜けて commit が通ってしまう**
- 結果、一部 commit が DCO 未署名のまま積み上がる事故が起きる
- 予防: 新しい clone / session 冒頭で、最初の commit 前に必ず `mise run check` を一度走らせる

### 一度作った commit に GPG 署名を後付けする
- `commit.gpgsign = true` でも、GPG agent がロック中で pinentry が出ない非対話環境では署名されず、`%G?` が `N` の commit が生まれる
- 対処: ブランチを `git rebase -f -S main` で強制再適用すると全 commit が署名付きで書き換わる（`-f` は no-op rebase を強制、`-S` で signing）
- force push は `--force-with-lease` を使うこと（他セッションの push に上書きしない）
- ただし **main / master に対しては force push しない**（CLAUDE.md 規約と git hook で二重防衛）

### `rusqlite::Connection` は `Send` だが `Sync` ではない
- 内部に `RefCell<StatementCache>` を持つため、複数スレッドから `&Connection` を共有できない
- `pub trait Index: Send + Sync` を満たすには、`SqliteIndex` 内部で `Mutex<Connection>` として包んで interior mutability を提供する必要がある
- 副次効果として trait メソッドが全て `&self` になり、`FileSystem` / `MetaStore` と API style が揃う（`&mut self` だと `Arc<dyn Index>` で扱いづらい）
- SQLite 自体が内部でシリアライズするのでアプリ側 `Mutex` のオーバーヘッドは無視できる
- 場所: `crates/progest-core/src/index/store.rs::SqliteIndex`

### `PRAGMA foreign_keys` は接続ごと、かつデフォルト OFF
- `CREATE TABLE ... REFERENCES ... ON DELETE CASCADE` を書いても、接続で `PRAGMA foreign_keys = ON` を実行しないと **cascade が静かに無視される**（制約違反も検知されない）
- `SqliteIndex::init` で `conn.pragma_update(None, "foreign_keys", true)` を必ず呼ぶ
- Regression test: 不明な `file_id` で `tag_add` すると失敗することを確認する。これが通らないなら pragma が消えている
- `tags` の cascade delete が機能するかは doctor の orphan detection（M2+）の正しさに直結する

### Migration 冪等性は「再実行で壊れる設計」で間接的に担保
- `schema_version` row の `COUNT` を見るテストは書けるが、より強い保証は「もし migration が再実行されたら `CREATE TABLE files` が既存テーブル衝突で panic する」状態を保つこと
- つまり初期 migration の SQL に `IF NOT EXISTS` を**入れない**。壊れたら即 test が落ちる
- 場所: `crates/progest-core/src/index/migrations/0001_initial.sql`、検証は integration test `opening_an_existing_database_does_not_reapply_migrations`

### rusqlite の `row.get::<_, String>(...)` が `owned String` を返すが後で `&str` だけ使う場合
- clippy の `needless_pass_by_value` が厳しく、ヘルパ関数に `String` を渡して `.parse()` だけ呼ぶと拒否される
- 対処: `row.get::<_, String>()` で一度受けてから `&str` で下流ヘルパに渡す（`as_deref()` 等）
- `Option<String>` は `.as_deref()` で `Option<&str>` に変換してから `.map(str::parse::<...>).transpose()?` で parse

### SQL 文を埋め込む時は `const &str` に切り出す
- rustfmt は長いリテラル SQL の内側には手を入れないが、行数が嵩むと関数本体の他ロジックが読みにくくなる
- 複数の SELECT で同じカラム順を使い回すなら `const SELECT_COLUMNS: &str = "..."` に切り出して `format!("SELECT {SELECT_COLUMNS} FROM ...")` で合成すると、スキーマ変更時の更新箇所が 1 個所になる

---

## 現在の開発ステージ

**M1 Core data layer（進行中）**。M0 足場は出来上がり、`core::fs` / `core::identity` / `core::meta`（の最初のスライス）/ `core::index` が landed。

M0 完了済み:
- Cargo workspace（resolver v3、Rust 1.95、edition 2024）
- pnpm workspace（Vite + React 19 + TS の `app/`）
- Tauri v2 シェル（`crates/progest-tauri`、`pnpm tauri:dev` 可）
- CI（GitHub Actions、`mise run check` と `test`、macOS build on main push）
- mise.toml にタスク一式

M1 進捗:
- [x] `core::fs`（`ProjectPath`、`FileSystem` trait + `StdFileSystem`、`IgnoreRules`、`Scanner`、`MemFileSystem`）— PR #3
- [x] `core::identity`（`FileId` UUIDv7、`Fingerprint` blake3 128bit truncated、`compute_fingerprint(reader)`、`IdentityConflict`）— PR #4
- [~] `core::meta` — PR #5 で `MetaDocument` TOML schema（forward-compat round-trip）、`MetaStore` trait + `StdMetaStore`、`sidecar_path` が landed。以下は別 PR で残：
  - [ ] `.progest/local/pending/` への失敗書込キュー + バックオフ再試行
  - [ ] `.dirmeta.toml` loader（M2 の `core::accepts` が乗る土台）
- [~] `core::index` — PR #6 で migration runner（`schema_version` テーブルベース）、初期 schema（`files` + `tags` + indices）、`Index` trait + `SqliteIndex`（`Mutex<Connection>` で `&self` API）、files CRUD（`upsert_file` は `file_id` と `path` の両 unique key を `ON CONFLICT DO UPDATE` で吸収）、tag ops（idempotent add、cascade delete）が landed。以下は別 PR で残：
  - [ ] FTS5 virtual table（M3 search の土台）
  - [ ] `custom_fields` テーブル（M2 rules engine と同時で良い）
- [ ] `core::reconcile`（startup scan + periodic reconcile）
- [ ] `core::watch`（notify ラッパー）
- [ ] CLI `init`/`scan`/`doctor`
- [ ] M1 完了条件ベンチ（10k files < 5s）

未着手（M2 以降）:
- `progest-cli` のサブコマンド実体（現在は `todo!()`）
- `progest-merge` の実装（現在は `todo!()`）
- shadcn/ui 導入（M3）
- アイコン（placeholder のまま）、署名、配布（M5）

着手順は [docs/IMPLEMENTATION_PLAN.md §5](./docs/IMPLEMENTATION_PLAN.md) のマイルストーンに従う。

---

## 参照すべきドキュメント

作業前に必ず目を通す:
- [docs/REQUIREMENTS.md](./docs/REQUIREMENTS.md) — 要件定義書（日本語）
- [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) — 実装計画・マイルストーン・スキーマ
- [docs/_DRAFT.md](./docs/_DRAFT.md) — 初期ドラフト（歴史参考）

ユーザー向け:
- [README.md](./README.md) — 英語版 README
- [README.ja.md](./README.ja.md) — 日本語版 README

---

## 作業パターン

1. 着手前に該当セクションを REQUIREMENTS / IMPLEMENTATION_PLAN で確認
2. **仕様に書かれていない判断が必要なら、手を動かす前にユーザーに確認**
3. 小さく変更、先にテスト、**1 論理単位ごとに必ずコミット**
4. 破壊的操作（rename、delete、移動、git の force 系）は必ずユーザー確認
5. 終了時は変更内容と次にやるべきことを 1〜2 文で報告
