# Progest 実装計画書

**バージョン:** 1.0
**作成日:** 2026-02-15
**最終更新:** 2026-02-15

## 実装進行状況概要

| フェーズ | コミット範囲 | 進行度 | 状態 |
|---------|-------------|--------|------|
| Phase 1: テストインフラ | 1 | 100% | ✅ 完了 |
| Phase 2: 基本ルール | 2-5 | 100% | ✅ 完了 |
| Phase 3: 集約ルール | 6-7 | 50% | 🟡 進行中 |
| Phase 4: 文字列変換 | 8-9 | 50% | 🟡 進行中 |
| Phase 5: サービス層 | 10-12 | 0% | ⬜ 未着手 |
| Phase 6: データベース | 13-17 | 0% | ⬜ 未着手 |
| Phase 7: アセット管理 | 18-22 | 0% | ⬜ 未着手 |
| Phase 8: プロジェクト管理 | 23-26 | 0% | ⬜ 未着手 |
| Phase 9: CLI実装 | 27-30 | 0% | ⬜ 未着手 |

**全体進行度: 25% (8/30 コミット完了)**

---

## Phase 1: テストインフラ整備

### ✅ Commit 1: テストプロジェクト作成
**ステータス:** 完了

**実装内容:**
- [x] `Progest.Core.Tests` プロジェクト作成
- [x] xUnit (2.9.2), FluentAssertions (7.0.0), Moq (4.20.72) 追加
- [x] テストディレクトリ構造作成
- [x] `.slnx` にテストプロジェクト追加
- [x] インフラストラクチャテスト作成

**検証:**
- [x] `dotnet test` が成功すること
- [x] 1 test passing

**成果物:**
- `src/Progest.Core.Tests/Progest.Core.Tests.csproj`
- `src/Progest.Core.Tests/InfrastructureTests.cs`

---

## Phase 2: 命名規則 - 基本型とルール

### ✅ Commit 2: 列挙型定義
**ステータス:** 完了

**実装内容:**
- [x] `ConventionType.cs` - 6種類の命名規則
- [x] `PrefixType.cs` - None, Date, Fixed
- [x] `SuffixType.cs` - None, Version, Fixed
- [x] `DateFormat.cs` - 4種類の日付形式
- [x] `VersionFormat.cs` - Semantic, Sequential, Simple
- [x] 基本検証の単体テスト

**検証:**
- [x] 全ての列挙型値が正しく表現される
- [x] 11 tests passing

**成果物:**
- `src/Progest.Core/Models/NamingConvention/ConventionType.cs`
- `src/Progest.Core/Models/NamingConvention/PrefixType.cs`
- `src/Progest.Core/Models/NamingConvention/SuffixType.cs`
- `src/Progest.Core/Models/NamingConvention/DateFormat.cs`
- `src/Progest.Core/Models/NamingConvention/VersionFormat.cs`
- `src/Progest.Core.Tests/Models/NamingConvention/ConventionTypeTests.cs`
- `src/Progest.Core.Tests/Models/NamingConvention/EnumerationTypeTests.cs`

### ✅ Commit 3: PrefixRule
**ステータス:** 完了

**実装内容:**
- [x] `PrefixRule.cs` クラス作成
- [x] 3種類のコンストラクタ（None, Fixed, Date）
- [x] `GeneratePrefix()` メソッド実装
- [x] 単体テスト: 各タイプの生成ロジック

**検証:**
- [x] 日付形式で正しいプレフィックスが生成される
- [x] 固定文字列が正しく適用される
- [x] Noneの場合は空文字列が返される
- [x] 9 tests passing

**成果物:**
- `src/Progest.Core/Models/NamingConvention/PrefixRule.cs`
- `src/Progest.Core.Tests/Models/NamingConvention/PrefixRuleTests.cs`

### ✅ Commit 4: SuffixRule
**ステータス:** 完了

**実装内容:**
- [x] `SuffixRule.cs` クラス作成
- [x] 複数のコンストラクタオーバーロード
- [x] `GenerateSuffix()` メソッド実装
- [x] 単体テスト: 各バージョン形式

**検証:**
- [x] Semantic versioning (v1.2.3)
- [x] Simple versioning (v1.2)
- [x] Sequential (042) - 3桁パディング
- [x] 固定文字列
- [x] 9 tests passing

**成果物:**
- `src/Progest.Core/Models/NamingConvention/SuffixRule.cs`
- `src/Progest.Core.Tests/Models/NamingConvention/SuffixRuleTests.cs`

### ✅ Commit 5: SequentialNumberingRule
**ステータス:** 完了

**実装内容:**
- [x] `SequentialNumberingRule.cs` クラス作成
- [x] `Format(int number)` メソッド実装
- [x] 開番、桁数、セパレータのカスタマイズ

**検証:**
- [x] デフォルトパラメータで正しく動作
- [x] カスタム開始番号
- [x] 桁数パディング
- [x] カスタムセパレータ
- [x] 空セパレータ
- [x] 大きな数値、0、桁あふれ
- [x] 9 tests passing

**成果物:**
- `src/Progest.Core/Models/NamingConvention/SequentialNumberingRule.cs`
- `src/Progest.Core.Tests/Models/NamingConvention/SequentialNumberingRuleTests.cs`

---

## Phase 3: 命名規則 - 集約ルール

### ✅ Commit 6: NamingConvention 集約ルート
**ステータス:** 完了

**実装内容:**
- [x] `NamingConvention.cs` クラス作成（集約ルート）
- [x] `Apply(string input)` メソッド実装
- [x] `ApplyBatch(string input, int index)` メソッド実装
- [x] Prefix, Suffix, Case Conversion, Sequential Numbering の統合
- [x] 単体テスト

**検証:**
- [x] 単一命名規則の適用
- [x] Prefix + Base + Suffix の組み合わせ
- [x] ケース変換（6種類）
- [x] バッチ処理でのシーケンシャル番号
- [x] 全コンポーネントの統合
- [x] 14 tests passing

**成果物:**
- `src/Progest.Core/Models/NamingConvention/NamingConvention.cs`
- `src/Progest.Core.Tests/Models/NamingConvention/Aggregate/NamingConventionAggregateTests.cs`

### ⬜ Commit 7: NamingConvention 拡張メソッド
**ステータス:** 未着手

**予定実装内容:**
- [ ] `UpdatePrefix(PrefixRule prefix)` メソッド
- [ ] `UpdateSuffix(SuffixRule suffix)` メソッド
- [ ] `AddSequentialNumbering(SequentialNumberingRule rule)` メソッド
- [ ] 単体テスト: ルール更新、連番追加

**予定検証:**
- [ ] 既存ルールの正しい更新
- [ ] ルール追加時の検証
- [ ] 不正な操作時の例外処理

**予定成果物:**
- `src/Progest.Core/Models/NamingConvention/NamingConventionExtensions.cs`
- `src/Progest.Core.Tests/Models/NamingConvention/NamingConventionExtensionsTests.cs`

---

## Phase 4: 文字列変換エクステンション

### ✅ Commit 8: StringExtensions 作成
**ステータス:** 完了（先行実装）

**実装内容:**
- [x] `StringExtensions.cs` クラス作成
- [x] 変換メソッド:
  - [x] `ToSnakeCase()`
  - [x] `ToCamelCase()`
  - [x] `ToPascalCase()`
  - [x] `ToKebabCase()`
  - [x] `ToTitleCase()`
- [x] インテリジェントな単語分割アルゴリズム

**検証:**
- [x] 各ケース変換が正しく動作
- [x] エッジケース（空文字列、特殊文字、数字）
- [x] 異なるケースからの変換
- [x] 31 tests passing

**成果物:**
- `src/Progest.Core/Extensions/StringExtensions.cs`
- `src/Progest.Core.Tests/Extensions/StringExtensionsTests.cs`

### ⬜ Commit 9: StringExtensions テスト拡張
**ステータス:** 未着手

**予定実装内容:**
- [ ] 追加のエッジケーステスト
- [ ] パフォーマンステスト
- [ ] 国際化対応テスト（日本語など）
- [ ] 特殊文字・絵文字のテスト

---

## Phase 5: 命名規則サービス

### ⬜ Commit 10: INamingService インターフェース
**ステータス:** 未着手

**予定実装内容:**
- [ ] `INamingService.cs` インターフェース定義
- [ ] CRUD メソッド定義:
  - [ ] `CreateAsync()`
  - [ ] `GetByIdAsync()`
  - [ ] `GetByNameAsync()`
  - [ ] `GetAllAsync()`
  - [ ] `UpdateAsync()`
  - [ ] `DeleteAsync()`
- [ ] 適用メソッド定義:
  - [ ] `ApplyConventionAsync()`
  - [ ] `ApplyConventionBatchAsync()`
- [ ] XML ドキュメントによる詳細なドキュメンテーション

**成果物:**
- `src/Progest.Core/Services/INamingService.cs`
- `src/Progest.Core.Tests/Services/INamingServiceTests.cs` (インターフェースのテストダミー)

### ⬜ Commit 11: NamingService 実装
**ステータス:** 未着手

**予定実装内容:**
- [ ] `NamingService.cs` クラス実装
- [ ] 全CRUDオペレーション実装
- [ ] 単体テスト: モックリポジトリでテスト
- [ ] エラーハンドリング
- [ ] バリデーション

**検証:**
- [ ] モックリポジトリを使用した単体テスト
- [ ] 例外処理のテスト
- [ ] バリデーションのテスト

**成果物:**
- `src/Progest.Core/Services/NamingService.cs`
- `src/Progest.Core.Tests/Services/NamingServiceTests.cs`

### ⬜ Commit 12: 命名規則関連エクセプション
**ステータス:** 未着手

**予定実装内容:**
- [ ] `ConventionNotFoundException.cs` 作成
- [ ] `InvalidConventionException.cs` 作成
- [ ] 例外メッセージのローカライズ対応（将来）
- [ ] 単体テスト

**成果物:**
- `src/Progest.Core/Exceptions/ConventionNotFoundException.cs`
- `src/Progest.Core/Exceptions/InvalidConventionException.cs`
- `src/Progest.Core.Tests/Exceptions/NamingConventionExceptionTests.cs`

---

## Phase 6: データベース設計・実装

### ⬜ Commit 13: Db プロジェクト設計
**ステータス:** 未着手

**予定実装内容:**
- [ ] EF Core 9.0 パッケージ追加
- [ ] SQLite プロバイダ追加
- [ ] Core プロジェクト参照追加
- [ ] 名前空間構造作成:
  - [ ] `Progest.Db/Entities/`
  - [ ] `Progest.Db/Data/`
  - [ ] `Progest.Db/Repositories/`
  - [ ] `Progest.Db/Migrations/`

**成果物:**
- `src/Progest.Db/Progest.Db.csproj` (更新)

### ⬜ Commit 14: Entity クラス - NamingConvention
**ステータス:** 未着手

**予定実装内容:**
- [ ] `NamingConventionEntity.cs` 作成
- [ ] `PrefixRuleEntity.cs` 作成
- [ ] `SuffixRuleEntity.cs` 作成
- [ ] `SequentialNumberingRuleEntity.cs` 作成
- [ ] Fluent API によるリレーション設定

**成果物:**
- `src/Progest.Db/Entities/NamingConventionEntity.cs`
- `src/Progest.Db/Entities/PrefixRuleEntity.cs`
- `src/Progest.Db/Entities/SuffixRuleEntity.cs`
- `src/Progest.Db/Entities/SequentialNumberingRuleEntity.cs`

### ⬜ Commit 15: ProgestDbContext 作成
**ステータス:** 未着手

**予定実装内容:**
- [ ] `ProgestDbContext.cs` クラス作成
- [ ] `DbSet` プロパティ定義
- [ ] エンティティ関係設定
- [ ] インデックス設定
- [ ] カスケード挙動設定
- [ ] SQLite特有の設定

**成果物:**
- `src/Progest.Db/Data/ProgestDbContext.cs`

### ⬜ Commit 16: Repository パターン
**ステータス:** 未着手

**予定実装内容:**
- [ ] `IRepository.cs` 汎用インターフェース作成
- [ ] `Repository.cs` 汎用実装作成
- [ ] `INamingConventionRepository.cs` インターフェース作成
- [ ] `NamingConventionRepository.cs` 実装作成

**成果物:**
- `src/Progest.Db/Repositories/IRepository.cs`
- `src/Progest.Db/Repositories/Repository.cs`
- `src/Progest.Db/Repositories/INamingConventionRepository.cs`
- `src/Progest.Db/Repositories/NamingConventionRepository.cs`

### ⬜ Commit 17: EF Core Migrations
**ステータス:** 未着手

**予定実装内容:**
- [ ] 初回マイグレーション作成
- [ ] `CreateInitialSchema` マイグレーション
- [ ] SQLite でマイグレーション検証
- [ ] マイグレーションスクリプトのドキュメント化
- [ ] ロールバック手順の作成

**検証:**
- [ ] マイグレーションが正しく適用される
- [ ] データベーススキーマが設計通り
- [ ] インデックスが正しく作成される

**成果物:**
- `src/Progest.Db/Migrations/20250215_InitialSchema.cs`

---

## Phase 7: アセット管理モデル

### ⬜ Commit 18: Asset Value Objects
**ステータス:** 未着手

**予定実装内容:**
- [ ] `Tag.cs` レコード型作成
- [ ] `AssetMetadata.cs` レコード型作成
- [ ] `FileDependency.cs` クラス作成
- [ ] 単体テスト: 等価性、検証ロジック

**成果物:**
- `src/Progest.Core/Models/Assets/Tag.cs`
- `src/Progest.Core/Models/Assets/AssetMetadata.cs`
- `src/Progest.Core/Models/Assets/FileDependency.cs`
- `src/Progest.Core.Tests/Models/Assets/AssetValueObjectsTests.cs`

### ⬜ Commit 19: Asset 集約ルート
**ステータス:** 未着手

**予定実装内容:**
- [ ] `Asset.cs` クラス作成
- [ ] タグ管理メソッド:
  - [ ] `AddTag()`
  - [ ] `RemoveTag()`
  - [ ] `HasTag()`
- [ ] 依存関係管理メソッド:
  - [ ] `AddDependency()`
  - [ ] `RemoveDependency()`
  - [ ] `GetDependents()`
  - [ ] `CanBeDeleted()`
- [ ] メタデータ更新メソッド
- [ ] 網羅的単体テスト

**成果物:**
- `src/Progest.Core/Models/Assets/Asset.cs`
- `src/Progest.Core.Tests/Models/Assets/AssetTests.cs`

### ⬜ Commit 20: IAssetService インターフェース
**ステータス:** 未着手

**予定実装内容:**
- [ ] `IAssetService.cs` インターフェース
- [ ] `SearchCriteria` レコード型定義
- [ ] 全サービスメソッド定義:
  - [ ] CRUD
  - [ ] タグ管理
  - [ ] 依存関係管理
  - [ ] 検索

**成果物:**
- `src/Progest.Core/Services/IAssetService.cs`

### ⬜ Commit 21: AssetService 実装
**ステータス:** 未着手

**予定実装内容:**
- [ ] `AssetService.cs` 実装
- [ ] CRUD オペレーション実装
- [ ] 検索ロジック実装
- [ ] 単体テスト

**成果物:**
- `src/Progest.Core/Services/AssetService.cs`
- `src/Progest.Core.Tests/Services/AssetServiceTests.cs`

### ⬜ Commit 22: Asset エンティティとRepository
**ステータス:** 未着手

**予定実装内容:**
- [ ] `AssetEntity.cs` 作成
- [ ] `TagEntity.cs` 作成
- [ ] `AssetTagEntity.cs` (多対多)
- [ ] `FileDependencyEntity.cs` 作成
- [ ] `IAssetRepository.cs` インターフェース
- [ ] `AssetRepository.cs` 実装
- [ ] 統合テスト（インメモリSQLite）

**成果物:**
- `src/Progest.Db/Entities/AssetEntity.cs`
- `src/Progest.Db/Entities/TagEntity.cs`
- `src/Progest.Db/Entities/AssetTagEntity.cs`
- `src/Progest.Db/Entities/FileDependencyEntity.cs`
- `src/Progest.Db/Repositories/IAssetRepository.cs`
- `src/Progest.Db/Repositories/AssetRepository.cs`

---

## Phase 8: プロジェクト管理

### ⬜ Commit 23: Project モデル
**ステータス:** 未着手

**予定実装内容:**
- [ ] `Project.cs` 集約ルート
- [ ] `ProjectSettings.cs` レコード型
- [ ] `DirectoryRule.cs` クラス
- [ ] 単体テスト

**成果物:**
- `src/Progest.Core/Models/Projects/Project.cs`
- `src/Progest.Core/Models/Projects/ProjectSettings.cs`
- `src/Progest.Core/Models/Projects/DirectoryRule.cs`
- `src/Progest.Core.Tests/Models/Projects/ProjectTests.cs`

### ⬜ Commit 24: ProjectService
**ステータス:** 未着手

**予定実装内容:**
- [ ] `IProjectService.cs` インターフェース
- [ ] `ProjectService.cs` 実装
- [ ] 単体テスト

**成果物:**
- `src/Progest.Core/Services/IProjectService.cs`
- `src/Progest.Core/Services/ProjectService.cs`
- `src/Progest.Core.Tests/Services/ProjectServiceTests.cs`

### ⬜ Commit 25: Project データベース
**ステータス:** 未着手

**予定実装内容:**
- [ ] `ProjectEntity.cs` 作成
- [ ] `DirectoryRuleEntity.cs` 作成
- [ ] DbContext に追加
- [ ] Repository 実装
- [ ] 統合テスト

**成果物:**
- `src/Progest.Db/Entities/ProjectEntity.cs`
- `src/Progest.Db/Entities/DirectoryRuleEntity.cs`
- `src/Progest.Db/Repositories/IProjectRepository.cs`
- `src/Progest.Db/Repositories/ProjectRepository.cs`

### ⬜ Commit 26: サイドカーファイル対応
**ステータス:** 未着手

**予定実装内容:**
- [ ] JSON 形式サイドカーファイル読取/書込ロジック
- [ ] アセットとサイドカーファイルの同期
- [ ] `ISidecarFileService.cs` インターフェース
- [ ] `SidecarFileService.cs` 実装
- [ ] 単体テスト

**成果物:**
- `src/Progest.Core/Services/ISidecarFileService.cs`
- `src/Progest.Core/Services/SidecarFileService.cs`
- `src/Progest.Core.Tests/Services/SidecarFileServiceTests.cs`

---

## Phase 9: CLI 実装

### ⬜ Commit 27: CLI 設定
**ステータス:** 未着手

**予定実装内容:**
- [ ] System.CommandLine パッケージ追加
- [ ] DI コンテナ設定
- [ ] 基本的CLI構造
- [ ] コマンドルーティング

**成果物:**
- `src/Progest.Cli/Program.cs` (更新)
- `src/Progest.Cli/ServiceCollectionExtensions.cs`

### ⬜ Commit 28: CLI Commands - Naming
**ステータス:** 未着手

**予定実装内容:**
- [ ] `progest naming apply` コマンド
- [ ] `progest naming list` コマンド
- [ ] `progest naming create` コマンド
- [ ] `progest naming delete` コマンド
- [ ] `progest naming preview` コマンド
- [ ] コマンドテスト

**成果物:**
- `src/Progest.Cli/Commands/NamingCommands.cs`
- `src/Progest.Cli.Tests/Commands/NamingCommandsTests.cs`

### ⬜ Commit 29: CLI Commands - Assets
**ステータス:** 未着手

**予定実装内容:**
- [ ] `progest asset add` コマンド
- [ ] `progest asset search` コマンド
- [ ] `progest asset tag` コマンド
- [ ] `progest asset info` コマンド
- [ ] `progest asset scan` コマンド
- [ ] コマンドテスト

**成果物:**
- `src/Progest.Cli/Commands/AssetCommands.cs`
- `src/Progest.Cli.Tests/Commands/AssetCommandsTests.cs`

### ⬜ Commit 30: CLI Commands - Projects
**ステータス:** 未着手

**予定実装内容:**
- [ ] `progest project init` コマンド
- [ ] `progest project scan` コマンド
- [ ] `progest project add-rule` コマンド
- [ ] `progest project list-rules` コマンド
- [ ] コマンドテスト

**成果物:**
- `src/Progest.Cli/Commands/ProjectCommands.cs`
- `src/Progest.Cli.Tests/Commands/ProjectCommandsTests.cs`

---

## 優先順位と依存関係

### クリティカルパス
```
Phase 1 (テスト)
  ↓
Phase 2 (基本ルール)
  ↓
Phase 3 (集約ルール) ← 現在ここ
  ↓
Phase 5 (サービス層)
  ↓
Phase 6 (データベース)
  ↓
Phase 7 (アセット管理)
  ↓
Phase 9 (CLI) ← MVP リリース
```

### 並行実施可能なフェーズ
- **Phase 4** (文字列変換) は Phase 3 依存だが既に完了
- **Phase 8** (プロジェクト管理) は Phase 7 後であれば独立

### 早期実装推奨
- **Phase 5 (サービス層)**: 名名規則機能をCLIで使用するために必須
- **Phase 6 (データベース)**: データ永続化に必須

---

## 各フェーズの詳細見積もり

| フェーズ | コミット数 | 予定工時間 | 実績工時間 |
|---------|----------|----------|----------|
| Phase 1: テストインフラ | 1 | 1h | - |
| Phase 2: 基本ルール | 4 | 4h | - |
| Phase 3: 集約ルール | 2 | 3h | - |
| Phase 4: 文字列変換 | 2 | 2h | - |
| Phase 5: サービス層 | 3 | 4h | - |
| Phase 6: データベース | 5 | 6h | - |
| Phase 7: アセット管理 | 5 | 7h | - |
| Phase 8: プロジェクト管理 | 4 | 5h | - |
| Phase 9: CLI実装 | 4 | 5h | - |
| **合計** | **30** | **37h** | **約8h** |

---

## マイルストーン

### Milestone 1: 命名規則コア完了
**予定完了日:** 2026-02-15
**ステータス:** ✅ 達成

**完了したコミット:**
- Commit 1-6, 8
- テストカバレッジ: 83 tests passing

**次のステップ:**
- Commit 7, 9 を完了して Phase 3-4 を完全にする

### Milestone 2: サービス層完了
**予定完了日:** 未定

**対象コミット:**
- Commit 10-12

**完了条件:**
- [ ] INamingService インターフェース定義
- [ ] NamingService 実装
- [ ] カスタム例外クラス実装
- [ ] 単体テスト完了

### Milestone 3: データ永続化完了
**予定完了日:** 未定

**対象コミット:**
- Commit 13-17

**完了条件:**
- [ ] データベーススキーマ作成
- [ ] EF Core Migrations 完了
- [ ] Repository パターン実装
- [ ] 統合テスト完了

### Milestone 4: MVP リリース
**予定完了日:** 未定

**対象コミット:**
- Commit 1-30 のうち主要なもの

**完了条件:**
- [ ] 命名規則の適用がCLIで可能
- [ ] アセット追加・検索・タグ付けが可能
- [ ] プロジェクト初期化・スキャンが可能
- [ ] 全体テストカバレッジ 80%以上

---

## リスクと課題

### 技術的リスク
1. **EF Core 9.0 の安定性**
   - 新しいためバグの可能性
   - 対策: 公式ドキュメントの確認、問題があれば8.0にダウングレード

2. **Avalonia UI の学習コスト**
   - チームへの不慣れ
   - 対策: サンプルプロジェクトの作成、公式チュートリアルの参照

3. **SQLite のパフォーマンス**
   - 大量データ時の遅延
   - 対策: インデックス最適化、必要に応じて別DBへの移行を検討

### スケジュールリスク
1. **機能範囲の膨張**
   - 要件の増加
   - 対策: スコープの厳密な管理、MVP後の機能に分ける

2. **テスト工数の増大**
   - 網羅的なテスト作成
   - 対策: TDDの厳守、重要な機能から優先

---

## 次のアクション

### 即時アクション（次回のセッション）
1. ✅ **完了**: Commit 7 - NamingConvention 拡張メソッド実装
2. **Commit 9**: StringExtensions の追加テスト実装
3. **Commit 10**: INamingService インターフェース定義

### 今後1週間の目標
- [ ] Phase 3-4 の完了 (Commit 7, 9)
- [ ] Phase 5 の開始 (Commit 10-12)
- [ ] サービス層の基盤確立

### 今後1ヶ月の目標
- [ ] Phase 5-6 の完了
- [ ] サービス層とデータベース層の統合
- [ ] 基本的なデータ永続化の実現

---

## 用語定義

- **TDD (Test-Driven Development)**: テスト駆動開発。テストを先に書き、そのテストを通す実装を行う開発手法
- **CRUD**: Create, Read, Update, Delete の4つの基本操作
- **集約ルート (Aggregate Root)**: DDDにおけるパターン。関連するオブジェクト群を管理するルートオブジェクト
- **値オブジェクト (Value Object)**: 属性値によって同一性が決まるオブジェクト
- **Repository パターン**: データアクセスの抽象化レイヤー
- **MVVM**: Model-View-ViewModel。GUIアーキテクチャパターン
