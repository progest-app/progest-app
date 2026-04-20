# Progest

> クリエイティブ制作のためのメタデータファースト・ファイル管理ツール。

Progest はローカル完結の OSS ツール。命名規則の検証・強制、サイドカーメタデータ（`.meta`）、プロジェクト横断の高速検索を、映像・ゲーム・3DCG・VFX などのクリエイティブパイプラインに持ち込む。

**ステータス:** 設計・開発初期（pre-alpha）。v1.0 は macOS 先行で 2026年Q3 目標。

[English README](./README.md)

---

## なぜ作るか

クリエイティブ案件はファイルに溺れる。レイヤー、バージョン、レンダー、参照、キャッシュ。既存ツールは独自アセットサーバーにファイルを閉じ込めるか、新人が入った瞬間崩れる場当たりフォルダ慣習を放置するかのどちらか。

Progest は既存ディレクトリにそのまま寄り添い、現場が既に持っているルールを学習し、強制・検索・共有できる形に変換する。ファイルは人質にしない。

**設計原則**

- **ファイルシステムが真実源**: `.meta` をファイルの隣に書くだけ。取り込みもしないし、隠さない
- **規則ファースト**: 命名規則は後付け lint プラグインではなく、一級市民
- **CLI = GUI**: GUI でできることは全て CLI でできる。パイプラインと自動化のために
- **git フレンドリー**: `.meta` は素の TOML。マージドライバ前提、diff 可、レビュー可
- **ロックインしない**: Progest をアンインストールしても、ファイルはただのファイルとして残る

---

## コア機能（v1 MVP）

| 機能 | 内容 |
| --- | --- |
| 命名規則 DSL | テンプレート構文 `{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}` + 緩い制約規則（charset、casing、禁止文字等） |
| 規則モード | `strict` / `warn`（デフォルト）/ `hint` / `off`、ディレクトリ単位で継承・上書き |
| `.meta` サイドカー | TOML、UUID ベース、スキーマ版管理、セクション分離で衝突低減 |
| 複製の扱い | コピーは常に新 UUID を発行、`source_file_id` に元 ID を記録。衝突は UI で解決 |
| 検索 | `tag:character type:psd is:violation` 形式の DSL、GUI コマンドパレットと CLI で共通 |
| フラットビュー / 保存済みビュー | クエリ駆動の閲覧、`.progest/views.toml` でスマートフォルダをチーム共有 |
| サムネイル | 画像（PNG/JPEG/WebP/TIFF/HEIC）、動画（ffmpeg）、PSD 埋込プレビュー抽出 |
| AI 命名支援 | BYOK（OpenAI / Anthropic 互換）、API キーは OS キーチェーン、ローカル完結 |
| テンプレート | 構造・規則・スキーマ・保存済みビューを単一 TOML として書き出し |
| 外部連携 | D&D 双方向、外部アプリで開く |
| CLI | `progest init/scan/lint/rename/tag/search/doctor/meta-merge` |
| i18n | 日英両対応 UI |

---

## ロードマップ

| バージョン | 主要項目 |
| --- | --- |
| **v1.0**（macOS） | 上記 MVP 機能。2026年Q3 目標 |
| **v1.1** | Windows 対応（長パス、ファイルロック耐性、OneDrive 検出）、git URL テンプレ、Blender サムネ、lindera 形態素検索、OSファイラー統合 |
| **v1.2** | 履歴/undo 拡張、メタ暗号化、プロジェクト間参照 |
| **v2.0** | Lua 拡張 API（サンドボックス）、クラウド同期（オプトイン有償候補）、テンプレート Registry |
| **v2.x** | Linux 正式対応、ローカル LLM、DCC 連携 API |

---

## 技術スタック

- **コア**: Rust
- **UI シェル**: [Tauri](https://tauri.app/) v2
- **UI**: React + [shadcn/ui](https://ui.shadcn.com/)
- **インデックス**: SQLite + FTS5
- **モノレポ**:
  - `crates/progest-core` — ドメインロジック
  - `crates/progest-cli` — CLI
  - `crates/progest-merge` — `.meta` 用 git merge driver
  - `crates/progest-tauri` — Tauri IPC 接続層
  - `app/` — フロントエンド

詳細は [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) 参照。

---

## 開発セットアップ

Progest は [mise](https://mise.jdx.dev/) を単一のソースオブトゥルースとしてツールチェーン（Rust / Node / pnpm）と開発タスクを管理する。クローン後の唯一の前提は mise インストール。

```bash
# 初回のみ: mise インストール（macOS / Linux）
curl https://mise.run | sh

# リポジトリルートで実行: mise.toml に固定されたバージョンを自動導入
mise install

# 依存インストール（他タスクからも必要時に呼び出される）
mise run install
```

### よく使うコマンド

| コマンド | 内容 |
| --- | --- |
| `mise run check` | rustfmt `--check` + clippy `-D warnings` + `tsc --noEmit`。**コミット前に必ず通す** |
| `mise run test` | `cargo test --workspace` |
| `mise run build` | `cargo build --workspace` + `vite build` |
| `mise run fmt` | `cargo fmt --all` |
| `mise run dev` | Vite 開発サーバーのみ起動（フロントのみで反復する時） |
| `mise run tauri-dev` | デスクトップアプリを開発モードで起動（Vite + Tauri ウィンドウ） |
| `mise run tauri-build` | リリース用デスクトップバンドル |
| `mise run cli -- <args>` | `progest` CLI を実行（例: `mise run cli -- scan`） |

`cargo test` や `pnpm --filter @progest/app dev` といった素のコマンドも動くが、`mise run` は CI と完全同一なので、ローカルが通れば CI も通る。

### プロジェクト構成

```
.
├── app/                     # Vite + React 19 + TS フロントエンド（pnpm workspace）
├── crates/
│   ├── progest-core/        # ドメインロジック本体
│   ├── progest-cli/         # `progest` バイナリ
│   ├── progest-merge/       # .meta 用 git merge driver
│   └── progest-tauri/       # Tauri v2 デスクトップシェル（tauri.conf.json 同梱）
├── docs/                    # 要件定義・実装計画
└── mise.toml                # ツールチェーン固定 + タスク定義
```

---

## ドキュメント

- [docs/REQUIREMENTS.md](./docs/REQUIREMENTS.md) — 要件定義書
- [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) — 実装計画・マイルストーン
- [docs/_DRAFT.md](./docs/_DRAFT.md) — 初期要件ドラフト（履歴保存）
- [CLAUDE.md](./CLAUDE.md) — Claude Code 作業者向け指示

---

## 開発状況

現在は要件定義と計画策定フェーズ。コードは M0（スケルトン）着手段階。

特にパイプライン現場（映像・ゲーム・VFX）からのフィードバックは大歓迎。GitHub Issue で気軽に。

---

## ライセンス

Progest は **Apache License 2.0** で提供。[LICENSE](./LICENSE) を参照。

### 同梱する第三者ソフトウェア

動画サムネ生成のために [FFmpeg](https://ffmpeg.org/) を子プロセスとして同梱。同梱ビルドは **LGPL 2.1+ 構成のみ**（`--enable-gpl` および `--enable-nonfree` は使用しない）。ライセンス全文・ビルド構成・ソースコード入手手段は `LICENSES/ffmpeg/` およびアプリ内「About」画面で提供（LGPL 義務遵守）。

### コントリビューション

[Developer Certificate of Origin](https://developercertificate.org/)（DCO）を採用。全コミットに `Signed-off-by: Your Name <you@example.com>` を付与（`git commit -s` で自動付与可能）。CLA は採用しない。

### ユーザー生成コンテンツ

あなたが作成した規則ファイル・スキーマ・保存済みビュー・テンプレートは完全にあなたのもの。Progest は独自ライセンスを主張しない。v2 でテンプレート Registry を開設する際は、投稿者がテンプレート毎にライセンスを明示する運用。

---

## 名前の由来

*Progest* = **Pro**ject + Manage（suggest / digest / ingest 等）。プロジェクトファイルを管理し、咀嚼するためのツール。
