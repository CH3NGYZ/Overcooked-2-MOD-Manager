//! 共享配置与 IPC 模型；根只声明和导出，各领域保留唯一 serde 定义。

mod app;
mod config;
mod game;
mod levels;
mod maps;
mod mods;
mod network;
mod network_plan;
mod operations;
mod publish;
mod relations;
mod releases;
mod repositories;

pub use app::{
    AppUpdateChannelEvent, AppUpdateChannelStatus, AppUpdateCheckResult, AppUpdateInfo,
    AppUpdatePhaseEvent, DashboardData, RustLog,
};
pub use config::{AppConfig, AuthConfig, LaunchConfig, PublicConfig, SettingsPatch, TokenPatch};
pub use game::{BepInExInfo, DiyLevelInfo, GameConfig, GameInfo, GameRuntime, WinhttpState};
pub use levels::{
    LevelSetFlattenCandidate, LevelSetFlattenReport, LevelSetImportResult, LevelSetInfo,
    LevelSetLayout, LevelSetPlugin, LevelSetScanProgress, LevelSetZipProgress, MapInstallOrigin,
    StagedMapSource, ZipLevelSet, ZipLevelSetPlugin,
};
pub use maps::{
    AssetVersionEntry, MapInfo, MapRelationSnapshot, MapRelationSnapshots, MapRepositoryConfig,
    RequiredFramework,
};
pub use mods::{
    DllPluginInfo, DllRole, InstalledMod, InstalledModStatusLabel, ModNote, ModRecycleEntry,
    UninstallLocalModResult,
};
pub use network::{GitHubProxy, GitHubProxyConfig, GitHubProxyManifest, NetworkConfig, ProxyMode};
pub use operations::{BepInExInstallResult, InstallConflict, OperationResult};
pub use publish::{
    MapPublishMetadata, PublishAsset, PublishAuthMode, PublishAuthPoll, PublishAuthState,
    PublishAuthorization, PublishDeleteReleaseRequest, PublishRelease, PublishRepository,
    PublishRepositoryList, PublishRepositoryTopics, PublishSaveRequest, PublishSessionCheck,
    PublishSessionOutcome, PublishTokens, PublishUploadProgress, PublishUploadRequest,
    PublishZipInspection,
};
pub use relations::{
    DependencySyncResult, RelationKind, ReleaseRelationOverride, RepositoryRelation,
};
pub use releases::{ReleaseAsset, ReleaseInfo, UpdateStatus};
pub use repositories::{
    AssetRule, AssetRuleType, AvatarRequest, Channel, CommunityDiscovery, CommunityMapRepository,
    ImportPreview, Provider, RepositoryBinding, RepositoryBundle, RepositoryConfig,
    RepositoryDownloadMode, RepositoryExport, RepositoryPatch, RepositorySource,
    RepositorySourceImportResult, StagedReleaseCache,
};

pub(crate) use maps::default_snapshot_schema_version;

pub use network_plan::{
    NetworkPlanSelection, PlanOverride, PlanReference, PlanStep, RepositoryKind, RouteOverride,
};
