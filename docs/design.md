# Progest 設計書

**バージョン:** 1.0
**作成日:** 2026-02-15
**最終更新:** 2026-02-15

## 1. アーキテクチャ概要

### 1.1 全体アーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                      Presentation Layer                       │
├──────────────┬──────────────┬──────────────┬──────────────┤
│     CLI      │     GUI      │     TUI      │   (Future)    │
│  (Commands)  │ (Views/VMs)  │   (Screens)  │      API      │
└──────┬───────┴──────┬───────┴──────┬───────┴──────────────┘
       │              │              │
       └──────────────┼──────────────┘
                      │
┌─────────────────────┼───────────────────────────────────────┐
│              Application Layer (Services)                    │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐           │
│  │   Naming    │ │    Asset    │ │  Project    │           │
│  │  Service    │ │  Service    │ │  Service    │           │
│  └──────┬──────┘ └──────┬──────┘ └──────┬──────┘           │
└─────────┼───────────────┼───────────────┼──────────────────┘
          │               │               │
┌─────────┼───────────────┼───────────────┼──────────────────┐
│              Domain Layer (Core & Models)                  │
│  ┌────────────────────────────────────────────────┐        │
│  │      NamingConvention (Aggregate Root)         │        │
│  │  - PrefixRule, SuffixRule, Sequential...      │        │
│  ├────────────────────────────────────────────────┤        │
│  │      Asset (Aggregate Root)                    │        │
│  │  - Tags, Metadata, Dependencies               │        │
│  ├────────────────────────────────────────────────┤        │
│  │      Project (Aggregate Root)                  │        │
│  │  - DirectoryRules, Settings                    │        │
│  └────────────────────────────────────────────────┘        │
└─────────┼───────────────┼───────────────┼─────────────────┘
          │               │               │
┌─────────┼───────────────┼───────────────┼─────────────────┐
│           Infrastructure Layer (Db & Repositories)        │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐         │
│  │NamingConv   │ │   Asset     │ │  Project    │         │
│  │Repository   │ │ Repository  │ │ Repository  │         │
│  └──────┬──────┘ └──────┬──────┘ └──────┬──────┘         │
└─────────┼───────────────┼───────────────┼─────────────────┘
          │               │               │
┌─────────┼───────────────┼───────────────┼─────────────────┐
│                   Data Layer (SQLite)                      │
│              ProgestDbContext (EF Core)                    │
└────────────────────────────────────────────────────────────┘
```

### 1.2 レイヤー責務

#### Presentation Layer
- ユーザーインターフェースの提供
- ユーザー入力の受付と検証
- サービス層の呼び出し
- 結果の表示

#### Application Layer (Services)
- ユースケースの実装
- ドメインオブジェクトの調整
- トランザクション管理
- ビジネスロジックのオーケストレーション

#### Domain Layer
- ビジネスルールの実装
- ドメインモデルの定義
- 値オブジェクトと集約の管理
- ドメインイベントの発行

#### Infrastructure Layer
- データアクセスの実装
- 外部システムとの連携
- 技術的な詳細の隠蔽

## 2. ソリューション構成

```
Progest.slnx
├── Progest.Core                  # ドメイン層
│   ├── Models/                   # ドメインモデル
│   │   ├── NamingConvention/     # 命名規則関連
│   │   ├── Assets/               # アセット関連
│   │   └── Projects/             # プロジェクト関連
│   ├── Services/                 # ドメインサービス
│   ├── Extensions/               # 拡張メソッド
│   ├── Exceptions/               # カスタム例外
│   └── Interfaces/               # リポジトリインターフェース
│
├── Progest.Core.Tests            # Coreのテスト
│   └── (ミラーリング構造)
│
├── Progest.Db                    # インフラ層
│   ├── Entities/                 # EF Core エンティティ
│   ├── Data/                     # DbContext
│   ├── Repositories/             # リポジトリ実装
│   └── Migrations/               # データベースマイグレーション
│
├── Progest.Cli                   # CLI プレゼンテーション層
│   └── Commands/                 # コマンド定義
│
├── Progest.Gui                   # GUI プレゼンテーション層
│   ├── Views/                    # XAML ビュー
│   ├── ViewModels/               # ViewModel
│   └── Models/                   # UI モデル
│
├── Progest.Tui                   # TUI プレゼンテーション層
│   └── Screens/                  # 画面定義
│
└── Progest.Db.Tests              # Dbの統合テスト
```

## 3. ドメインモデル設計

### 3.1 命名規則（NamingConvention）

#### 集約ルート
```csharp
public class NamingConvention
{
    public Guid Id { get; }
    public string Name { get; }
    public ConventionType Type { get; }

    // 値オブジェクト
    public PrefixRule? Prefix { get; }
    public SuffixRule? Suffix { get; }
    public SequentialNumberingRule? SequentialNumbering { get; }

    // メソッド
    public string Apply(string input);
    public string ApplyBatch(string input, int index);
}
```

#### 値オブジェクト

**PrefixRule:**
```csharp
public class PrefixRule
{
    public PrefixType Type { get; }
    public string? FixedValue { get; }
    public DateFormat? DateFormat { get; }

    public string GeneratePrefix();
}
```

**SuffixRule:**
```csharp
public class SuffixRule
{
    public SuffixType Type { get; }
    public string? FixedValue { get; }
    public VersionFormat? VersionFormat { get; }
    public int MajorVersion { get; }
    public int MinorVersion { get; }
    public int PatchVersion { get; }

    public string GenerateSuffix();
}
```

**SequentialNumberingRule:**
```csharp
public class SequentialNumberingRule
{
    public int StartNumber { get; }
    public int DigitCount { get; }
    public string Separator { get; }

    public string Format(int number);
}
```

### 3.2 アセット（Asset）

#### 集約ルート
```csharp
public class Asset
{
    public Guid Id { get; }
    public string FilePath { get; }
    public string FileName { get; }
    public DateTime CreatedAt { get; }
    public DateTime UpdatedAt { get; }

    // コレクション
    private readonly List<Tag> _tags = new();
    public IReadOnlyList<Tag> Tags => _tags.AsReadOnly();

    private readonly List<FileDependency> _dependencies = new();
    public IReadOnlyList<FileDependency> Dependencies => _dependencies.AsReadOnly();

    public AssetMetadata Metadata { get; }

    // ビヘイビア
    public void AddTag(Tag tag);
    public void RemoveTag(Tag tag);
    public void AddDependency(FileDependency dependency);
    public void RemoveDependency(FileDependency dependency);
    public bool CanBeDeleted();
}
```

#### 値オブジェクト

**Tag:**
```csharp
public record Tag(
    Guid Id,
    string Name,
    string Color,
    Guid? ParentTagId = null
);
```

**AssetMetadata:**
```csharp
public record AssetMetadata(
    string Description,
    string Creator,
    Dictionary<string, string> CustomProperties
);
```

**FileDependency:**
```csharp
public class FileDependency
{
    public Guid AssetId { get; }
    public Guid DependentAssetId { get; }
    public DependencyType Type { get; }
    public string? Description { get; }

    public bool IsStrongDependency { get; }
}
```

### 3.3 プロジェクト（Project）

#### 集約ルート
```csharp
public class Project
{
    public Guid Id { get; }
    public string Name { get; }
    public string RootPath { get; }
    public DateTime CreatedAt { get; }

    public ProjectSettings Settings { get; }
    private readonly List<DirectoryRule> _directoryRules = new();

    public NamingConvention DefaultNamingConvention { get; }

    // ビヘイビア
    public void AddDirectoryRule(DirectoryRule rule);
    public void RemoveDirectoryRule(DirectoryRule rule);
    public DirectoryRule? GetRuleForPath(string path);
}
```

#### 値オブジェクト

**ProjectSettings:**
```csharp
public record ProjectSettings(
    bool AutoTag,
    bool TrackDependencies,
    bool UseSidecarFiles,
    string DatabasePath
);
```

**DirectoryRule:**
```csharp
public class DirectoryRule
{
    public string Pattern { get; }
    public NamingConvention NamingConvention { get; }
    public bool ApplyRecursively { get; }

    public bool Matches(string path);
}
```

## 4. データベース設計

### 4.1 エンティティ関係図

```
┌──────────────────┐
│ NamingConvention│
├──────────────────┤
│ Id (PK)         │──┐
│ Name            │  │
│ Type            │  │
└──────────────────┘  │
                     │
┌──────────────────┐  │     ┌──────────────────┐
│ PrefixRule      │  │     │ SuffixRule       │
├──────────────────┤  │     ├──────────────────┤
│ Id (PK)         │  │     │ Id (PK)         │
│ ConventionId (FK)│─┼─────│ ConventionId (FK)│─┼──┐
│ Type            │  │     │ Type            │  │  │
│ FixedValue      │  │     │ FixedValue      │  │  │
│ DateFormat      │  │     │ VersionFormat   │  │  │
└──────────────────┘  │     │ MajorVersion    │  │  │
                     │     │ MinorVersion    │  │  │
┌──────────────────┐  │     │ PatchVersion    │  │  │
│ SequentialNum   │  │     └──────────────────┘  │  │
├──────────────────┤  │                            │  │
│ Id (PK)         │  │                            │  │
│ ConventionId (FK)│─┼────────────────────────────┘  │
│ StartNumber     │  │                               │
│ DigitCount      │  │                               │
│ Separator       │  │                               │
└──────────────────┘  │                               │
                     │                               │
                     │     ┌──────────────────┐      │
                     │     │      Asset       │      │
                     │     ├──────────────────┤      │
                     │     │ Id (PK)         │      │
                     │     │ FilePath        │      │
                     │     │ FileName        │      │
                     │     │ CreatedAt       │      │
                     │     │ UpdatedAt       │      │
                     │     └──────────────────┘      │
                     │              │                │
                     │              │                │
                     │     ┌────────┴────────┐       │
                     │     │                 │       │
                     │  ┌──┴──────┐    ┌────┴───┐   │
                     │  │AssetTag │    │Dependency│  │
                     │  ├─────────┤    ├─────────┤  │
                     │  │AssetId  │    │AssetId  │  │
                     │  │TagId    │    │Dependent│  │
                     │  └─────────┘    │AssetId  │  │
                     │                 └─────────┘  │
                     │                               │
                     │     ┌──────────────────┐      │
                     └─────│      Project     │      │
                           ├──────────────────┤      │
                           │ Id (PK)         │      │
                           │ Name            │      │
                           │ RootPath        │      │
                           │ DefaultConvId(FK)│─────┘
                           └──────────────────┘
```

### 4.2 テーブル定義

**NamingConventions:**
```sql
CREATE TABLE NamingConventions (
    Id TEXT PRIMARY KEY,
    Name TEXT NOT NULL,
    Type INTEGER NOT NULL, -- ConventionType enum
    CreatedAt TEXT NOT NULL, -- datetime
    UpdatedAt TEXT NOT NULL -- datetime
);
```

**PrefixRules:**
```sql
CREATE TABLE PrefixRules (
    Id TEXT PRIMARY KEY,
    ConventionId TEXT NOT NULL,
    Type INTEGER NOT NULL, -- PrefixType enum
    FixedValue TEXT,
    DateFormat INTEGER, -- DateFormat enum (nullable)
    FOREIGN KEY (ConventionId) REFERENCES NamingConventions(Id) ON DELETE CASCADE
);
```

**SuffixRules:**
```sql
CREATE TABLE SuffixRules (
    Id TEXT PRIMARY KEY,
    ConventionId TEXT NOT NULL,
    Type INTEGER NOT NULL, -- SuffixType enum
    FixedValue TEXT,
    VersionFormat INTEGER, -- VersionFormat enum (nullable)
    MajorVersion INTEGER DEFAULT 0,
    MinorVersion INTEGER DEFAULT 0,
    PatchVersion INTEGER DEFAULT 0,
    FOREIGN KEY (ConventionId) REFERENCES NamingConventions(Id) ON DELETE CASCADE
);
```

**Assets:**
```sql
CREATE TABLE Assets (
    Id TEXT PRIMARY KEY,
    FilePath TEXT NOT NULL UNIQUE,
    FileName TEXT NOT NULL,
    Description TEXT,
    Creator TEXT,
    CreatedAt TEXT NOT NULL,
    UpdatedAt TEXT NOT NULL
);

CREATE INDEX idx_assets_filepath ON Assets(FilePath);
CREATE INDEX idx_assets_filename ON Assets(FileName);
```

**Tags:**
```sql
CREATE TABLE Tags (
    Id TEXT PRIMARY KEY,
    Name TEXT NOT NULL UNIQUE,
    Color TEXT NOT NULL,
    ParentTagId TEXT,
    FOREIGN KEY (ParentTagId) REFERENCES Tags(Id) ON DELETE SET NULL
);
```

**AssetTags (多対多):**
```sql
CREATE TABLE AssetTags (
    AssetId TEXT NOT NULL,
    TagId TEXT NOT NULL,
    PRIMARY KEY (AssetId, TagId),
    FOREIGN KEY (AssetId) REFERENCES Assets(Id) ON DELETE CASCADE,
    FOREIGN KEY (TagId) REFERENCES Tags(Id) ON DELETE CASCADE
);

CREATE INDEX idx_assettags_tag ON AssetTags(TagId);
```

**FileDependencies:**
```sql
CREATE TABLE FileDependencies (
    Id TEXT PRIMARY KEY,
    AssetId TEXT NOT NULL,
    DependentAssetId TEXT NOT NULL,
    Type INTEGER NOT NULL, -- DependencyType enum
    Description TEXT,
    FOREIGN KEY (AssetId) REFERENCES Assets(Id) ON DELETE CASCADE,
    FOREIGN KEY (DependentAssetId) REFERENCES Assets(Id) ON DELETE CASCADE
);

CREATE INDEX idx_dependencies_asset ON FileDependencies(AssetId);
CREATE INDEX idx_dependencies_dependent ON FileDependencies(DependentAssetId);
```

**Projects:**
```sql
CREATE TABLE Projects (
    Id TEXT PRIMARY KEY,
    Name TEXT NOT NULL,
    RootPath TEXT NOT NULL UNIQUE,
    AutoTag INTEGER NOT NULL DEFAULT 0, -- boolean
    TrackDependencies INTEGER NOT NULL DEFAULT 1, -- boolean
    UseSidecarFiles INTEGER NOT NULL DEFAULT 1, -- boolean
    DatabasePath TEXT,
    DefaultNamingConventionId TEXT,
    CreatedAt TEXT NOT NULL,
    FOREIGN KEY (DefaultNamingConventionId) REFERENCES NamingConventions(Id)
);
```

## 5. サービス層設計

### 5.1 INamingService

```csharp
public interface INamingService
{
    // CRUD
    Task<NamingConvention> CreateAsync(NamingConvention convention);
    Task<NamingConvention?> GetByIdAsync(Guid id);
    Task<NamingConvention?> GetByNameAsync(string name);
    Task<IEnumerable<NamingConvention>> GetAllAsync();
    Task UpdateAsync(NamingConvention convention);
    Task DeleteAsync(Guid id);

    // ビヘイビア
    Task<string> ApplyConventionAsync(Guid conventionId, string input);
    Task<IEnumerable<string>> ApplyConventionBatchAsync(
        Guid conventionId,
        IEnumerable<string> inputs
    );
}
```

### 5.2 IAssetService

```csharp
public interface IAssetService
{
    // CRUD
    Task<Asset> CreateAsync(Asset asset);
    Task<Asset?> GetByIdAsync(Guid id);
    Task<Asset?> GetByFilePathAsync(string filePath);
    Task<IEnumerable<Asset>> GetAllAsync();
    Task UpdateAsync(Asset asset);
    Task DeleteAsync(Guid id);

    // タグ管理
    Task AddTagAsync(Guid assetId, Tag tag);
    Task RemoveTagAsync(Guid assetId, Guid tagId);
    Task<IEnumerable<Asset>> GetByTagAsync(Guid tagId);

    // 依存関係管理
    Task AddDependencyAsync(Guid assetId, FileDependency dependency);
    Task RemoveDependencyAsync(Guid assetId, Guid dependencyId);
    Task<bool> CanDeleteAssetAsync(Guid id);

    // 検索
    Task<IEnumerable<Asset>> SearchAsync(SearchCriteria criteria);
}

public record SearchCriteria(
    string? Keyword = null,
    IEnumerable<Guid>? TagIds = null,
    DateTime? CreatedAfter = null,
    DateTime? CreatedBefore = null,
    string? FileNamePattern = null
);
```

### 5.3 IProjectService

```csharp
public interface IProjectService
{
    // CRUD
    Task<Project> CreateAsync(Project project);
    Task<Project?> GetByIdAsync(Guid id);
    Task<Project?> GetByRootPathAsync(string rootPath);
    Task<IEnumerable<Project>> GetAllAsync();
    Task UpdateAsync(Project project);
    Task DeleteAsync(Guid id);

    // ディレクトリルール
    Task AddDirectoryRuleAsync(Guid projectId, DirectoryRule rule);
    Task RemoveDirectoryRuleAsync(Guid projectId, Guid ruleId);
    Task<DirectoryRule?> GetRuleForPathAsync(Guid projectId, string path);

    // スキャン
    Task<ScanResult> ScanProjectAsync(Guid projectId);
    Task ApplyNamingConventionAsync(Guid projectId, Guid conventionId);
}

public record ScanResult(
    int TotalFiles,
    int FilesProcessed,
    int FilesRenamed,
    IEnumerable<string> Errors
);
```

## 6. リポジトリパターン

### 6.1 IRepository (汎用)

```csharp
public interface IRepository<T> where T : class
{
    Task<T?> GetByIdAsync(Guid id);
    Task<IEnumerable<T>> GetAllAsync();
    Task AddAsync(T entity);
    Task UpdateAsync(T entity);
    Task DeleteAsync(Guid id);
    Task<bool> ExistsAsync(Guid id);
}
```

### 6.2 INamingConventionRepository

```csharp
public interface INamingConventionRepository : IRepository<NamingConvention>
{
    Task<NamingConvention?> GetByNameAsync(string name);
    Task<IEnumerable<NamingConvention>> GetByTypeAsync(ConventionType type);
}
```

### 6.3 IAssetRepository

```csharp
public interface IAssetRepository : IRepository<Asset>
{
    Task<Asset?> GetByFilePathAsync(string filePath);
    Task<IEnumerable<Asset>> GetByTagAsync(Guid tagId);
    Task<IEnumerable<Asset>> SearchAsync(SearchCriteria criteria);
    Task<IEnumerable<Asset>> GetDependentAssetsAsync(Guid assetId);
}
```

## 7. GUI デザイン（Avalonia UI）

### 7.1 MVVM パターン

```
View (AXAML)  ←→  ViewModel  ←→  Model  ←→  Service
     ↓                    ↓
   Binding           Command
```

### 7.2 メインウィンドウ構成

```
┌────────────────────────────────────────────────────────┐
│ Progest                                   [_][□][×]  │
├────────────────────────────────────────────────────────┤
│ File  Edit  View  Tools  Help                         │
├────────────────────────────────────────────────────────┤
│ ┌────────────┐ ┌─────────────────────────────────────┐│
│ │            │ │                                     ││
│ │  Navigator │ │         Main Content Area           ││
│ │            │ │                                     ││
│ │            │ │                                     ││
│ │            │ │                                     ││
│ └────────────┘ └─────────────────────────────────────┘│
├────────────────────────────────────────────────────────┤
│ Status: Ready | Files: 1,234 | Selected: 5            │
└────────────────────────────────────────────────────────┘
```

### 7.3 主要画面

1. **命名規則管理画面**
   - 命名規則一覧
   - 作成/編集/削除ボタン
   - プレビューパネル

2. **アセットブラウザ画面**
   - ファイルツリー
   - タグフィルター
   - 検索バー
   - メタデータパネル

3. **プロジェクト設定画面**
   - プロジェクト情報
   - ディレクトリルール設定
   - デフォルト命名規則設定

## 8. CLI デザイン

### 8.1 コマンド構造

```
progest
├── naming
│   ├── apply <convention> <files...>
│   ├── create <name> [options]
│   ├── list
│   ├── delete <id>
│   └── preview <convention> <files...>
├── asset
│   ├── scan <directory>
│   ├── tag <file> <tags...>
│   ├── search <keyword>
│   └── info <file>
└── project
    ├── init <name> [path]
    ├── add-rule <pattern> <convention>
    └── list-rules
```

### 8.2 使用例

```bash
# 命名規則の作成
progest naming create "Snake Case" --type snake --prefix "img_"

# 命名規則の適用
progest naming apply "Snake Case" ./photos/*.jpg

# アセットのスキャン
progest asset scan ./project

# タグ付け
progest asset tag photo.jpg "nature" "landscape"

# 検索
progest asset search "landscape" --tag "nature"

# プロジェクトの初期化
progest project init "MyProject" ./myproject
```

## 9. エラーハンドリング

### 9.1 カスタム例外

```csharp
// 命名規則関連
public class ConventionNotFoundException : Exception
public class InvalidConventionException : Exception
public class ConventionApplicationException : Exception

// アセット関連
public class AssetNotFoundException : Exception
public class AssetHasDependenciesException : Exception
public class InvalidFilePathException : Exception

// プロジェクト関連
public class ProjectNotFoundException : Exception
public class ProjectAlreadyExistsException : Exception
```

### 9.2 エラー処理方針

1. **プレゼンテーション層**: ユーザーフレンドリーなメッセージに変換
2. **サービス層**: 適切な例外にラップして再スロー
3. **ドメイン層**: ビジネスルール違反を例外で通知
4. **インフラ層**: 技術的な例外をドメイン例外に変換

## 10. パフォーマンス設計

### 10.1 キャッシュ戦略

- メタデータキャッシュ（IMemoryCache）
- クエリ結果キャッシュ
- データベース接続プーリング

### 10.2 インデックス戦略

- FilePath: 高速ファイル検索
- FileName: ファイル名検索
- TagId: タグによるフィルタリング
- CreatedAt/UpdatedAt: 日付範囲検索

### 10.3 バッチ処理

- 一括リネーム: 1000件/チャンク
- 一括タグ付け: 500件/チャンク
- データベース一括挿入: BulkInsert使用

## 11. セキュリティ設計

### 11.1 データ保護

- ファイルパスのサニタイズ
- SQLインジェクション対策（パラメータ化クエリ）
- ユーザー入力のバリデーション

### 11.2 ファイルシステム操作

- 原子的なファイル操作
- ロールバック機能
- バックアップ作成

## 12. テスト戦略

### 12.1 テストピラミッド

```
        ┌─────┐
       / E2E  \      5% (将来実装)
      /────────\
     /  統合   \     15% (Db.Tests)
    /────────────\
   /   単体テスト  \   80% (Core.Tests)
  /────────────────\
```

### 12.2 テストカバレッジ目標

- **単体テスト**: 90%以上
- **統合テスト**: 70%以上
- **全体的**: 80%以上

## 13. 移行計画

### 13.1 フェーズ1（MVP）
- 基本的な命名規則機能
- 簡単なタグ機能
- CLI の基本操作

### 13.2 フェーズ2
- 完全なGUI実装
- 高度なタグ機能
- 依存関係管理

### 13.3 フェーズ3
- TUI実装
- パフォーマンス最適化
- プラグインシステム

## 14. 技術的負債管理

### 14.1 負債追跡
- GitHub Issues で技術的負債を管理
- 優先度付け: Critical > High > Medium > Low

### 14.2 リファクタリング計画
- 四半期に1回の大規模リファクタリング
- 月次の小規模リファクタリング
- コードレビューでの負債検出
