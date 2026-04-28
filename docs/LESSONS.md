# 学び・はまりどころ

過去セッションで踏んだ落とし穴と解決策。同じ穴を踏まないための備忘録。モジュール・テーマ別に整理してあり、grep でも辿りやすくしてある。

新しく気づいた落とし穴は該当セクションの末尾に追記する。セクションが見当たらない場合は迷わず新設する。

---

## 1. セットアップ・ツールチェーン

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

### lefthook の hook install はタイミング依存
- lefthook は `pnpm install` の postinstall で入るが、pnpm v10 は postinstall をスキップするので、明示的に `pnpm exec lefthook install` を走らせるまで `.git/hooks/` は空
- `mise run check` が都度 `lefthook install` を呼ぶので、check を一度も通していない状態で commit すると **DCO 署名チェックも pre-commit の fmt もすり抜けて commit が通ってしまう**
- 結果、一部 commit が DCO 未署名のまま積み上がる事故が起きる
- 予防: 新しい clone / session 冒頭で、最初の commit 前に必ず `mise run check` を一度走らせる

### 一度作った commit に GPG 署名を後付けする
- `commit.gpgsign = true` でも、GPG agent がロック中で pinentry が出ない非対話環境では署名されず、`%G?` が `N` の commit が生まれる
- 対処: ブランチを `git rebase -f -S main` で強制再適用すると全 commit が署名付きで書き換わる（`-f` は no-op rebase を強制、`-S` で signing）
- force push は `--force-with-lease` を使うこと（他セッションの push に上書きしない）
- ただし **main / master に対しては force push しない**（規約と git hook で二重防衛）

### `mise exec --cd <repo>` は cwd を repo ルートに戻す
- `cd /tmp/X && mise exec --cd /path/to/repo -- cargo run ...` だと、cargo は repo から動くが、実行時の working directory は `--cd` で指定した repo になる
- 結果: 手動で tempdir に cd しても `progest init` が repo ルートに `.progest/` を作ってしまう事故が起きる
- 対策: smoke test は `cargo build` で binary を `target/debug/progest` に作り、その path を絶対パスで叩く（`$BIN init` のように）。cargo 経由だと cwd 制御が効かない
- 検証ルール: CLI の smoke test で tempdir を使うなら、実行前に必ず `pwd` を echo して想定通りか確認する

---

## 2. Rust / clippy pedantic の癖

### clippy pedantic の doc_markdown は頭字語を「項目」扱いする
- doc コメントに `UUIDv7`, `RFC9562`, `DCC` 等の裸の頭字語を書くと `item in documentation is missing backticks` でエラー
- バッククオートで囲む（`` `UUIDv7` ``）か、英単語化する
- 通常の proper noun（`Tauri`、`macOS`）はセーフ、完全大文字トークンで要注意

### `io::Error::new(ErrorKind::Other, ...)` は clippy pedantic で禁止
- Rust 1.74+ の `io::Error::other(msg)` に置換される。clippy の `io_other_error` が `-D warnings` で拒否する
- `ErrorKind::Other` に特定の意味を持たせる他用途では `io::Error::new` で別の kind を指定する

### serde の `with` module は `serialize(value: &T, ...)` を要求、clippy `ref_option` に引っかかる
- `#[serde(with = "my_mod")]` を `Option<FileId>` に当てると、`my_mod::serialize` の第一引数は仕様上 `&Option<FileId>` 固定
- clippy pedantic の `ref_option` は `Option<&T>` への書換えを要求してくるが、serde 契約と競合して書換え不可
- 妥協: `serialize` 関数だけに `#[allow(clippy::ref_option)]` をローカル付与し、理由コメント（「serde's `with` contract requires `&Option<T>`」）を添える
- 場所: `crates/progest-core/src/meta/document.rs::source_file_id_serde::serialize`

### `SystemTime` を i64 unix 秒に落とす時は unwrap を避ける
- `duration_since(UNIX_EPOCH)` は epoch 前の時刻で `Err(SystemTimeError)` を返すし、`as_secs()` → `i64` の変換も 2262 年以降でオーバーフローしうる
- clippy pedantic 環境では `map(..).unwrap_or(..)` が `map_unwrap_or` で拒否されるので `map_or(default, closure)` を使う
- `time` / `chrono` 依存を足さずに FileRow.mtime を埋めるためだけなら、`t.duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))` で十分。`created_at` / `last_seen_at` の RFC3339 文字列化が必要になったら crate 導入を検討する

### integration test のヘルパ関数は `&self` を取らないと `unused_self` で弾かれる
- `Harness::sidecar(&self, rel)` のようなヘルパは clippy pedantic の `unused_self` に刺さる
- 対処: `&self` を使わない関数はモジュールレベルの free function に切り出す（`fn sidecar(rel: &str) -> ProjectPath`）
- 場所: `crates/progest-core/tests/reconcile_flow.rs`

### `MemFileSystem` は Clone 不可、共有が必要なら借用で渡す
- `Mutex<BTreeMap<...>>` を内部に持つため `MemFileSystem` に Clone を derive できない（`Mutex` 自体が Clone ではない）
- `StdFileSystem` は Clone 可能だが、どちらでも動くヘルパを書きたいとき `F: FileSystem + Clone` を要求すると `MemFileSystem` 側が脱落する
- 対処: 所有せず `&F` を受け取る設計にする。`PendingQueue<'a, F>` がこの例。`PhantomData` 抜きの借用ジェネリックで十分
- 場所: `crates/progest-core/src/meta/pending.rs::PendingQueue`

---

## 3. core::fs（Path / ignore）

### `ignore` crate の `Gitignore::matched` はディレクトリ除外をカスケードしない
- `matched(path, is_dir)` は渡されたパス自身しか判定しない。例えば `.progest/` パターンが有効でも `.progest/index.db` に対して `matched` は `None` を返す
- ディレクトリ除外を子孫まで効かせるには `matched_path_or_any_parents(path, is_dir)` を使う
- 場所: `crates/progest-core/src/fs/ignore.rs::IgnoreRules::is_ignored`

### `std::path::Path::components()` は `foo//bar` を正規化する
- 多重スラッシュは `Path::components()` で自動的に 1 つに潰される。結果、`ProjectPath::new("foo//bar")` を `Path::components()` ベースだけで検証しても空セグメント違反を検出できない
- 文字列入力ベースの `new` 側で `raw.contains("//")` を事前チェックする
- 場所: `crates/progest-core/src/fs/path.rs::ProjectPath::new`

---

## 4. TOML round-trip / serde

### TOML round-trip で未知フィールドを保持する実装パターン
- `.meta` は git 同期でバージョン差のある teammate 間を行き来するので、現行ビルドが知らないキーを save で落とさないことが要件
- 解決: 既知フィールドを typed struct で宣言しつつ、各 struct に `#[serde(flatten)] pub extra: toml::Table` を足す。top-level だけでなく `[core]` 等のセクション内の未知キーまで拾える
- `toml` crate 0.8 では `flatten` + `toml::Table` の組み合わせが round-trip で期待通り動く（`toml_edit` の重量級 API を入れずに済む）
- 場所: `crates/progest-core/src/meta/document.rs::MetaDocument` と `CoreSection` / `NamingSection` / `TagsSection` / `NotesSection`

### `.dirmeta.toml` の未知セクションは `extra: toml::Table` で round-trip
- `[accepts]` は M2 `core::accepts` で typed スキーマを追加する予定。それまでに `.dirmeta.toml` を手編集されても loader が round-trip で保存を壊さないよう、`#[serde(flatten)] extra` を付けておく
- `core::accepts` は `document.extra.get("accepts")` で typed に parse し、書き戻しは `document.extra.insert("accepts", ...)` で行う。loader 側が知らない他セクション（`[import]` など将来の拡張）も同じパスで保護される
- 場所: `crates/progest-core/src/meta/dirmeta.rs::DirmetaDocument`

---

## 5. SQLite / rusqlite

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

## 6. core::reconcile

### Scanner は `.meta` も通常ファイルとして yield する
- `.meta` はデフォルト `ignore` パターンに入っていない（むしろ git 同期対象なので入れてはいけない）
- 結果 `core::fs::Scanner` は `foo.psd.meta` を普通の `ScanEntry { kind: File }` として yield する
- reconcile 側で `path.as_str().ends_with(".meta")` を判定して、本体ファイル集合と sidecar 集合に分岐させる責務がある。orphan 検出はこの分岐を前提にする
- 場所: `crates/progest-core/src/reconcile/reconciler.rs::is_sidecar`

### `ignore::Walk` の出力順は FS 依存
- macOS / Linux / Windows でディレクトリ読込順が異なり、scan 結果をそのまま `ScanReport.outcomes` に流すと test の assertion が flaky になる
- 対処: reconcile 内で `path.as_str()` で sort してから outcome を積む。ordering を決定論にしておくと CLI 出力の diff も安定する
- 場所: `crates/progest-core/src/reconcile/reconciler.rs::full_scan`

---

## 7. core::watch（notify / FSEvents）

### macOS `TempDir` は非 canonical パス、FSEvents は canonical path を返す
- `TempDir::new()` が返す `/var/folders/.../X` は `/private/var/folders/.../X` への symlink。`fs::canonicalize` でないと resolve されない
- `notify` の FSEvents backend は **canonical path**（`/private/var/folders/...`）でイベントを発行する
- 結果: watcher を TempDir の raw path で attach すると、受信した event path を `strip_prefix(root)` で相対化する段で全件 drop されテストが 0 件 assertion で panic する
- 対処: `Watcher::start` 内で `fs::canonicalize(&root).unwrap_or(root)` で正規化、`IgnoreRules` も正規化後の root を基準に作り直す
- 場所: `crates/progest-core/src/watch/watcher.rs::Watcher::start_with_debounce`

### `notify-debouncer-full` + `std::sync::mpsc` 構成での Drop 順序
- 典型構造: `Watcher { debouncer, worker: JoinHandle }`。worker は `raw_rx.recv()` で blocking、`raw_tx` を閉じるのは debouncer の Drop
- `Drop::drop` で worker を先に join しようとすると、自スレッドが debouncer を手放さないまま worker が待ち続け **deadlock** する
- 対処: debouncer を `Option` で包み `take()` → `drop` してから worker を join する。field declaration 順序に頼ると manual `Drop::drop` の前に drop されないので、明示的に take する必要あり
- 場所: `crates/progest-core/src/watch/watcher.rs::Watcher::drop`

### macOS FSEvents の `Remove` は Modify に縮退しやすい
- `fs::write(p, ..)` → `fs::remove_file(p)` を数十 ms 間隔で走らせると、FSEvents は 2 件の `Modify(Data)` としてまとめがち。notify の分類も Modify になり、Remove にならない
- テスト側で「Remove event が来る」まで厳密に待つと flaky になる。Reconcile 側の `apply_changes` は「Modified 来たが FS に無い」を「index row 削除」にフォールバックする挙動があるので、watcher 単体テストでは「削除対象パスに関する event が 1 件以上届く」までに留める
- 場所: `crates/progest-core/tests/watch_flow.rs::removing_a_file_surfaces_an_event_for_that_path`

---

## 8. core::project / CLI / init

### `progest init` の `.gitignore` 追記は trailing slash を正規化する
- `.progest/thumbs` と `.progest/thumbs/` は gitignore semantics では同一
- 既存 `.gitignore` が slash なし、shipped pattern が slash 付きだと、単純 string 比較で重複追記が起きる
- 対処: 両方を `trim_end_matches('/')` で正規化してから `HashSet` 比較
- 場所: `crates/progest-core/src/project/layout.rs::ensure_gitignore`

### `project::initialize` は `.gitignore` を 1 件作るので scan 件数が +1 になる
- 10k fixture を作ってベンチを走らせると `added` が 10001 になる
- 理由: `initialize` が project root 直下に `.gitignore` を書き込むため、scanner から見るとそれも tracked file
- ベンチ assertion は `>=` 比較にしておくと安全。テストで exact count を assert したい場合は fixture 用意 → `initialize` の順ではなく、`initialize` → fixture の順にして `.gitignore` を先に取り込むか、明示的に +1 を吸収する
- 場所: `crates/progest-core/benches/scan.rs::prepare_project`

---

## 9. core::meta pending / StdMetaStore 挙動

### `StdMetaStore` の暗黙 flush は best-effort、エラーは握りつぶす
- pending queue の flush は load / save / delete の先頭で毎回走る。ただし flush 自身が失敗しても呼び出し元のメイン操作は続行する
- 理由: flush が失敗する原因（FS 不調、書込権限など）は大抵キューに入った原因と同じで、この場でも治らない。メイン操作の結果（成功 or 同じ原因で失敗）を正しく返す方が有用
- 逆にメインの save が失敗したらその場で enqueue してからエラーを返す。呼び出し元は `MetaStoreError::Fs(...)` を見て失敗を知り、ユーザーへの通知や再試行誘導ができる
- 場所: `crates/progest-core/src/meta/store.rs::StdMetaStore::{run_flush, save}`

---

## 10. テスト / ベンチ基盤

### `CARGO_BIN_EXE_<name>` で integration test から binary を叩く
- binary crate の integration test（`tests/*.rs`）で Cargo は `env!("CARGO_BIN_EXE_<bin_name>")` をコンパイル時に埋め込む
- `std::process::Command::new(env!("CARGO_BIN_EXE_progest"))` だけで `assert_cmd` 依存なしに end-to-end test が書ける
- 場所: `crates/progest-cli/tests/cli_flow.rs::binary_path`

### criterion の default は大型ワークロードに向かない
- `sample_size = 100`, `measurement_time = 5s` がデフォルト。1 iteration が 100 ms の 10k scan でも ~50 s 以上かかる
- 完了条件ベンチのような「1 回走って数字が欲しい」用途では `sample_size(10)` + `measurement_time(Duration::from_secs(30))` 程度にして 1 分以内に収める
- 回帰検知を継続的にやる日が来たら数字を戻す

---

## 11. core::rules（DSL evaluator / parser）

### `schema_version > N` の forward-compat は「未知ルールを捨てる」ではなく「既知ルールをパース + 未知キーを extra 保持」
- 最初の実装は newer schema で `rules: Vec::new()` を返していた → v1 バイナリが v2 rules.toml を読むと命名規則が全部スキップされ、lint が silently pass する
- 正しい挙動: known shape でパースし、未知の top-level / rule キーは `extra: Table` に格納、warning は出さない
- 同 schema_version の時のみ typo 候補として warning 化する
- 場所: `crates/progest-core/src/rules/loader.rs::load_document`

### `{ext}` は構造的分離が必要、greedy regex だけで扱うと compound ext が曖昧
- 当初実装は `{ext}` を `[A-Za-z0-9.]+` の regex fragment にして全体 regex で捕獲 → `archive.tar.gz` を `(desc=archive.tar, ext=gz)` と `(desc=archive, ext=tar.gz)` のどちらにマッチするかが regex 依存（非決定）
- 正解: `split_basename` で先にコンパウンドを剥がし、stem を非`{ext}`アトムから組んだ regex で照合、ext は別途 capture map に格納
- `{ext}` 直前のリテラル `.` は split が食べているので stem regex 側では trim する必要あり
- 場所: `crates/progest-core/src/rules/template.rs::match_basename`

### `required_prefix` / `required_suffix` は stem に当てる。basename 全体だと拡張子を跨ぐ
- `required_prefix = "foo."` を basename で照合すると `foo.psd` が通ってしまう（`"foo.psd".starts_with("foo.")` = true）
- suffix 側は元から stem で見ていたので、prefix 側だけズレていた。両方を stem に統一
- 場所: `crates/progest-core/src/rules/constraint.rs::evaluate_constraint` の `required_prefix` 判定

### §5.7 grapheme 数は NFC 後で数える
- decomposed（`cafe\u{0301}` など）で入ってきた時、`graphemes(true)` は cluster を正しく 4 と返すが spec が NFC を明示しているので NFC 正規化を先に入れる
- macOS APFS など decomposed を返す FS がある環境での挙動安定化にもなる
- 依存: `unicode-normalization` crate
- 場所: `crates/progest-core/src/rules/constraint.rs::grapheme_count`

### full-replace override は scope 限定ではなく rule 単位のグローバル置換
- child dirmeta で同 id+kind のルールが現れたら、parent rule は ruleset 全体から除去される
- CSS cascade のように「child の scope 内だけ上書き」ではない
- child の `applies_to` 外のパスには **どちらのルールも** 適用されなくなる点に注意（DSL §10.4 Case B 参照）
- child scope 外で parent rule を残したい場合は別の rule_id を使う
- 場所: `crates/progest-core/src/rules/inheritance.rs::compile_ruleset`

### clippy pedantic の細かい罠（rules モジュールで踏んだ分）
- `FormatSpec::is_numeric(self)` を `.any(FormatSpec::is_numeric)` に渡すと `fn(&Self)` 期待でコケる → `|s| s.is_numeric()` に展開
- `if !cond { panic!(..) }` は `manual_assert` に引っかかる → `assert!(cond, ...)` に書き換え
- テストで `Vec<&str>` を作るとき `.iter().copied().collect()` は `iter_cloned_collect` 警告 → `let compound: &[&str] = &["..."];` で済ませる
- 関数引数が 7 を超えると `too_many_arguments` → 仕方ない場合は `#[allow(clippy::too_many_arguments)]` + 理由コメント
- `format!` で push_string するパターンは `format_push_string` → `use std::fmt::Write; write!(out, "{...}")` または `expect("writing to String never fails")`

---

## 12. core::accepts（placement lint / alias catalog）

### 拡張子トークンは `:alias` / `.ext` / `""` の 3 形式を loader 段階で強制
- `"psd"` のような leading-dot 無しのトークンを silent に通すと「何にもマッチしない literal」として残り、後段で「なぜ `.psd` が reject されるのか分からん」デバッグ地獄になる
- ACCEPTS_ALIASES.md §3.1 に validation matrix を書いて、loader / schema 両方で同じ規則を hard-error させる
- 場所: `crates/progest-core/src/accepts/loader.rs::parse_ext_token` / `schema.rs::load_alias_catalog_from_table`

### `inherit = true` は rule 単位の継承ではなく **ディレクトリ単位の accepts union**
- 仕様 §3.13.2: `effective_accepts(dir) = own ∪ (inherit ? effective_accepts(parent) : ∅)`
- 祖先の `inherit` フラグは child の walk に影響しない（child が walk を始めると、あとは parent の own を順に連結）
- rules の `full-replace override` とは真逆のセマンティクス。どちらも「継承」と呼ばれるので混同しやすい。accepts は additive、rules は replacive と覚える
- 場所: `crates/progest-core/src/accepts/resolve.rs::compute_effective_accepts`

### Builtin alias 上書きは **full replace**、union ではない
- `[alias.image] = [".jpg"]` と書いたら、builtin の `.png` 等は消える（`:image` でヒットしなくなる）
- 部分追加したいなら `[alias.image] = [":image", ".extra"]`…は仕様上禁止（ネスト禁止）なので、別 alias を作るか全部列挙し直すしかない
- ローダは override 時に `SchemaWarning::BuiltinAliasOverridden` を必ず emit。doctor で surfaceして事故防止
- 場所: `crates/progest-core/src/accepts/schema.rs::load_alias_catalog_from_table`

### ビルトイン alias の拡張子セットは「現場の置き場判断」基準で決める
- SVG は XML だが `docs/` に置かれるより `icons/` に置かれる方が圧倒的に多い → `:image` に分類（`:text` ではない）
- PSD / PSB / BLEND / HIP などは「3D アセット」「編集中プロジェクト」両方の dir で正解なので `:3d` と `:project` に **意図的に重複** させる
- `prores` は codec 名（ext ではマッチ不能）、`fcpbundle` は macOS package directory → どちらも accepts から除外。仕様定義 (REQUIREMENTS §3.13.1) の「拡張子文字列で比較」前提に素直に従う
- 詳細は `docs/ACCEPTS_ALIASES.md` §2
- 場所: `crates/progest-core/src/accepts/types.rs::BUILTIN_ALIASES`

### `""` は明示的な「拡張子なし」エイリアスで、leading-dot ファイルも含む
- `split_basename` が `.gitignore` を `(stem=".gitignore", ext=None)` として返す挙動を accepts 側も踏襲
- `[accepts].exts = [""]` を書いたら `README` / `Makefile` だけでなく `.gitignore` / `.env` も通す（golden 60_no_extension_sentinel で固定）
- 逆に hidden file だけ別扱いしたい場合は v1 では手段なし（将来の `:hidden` alias 検討）
- 場所: `crates/progest-core/src/accepts/types.rs::normalize_ext_from_basename`

### `Violation` の placement フィールドは optional、naming 側は None 固定
- naming violations に余計なフィールド持たせない設計で、`Violation.placement_details: Option<PlacementDetails>` を採用
- `category` で分岐すれば CLI `lint` は同じ Vec<Violation> から両カテゴリをまとめて出せる
- `suggested_destinations` は import ランキング API (未着手) で埋める。この PR では常に空 Vec
- 場所: `crates/progest-core/src/rules/types.rs::Violation`

### placement violation の `rule_id` は予約語 `"placement"` で固定
- naming の rule_id グラマ (`[a-z][a-z0-9_-]{0,63}`) に合致する文字列を選んでおくと、lint UI / saved-search / 集計が一つのキー空間で扱える
- テストで `placement_rule_id_impl().unwrap()` を必ず走らせて、将来 RuleId grammar が変わった時に検知できるようにする
- 場所: `crates/progest-core/src/accepts/evaluate.rs::placement_rule_id`

### clippy 追加の罠
- doc コメント内の `ACCEPTS_ALIASES.md` はバッククォート必須（rules の時と同じ doc_markdown）。`.md` 拡張子付きも対象
- `zero_sized_map_values` — `BTreeMap<K, ()>` は `BTreeSet<K>` に置換せよの警告。dedup 用途は `BTreeSet` が正解
- `collapsible_if` / `needless_raw_string_hashes` は rules で踏んだ時と同じ。テスト追加時は最初から `assert!(..., "msg")` と `r"..."` / `let-chains` を使う

---

## 13. core::naming（cleanup pipeline / heck）

### remove_copy_suffix は **tail-only**、中間一致させない
- OS が copy suffix を付けるのは常にファイル名の末尾。`foo (1)_bar.png` の `(1)` は人が意図して入れた可能性が高く、剥がすと破壊的
- 実装は stem の末尾に対してのみ regex 的に一致。`strip_dash_copy` → `strip_japanese_copy` → `strip_paren_number` の順で試す（dash-copy が `(N)` を含み得るので最初に試さないと paren_number が先に食って `"doc - Copy"` が残る）
- 非再帰: `"foo (1) (2)"` は `(2)` のみ剥がす。連鎖適用すると「人が付けた `(1)`」まで失う
- 場所: `crates/progest-core/src/naming/pipeline.rs::remove_copy_suffix`

### heck は leading / trailing underscore を **食う**
- `to_snake_case("_MainRole_v01")` → `"main_role_v01"`（先頭 `_` が消える）
- pipeline で CJK ラン → Hole にした直後に残る literal `"_v01"` は、snake 化すると `"v01"` になる
- 結果: sentinel rendering が `"⟨cjk-1⟩v01.png"` になって `_` が失われたように見える。これは仕様として受容（Hole と literal の境界 `_` は元々 separator 扱いで、case 変換の結果として削られて正しい）
- 回避したい場合は `CaseStyle::Off` + 別途 snake 相当の変換を書くしかない。v1 では許容
- 場所: `crates/progest-core/src/naming/pipeline.rs::apply_case_to_literals`

### CJK 文字判定は Unicode ブロックをハードコード
- `unicode-script` crate を入れるほどの usage でもないので、`is_cjk_char` で範囲直書き（Hiragana 3040–309F、Katakana 30A0–30FF + phonetic ext、Han 3400–9FFF + Ext B/C/D/E/F/G、Compat Ideographs）
- 韓国語 Hangul（AC00-D7AF）は v1 スコープ外（要件上 CJK は「日中韓」だが、日本語現場向けに絞っておく）。追加需要があれば `HoleKind::Hangul` を足す流れ
- 場所: `crates/progest-core/src/naming/pipeline.rs::is_cjk_char`

### Hole は silent delete の代わり、disk に出さない
- `NameCandidate` は `Vec<Segment>` で、`Segment::Hole` は原文・種別・位置を持つ
- `to_sentinel_string()` は text dry-run 用の `⟨cjk-N⟩` 表記。disk には書いてはいけない
- `FillMode::Skip` で resolve → holes があると `UnresolvedHoleError::HolesRemain` で拒否。これが disk-safe な唯一の contract
- `FillMode::Prompt` は core 層では `PromptUnavailable` を返す（対話 I/O は CLI/UI 層の責務。現時点では `core::rename` 着手時に実装）
- 場所: `crates/progest-core/src/naming/fill.rs`

### `[cleanup]` は flat key、`convert_case = "off"` で無効化
- `project.toml` の `[cleanup]` は `remove_copy_suffix: bool` / `remove_cjk: bool` / `convert_case: "off"|"snake"|...` の 3 フィールド
- 未知キーは warning（`CleanupConfigWarning::UnknownKey`）で forward-compat。error にすると旧 progest で新 `[cleanup]` を開けなくなる
- ネスト table（`[cleanup.case]` 等）は v1 では使わない。将来 protected_tokens / locale を足す時の拡張余地として残してある
- 場所: `crates/progest-core/src/naming/loader.rs::extract_cleanup_config`

### suggested_names[] は **naming 系 Violation のみ**、hole があれば空
- placement violation は `suggested_destinations` 側の仕事（将来実装）。両者に rename 候補を出すと UI が錯綜する
- hole が残る候補は `try_resolve_clean` が None を返すので suggested_names には入らない。`⟨cjk-N⟩` が入った文字列を user-facing suggestion に混ぜない設計
- 既存リストへの重複挿入を避けるため、push 前に `contains` でチェック（`fill_suggested_names` は idempotent）
- 場所: `crates/progest-core/src/naming/suggest.rs::fill_suggested_names`

### `core::rules::template` の case 変換は `naming::case::rules_format_spec` に一本化
- 旧実装の `word_chunks` は非 alnum でしか split しないので `"ForestNight"` → `"forestnight"` と collapse していた（サイレントバグ）
- `RulesCase` enum は `CaseStyle` とは別物にした（rules は `Slug` を追加で持ち、`Off` は持たない）
- golden 更新は不要だった（rules_golden は numeric `{field:*:03d}` しか exercise していなかった）。string spec を使う template を追加する時は case 違いを意識した fixture を同時に足す
- 場所: `crates/progest-core/src/rules/template.rs::apply_string_specs` → `crates/progest-core/src/naming/case.rs::rules_format_spec`

### CLI `progest clean` は preview 限定、`--apply` は `core::rename` 合流時に解禁
- 破壊的操作（FS rename）は history 連携 + 原子トランザクション前提。`core::rename` の landed 後に wire
- 現状フラグ: `--case`/`--strip-cjk`/`--strip-suffix` は config を上書き（on 方向のみ、off への override は project.toml 編集）、`--fill-mode skip|placeholder` / `--placeholder <STR>`（デフォルト `_`）、`--format text|json`、末尾の `[PATH]...` は接頭辞フィルタ
- JSON 出力は `candidates[].{path, original, sentinel, resolved?, skipped_reason?, holes[]?, changed}` + `summary.{scanned, would_rename, skipped_due_to_holes, unchanged}`。clean_smoke テストで固定
- `.gitignore` のような project-init 生成物も walk に乗る（`.progest/ignore` で明示除外されていない限り）。smoke テストでは特定候補のみ assert して、summary は下限チェックに留めた

---

## 14. core::history（undo/redo SQLite）

### `Operation::Import` の inverse は **別 op kind ではなく同じ op + `is_inverse: true`**
- 当初案は `ImportUndo`/`Delete` を別 variant にする案だったが、undoer（FS 層）が「inverse なのか forward なのか」を判定できれば 1 variant で済むので、フラグ方式を採った
- `invert(Import { is_inverse: false })` → `Import { is_inverse: true }`、`invert(Import { is_inverse: true })` → `Import { is_inverse: false }` で double-inverse 恒等が保たれる
- 新規 op を足す時は `double_inverse_is_identity_for_all_variants` テストに追加するのを忘れない
- 場所: `crates/progest-core/src/history/inverse.rs::invert`

### `inverse_json` は **append 時に synthesize してそのまま保存**
- `invert` 関数は純粋なので毎回 undo 時に再計算しても同じ結果が出るはずだが、将来 inverse の仕様が変わった時に「古いエントリがどういう前提で書かれたか」を追えるよう、append 時に算出したものを pin する
- 読み取り側は payload と inverse 両方を deserialize する。`debug_assert_eq!(op.kind(), op_kind)` で wire の op_kind と payload discriminant が一致することを確かめておく（migration バグの早期検出）
- 場所: `crates/progest-core/src/history/store.rs::Store::append` / `row_to_entry`

### redo branch は append 時に erase、pointer より id が大きい consumed 行を削除
- 「undo を 2 回してから新しい op を append」した瞬間、redo スタック（= consumed=1 かつ id > pointer）はその場で削除するのが UI コンセンサス
- pointer が None（全 undo 済）の場合は `WHERE consumed = 1` だけで良い — この分岐を忘れると「pointer なし状態で redo 行が残って append 後に redo できてしまう」事故に繋がる
- 場所: `crates/progest-core/src/history/store.rs::Store::append`

### retention は tail 削除、pointer が evict されたら最新 non-consumed に reconcile
- `DELETE FROM entries WHERE id < <cutoff>` で物理削除する構造上、pointer が指している row が消える可能性がある（大量 undo の後に append が走って古い行を押し出すケース）
- 削除後に `SELECT EXISTS(SELECT 1 FROM entries WHERE id = pointer AND consumed = 0)` で確認、不在なら `SELECT MAX(id) WHERE consumed=0` に付け替える
- テスト: `retention_reconciles_pointer_when_head_gets_evicted`
- 場所: `crates/progest-core/src/history/store.rs::enforce_retention`

### `MetaDocument` を payload に載せる時は `Box<MetaDocument>` で包む
- `Operation` enum の各 variant サイズは、最大 variant（MetaEdit の before/after 両方 inline）に揃う
- `MetaDocument` はそれなりに大きい（custom: Table + tags + timestamps...）ので、Box 越しに持つと他の variant（Rename とか TagAdd）が軽量なまま
- clippy `large_enum_variant` はワーク周辺で常に仕掛けがあるので、将来 op を足す時もこの原則を維持する
- 場所: `crates/progest-core/src/history/types.rs::Operation::MetaEdit`

### history は「記録だけ」、apply 原子性は呼び出し側の責務
- `append` は単に SQLite トランザクションを張って 1 行挿入するだけ。FS / .meta / index の三つ巴整合性は `core::rename` / `core::import` が自前で確保
- これを history 側に寄せる（「history が transaction を主導する」）案は初期にあったが、テストのしづらさ + 複数 DB にまたがるトランザクション不可能性（sqlite 間は XA 不可）で見送った
- その代わり `group_id` があるので、bulk 操作で途中失敗した場合は呼び出し側が「該当 group_id の entries を list → undo を N 回叩く」で rollback 風に振る舞える
- 場所: module doc / `crates/progest-core/src/history/mod.rs`

### `.progest/local/history.db` は machine-local、gitignore 対象
- undo 履歴は per-workstation（他人が pull しても undo できるべきではない）なので、`.progest/local/` 以下に配置
- `GITIGNORE_ENTRIES` に `.progest/local/` が既に入っているので新たな追記は不要
- ただし `ProjectRoot::history_db()` は `dot_dir().join(LOCAL_DIR).join(HISTORY_DB_FILENAME)` を返すので、callers が local dir を作らずに open すると SQLite が親ディレクトリ不在で失敗する。`progest init` が `local/` を create_dir_all するので init 後なら OK
- 場所: `crates/progest-core/src/project/layout.rs`, `root.rs::history_db`

### `usize` → i64 / u32 cast は `try_from(...).unwrap_or(MAX)` で
- `limit as i64` / `RETENTION_LIMIT as u32` 系は clippy `cast_possible_wrap` / `cast_possible_truncation` で弾かれる
- これまでのテストで学んだ pattern: `i64::try_from(limit).unwrap_or(i64::MAX)` — 実運用で usize::MAX を入れることは無いので unwrap_or で十分、それ以外も同じ手口
- 場所: `crates/progest-core/src/history/store.rs::Store::list`

## 15. core::rename + core::sequence（atomic apply / sequence detection）

### 直接 rename ではダメ — chain `foo→bar→baz` は staging 経由必須
- 「from→to を順に rename」だと `foo→bar` の時点で既存の `bar` を上書きしてしまう（または `bar` の `foo→` で `bar` が消える）。preview の chain detection が「target が他の op の from にある」を許容しているのは、apply が staging 経由で全 from を中立地に退避してから to に展開するから
- staging dir: `.progest/local/staging/<batch_uuid>/<idx>.f` (file) / `.<idx>.m` (sidecar)。各バッチが独自 dir を持つので並行 apply でも衝突しない
- rollback は逆順: Phase 2 失敗 → in-flight op の commit を逆 → 既 commit を staging に戻す → 全 staging を from に戻す
- 場所: `crates/progest-core/src/rename/apply.rs::Rename::commit_all`, `rollback_phase1`, `rollback_phase2`

### `.meta` sidecar も file と一緒に move する。両方 atomic
- ファイル単独で動かして sidecar が orphan になる事故が一番まずい。`from.meta` が存在すれば必ずペアで stage → commit → rollback
- `MetaStore::rename` のような API は作らず `fs::rename` 直接でいい — `.meta` も普通のファイルなので、rename には `MetaStore` の load/save 規約は要らない
- ペアの一致は property test で担保: 「apply 失敗時、file と sidecar は両方 from に、両方 to にあってはならない」を 5-op × 20 fault placements で検証
- 場所: `crates/progest-core/src/rename/apply.rs::stage_all`, tests `fs_is_all_or_nothing_under_random_rename_fault`

### index update は post-commit best-effort、FS は rollback しない
- `Index::upsert_file` 失敗時に FS rollback すると「キャッシュが古い」を理由にユーザーの実体ファイル変更を巻き戻すことになる。それは誤り
- 失敗は `IndexWarning` に積んで返す。reconcile が次回 scan で修復するのが正しいレイヤリング
- 同じ contract が `HistoryWarning` にも適用される: history append 失敗で FS を巻き戻すのは過剰
- 場所: `crates/progest-core/src/rename/apply.rs::Rename::apply` doc comment / `update_index`

### `FaultyFileSystem` は inner FS の **前** で fault を投げる
- maybe_fail を inner.rename の前にチェックすることで、fault が投げられた op は inner FS に触れない → 「operation never started」セマンティクス。テスト assertion で「fault 後の inner state は事前と同じ」が確実に成立
- 「inner.rename を実行してから fault を投げる」も実装可能だが、partial-apply のシミュレーション目的でない限り過剰。今回は前者だけ提供
- proptest との組み合わせで「N-op batch の任意の rename call に fault を一発入れる → FS は all-or-nothing」を網羅的にチェックできる
- 場所: `crates/progest-core/src/fs/fault.rs::FaultyFileSystem::maybe_fail`

### bulk rename の group_id: caller per-op > auto-batch
- `Rename::apply` は `preview.ops.len() >= 2` で auto-batch group_id を生成するが、各 op が既に `group_id` を持つ（`core::sequence::requests_from_sequence` がセット済み）場合は per-op を優先
- `outcome.group_id` も「全 op が同じ caller-supplied group を持つ」なら caller group を返す。そうでなければ auto group。これで sequence rename の呼び出し側が「自分の渡した seq-... id」をそのまま使える
- 場所: `crates/progest-core/src/rename/apply.rs::Rename::apply` / `unified_caller_group`

### sequence detection の正規表現 `^(.*?)([._-]?)(\d+)\.([^.]+)$`
- `.*?` (lazy) で最短マッチ、`[._-]?` で separator は **末尾連続数字の直前 1 文字のみ**（`shot_v01` なら sep="" stem="shot_v"）
- padding mismatch (`frame_001` vs `frame_1`) は別グループ。VFX で部分的に renumber されたバッチが「混在」として可視化されるのが意図
- gap は許容（`frame_001`, `frame_002`, `frame_005` は 1 sequence）。retake で抜け番が出るのが普通だから
- compound extension (`.exr.gz` 等) は v1 スコープ外
- 場所: `crates/progest-core/src/sequence/detect.rs::PATTERN`

### `HolePrompter` trait は core、impl は CLI
- core が trait 定義のみ持って interactive I/O を抽象化。CLI 側 `StdinHolePrompter` が `Read + Send` / `Write + Send` で generic、テストは canned input/output で driven
- prompts は **stderr** に出す。stdout は JSON pipe を維持したいので絶対に汚さない
- 空行 → `PromptError::Invalid`、EOF → `PromptError::Cancelled`（cancellation を意図的に区別）
- 場所: `crates/progest-core/src/naming/fill.rs::HolePrompter`, `crates/progest-cli/src/prompter.rs`

### `clean --apply` は Identity + Unresolved op を **filter で落とす**
- `progest clean` は project 全体を walk するので 99% のファイルは Identity (resolved == original)。これを `is_clean()` に通すと "1 op carries conflicts" で apply が refuse される
- 解決: `from == to` の op は apply 前に filter で除外（Identity / Unresolved fallback to from の両方をカバー）。残ったものだけ cleanness check に掛ける
- preview report は filter 前のものを emit するので、ユーザーは何が起きるか/起きないかを両方見られる
- 場所: `crates/progest-cli/src/commands/clean.rs::commit`

---

## 16. core::lint / core::sequence::drift / undo-redo CLI

### `Rename::apply` は常に history へ append する前提だった → `Option` 化が必要
- `progest undo` / `redo` で inverse を replay するとき、**history には既に original entry があり、それを flip するだけでよい**。`Rename::apply` がそのまま append してしまうと「undo で 2 件目の entry が足され、redo が意味を失う」という二重記帳になる
- 解決: `Rename::new_without_history(fs, index)` コンストラクタを追加し、`history: Option<&dyn Store>` に変更。apply ループ内で `if let Some(history)` ガード
- 使い分け: `progest rename` / `clean --apply` は `Rename::new(fs, index, history)`、`progest undo` / `redo` は `Rename::new_without_history(fs, index)` + 別途 `Store::undo/redo` を呼ぶ
- 場所: `crates/progest-core/src/rename/apply.rs`, `crates/progest-cli/src/commands/undo.rs`

### undo の order-of-operations — replay → flip
- `Store::undo()` は副作用込み（consumed flip + pointer 遷移）なので、「先に flip してから replay」だと replay 失敗時に history と disk が乖離する
- 解決: まず `Store::head()` で peek（read-only）、FS replay（`Rename::apply_without_history`）、成功したら `Store::undo()` で flip
- group 単位 undo の場合: `Store::list()` で group の全 entry を取得 → head から順に同じパターン（replay → flip）を繰り返す。途中で失敗しても前半は既に flip 済みなので、次回実行で残り分から再開できる
- 場所: `crates/progest-cli/src/commands/undo.rs::run`

### `MemFileSystem` は `Clone` ではない → `StdMetaStore::new(fs)` が消費した後も fs reference が必要なら `.filesystem()` 経由で戻す
- テストで `fs` と `meta_store` を両方引数に渡したくなる場面が多いが、`StdMetaStore::new(fs: F)` が fs を move で取るので単純には書けない
- 解決: fs をまず `MetaStore` に渡し、必要な場所では `store.filesystem()` で `&F` を取り出す。テスト helper として `store_with(files) -> StdMetaStore<MemFileSystem>` を置くのが読みやすい
- 場所: `crates/progest-core/src/lint/orchestrator.rs::tests::store_with`

### `applies_to` グロブは `./` 必須（spec §3.1）
- lint の smoke で `applies_to = "assets/**/*.psd"` と書いたら `MissingLeadingDotSlash` で落ちる
- 解決: 必ず `"./assets/**/*.psd"` と書く（project root 相対の明示）
- 場所: `docs/NAMING_RULES_DSL.md §3.1` と `crates/progest-core/src/rules/applies_to.rs`

### sequence drift の majority canonical は `max_by` の比較向き
- `max_by(|a, b| ...)` は `Less` で b を採用 / `Greater` で a を保持。アルファベット小さい方を勝たせたいとき、**単純に `a.stem.cmp(&b.stem)` を追加すると逆向き**（辞書順で大きい方が勝つ）
- 解決: tie-break で `b.stem_prefix.cmp(&a.stem_prefix)` と書く（または `a.cmp(&b).reverse()`）。テストで `"Shot"` が `"shot"` に勝つことを確認
- 場所: `crates/progest-core/src/sequence/drift.rs::detect_drift`

### clippy pedantic: `too_many_lines` (100 行上限) は main.rs の match で引っかかる
- `fn main()` がサブコマンド dispatch で 108 行に膨らみ clippy-deny
- 解決: 共通 helper（例: `to_exit_code(i32) -> ExitCode`）を抽出。それでも 100 行超えるなら `#[allow(clippy::too_many_lines)]` を main に付ける（巨大 match は意味的にまとまっているので分割しないほうが読みやすい）
- 場所: `crates/progest-cli/src/main.rs`

## 17. shadcn / フロントエンド primitive

### shadcn の `Resizable` は v4 の `react-resizable-panels` を呼ぶ — `direction` ではなく `orientation`、`autoSaveId` ではなく `id`
- v0/v1/v2 の `react-resizable-panels`（多くのチュートリアルが書いている API）と v4 で props が非互換
- v4: `<ResizablePanelGroup orientation="horizontal" id="my-shell">`、layout の永続化は `id` を渡せば自動で localStorage に保存される
- 旧 API（`direction` / `autoSaveId`）で書くと TS で `Property 'direction' does not exist on type 'IntrinsicAttributes & HTMLAttributes<HTMLDivElement>'` と言われる（shadcn wrapper が `GroupProps = HTMLAttributes<HTMLDivElement> & {...}` を `...rest` で渡すため、誤入力した時のエラーが「div の attribute にない」という遠回しな形になる）
- 解決: shadcn `add resizable` で生成された `components/ui/resizable.tsx` の `ResizablePrimitive.GroupProps` を実際に開いて、現行 v4 のプロップ名を確認する
- 場所: `app/src/App.tsx::MainShell`

### accepts 編集 → 保存だけでは placement 違反バッジは更新されない（lint pass を通さないと violations table が古いまま）
- インスペクタが `accepts_write` で `.dirmeta.toml` を更新しても、フロントが見ている `is:violation` / `is:misplaced` は SQLite `violations` テーブル経由 — `progest lint` が走らない限り古い違反が残り続ける
- 解決: `lint_run` Tauri IPC を新設（CLI lint と同じ蓋: rules/schema/cleanup を inline ロード → `Index::list_files` → `lint_paths` → `write_to_index`）し、accepts 保存後に自動 trigger。フロントは `ProjectContext.refreshTick` を bump して FlatView / TreeView に再 fetch を促す
- 場所: `crates/progest-tauri/src/lint_commands.rs`、`app/src/components/directory-inspector.tsx::onSave`、`app/src/lib/project-context.tsx::refreshTick`
- 注意: TreeView は lazy load + cache なので、`refreshTick` 反応でキャッシュ全消し → **現在 expanded な path 全部を再 fetch** する追加 effect が必要（root だけ refetch だと展開済 dir が空になる）

### `tsc -b` は gitignored な `vite.config.{js,d.ts}` を生成する — `vp fmt --check` から除外しないと CI で落ちる
- `app/package.json` の `build: "tsc -b && vp build"` は素朴に `vite.config.ts` を上書き compile する
- gitignore には入っているが、ローカルで build 走らせた直後に `mise run check` すると oxfmt が「フォーマットが乱れた `vite.config.js` を見つけた」と fail する
- 解決: `app/vite.config.ts` の `fmt.ignorePatterns` に `vite.config.js` / `vite.config.d.ts` を追加（dist/** や registry コンポーネントと同じ枠組み）
- 場所: `app/vite.config.ts`
