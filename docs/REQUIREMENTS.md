# Progest 要件定義書

作成日: 2026-04-20
ステータス: v1.0 MVP 要件（策定中）

## 1. プロダクト定義

### 1.1 一文要件
Progest は、既存のクリエイティブ案件ディレクトリに後付けで導入でき、命名規則の検証・強制、サイドカーメタデータ、プロジェクト横断の高速検索を提供するローカル完結型ツール。

### 1.2 ミッション
クリエイティブパイプラインにおけるファイル管理の無秩序を、現場のワークフローを壊さずに整える。「既に存在する暗黙ルールを明文化し、強制・検索・共有できる形に変換する」ことが中核価値。ファイルを独自形式に閉じ込めない。

### 1.3 対象ユーザー
- **主対象**: 映像・ゲーム・3DCG・VFX 等、パイプラインと命名規則の文化が既にある個人〜小規模スタジオ（5〜30人規模）のクリエイター
- **副対象**: イラスト・写真・音楽等、単独成果物中心のクリエイター（タグ検索用途）

### 1.4 非対象
- 大規模スタジオ向けアセット管理基盤（Shotgrid/ftrack/Perforce Helix の代替は目指さない）
- クラウド完結型 SaaS
- バージョン管理（git の代替・競合は狙わない）
- バイナリファイル内容の編集・レンダリング

### 1.5 差別化
- git 前提の .meta 設計（他ツールはDBロックイン）
- 命名規則エンジンが一級市民（他は後付けlintプラグイン）
- CLI が GUI と同等の一等市民（パイプラインに組み込み可能）
- OSS（既存商用アセット管理の隙間）

---

## 2. スコープ

### 2.1 v1.0 MVP（macOS 先行、6ヶ月想定）
| 機能 | 範囲 |
| --- | --- |
| プロジェクト管理 | `.progest/` ディレクトリ方式、`progest init` |
| サイドカーメタ | `file.ext.meta`（TOML、セクション分離） |
| ファイル ID | UUID（複製時は新規発行 + `source_file_id` 記録）、blake3 fingerprint |
| 命名規則エンジン | テンプレートDSL + 制約DSL、継承、4モード、rule_id トレース、rename preview、一括適用、ロールバック |
| 配置規則 (accepts) | ディレクトリごとの受入拡張子、カテゴリエイリアス、import 先サジェスト、`placement` lint |
| 連番 | ディレクトリローカル + 名前空間指定 |
| AI命名支援 | BYOK（OpenAI/Anthropic互換、簡易実装）、OS keychain保存 |
| FS監視 | startup scan + OS watch + periodic reconcile（三段構え） |
| 検索 | SQLite + FTS5 + trigram/N-gram、GitHub風 key:value DSL |
| ビュー | ツリー、フラット、保存済みビュー（.progest/views.toml） |
| コマンドパレット | GUI |
| CLI | init/scan/lint/rename/tag/search/import/doctor/meta merge |
| サムネ | 画像（image crate）、動画（ffmpeg）、PSD埋込（psd crate） |
| 外部連携 | D&D 双方向（accepts ベースのインポート先サジェスト付き）、外部アプリで開く |
| テンプレート | 単一TOML 書出・読込（ローカルファイルのみ） |
| i18n | UI 日英両対応（i18next / react-i18next） |
| ライセンス | Apache License 2.0 |

### 2.2 v1.1（MVP後）
- Windows 対応（長パス `\\?\`、ファイルロック耐性、rename 複数イベント吸収、OneDrive Placeholder 検出、大小文字正規化）
- git URL テンプレート参照（`progest init --template git@...`）
- Blender 埋込サムネ
- lindera 形態素検索
- 複数プロジェクト同時オープン
- OSファイラー統合（Finder クイックアクション / Explorer コンテキストメニュー）
- メタ変更履歴と undo

### 2.3 v2+
- Lua 拡張 API（sandbox、権限制御、on_import/on_rename/on_tag_change フック）
- クラウド同期 SaaS（有償候補）
- テンプレート Registry / マーケットプレイス
- Linux 対応強化
- ローカル LLM 対応（Ollama 等）

### 2.4 明示的な非スコープ
- DCC プラグイン提供（ユーザー実装に委ねる、API は公開）
- 動画・画像編集機能
- git 操作の代替
- パーミッション管理
- プロジェクト間の参照解決（単一プロジェクト完結）

---

## 3. 機能要件

### 3.1 プロジェクト管理

- プロジェクトルート = `.progest/` が存在するディレクトリ
- `progest init` で作成
- 初期化時オプション: `--template <path>`、`--no-rules`、`--schema <path>`

配置:
```
project-root/
├── .progest/
│   ├── project.toml       # プロジェクトID、名前、progest バージョン
│   ├── rules.toml         # 命名規則（共有）
│   ├── schema.toml        # カスタムメタスキーマ（共有）
│   ├── views.toml         # 保存済みビュー（共有）
│   ├── ignore             # ignoreルール（共有）
│   ├── thumbs/            # サムネキャッシュ（gitignore）
│   ├── index.db           # SQLite+FTS5 インデックス（gitignore）
│   └── local/             # ローカル固有（gitignore）
│       ├── history.json
│       ├── logs/
│       └── pending/       # 書込再試行キュー
├── .gitignore
├── .gitattributes         # meta merge driver 登録
└── （ユーザーファイル）
```

### 3.2 サイドカーメタ（.meta）

配置:
- ファイル: Unity 式隣接型。`foo.psd` の隣に `foo.psd.meta`
- ディレクトリ: ディレクトリ直下に `.dirmeta.toml`（各ディレクトリ 1つ、`.meta` スキーマに加えて §3.13 の `[accepts]` セクションを許容）

フォーマット: TOML。セクション分離で衝突頻度を下げる。

スキーマ例:
```toml
schema_version = 1
file_id = "0190f3d7-5dbc-7abc-8000-0123456789ab"   # UUIDv7 (RFC9562)
content_fingerprint = "blake3:abc123..."
source_file_id = ""                         # 複製元の file_id（複製以外は空）
created_at = "2026-04-20T10:00:00Z"

[core]
kind = "asset"                              # asset | directory | derived
status = "active"                           # active | archived | deprecated

[naming]
rule_id = "shot-assets-v1"
last_validated_name = "ch010_sc020_bg_forest_v03.psd"
last_validated_at = "2026-04-20T10:00:00Z"

[tags]
list = ["approved", "forest", "night"]      # ソート済み集合

[notes]
body = """
差し替え候補あり
"""

[custom]
scene = 20
shot = 10
asset_type = "bg"

[meta_internal]                             # マージしない、ローカル派生情報
last_seen_at = "2026-04-20T10:15:33Z"
```

書込み要件:
- 原子的（temp file `foo.psd.meta.tmp` → rename）
- 失敗時 `.progest/local/pending/` にキュー、後続で再試行
- 整合性: `file_id` と `content_fingerprint` は必須、それ以外は optional

### 3.3 ファイルアイデンティティと複製意味論

識別子:
- `file_id`: UUIDv7（RFC9562、時系列ソート可、uuid crate v1.11+）
- `content_fingerprint`: blake3 128bit truncated hex
- `source_file_id`: 複製元の file_id（複製以外は空文字列）

複製検出と処理:
- 新規パスに現れたファイルは常に新規 `file_id` 発行
- 同一 fingerprint を持つ既存ファイルがある場合、`source_file_id` にそれを記録
- 複製元側の `.meta` を上書きしない

衝突（conflict）の種類:
```rust
enum IdentityConflict {
    SameFileIdMultiplePaths,  // 同一 file_id が複数パスに存在
    MetaWithoutFile,          // .meta だけ残っている
    FileWithoutMeta,          // ファイルに .meta がない
    FingerprintCollision,     // 同内容が別 file_id で存在（warn のみ）
}
```

UI での解決肢:
```rust
enum CopyResolution {
    TreatAsMove,                       // 旧パスの .meta を新パスに付け替え
    DuplicateWithNewFileId { source_file_id: String },  // 複製扱い
    ReattachMeta,                      // 孤児 .meta を再結合
}
```

### 3.4 命名規則エンジン

二層構造:
1. **テンプレート規則**: 厳密なパターン `{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}`
2. **制約規則**: 緩い制約の集合（「日本語禁止のみ」「snake_case 要求のみ」等）

テンプレート構文サポート:
- `{prefix}` `{desc}`: 自由部分
- `{seq:03d}`: 0埋め連番
- `{version:02d}`: バージョン
- `{ext}`: 拡張子
- Casing 指定: `{desc:snake}` `{desc:kebab}` `{desc:camel}` `{desc:pascal}`
- 名前空間: `{seq@scene}`（scene キー単位で連番）

制約規則フィールド:
```toml
[[rules]]
id = "ascii-only"
applies_to = "./references/**"
mode = "warn"
charset = "ascii"                  # ascii | utf8 | no_cjk
casing = "any"                     # any | snake | kebab | camel | pascal
forbidden_chars = [" ", "　"]       # 全角/半角スペース等
max_length = 64
required_prefix = ""
required_suffix = ""
```

適用粒度:
- ディレクトリ単位で定義、継承、上書き（CSS カスケード式）
- 最近接祖先優先。子レイヤで同一 `id` を再定義するとルール単位 full replace（フィールド部分マージなし）。`kind` を変える置換は `override = true` 必須、同 `kind` なら省略可（parser が warning）。
- 規則評価結果は必ず「なぜその規則が勝ったか」をトレース（勝利規則 ID、継承チェーン）

DSL の正規仕様は [NAMING_RULES_DSL.md](./NAMING_RULES_DSL.md) を参照。parser / evaluator 実装は同文書と bit-for-bit 一致させる。

モード:
| モード | 挙動 |
| --- | --- |
| `strict` | 違反は保存・リネームを拒否 |
| `warn` (default) | 違反を可視化、lint バッジ、修正提案、一括適用可 |
| `hint` | 新規作成・リネーム時に候補提示のみ、既存違反は検出しない |
| `off` | 規則適用なし |

違反レポート必須フィールド:
- `file_id`, `path`, `rule_id`, `winning_rule_source`, `violation_reason`, `suggested_names[]`

rename preview/apply:
- 一括 rename は preview → confirm → apply の2段
- apply は原子的: .meta・実ファイル・index を一括トランザクション
- ロールバック: 直前 N 件（デフォルト 50）を `.progest/local/history.json` に記録、`progest undo`

履歴（undo）対象（v1）:
- rename 操作（単体・一括・移動）
- tag 操作（add / remove / set）
- meta 編集（custom field 変更、notes 書換）
- 操作単位で取り消し可、連続 undo / redo 対応

### 3.5 連番

- デフォルト: ディレクトリローカル
- 名前空間指定可: `{seq@scene}` は scene キー値ごとに独立カウント
- 既存ファイル解析から「次の番号」提案
- 欠番: デフォルト詰めない（設定可）
- 複数人の同時採番衝突時: 両採用リネーム提案（lint）

### 3.5.5 機械的命名整理

AI に依らず決定的な transform で命名候補を生成する機能。命名提案の基盤であり、AI 提案（§3.6）は opt-in の別経路。`suggested_names[]` の機械的充填と CLI `progest clean` の両方を駆動する。

パイプライン（固定正規順序、各 stage は独立に on/off 可）:

1. `remove_copy_suffix` — OS デフォルトの複製 suffix を stem 末尾で検出・除去。対象は `foo (2)` / `foo - Copy` / `foo - Copy (2)` / `foo のコピー` / `foo のコピー 2` の 3 系統。`v01` のようなバージョン token は非対象
2. `remove_cjk` — ひらがな・カタカナ・漢字を削除。連続ランごとに 1 穴（placeholder）を残し、位置情報を保持
3. `convert_case` — snake / kebab / camel / pascal に正規化。PascalCase 境界を含む完全な case 変換（`heck` crate）。穴は literal 扱いせず、ASCII 区間のみを対象

有効化:

- `.progest/project.toml [cleanup]` に既定値を宣言（team 共有）
- `convert_case` のみデフォルト on（opt-out）、`remove_copy_suffix` / `remove_cjk` はデフォルト off（opt-in）
- UI ダイアログ / CLI フラグで毎回上書き可能（per-action override）

候補のデータモデル:

- 候補は「literal 区間と穴の混在列」として保持し、fill-mode で解消するまで `String` に flatten しない
- ディスクに書き出す文字列へ穴が残ることは禁止（後述 fill-mode で必ず解消）

fill-mode（`progest clean` / rename apply 共通）:

| mode | 挙動 |
| --- | --- |
| `prompt` | 穴を 1 つずつ対話で埋める（TTY 既定） |
| `placeholder[:STR]` | 全穴を固定文字列で埋める（既定 STR=`untitled`） |
| `skip` | 穴が 1 つでも残る候補は rename 対象外（非 TTY 既定） |

text 出力時の穴表記:

- 視覚的に明らかな sentinel（例 `⟨cjk-1⟩`）を用い、`{}` は使わない（FS に有効な文字を含めると誤 rename / シェル展開事故の原因）
- `--format json` は構造化された `holes[]` フィールドを返す

lint `suggested_names[]`:

- 違反検出時、`[cleanup]` 既定値でパイプラインを走らせ候補を生成（デフォルト: case 変換のみ）
- 穴が残る場合は JSON に構造のまま保持、text 出力では sentinel 表記

### 3.6 AI命名支援

形態: BYOK（OpenAI / Anthropic 互換 API）

- API キー: OS keychain（keyring crate）に保存、プレーンテキスト保存禁止
- ユーザー明示起動のみ（自動化しない、opt-in ボタン）
- 機械的命名整理（§3.5.5）がデフォルト経路、AI は opt-in で別途発火
- 提案対象: テンプレート内の `{desc}` 等自由スロット
- コンテキスト: 規則定義 + 周囲ファイル名 + 既存タグ + （オプション）プロジェクト用語集
- notes 本文は明示的許可なしに送信しない（プライバシー）
- 失敗時は即エラー表示、自動リトライしない

### 3.7 検索

DSL: GitHub / Linear 風 key:value + 自由テキスト。

予約キー:
- `tag:<name>`, `-tag:<name>`
- `type:<ext>` / `kind:asset|directory|derived`
- `is:violation|orphan|duplicate|misplaced`
- `name:<glob>`, `path:<glob>`
- `scene:<int>` `shot:<int>` `status:<enum>` (カスタムフィールド)
- `created:<iso>..<iso>`, `updated:<iso>..<iso>`
- ブール: `-`（NOT）、カッコでグループ
- 自由テキスト: notes と name に対して FTS5 マッチ

日本語処理:
- v1: trigram/N-gram（FTS5 の tokenize='trigram' 相当、またはカスタムトークナイザ）
- v1.x: lindera オプション追加

保存済みビュー:
- `.progest/views.toml` に定義（チーム共有）
- 個人履歴は `.progest/local/history.json`

### 3.8 ビュー

- **ツリービュー**: 通常のディレクトリツリー + ファイル一覧
- **フラットビュー**: 現在ディレクトリの子孫をフラット表示、クエリ適用、グルーピング可
- **コマンドパレット**: Cmd+K 起動、検索 DSL + アクション起動（open/rename/tag/...）

### 3.9 CLI

GUI と同じコアを叩く一級市民。

サブコマンド:
```
progest init [--template <path>] [--schema <path>]
progest scan [--force]
progest doctor                  # 整合性診断
progest lint [<path>] [--format json|text]
progest rename --preview <pattern>
progest rename --apply <pattern>
progest undo [--steps N]
progest tag add <tag> <files...>
progest tag remove <tag> <files...>
progest tag list <files...>
progest search <query> [--format json|text]
progest import <files...> [--dest <path>] [--auto] [--move] [--dry-run] [--format json|text]
progest export-template --include structure,rules,schema,views --out <path>
progest meta merge <ours> <theirs> <base> --output <path>   # git merge driver
```

終了コード:
- `0`: 成功
- `1`: 違反検出あり（lint 等）
- `2`: ユーザーエラー（引数・設定）
- `3`: 内部エラー
- `4`: 競合（merge driver 等）

JSON 出力モード: `--format json` で全サブコマンド対応、CI/スクリプト連携前提。

### 3.10 サムネイル

対応形式 (v1):
- 画像: PNG, JPEG, WebP, TIFF, HEIC（image crate）
- 動画: MP4, MOV, WebM（ffmpeg 子プロセス）
- PSD: 埋込サムネ抽出（psd crate）

キャッシュ:
- 場所: `.progest/thumbs/`
- キー: `{file_id}_{fingerprint_short}_{size}.webp`
- 容量上限: デフォルト 1GB（設定可）、LRU 破棄
- バックグラウンド生成、UI 操作をブロックしない
- 生成失敗はアイコンフォールバック

ffmpeg 配布:
- **LGPL ビルドを同梱**（`--enable-gpl` / `--enable-nonfree` は使用禁止）
- リンクせず子プロセスとして起動（LGPL 遵守の安全策）
- ライセンス表記: `LICENSES/ffmpeg/` にライセンス全文とビルド構成情報、「About」画面にも表示
- ffmpeg ソースコード入手手段を README と About 画面に明記（LGPL 義務）
- v1.x: システム ffmpeg 優先、同梱オプション化検討

### 3.11 外部連携（v1）

- Finder/Explorer → Progest の D&D 受入（ファイル取り込み、meta 生成、規則適用）
  - flat view に落とされた場合は §3.13 の配置規則に従ってインポート先をサジェスト
  - tree view に落とされた場合は落下先 dir の accepts と突合し、mismatch 時は確認ダイアログ
  - 複数ファイルは各ファイルの最上位 typed match へ自動振り分け、一覧確認モーダルで確定
- Progest → 外部アプリ起動（OSデフォルト、拡張子別指定）
- Progest → 外部への D&D 出（ファイルパス渡し）

### 3.12 テンプレート（v1）

- フォーマット: 単一 TOML
- 書出時に含める内容を選択可（構造は必須）:
  - ディレクトリ構造（空ディレクトリ階層、必須）
  - 命名規則（rules.toml）
  - カスタムスキーマ（schema.toml、alias 含む）
  - 保存済みビュー（views.toml）
  - `.dirmeta.toml`（accepts を含むディレクトリメタ）
- `progest init --template <path>` で適用
- テンプレート内にメタデータ（id, version, author, description）記録
- v1 はローカルパスのみ、git URL は v1.1

### 3.13 配置規則（accepts）

ディレクトリごとに「受け入れる拡張子」を宣言し、import 時のインポート先サジェストと既存ファイルの配置違反 lint を実現する。命名規則（§3.4）とは独立したカテゴリとして扱う。

#### 3.13.1 基本モデル

- 配置: `.dirmeta.toml` の `[accepts]` セクション
- 未設定: 当該ディレクトリは全拡張子を受け入れる（制約なし）
- 記述形式: 拡張子文字列 + カテゴリエイリアス混在可

スキーマ例:
```toml
[accepts]
inherit = false              # デフォルト。true で親 dir の accepts と union
exts = [".psd", ".tif", ":image", ""]   # "" は拡張子なしファイル（README 等）
```

- 拡張子: 先頭ドット必須、大小文字非感知で比較（TOML には小文字で記録推奨）
- 複合拡張子（`.tar.gz`, `.blend1`）は文字列末尾の最長一致で評価
- カテゴリエイリアス: `:image` `:video` 等。定義は `.progest/schema.toml` の `[alias.<name>]` で拡張可能（後述）

#### 3.13.2 継承（opt-in）

命名規則と異なり、デフォルトは非継承。子で `inherit = true` を明示した時のみ、祖先の accepts を再帰的に union する。

```
effective_accepts(dir) = own_accepts(dir) ∪ (inherit ? effective_accepts(parent) : ∅)
```

祖先自身の inherit フラグは effective 計算に影響しない（あくまで子の宣言でその鎖を辿るかを決める）。

#### 3.13.3 カテゴリエイリアス

組み込みエイリアス（v1 で必ず提供）:
- `:image`, `:video`, `:audio`, `:raw`, `:3d`, `:project`, `:text`
- 正確な構成拡張子は実装時に確定し、`docs/` 配下のリファレンスに明示する

プロジェクト定義エイリアスは `.progest/schema.toml` に記述:
```toml
[alias.studio_3d]
exts = [".fbx", ".usd", ".usda", ".usdc", ".abc"]
```

- ネスト（alias 内で alias 参照）は v1 ではサポートしない
- 同名のプロジェクト定義は組み込みを上書きする（診断ログに警告）

#### 3.13.4 インポート先サジェスト

対象ファイルの拡張子（または `""`）と各 dir の effective_accepts を突合し、候補をランキングして提示する。

順位付けルール（上位優先）:
1. 明示一致: dir 自身の own_accepts に当該拡張子が含まれる
2. 継承一致: effective_accepts に含まれるが own_accepts には無い
3. MRU: 最近インポート先に選ばれた dir
4. パス深さ: 浅いほど優先

UI 構成:
- 上部: typed match（1〜4 のルールで並ぶ）
- 折りたたみ: "Other locations"（accepts 未設定 dir、typed match ゼロ時のフォールバックも兼ねる）

#### 3.13.5 発動導線

| 起点 | 挙動 |
| --- | --- |
| flat view D&D | typed match が 1 件なら自動配置 + toast（undo 可）、複数なら一覧モーダル |
| tree view D&D | 落下先の accepts に合致しない場合、確認ダイアログ（推奨候補を併記） |
| CLI `progest import` | stdin が tty なら対話選択。非 tty 時は `--dest` または `--auto` 必須、どちらも無ければエラー。`--auto` は typed match 1 位を自動選択（2 件以上ある時はエラー）、`--dry-run` で移動せずプレビュー |
| 複数ファイルの一括 D&D / `progest import <files...>` | 各ファイルを最上位 typed match に自動振り分け、一覧確認モーダルで変更可、Apply で確定 |

import 操作の実体:
- デフォルトはコピー（元ファイルを残す）
- `--move` または D&D 時の明示モディファイアで移動に切り替え
- 失敗時はロールバック（原子トランザクション）

naming rule との連鎖:
- 配置先 dir に naming rule がある場合、rename preview を同一ダイアログに一体化して提示
- Apply で「配置 + 名前付け」を単一の history エントリとして記録、undo 可

#### 3.13.6 lint（placement カテゴリ）

accepts に違反する既存ファイルを検出する。命名違反とは別カテゴリ `placement` として扱う。

- 検出対象: ファイルの直接親 dir の effective_accepts に当該拡張子が含まれないもの
- モード: naming と同じ 4 モード（`strict` / `warn`（default）/ `hint` / `off`）を `.dirmeta.toml` の `[accepts]` 内で指定可
- 違反レポート必須フィールド: `file_id`, `path`, `category = "placement"`, `expected_exts[]`, `winning_rule_source`（own か inherited か）, `suggested_destinations[]`（ランキング上位 N 件）
- 検索クエリ: `is:misplaced`
- UI: 違反バッジを naming と別色で表示

#### 3.13.7 編集 UX

- ツリーで dir を選択 → インスペクターパネルに `accepts` 編集フォーム
- `exts`: chip input。`:image` 等のエイリアスはオートコンプリート
- `inherit`: チェックボックス
- 変更は `.dirmeta.toml` の原子書込（§3.2 と同等）で反映

---

## 4. 非機能要件

### 4.1 規模・性能目標

| 項目 | 目標値 |
| --- | --- |
| 想定最大ファイル数 | 100,000 |
| 常用規模 | 10,000 |
| 起動スキャン（1万ファイル） | < 5 秒 |
| コマンドパレット検索レスポンス | < 100 ms |
| rename preview（1000件） | < 500 ms |
| メモリ上限（1万ファイル運用時） | < 500 MB |
| サムネ生成スループット | > 10 枚/秒（画像） |

### 4.2 信頼性

- **三段構え FS 同期**: startup full scan + OS watch + periodic reconcile（5分間隔、設定可）
- watch は即時反映のヒントとして使い、信頼しない
- .meta 書込は原子的（temp + rename）
- 書込失敗は `.progest/local/pending/` にキュー、バックオフ再試行
- 起動時に pending を優先処理

### 4.3 セキュリティ

- AI API キーは OS keychain 保存（keyring crate）、平文設定ファイル禁止
- AI コンテキストに notes を含める場合は明示確認
- ローカル完結、テレメトリ送信なし（オプトインでクラッシュレポート将来検討）
- Lua 拡張（v2+）は sandbox 前提、FS/ネットワークは capability 明示付与

### 4.4 プライバシー

- データは全てローカル
- BYOK クラウド AI 以外の外部通信なし
- AI 呼出し時に送信内容をログ可能（ユーザー監査用）

### 4.5 ポータビリティ

- `.meta` と `.progest/` 設定ファイルは全てテキスト（TOML）、git 管理可
- プロジェクトごと zip/git で丸ごと移動可能
- 絶対パス記録禁止、全て project-root 相対

### 4.6 i18n

- UI: 日本語・英語両対応（i18next / react-i18next）
- ロケール自動検出 + ユーザー設定上書き
- 翻訳ファイル: `app/public/locales/{ja,en}/*.json`
- 検索: v1 は trigram 共通、v1.x で lindera 追加

---

## 5. プラットフォーム固有要件

### 5.1 macOS（v1.0 対象）

- Darwin 11+ サポート
- FSEvents 経由で notify
- 大小文字非感知 FS（HFS+）対応
- Spotlight から `.progest/thumbs/` `.progest/index.db` を除外推奨
- `code signing + notarization` 必須（配布時）

### 5.2 Windows（v1.1）

- 長パス: `\\?\` プレフィックス対応（dunce crate 相当）
- ReadDirectoryChangesW + 定期 reconcile 必須（イベント欠落あり）
- ファイルロック検出とリトライ（Photoshop/Premiere 等が掴むケース）
- 大小文字差異の正規化
- OneDrive Placeholder（仮想化ファイル）検出 + 警告
- UNC パス対応
- 予約ファイル名（`CON`, `PRN` 等）回避
- MSIX または code-signed installer

### 5.3 Linux（v2+、ベストエフォート）

- inotify 上限（`fs.inotify.max_user_watches`）検出 + ユーザー通知
- 大小文字感知 FS 前提
- AppImage または flatpak 配布

---

## 6. チーム共有とマージ戦略

### 6.1 共有方式

- git 前提（Progest 自身は同期しない）
- 共有対象: `.progest/project.toml`, `rules.toml`, `schema.toml`, `views.toml`, `ignore`, 全 `.meta`
- gitignore 対象: `.progest/index.db`, `.progest/thumbs/`, `.progest/local/`

### 6.2 .meta マージ戦略

`.gitattributes`:
```
*.meta merge=progest-meta
```

`progest meta merge` の規則:
- `file_id` 不一致 → 手動解決必須（致命的エラー）
- `[tags].list` → 集合和、重複排除、ソート
- `[notes].body` → 両バージョンを併記（区切り線付き）
- `[meta_internal]` → マージしない（各自ローカル）
- `created_at` → 古い方を採用
- `[custom]` → キー単位マージ、同一キー衝突は手動
- `[naming]` → last_validated_at の新しい方を採用

### 6.3 index.db と thumbs/

- 各自の環境で再生成
- `.gitignore` に必ず含める（`progest init` 時自動設定）

---

## 7. ignore と特殊ファイル

### 7.1 デフォルト ignore

- `.git/`, `.svn/`, `.hg/`
- `node_modules/`, `__pycache__/`, `venv/`
- `.DS_Store`, `Thumbs.db`, `desktop.ini`
- `*.tmp`, `*.bak`, `*.swp`, `*~`
- DCC autosave: `*.blend1`, `*.psd~`, `.autosave/`
- レンダーキャッシュディレクトリ（ユーザー定義）

プロジェクト固有追加: `.progest/ignore`（gitignore 構文）

### 7.2 特殊 FS 要素

| 要素 | v1 | v1.x |
| --- | --- | --- |
| symlink / alias / junction | 追跡せず、警告表示 | オプトイン追跡 |
| NAS / ネットワークドライブ | ベストエフォート、保証外 | テスト対象追加 |
| クラウド同期（Dropbox/iCloud） | 通常ファイルとして扱う、仮想化は警告 | Placeholder 検出対応 |
| 隠しディレクトリ | デフォルトスキップ | 同左 |
| ディレクトリ自体へのメタ付与 | 対応（`.dirmeta.toml`） | 同左 |

---

## 8. エラー処理と可観測性

- 規則違反: lint レポートに勝利 rule_id + 継承チェーン必須
- .meta 書込失敗: pending キュー、バックオフ再試行、3回失敗でユーザー通知
- スキャン進捗: プログレスバー、ETA 表示
- 診断ログ: `.progest/local/logs/`、tracing 経由
- `progest doctor`: 孤児 .meta、fingerprint 不一致、UUID 衝突、インデックス drift を検出

---

## 9. ライセンスと貢献

### 9.1 本体ライセンス
- **Apache License 2.0**
- 全コード、ドキュメント、設計ノートに適用
- `LICENSE` ファイルをリポジトリルートに配置

### 9.2 同梱バイナリのライセンス
- **ffmpeg（LGPL 2.1+ ビルド）**: 子プロセスとして同梱。`LICENSES/ffmpeg/` にライセンス全文、ビルド時のフラグ・バージョン・ソース入手先を記録
- GPL ビルドおよび non-free コーデックは使用禁止
- 将来的に他 LGPL/GPL 依存を追加する場合は同等の手続きを踏む

### 9.3 ユーザー生成コンテンツのライセンス
- `rules.toml`, `schema.toml`, `views.toml`, テンプレート TOML 等の設定成果物はユーザー所有
- Progest はこれらに独自ライセンスを主張しない
- テンプレート Registry（v2+）開設時は、投稿者がライセンスを明示する運用にする

### 9.4 コントリビューション
- **DCO（Developer Certificate of Origin）** のみ採用
- PR は `Signed-off-by: Name <email>` を必須とする
- CLA は導入しない（小〜中規模 OSS に合わせた運用）
- `CONTRIBUTING.md` に DCO の運用と `git commit -s` の使い方を記載

---

## 10. 未決事項

v1.x 以降で検討:

- メタの暗号化（秘匿 notes 用途）
- マルチユーザーのロック機構（git branch ベースで十分か）
- プロジェクト間参照（asset 再利用）
- サムネ生成のリモートワーカー化

---

## 11. 参考・関連

- [IMPLEMENTATION_PLAN.md](./IMPLEMENTATION_PLAN.md) — 実装計画
- [_DRAFT.md](./_DRAFT.md) — 初期ドラフト（参考保存）
- [README.md](../README.md) — 英語版 README
- [README.ja.md](../README.ja.md) — 日本語版 README
- [CLAUDE.md](../CLAUDE.md) — Claude Code 向けプロジェクト指示
