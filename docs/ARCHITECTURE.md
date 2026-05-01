# Architecture

Progest の at-a-glance アーキテクチャ。詳細・マイルストーン別のモジュール内訳は [`IMPLEMENTATION_PLAN.md`](./IMPLEMENTATION_PLAN.md)、要件は [`REQUIREMENTS.md`](./REQUIREMENTS.md) を参照。

---

## モノレポ構成

| パッケージ | 役割 |
| --- | --- |
| `crates/progest-core` | ドメインロジック全て（meta I/O、FS、規則エンジン、index、search、watch、reconcile、thumbnail、template、AI クライアント、rename） |
| `crates/progest-cli` | CLI バイナリ。core を直接使用 |
| `crates/progest-merge` | `.meta` 用 git merge driver（単機能バイナリ） |
| `crates/progest-tauri` | Tauri IPC glue。薄層、core を呼ぶだけ |
| `app/` | React + shadcn/ui フロントエンド。Tauri IPC 経由で core にアクセス |

**ビジネスロジックをフロントエンド層に書かない。** UI は描画とユーザー入力の受け流しのみ。全てのロジックは core に集約する。理由: CLI、Lua 拡張（v2+）、将来のヘッドレス利用で同じロジックが使われるため。

---

## プラットフォーム優先度

| OS | v1.0 | 備考 |
| --- | --- | --- |
| macOS | 主対象 | Darwin 11+、FSEvents 経由 notify、notarization 必須 |
| Windows | 対象 | `dunce` で `\\?\` 除去、COLLATE NOCASE、file lock retry、reserved name lint、OneDrive placeholder skip、NSIS installer。CI で `windows-latest` テスト |
| Linux | ベストエフォート（v2+） | inotify 上限対応が必要 |

v1.0 は macOS + Windows を対象にビルド・テストする。core のパス抽象・FS trait はクロスプラ前提で設計済み。
