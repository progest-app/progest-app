# Accepts エイリアスリファレンス

`.dirmeta.toml` の `[accepts]` セクションで使える **ビルトインカテゴリエイリアス** の構成拡張子を定義する。
REQUIREMENTS.md §3.13 の配置規則仕様と対で読むこと。

最終更新: 2026-04-26（`:3d` を `:model` + `:scene` に分割。識別子に leading digit が出ないように整理）

---

## 1. 使い方

```toml
# .dirmeta.toml
[accepts]
exts = [":image", ":raw", ".psd", ""]   # alias + 拡張子 + 空文字（拡張子なし）
inherit = false
mode = "warn"
```

エイリアスはコロン接頭辞 `:` で書く。評価時に本ファイル記載の拡張子セットへ展開される。

- 拡張子は **小文字で記録** を推奨、比較は大小文字非感知
- 複合拡張子（`.tar.gz` 等）は末尾最長一致で評価（rules の `BUILTIN_COMPOUND_EXTS` と同じ扱い）
- `""`（空文字）は「拡張子なしファイル」を明示的に許容する。`README` / `Makefile` のような完全な拡張子無しファイルに加え、`split_basename` が **no-extension として扱うリーディングドットファイル**（`.gitignore`, `.env`, `.dockerignore` 等）も同じカテゴリに入る

---

## 2. ビルトインエイリアス一覧（v1）

**拡張子は先頭ドットなしの小文字トークンで内部表現する。** TOML 記述上は `.psd` のように先頭ドットを書くが、loader が `psd` に正規化して比較する。

### `:image`

静止画全般。撮影データ / VFX テクスチャ / 汎用画像 / ベクタ / ゲームテクスチャ。RAW は別エイリアス `:raw` に分離。

| 拡張子 | 備考 |
| --- | --- |
| `jpg` / `jpeg` | JPEG |
| `png` | PNG |
| `gif` | GIF |
| `webp` | WebP |
| `bmp` | Windows Bitmap |
| `tif` / `tiff` | TIFF（印刷・スキャン） |
| `psd` | Photoshop（ラスタ編集ファイルだが画像扱い） |
| `psb` | Photoshop Large Document |
| `tga` | Targa（ゲーム/VFX テクスチャ） |
| `dds` | DirectDraw Surface（ゲームテクスチャ） |
| `dpx` | Digital Picture Exchange（シネマ DI） |
| `exr` | OpenEXR（HDR、VFX 中間フォーマット） |
| `hdr` | Radiance HDR |
| `heic` / `heif` | HEIF（iOS 標準） |
| `avif` | AVIF |
| `svg` | SVG（XML ベースのベクタ画像。placement UX 上は画像 dir に置かれることが圧倒的に多いので `:text` ではなくこちらに収容） |

### `:video`

Moving-image source（カメラ RAW 含む）+ editorial / delivery コンテナ全般。動画 RAW を独立に切り出したくなったら将来 `:video_raw` を追加する余地を残す。

| 拡張子 | 備考 |
| --- | --- |
| `mp4` | H.264/H.265 コンテナ |
| `mov` | QuickTime |
| `mkv` | Matroska |
| `avi` | Windows AVI |
| `webm` | WebM |
| `m4v` | iTunes 動画 |
| `mpg` / `mpeg` | MPEG |
| `ts` / `m2ts` | MPEG-TS（放送・Blu-ray） |
| `mxf` | Material Exchange Format（放送・ENG コンテナ、RAW ではない） |
| `r3d` | RED Camera RAW |
| `braw` | Blackmagic RAW |
| `ari` | ARRIRAW |

### `:audio`

音声。収録素材 / マスター / 一般。lossless / lossy を区別しない。

| 拡張子 | 備考 |
| --- | --- |
| `wav` | WAV |
| `aif` / `aiff` | AIFF |
| `flac` | FLAC |
| `alac` | Apple Lossless |
| `mp3` | MP3 |
| `aac` | AAC |
| `m4a` | MPEG-4 Audio |
| `ogg` | Ogg Vorbis |
| `opus` | Opus |
| `wma` | Windows Media Audio |

### `:raw`

カメラ RAW。メーカー固有のネガティブデータ。`:image` とは別軸で、現像ワークフロー dir に限定的に置きたい時に使う。

| 拡張子 | メーカー |
| --- | --- |
| `arw` | Sony |
| `cr2` / `cr3` | Canon |
| `nef` | Nikon |
| `raf` | Fujifilm |
| `rw2` | Panasonic |
| `orf` | Olympus / OM SYSTEM |
| `pef` | Pentax |
| `srw` | Samsung |
| `x3f` | Sigma |
| `dng` | Adobe DNG（汎用 RAW） |

### `:model`

メッシュ・ジオメトリ・ポイントクラウド系の交換フォーマット。アプリ非依存の 3D アセットファイルだけを集める。DCC のセッション保存ファイル（`blend` / `ma` 等）は `:scene` 側にある。

| 拡張子 | 種別 |
| --- | --- |
| `fbx` | FBX（Autodesk） |
| `obj` | Wavefront OBJ |
| `usd` / `usda` / `usdc` / `usdz` | OpenUSD |
| `abc` | Alembic |
| `gltf` / `glb` | glTF 2.0 |
| `stl` | STL |
| `ply` | Stanford PLY |
| `drc` | Draco 圧縮メッシュ |

### `:scene`

DCC アプリのセッション / シーンファイル + テクスチャ作成系プロジェクトファイル + ボリュームキャッシュ。「アプリで開いて編集する」性質の 3D 系ファイル。`:project` と一部重複する（`blend`, `hip`, `ma`, `mb`, `max`, `c4d`）— `:project` は「映像・編集中プロジェクトファイル全般」、`:scene` は「3D / DCC のセッション保存」の文脈で選ぶ。

| 拡張子 | アプリ |
| --- | --- |
| `blend` | Blender |
| `ma` / `mb` | Maya |
| `max` | 3ds Max |
| `c4d` | Cinema 4D |
| `hip` / `hiplc` / `hipnc` | Houdini |
| `ztl` / `zpr` | ZBrush |
| `spp` | Substance Painter |
| `sbs` / `sbsar` | Substance Designer |
| `vdb` | OpenVDB（VFX ボリューム / FX キャッシュ） |

### `:project`

DCC プロジェクトファイル。編集中 / セッション保存の意味が強いもの。`:scene` と重複するエントリは「3D 作業の project ファイル」として意図的に両方へ入れる。

| 拡張子 | アプリ |
| --- | --- |
| `prproj` | Adobe Premiere Pro |
| `aep` / `aepx` | Adobe After Effects |
| `psd` | Photoshop（編集中ファイルとして） |
| `psb` | Photoshop Large Document（編集中扱い） |
| `ai` | Adobe Illustrator |
| `drp` / `drt` | DaVinci Resolve（Project / Template） |
| `fcpxml` | Final Cut Pro プロジェクト XML |
| `nk` | Nuke |
| `hrox` | Hiero |
| `blend` | Blender |
| `ma` / `mb` | Maya |
| `max` | 3ds Max |
| `c4d` | Cinema 4D |
| `hip` / `hiplc` / `hipnc` | Houdini |
| `spp` | Substance Painter |
| `sbs` | Substance Designer |
| `veg` | Vegas Pro |
| `mocha` | Mocha Pro |

### `:text`

テキスト・設定・ドキュメント全般。バイナリ扱いの `pdf` は含めない（必要なら project alias で拡張）。`svg` は XML ベースだが placement 運用上は画像 dir に置かれる方が圧倒的に多いため `:image` に収容し、こちらには含めない。

| 拡張子 | 種別 |
| --- | --- |
| `txt` | プレーンテキスト |
| `md` / `markdown` | Markdown |
| `rst` | reStructuredText |
| `org` | Org-mode |
| `adoc` / `asciidoc` | AsciiDoc |
| `log` | ログ |
| `csv` / `tsv` | 区切り値 |
| `json` | JSON |
| `yaml` / `yml` | YAML |
| `toml` | TOML |
| `xml` | XML |
| `html` / `htm` | HTML |
| `css` | CSS |
| `ini` / `cfg` / `conf` | 設定ファイル |

---

## 3. プロジェクト定義エイリアス

`.progest/schema.toml` の `[alias.<name>]` テーブルで追加・上書きできる（REQUIREMENTS.md §3.13.3 準拠）。

```toml
# .progest/schema.toml
[alias.studio_3d]
exts = [".fbx", ".usd", ".usda", ".usdc", ".abc"]

[alias.image]
# 組み込み :image を上書き（ローダは警告ログに記録する）
exts = [".jpg", ".jpeg", ".png", ".webp"]
```

### 3.1 v1 validation ルール

loader が `.progest/schema.toml` をパースする時点で以下を強制する。曖昧な TOML を silently 許容すると、後段の accepts 展開で気付きにくい誤配置を招くため全部 **hard error** 扱い。

| 状況 | 挙動 |
| --- | --- |
| `[alias.<name>].exts` に `":other"` を含む | **schema load error**（ネスト禁止） |
| `[accepts].exts` に未定義エイリアス `":unknown"` | **schema load error**（誤字の silent pass を防ぐ） |
| `[alias.<name>].exts` に先頭ドット無しトークン `"psd"` | **schema load error**（`.psd` 形式に強制） |
| `[alias.<name>].exts` に `""`（空文字）単独 | 許可（`:no_ext` 相当のエイリアスが欲しい場合） |
| `[alias.<name>].exts` が空配列 `[]` | **schema load error**（意味が無い） |
| 同一 alias 内の重複 ext | 正規化時に dedup、warning なし |
| プロジェクト alias による組み込み同名上書き | **full replace**（union しない）、ローダは warning を emit |
| alias 名が `^[a-z][a-z0-9_-]*$` に不一致 | **schema load error** |

展開・正規化ルール:

- 比較前に `".PSD"` → `"psd"` と小文字化、先頭ドット除去（内部表現は **先頭ドット無し小文字**）
- `[accepts].exts` と alias 展開結果は最終的に `BTreeSet<String>` に統合、重複は自然に潰れる
- leading-dot ファイル（`.gitignore` 等）は `split_basename` の挙動に合わせて `""`（拡張子なし）として扱う

---

## 4. 実装メモ

- `core::accepts::BUILTIN_ALIASES` に const 配列で保持（`&[(&str, &[&str])]` 形式）
- 正規化: `".PSD"` → `"psd"`, `""` はそのまま保持（拡張子なし）。leading-dot ファイル（`.gitignore` 等）は `split_basename` 挙動に合わせて `""` へ寄せる
- provenance tracking: ランキング（`own_accepts` vs `inherited`）で必要になるので、alias 展開 **前** の own set を保持してから expansion 結果を重ねる
- effective_accepts 計算時に **一度だけ展開**、以降は `BTreeSet<String>`（小文字、先頭ドットなし）で照合
- 複合拡張子の照合は `core::rules::constraint::split_basename` と同じロジック（`BUILTIN_COMPOUND_EXTS` + 将来的には project の `[extension_compounds]`）を流用
- エイリアスと通常拡張子を同じリストに混ぜて TOML に書ける（`exts = [":image", ".psd"]`）

---

## 5. 今後の拡張（v1.x+）

- **エイリアスの入れ子**: `:image_all = [":image", ":raw"]` のような参照許可
- **除外記法**: `exts = [":image", "!.heic"]` で alias 経由のサブセット除外
- **MIME ベース判定**: 拡張子フリーなバイナリ検出（magic number 走査）

これらは REQUIREMENTS.md §3.13.3 の v1 スコープ外。設計の拡張余地として残す。
