# Progest 実装計画書

作成日: 2026-04-20
対象バージョン: v1.0 MVP（macOS 先行）
想定期間: 6ヶ月（M0〜M5、予備1ヶ月含む）

[REQUIREMENTS.md](./REQUIREMENTS.md) と対で読むこと。進行中フェーズの詳細な引き継ぎメモは [`M2_HANDOFF.md`](./M2_HANDOFF.md)。

---

## 0. 進捗スナップショット

最終更新: 2026-04-23

- **M0 Skeleton**: 完了
- **M1 Core data layer**: 完了 — `core::fs` / `core::identity` / `core::meta` / `core::index` / `core::reconcile` / `core::watch` / `core::project` + CLI `init`/`scan`/`doctor` + 10k-file incremental scan ベンチ（実測 ~82 ms、5 s gate の 60 倍下回り）
- **M2 Naming rules engine + accepts**: 進行中
  - [x] `core::meta` 残タスク（pending queue / `.dirmeta.toml` loader）
  - [x] DSL 仕様書 `docs/NAMING_RULES_DSL.md`
  - [x] `core::rules` — loader / applies_to / template / constraint / inheritance / evaluate + trace、§10 golden + Codex 指摘 5 件のホットフィックス + regression golden（feat/m2-core-rules）
  - [x] `core::accepts` — builtin alias catalog / project alias loader / `[accepts]` 抽出 / effective_accepts 計算 / placement lint、7 シナリオ × 12 golden、`docs/ACCEPTS_ALIASES.md` 初版（feat/m2-core-accepts）
  - [ ] `core::naming` — 機械的命名整理 pipeline（`remove_copy_suffix` → `remove_cjk` → `convert_case`）、`heck` 差し替え、`core::rules::template` の private case fn 移管、`[cleanup]` loader、violation.suggested_names[] 機械的充填（次着手）
  - [ ] `core::history` / `core::rename`
  - [ ] CLI `lint` / `rename` / `undo` / `redo` / `clean`
  - [ ] `core::rules` follow-up（suggested_names / §6 `{seq}` 採番 / trace の `NotApplicable` 拡張 / `match_basename` の Regex::new キャッシュ化 / §4.3 `{{`・§4.4 mixed spec 等の golden 追加）— 別 issue で管理
  - [ ] `core::accepts` follow-up（import ランキング API / `suggested_destinations` 充填 / `[extension_compounds]` loader）— 別 issue で管理
- **M3 以降**: 未着手

後続 PR に切り出した既完了モジュールの残タスク:
- `core::index`: FTS5 virtual table（M3 search）/ `custom_fields` テーブル（M2 rules と同時可）
- `core::reconcile`: periodic timer driver（Tauri ランタイム層）/ `last_seen_at` / `created_at` の埋込（doctor が drift 判定に使う時）

M2 以降のモジュール別完了条件は §5 のマイルストーンを参照。

---

## 1. モノレポ構成

```
progest-app/
├── Cargo.toml                    # workspace
├── pnpm-workspace.yaml
├── crates/
│   ├── progest-core/             # ドメインロジック、FS抽象、規則エンジン、meta I/O、index
│   ├── progest-cli/              # CLI バイナリ（core を依存）
│   ├── progest-merge/            # git merge driver（.meta 用、single-purpose binary）
│   └── progest-tauri/            # Tauri IPC glue（薄層、core を呼ぶだけ）
├── app/                          # フロントエンド React + shadcn/ui
│   ├── src/
│   ├── public/
│   ├── package.json
│   └── tauri.conf.json
├── docs/
│   ├── REQUIREMENTS.md
│   ├── IMPLEMENTATION_PLAN.md
│   ├── _DRAFT.md              # 初期ドラフト（保存）
│   ├── architecture.md
│   ├── cli-reference.md
│   └── meta-format.md
├── LICENSES/                   # 同梱バイナリのライセンス
│   └── ffmpeg/
│       ├── COPYING.LGPLv2.1
│       ├── BUILD_INFO.md       # バージョン/フラグ/ソース入手先
│       └── README.md
├── scripts/
│   ├── install-git-hooks.sh
│   └── build-release.sh
├── .github/
│   └── workflows/
├── README.md
├── README.ja.md
├── CLAUDE.md
├── CONTRIBUTING.md             # DCO 運用
└── LICENSE                     # Apache 2.0
```

設計原則:
- **ロジックはすべて core に集約**。Tauri 層・CLI 層・フロントエンドはオーケストレーションのみ
- **パス型は抽象化**: `std::path::Path` を直接使わず、`progest_core::fs::ProjectPath` 経由（Windows 移植準備）
- **IO は trait 越し**: `FileSystem`, `MetaStore`, `Index` trait を core で定義、実装差し替え可能（テスト、Lua 拡張準備）

---

## 2. レイヤー設計

```
┌────────────────────────────────────────────────┐
│  app/ (React + shadcn)                         │
│    UI state, views, command palette            │
├────────────────────────────────────────────────┤
│  progest-tauri  (IPC glue)                     │
│    Tauri commands → core API                   │
├──────────┬─────────────────────────────────────┤
│ progest- │  progest-core                       │
│ cli      │    fs / meta / rules / index /      │
│ (clap)   │    search / thumbnail / watch /     │
│          │    reconcile / template / ai        │
├──────────┴─────────────────────────────────────┤
│  OS / FS / SQLite+FTS5 / keychain / ffmpeg     │
└────────────────────────────────────────────────┘
```

progest-core のモジュール:
- `core::fs` — パス抽象、ignore、scanner
- `core::meta` — .meta I/O、原子書込、pending queue
- `core::identity` — UUID 発行、fingerprint、複製検出
- `core::rules` — 規則 DSL パーサ、評価、違反検出、継承解決
- `core::accepts` — `.dirmeta.toml` の `[accepts]` パース、エイリアス解決、effective 計算、placement 違反検出、インポート先ランキング
- `core::index` — SQLite+FTS5 schema、upsert、query
- `core::search` — DSL パーサ、クエリプラン、実行
- `core::watch` — notify ラッパー、三段構え制御
- `core::reconcile` — startup scan、periodic reconcile、drift detector
- `core::thumbnail` — 生成キュー、キャッシュ管理、LRU
- `core::template` — テンプレート入出力
- `core::ai` — BYOK クライアント、keychain 連携
- `core::rename` — preview、apply、undo history
- `core::import` — インポート実体（copy/move）、原子トランザクション、accepts と rename preview の一体適用
- `core::doctor` — 整合性診断

---

## 3. データフロー

### 3.1 変更検知・反映
```
FS change
   ↓
notify event   ─┐
                ├→ debounce → change queue → reconcile worker
periodic timer ─┘                                   ↓
                                             meta resolver
                                                    ↓
                                             index writer (SQLite transaction)
                                                    ↓
                                             UI notifier (Tauri event emit)
```

### 3.2 クエリ
```
UI input → DSL parser → query plan → SQLite/FTS5 → hydrate (meta attach) → UI
```

### 3.3 リネーム
```
pattern → preview builder → violation check → UI confirm
   ↓
bulk apply (atomic):
  begin transaction
    for each file:
      move(file, new_path)
      move(meta, new_meta_path)
      update index row
    push rename record to history
  commit or rollback all
```

---

## 4. ストレージレイアウト

プロジェクト内:
```
project-root/
├── .progest/
│   ├── project.toml
│   ├── rules.toml
│   ├── schema.toml
│   ├── views.toml
│   ├── ignore
│   ├── thumbs/              # gitignore
│   ├── index.db             # gitignore
│   └── local/               # gitignore
│       ├── history.json
│       ├── logs/
│       └── pending/
├── .gitignore               # init 時自動生成 / 追記
├── .gitattributes           # init 時自動生成 / 追記
└── assets/
    ├── .dirmeta.toml        # ディレクトリ自体のメタ + [accepts] セクション
    ├── foo.psd
    └── foo.psd.meta
```

`.dirmeta.toml` の `[accepts]` 抜粋:
```toml
[accepts]
inherit = false             # true で親の accepts を union
exts = [".psd", ".tif", ":image"]
mode = "warn"               # strict | warn (default) | hint | off
```

SQLite schema（主要テーブル）:
```sql
CREATE TABLE files (
  file_id TEXT PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  fingerprint TEXT NOT NULL,
  source_file_id TEXT,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  size INTEGER,
  mtime INTEGER,
  created_at TEXT,
  last_seen_at TEXT
);
CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_fingerprint ON files(fingerprint);

CREATE TABLE tags (
  file_id TEXT NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
  tag TEXT NOT NULL,
  PRIMARY KEY (file_id, tag)
);
CREATE INDEX idx_tags_tag ON tags(tag);

CREATE TABLE custom_fields (
  file_id TEXT NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
  key TEXT NOT NULL,
  value_text TEXT,
  value_int INTEGER,
  value_real REAL,
  PRIMARY KEY (file_id, key)
);
CREATE INDEX idx_custom_key_text ON custom_fields(key, value_text);
CREATE INDEX idx_custom_key_int ON custom_fields(key, value_int);

CREATE VIRTUAL TABLE fts_files USING fts5(
  path,
  name,
  notes,
  tags_text,
  content='',
  tokenize='trigram'
);

CREATE TABLE violations (
  file_id TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  category TEXT NOT NULL,     -- naming | placement
  reason TEXT,
  severity TEXT,
  suggested_names TEXT,       -- JSON (naming)
  suggested_destinations TEXT,-- JSON (placement)
  detected_at TEXT,
  PRIMARY KEY (file_id, rule_id)
);
CREATE INDEX idx_violations_category ON violations(category);

CREATE TABLE history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  applied_at TEXT NOT NULL,
  op_kind TEXT NOT NULL,      -- rename | tag | meta_edit
  operations TEXT NOT NULL,   -- JSON array (forward operations)
  inverse TEXT NOT NULL,      -- JSON array (operations for undo)
  undone INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_history_applied_at ON history(applied_at);
```

---

## 5. マイルストーン

### M0 — Skeleton（0.5ヶ月、ほぼ完了）
**目的**: 開発基盤を動く状態にする。

- [x] Cargo workspace（resolver v3、Rust 1.95、edition 2024）
- [x] `progest-core` / `progest-cli` / `progest-merge` / `progest-tauri` の 4 クレートスキャフォールド（`cargo build --workspace` 通る）
- [x] Vite + React 19 + TypeScript の `app/`（`pnpm build` 通る）
- [x] pnpm workspace（`@progest/app`）
- [x] Tauri v2 シェル（`pnpm tauri:dev` でウィンドウ起動確認）
- [x] GitHub Actions: `check`（fmt + clippy + tsc）, `test`, mac `build`
- [x] mise タスク（check/test/build/fmt/dev/tauri-dev/tauri-build/cli）
- [x] lefthook による pre-commit/commit-msg/pre-push hook
- [x] CLI サブコマンド骨格（`init`/`scan`/`doctor`/`lint`/`search` を todo!() で定義）
- [ ] shadcn/ui 初期化 → **M3 で導入**（UI 実装が始まるタイミング）
- [ ] アイコン差し替え → **M5**（現在は placeholder）
- [ ] macOS DMG / notarization → **M5**

完了条件: macOS で `mise run tauri-dev` がウィンドウ表示、`mise run check` 全グリーン、CI 全グリーン。

### M1 — Core data layer（1ヶ月）
**目的**: `.meta` が作れて、走査が通り、インデックスに入る。

- `core::fs` — ignore parser、scanner
- `core::identity` — UUIDv7 発行、blake3 fingerprint
- `core::meta` — TOML I/O（`.meta` / `.dirmeta.toml`）、原子書込、pending queue
- `core::index` — SQLite schema、migration、upsert
- `core::watch` — notify ラッパー
- `core::reconcile` — startup scan + periodic reconcile（watch なしでも一貫）
- CLI: `init`, `scan`, `doctor`
- 複製検出、孤児検出のユニット/統合テスト

完了条件: `progest init && progest scan` で 1万ファイル程度の fixture が 5秒以内にインデックス化、孤児 .meta を doctor が検出。

> 補足（2026-04-22 合意）: `core::reconcile` は scan 時に **未登録ファイルへ `.meta` を自動生成** する方針。ただし上記「5秒以内」は **SQLite index 書込みまで** の計測値を指し、`.meta` 原子書込み（温度差で初回スキャンは長くなる）は budget に含めない。2 回目以降の scan は size+mtime cheap compare で `.meta` 書込みが発生しないため、5s 要件は incremental scan で担保する。

### M2 — 命名規則エンジン + 配置規則（1ヶ月）
**目的**: ルールで lint と rename ができる。配置違反（placement）も同じ lint パイプラインで扱える。

DSL の正規仕様は [NAMING_RULES_DSL.md](./NAMING_RULES_DSL.md)（parser / evaluator は同文書と bit-for-bit 一致させる）。

- `core::rules` — DSL パーサ、2層規則、継承解決、勝利 rule_id トレース
- テンプレート構文（`{prefix}_{seq:03d}` 等）
- 制約規則（charset、casing、forbidden_chars 等、AND 合成）
- 違反検出、修正提案生成
- `core::accepts` — `.dirmeta.toml` の `[accepts]` パース、`schema.toml` のエイリアス解決、effective 計算（opt-in 継承）、placement 違反検出、インポート先ランキング算出
- 組み込みエイリアス（`:image`, `:video`, `:audio`, `:raw`, `:3d`, `:project`, `:text`）の構成拡張子を確定し `docs/` に記載
- `core::naming` — AI 非依存の機械的命名整理。pipeline（`remove_copy_suffix` → `remove_cjk` → `convert_case`）、NameCandidate（literal + 穴）モデル、fill-mode（`prompt` / `placeholder[:STR]` / `skip`）、`.progest/project.toml [cleanup]` loader、`core::rules::template` の private case fn を移管（`heck` crate 差し替え、PascalCase→snake_case 対応）、violation.suggested_names[] の機械的充填
- `core::history` — 操作ログ（rename / tag / meta_edit / import）、inverse 生成、undo/redo
- `core::rename` — preview、apply（原子トランザクション）、history 連携、naming の NameCandidate を入力に受ける
- CLI: `lint`（placement カテゴリ統合）, `rename --preview|--apply`, `undo`, `redo`, `clean`（`progest clean <path> [--case snake|kebab|camel|pascal] [--strip-cjk] [--strip-suffix] [--fill-mode prompt|placeholder[:STR]|skip] [--apply]`）
- ゴールデンテスト（naming / placement 評価結果を YAML に固定）

完了条件: fixture プロジェクトに対して lint が naming / placement 両方の違反を期待通り検出、rename preview と apply が動く、undo で戻せる。

### M3 — 検索とビュー（1ヶ月）
**目的**: クエリ駆動でファイルを操作できる UI。配置違反の可視化とディレクトリ単位の accepts 編集 UI も同時に提供する。

- `core::search` — DSL パーサ（key:value + 自由テキスト + ブール）、クエリプラン
- FTS5 + trigram の設定、日本語検索の基本動作
- コマンドパレット UI（shadcn Command + Dialog）
- ツリービュー、フラットビュー、保存済みビュー
- ディレクトリインスペクターパネル（accepts 編集フォーム: chip input + inherit チェックボックス + mode セレクタ）
- flat view / ツリー上の placement 違反バッジ（naming とは別色）
- `is:misplaced` クエリサポート
- views.toml の I/O
- CLI: `search`, `tag add|remove|list`
- カスタムフィールドのクエリ対応

完了条件: UI で `tag:foo type:psd is:violation` / `is:misplaced` 相当が 100ms 以下で返る、保存済みビューが永続化される、ディレクトリインスペクターで accepts を編集して `.dirmeta.toml` に反映される。

### M4 — サムネ + 外部連携 + AI + テンプレート（1ヶ月）
**目的**: 価値提案を完成させる。accepts を踏まえた D&D import / CLI import もここで完結する。

- `core::thumbnail` — 生成キュー、LRU キャッシュ
  - 画像（image crate）→ 動画（ffmpeg 子プロセス）→ PSD（psd crate）の順で実装
- `core::import` — インポート実体（copy / move）、原子トランザクション、accepts ランキング + rename preview の一体適用、history への単一エントリ記録
- 外部連携: D&D 受入（flat view は accepts サジェスト、tree view は mismatch 確認ダイアログ）、外部アプリで開く、D&D 出
- 複数ファイルの一括 import UI（一覧確認モーダル）
- CLI: `progest import <files...> [--dest|--auto|--move|--dry-run|--format json|text]`（対話 / 非対話両対応、tty 検出）
- `core::template` — 書出・読込、`progest init --template <path>`、テンプレートに `.dirmeta.toml`（accepts）を含める
- `core::ai` — BYOK クライアント、keychain 連携、簡易 UI

完了条件: 画像・動画・PSD のサムネが出る、D&D でファイル追加（accepts サジェストが動く）、CLI `progest import` が対話・非対話両方で動く、テンプレートから空プロジェクトが作れる（accepts 含む）、AI 提案が動く。

### M5 — 仕上げ・リリース（0.5ヶ月 + 予備1ヶ月）
**目的**: 公開可能な品質に整える。

- i18n（UI 日英、i18next / react-i18next、`app/public/locales/{ja,en}/*.json`）
- エラー処理 UI、`progest doctor` GUI、ヘルプ
- チュートリアル / ドキュメント
- macOS notarization、code signing、DMG 配布
- Homebrew Cask 準備（公開は v1 後）
- 最終 QA、パフォーマンス回帰確認
- v1.0 リリース

完了条件: DMG を配布、起動して実案件で触れる、ドキュメントが揃っている。

---

## 6. 主要依存クレート

| 用途 | クレート |
| --- | --- |
| 非同期 | tokio |
| エラー | anyhow (app), thiserror (lib) |
| ログ | tracing, tracing-subscriber |
| シリアライズ | serde, toml, serde_json |
| FS 監視 | notify, notify-debouncer-full |
| SQLite | rusqlite (+ bundled, FTS5 feature) |
| ハッシュ | blake3 |
| ID | uuid (v7, feature `v7`) |
| 画像 | image |
| PSD | psd |
| 動画 | 子プロセス（ffmpeg LGPL 2.1+ ビルド、同梱） |
| キーチェーン | keyring |
| CLI | clap (derive) |
| 正規表現 | regex |
| パステスト | tempfile |
| Tauri | tauri (v2) |
| i18n（フロント） | i18next, react-i18next, i18next-browser-languagedetector |
| git（v1.1） | git2 |
| Lua（v2） | mlua |

---

## 7. テスト戦略

### 7.1 ユニット
- core 各モジュール、純粋ロジック
- 規則評価はゴールデンテスト（入力 fixture → 期待 YAML）

### 7.2 統合
- 実 FS（tempdir）でシナリオテスト
- scan → rule → rename → search のパイプラインを通す
- watch flaky 対策: イベントタイムラインを模倣する `MockFsWatcher`

### 7.3 E2E
- CLI 起点、サンプルプロジェクト fixture 3種:
  - 映像（scene/cut 命名、動画素材）
  - ゲーム（asset 種別、テクスチャ/モデル混在）
  - 3DCG（psd/blend/fbx、バージョン付き）

### 7.4 手動 QA
- macOS 実機、Photoshop/Blender/Premiere 起動中のファイル操作
- 10万ファイル fixture での起動・検索・rename

### 7.5 カバレッジ目標
- core: 80% 以上
- cli: 60% 以上（E2E 補完）

---

## 8. リスクと対策

| リスク | 対策 |
| --- | --- |
| watch 不安定（イベント欠落） | reconcile + startup scan 三段構え、tests で欠落シナリオ |
| .meta 孤児化 | fingerprint + reconcile、`progest doctor`、復旧 UI |
| ffmpeg 配布サイズ | LGPL ビルドで機能絞り込み、将来システム ffmpeg 優先化 |
| ffmpeg ライセンス遵守漏れ | ビルドスクリプトでフラグ固定、LICENSES/ 配置を CI 検証、About 画面自動生成 |
| 規則評価デバッグ困難 | rule_id trace 必須化、`progest lint --explain` |
| AI 応答失敗 | タイムアウト、リトライなし、UI でフォールバック |
| SQLite WAL 競合 | 単一プロセス前提、Tauri/CLI の同時起動はロック検出して警告 |
| 大量 fixture での性能未達 | M3 で bench 導入、継続監視 |
| macOS notarization 遅延 | M5 前に一度 practice 実施 |

---

## 9. パフォーマンスベンチマーク

M3 時点で導入、M5 で合格確認:

| ベンチ | 目標 |
| --- | --- |
| scan 10,000 files | < 5s |
| scan 100,000 files | < 60s |
| search 単純 (`tag:foo`) | < 50ms |
| search 複合（3条件 AND） | < 100ms |
| rename preview 1,000 files | < 500ms |
| サムネ画像生成 | > 10/s |
| 常用メモリ（1万ファイル） | < 500 MB |

`criterion` crate でマイクロベンチ、`hyperfine` で CLI E2E 計測。

---

## 10. 品質ゲート

- `cargo fmt --check` 合格
- `cargo clippy --all-targets -- -D warnings` 合格
- `cargo test --workspace` 全グリーン
- ベンチ目標 80% 以上達成
- `cargo deny` 合格（依存ライセンス・脆弱性）

---

## 11. 配布

### v1.0
- macOS: notarized DMG、GitHub Releases
- CLI 単独: `cargo install progest-cli`（crates.io）
- ソースコード: GitHub
- 配布物内包物:
  - `LICENSE`（Apache 2.0）
  - `LICENSES/ffmpeg/`（LGPL 全文、ビルド情報、ソース入手 URL）
  - About 画面: 本体ライセンス + 同梱 OSS ライセンス一覧 + ffmpeg ソース入手手段
- Release notes に ffmpeg ビルド用ソース tarball リンクを必ず添付（LGPL 遵守）

### v1.1
- Windows: MSIX または code-signed installer
- macOS: Homebrew Cask 登録

### v2+
- Linux: AppImage / flatpak
- Registry（テンプレ）: 独自サービス検討

---

## 12. v1 後のロードマップ要約

| フェーズ | 主要項目 |
| --- | --- |
| v1.1 | Windows 完全対応、Blender サムネ、lindera、git URL テンプレ、OS ファイラー統合、複数プロジェクト |
| v1.2 | 履歴/undo 拡張、メタ暗号化、プロジェクト間参照 |
| v2.0 | Lua 拡張 API、クラウド同期（オプトイン・有償候補）、テンプレート Registry |
| v2.x | Linux 正式サポート、ローカル LLM、DCC 向け連携 API |

詳細な優先度付けは v1.0 リリース後のユーザーフィードバックで再評価。
