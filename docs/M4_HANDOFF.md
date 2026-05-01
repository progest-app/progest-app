# M4 引き継ぎメモ

M3 完了後に M4 を進めるための、次セッション向けハンドオフ。
**M4 に関わる実装に入る前に必ず目を通す。** 進捗に合わせて更新していく。

最終更新: 2026-05-01（**M4 全完了**）

---

## 1. 現在地

- M3 検索とビュー 完了（2026-04-30）。全 PR landed + 実機 smoke テスト済み。
- M4 kickoff 完了。技術決定・PR 分割・着手順序を本ドキュメントで管理する。
- `core::import` + CLI import 完了（PR #59, #60 landed）。
- `core::thumbnail` + CLI thumbnail + reconcile hook 完了（PR #61 landed）。
- `core::template` + CLI template + Tauri IPC 完了（PR #63 landed）。
- D&D import UI + thumbnail 統合完了（PR #65 landed）。
- ファイル削除 + toast + 進捗レポート完了（PR #66, #68, #69 landed）。
- **Windows 対応 W1–W6 完了**（PR #70–#75 landed）。v1.1 defer → M4 に前倒し。
- **`core::ai` + UI 完了**（PR #77, #78 landed）。BYOK + keychain + 命名/タグ/notes/配置先提案 + Settings dialog。
- **M4 polish 完了**: AI config キャッシュ（SettingsContext 集約）、apply 後インスペクタ即時反映、per-section AI ボタン UX、DotmSquare5 スピナー統一、dotfile 除外。
- **M4 全モジュール完了**。次は M5（ドキュメント + リリース準備）。

---

## 2. M4 kickoff 決定事項

### 2.1 スコープ

4 モジュール全て M4 で実装（v1.1 defer なし）:

| モジュール | 内容 |
|---|---|
| `core::import` | copy/move + accepts ランキング + rename preview 一体適用 + history `Operation::Import` |
| `core::thumbnail` | 生成キュー + LRU キャッシュ（画像 / PSD / 動画） |
| `core::template` | プロジェクトテンプレート書出/読込 + `progest init --template` |
| `core::ai` | BYOK クライアント + keychain 連携 + 命名提案 / タグ提案 / notes 自動生成 / 配置先提案 |

### 2.2 着手順序

import → thumbnail → template → AI（CLI first → D&D/UI）

### 2.3 PR 分割（フェーズ単位、5〜7 PR 想定）

| # | PR | 内容 | 依存 |
|---|---|---|---|
| 1 | `core::import` | accepts ランキング + import preview + atomic apply + history `Operation::Import` + Conflict ↔ Warning 整理 | M2 rename/accepts/sequence |
| 2 | CLI `progest import` | `<files...> [--dest\|--auto\|--move\|--dry-run\|--format]`、tty 検出で対話/非対話、sequence 集約表示 | #1 |
| 3 | `core::thumbnail` | 生成キュー + LRU キャッシュ + 画像 (image crate) + PSD (psd crate) + 動画 (ffmpeg sidecar) | M1 index |
| 4 | D&D import UI + thumbnail 統合 | Tauri D&D wire + import 確認モーダル + accepts サジェスト + grid view サムネ表示 | #1 #3 |
| 5 | `core::template` + CLI/UI | 書出/読込 + `progest init --template` + GUI テンプレート選択 | core::project |
| 6 | `core::ai` + UI | BYOK クライアント + keychain + 命名/タグ/notes/配置先提案 UI | M3 search/tag |
| 7 | M4 polish + docs | doctor staging cleanup + undo tag/meta_edit wiring + ドキュメント更新 | 全部 |

### 2.4 技術決定

- **ffmpeg**: sidecar バイナリ同梱（LGPL 準拠、子プロセス呼び出し）。`LICENSES/ffmpeg/` に BUILD_INFO + COPYING.LGPLv2.1 を既設。動画フレーム抽出のみ使用。静的リンクしない
- **thumbnail 出力**: WebP lossy (quality 80)、長辺 256px にリサイズ。保存先 `.progest/thumbs/<file_id>.webp`。LRU キャッシュ（ディスク容量ベースで上限設定）
- **import デフォルト**: copy（元ファイルを残す）。`--move` フラグで移動に切替。GUI は確認ダイアログで選択可能。破壊的操作は明示 opt-in の設計原則に合致
- **AI スコープ**: 命名提案 + タグ提案 + notes 自動生成 + 配置先提案。BYOK（Anthropic / OpenAI）、keychain 連携。`core::ai` に provider 抽象 + 各 feature 用 prompt template

---

## 3. `core::import` 設計メモ

M2_HANDOFF §5 の設計メモを正として、以下に具体化する。

### 3.1 基本フロー

1. ユーザーが D&D / CLI `progest import <paths...>` でファイル群を投入
2. `sequence::detect_sequences(paths)` で sequence / singletons 分離
3. `accepts::suggested_destinations(catalog, schema, ext)` で配置先ランキング算出
4. `import::build_preview(requests)` で import preview 生成（rename preview と同パターン）
5. preview → confirm → `import::apply()` で atomic commit
6. history `Operation::Import` append（sequence は同じ `group_id` で batch）

### 3.2 accepts ランキング API（M2 follow-up を M4 で実装）

`core::accepts::suggested_destinations(fs, meta_store, project_root, ext) -> Vec<SuggestedDestination>`

- プロジェクト内の全 `.dirmeta.toml` を walk し、各 dir の effective_accepts を計算
- `ext` が accepts に含まれる dir をスコア順で返す（own > inherited、specificity: literal `.psd` > alias `:image`）
- sequence 全体で 1 回だけ走らせ、全 member に同じ候補を適用（ext 統一保証）

### 3.3 Conflict 種別

| Conflict | 意味 | 解決 |
|---|---|---|
| `DestExists` | コピー/移動先に同名ファイルが既にある | rename / skip / overwrite |
| `SourceMissing` | import 元ファイルが消えている | skip |
| `SourceIsProject` | import 元がプロジェクト内のファイル | 警告（内部移動は rename コマンドを使うべき） |
| `PlacementMismatch` | 指定先 dir が ext を accepts しない | 警告 + サジェスト |

### 3.4 atomic apply

`core::rename::Rename::apply` と同パターン:
- `.progest/local/staging/<uuid>/` 経由 stage → commit → rollback
- FS copy/move + `.meta` 新規生成 + index 登録を一体
- history `Operation::Import` append（bulk auto `group_id`）
- `ApplyWarning::ImportFailed` variant 追加（B-4）

### 3.5 Conflict ↔ Warning 語彙整理（B-3）

M4 で import の Conflict variants が確定したタイミングで、`ApplyOutcome` の `{ conflicts, warnings }` を再構造化する。preview phase の blocking condition = `Conflict`、apply post-commit の recoverable = `Warning` の意味的区分は維持。

---

## 4. 横断的に忘れてはいけないこと

- **ffmpeg LGPL compliance**: sidecar binary 同梱、静的リンク禁止、`LICENSES/ffmpeg/` に BUILD_INFO + COPYING 配置済み。M4 #3 で ffmpeg 呼び出し実装時に `docs/LESSONS.md` へ注意事項を追記
- **ドキュメント更新**: 各 PR 完了時に `docs/IMPLEMENTATION_PLAN.md §0`、本ドキュメント（M4_HANDOFF）の履歴、`docs/LESSONS.md` に追記、古くなった記述の削除
- **性能**: thumbnail 生成は非同期キュー + バックグラウンド。UI がブロックされないこと
- **破壊的操作**: import `--move` は preview → confirm。copy はデフォルトだが大量ファイルは確認
- **M2 follow-up 合流**: `core::accepts` の `suggested_destinations` / `[extension_compounds]` / import ランキング API は `core::import` #1 で同時実装
- **undo wiring**: M4 #7 で tag_add / tag_remove / meta_edit / import の undo/redo を全 wire

---

## 5. thumbnail 実装メモ

- WebP エンコーダ: `image` crate の `WebPEncoder::new_lossless()` を使用。lossy エンコーダ（quality 80）は `image` 0.25 では純正サポートなし。将来的に `webp` crate（libwebp wrapper）追加でファイルサイズ最適化可能
- HEIC: `libheif-rs` 1.1.0 使用。`LibHeif::new()` → `heif.decode(&handle, ...)` パターン。システム `libheif` + `pkgconf` 必須（`brew install libheif pkgconf`）。Cargo feature `heic`（default on）で切替可能
- ffmpeg: `find_ffmpeg()` が `PROGEST_FFMPEG_PATH` env > sidecar adjacent > system PATH の順で探索。なければ動画サムネをスキップ（graceful degradation）
- LRU: ファイル mtime をプロキシ。`get()` で touch、`evict_lru()` で mtime 昇順ソート → oldest から削除。テストでは mtime 精度が低い（macOS 1秒）ため明示的に `set_modified()` でバックデート
- Cache key: `{file_id}_{fingerprint_hex_32}_{size}.webp`。fingerprint 変更 → key 変更 → orphan は `clean` で回収

---

## 6. 履歴

- 2026-04-30: 初版作成。M4 kickoff 決定事項記録。
- 2026-04-30: `core::thumbnail` + CLI + reconcile hook 完了。
- 2026-05-01: Windows 対応 W1–W6 完了（PR #70–#75）。dunce + COLLATE NOCASE + file lock retry + reserved names + OneDrive placeholder + platform UI + CI/NSIS。
- 2026-05-01: `core::ai` + UI 完了（PR #77, #78）。BYOK Anthropic/OpenAI + keychain + 命名/タグ/notes/配置先提案 + Settings dialog。
- 2026-05-01: M4 polish 完了。AI config キャッシュ（SettingsContext）、apply 後インスペクタ即時反映（localHit）、per-section AI ボタン、DotmSquare5 スピナー統一、dotfile 除外（`.*`）。
