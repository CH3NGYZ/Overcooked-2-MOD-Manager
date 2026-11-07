use super::maps::AssetVersionEntry;
use super::relations::ReleaseRelationOverride;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 这次授权打开的是哪个页面。
///
/// `install` = 还没装过 App，直接去安装页：装 App 与勾仓库一步完成，回调会把
/// `installation_id` 一起带回来；`authorize` = 已经装过，只重新授权，不再经过
/// 安装页 —— GitHub 对已安装的账号会停在配置页，不会回调本机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishAuthMode {
    Install,
    Authorize,
}

/// 授权流程启动后的返回：系统浏览器已经打开，前端可以给出备用链接。
#[derive(Debug, Clone, Serialize)]
pub struct PublishAuthorization {
    pub authorize_url: String,
    pub expires_in: u64,
    /// 前端据此决定按钮是「安装 App 并选择仓库」还是「登录 GitHub」。
    pub mode: PublishAuthMode,
}

/// 授权轮询结果。
///
/// 用 `status` 而不是 `Result`：等待、过期、拒绝都是正常分支，不该在前端被当成
/// 异常弹红字。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PublishAuthPoll {
    Pending,
    Authorized { login: String },
    Expired,
    Denied,
    Failed { reason: String },
}

/// 进入发布页时检查一次登录状态的结论。
///
/// - `valid`：本地令牌可用，没有刷新；
/// - `refreshed`：access token 失效，已用 refresh token 换到新的一对并落盘；
/// - `reauthorize`：令牌失效且无法刷新（没有 / 过期 / 被拒绝），本地令牌已清掉，
///   界面要请用户重新授权；`reason` 是语言表 tag；
/// - `unavailable`：网络或 GitHub 暂时不可用，保留当前凭据；若已刷新落盘，不回滚到旧令牌；
/// - `signed_out`：本来就没登录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PublishSessionOutcome {
    SignedOut,
    Valid,
    Refreshed,
    Reauthorize { reason: String },
    Unavailable { reason: String },
}

/// 会话检查的返回：结论 + 检查之后的授权状态（刷新或清除后会变）。
#[derive(Debug, Clone, Serialize)]
pub struct PublishSessionCheck {
    pub outcome: PublishSessionOutcome,
    pub auth: PublishAuthState,
}

/// 上传页的授权状态（登录名与 client_id 都是明文，Token 本身不出后端）。
///
/// `install_url` 是 GitHub App 的安装入口（安装时可以选择「Only select
/// repositories」）；`secret_configured` 为 false 说明这个构建没有注入 Client
/// Secret，根本换不到 Token，界面要直接说清楚而不是点登录才报错。
#[derive(Debug, Clone, Serialize)]
pub struct PublishAuthState {
    pub authenticated: bool,
    pub login: String,
    pub client_id: String,
    pub install_url: String,
    /// 已经记下上次安装的 installation id：有它时登录只重新授权，不再回安装页。
    pub installed: bool,
    pub secret_configured: bool,
}

/// 上传页可选的一个地图仓库。
#[derive(Debug, Clone, Serialize)]
pub struct PublishRepository {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub default_branch: String,
    pub topics: Vec<String>,
    /// 当前令牌对这个仓库有没有 push 权限（只读仓库不允许发布）。
    pub push: bool,
}

/// 发布页的仓库列表。
///
/// 光有仓库列表不够：`installations == 0` 说明 GitHub App 还没装到这个账号上，
/// 这时必须先走安装页勾选仓库，而不是让用户对着空列表「先新建一个」。
#[derive(Debug, Clone, Serialize)]
pub struct PublishRepositoryList {
    /// 授权到本 App 的安装数量；0 = 还没安装。
    pub installations: usize,
    /// 是否有安装范围是「全部仓库」：只有这时新建的仓库才自动落在授权范围内。
    pub all_repositories: bool,
    pub repositories: Vec<PublishRepository>,
}

/// 一个 Release 附件。
#[derive(Debug, Clone, Serialize)]
pub struct PublishAsset {
    pub id: u64,
    pub name: String,
    pub size: u64,
}

/// 本地 ZIP 识别摘要。发布前从 info 取版本，原文件名版本仅供核对。
#[derive(Debug, Clone, Default, Serialize)]
pub struct PublishZipInspection {
    /// ZIP 文件名（不含目录）。
    pub file_name: String,
    /// 本地 ZIP 的字节数，用于上传前确认。
    pub file_size: u64,
    /// 原 ZIP 文件名里的版本；仅供核对，不能作为上传版本。
    pub mapset_version: String,
    /// `info` 里的版本号。
    pub info_version: String,
    pub level_set_name: String,
    pub level_set_name_zh: String,
    pub author: String,
    pub level_count: usize,
    pub file_count: usize,
    /// 发布只接受单地图，数量不为 1 时拒绝上传。
    pub set_count: usize,
    /// `requires.txt` 声明的最低 runtime 版本（GUID → 版本区间）。
    pub requires: BTreeMap<String, String>,
}

/// 仓库标签检查结果：`required` 是发布页要求的三件套，`missing` 为空表示齐全。
#[derive(Debug, Clone, Default, Serialize)]
pub struct PublishRepositoryTopics {
    pub topics: Vec<String>,
    pub required: Vec<String>,
    pub missing: Vec<String>,
}

/// 上传页看到的「一版地图」，就是仓库里的一个 Release。
#[derive(Debug, Clone, Serialize)]
pub struct PublishRelease {
    pub id: u64,
    /// tag 是这张地图在仓库里的唯一标识（一个 tag = 一张地图集），不是版本号。
    pub tag: String,
    pub title: String,
    pub draft: bool,
    pub prerelease: bool,
    pub created_at: String,
    pub published_at: Option<String>,
    /// 块外面的说明文字（`oc2-map` 块已剥掉）。
    pub body: String,
    pub assets: Vec<PublishAsset>,
    /// 解析出的元数据；没有 `oc2-map` 块（不是地图 Release）时为 None。
    pub metadata: Option<MapPublishMetadata>,
}

/// 前端编辑并提交的元数据。
///
/// 与 `map_note::MapNoteMetadata` 的差别只有关系表：这里用「GUID → 区间字符串」，
/// 默认关系用简写表；按资产版本的覆盖关系复用 `ReleaseRelationOverride`，
/// 读写时保留其中的 `prerelease` / `optional`，避免编辑版本区间丢掉原声明。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MapPublishMetadata {
    #[serde(default)]
    pub level_set_name: String,
    #[serde(default)]
    pub level_set_name_zh: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub asset: String,
    #[serde(default)]
    pub latest_mapset_version: String,
    #[serde(default)]
    pub latest_info_version: String,
    #[serde(default)]
    pub requires: BTreeMap<String, String>,
    #[serde(default)]
    pub compatible: BTreeMap<String, String>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, String>,
    /// 版本表（末项最新）；空表 = 退回按附件名里的数字段排。
    #[serde(default)]
    pub asset_versions: Vec<AssetVersionEntry>,
    #[serde(default)]
    pub release_relations: Vec<ReleaseRelationOverride>,
}

/// 新建或修改一版地图的请求。
#[derive(Debug, Clone, Deserialize)]
pub struct PublishSaveRequest {
    pub owner: String,
    pub repository: String,
    /// 有值 = 改这一版；空 = 新建一版（Release 会自动建 tag）。
    #[serde(default)]
    pub release_id: Option<u64>,
    /// 地图集的地图 ID：一个 tag 一张图，版本号写在附件名里。
    pub tag: String,
    /// 块外面的说明文字。
    #[serde(default)]
    pub body: String,
    /// ZIP 前置四步流程在登记版本时带它；省略时保留远端已有的 `oc2-map` 块。
    #[serde(default)]
    pub metadata: Option<MapPublishMetadata>,
}

/// 上传附件请求。
#[derive(Debug, Clone, Deserialize)]
pub struct PublishUploadRequest {
    pub owner: String,
    pub repository: String,
    pub release_id: u64,
    /// 本地 ZIP 的绝对路径；附件名由后端从 ZIP 版本和 Release 规则生成。
    pub path: String,
}

/// 上传进度：已读入 HTTP 请求流的字节数；done 表示服务端确认成功。
#[derive(Debug, Clone, Serialize)]
pub struct PublishUploadProgress {
    pub sent: u64,
    pub total: u64,
    pub phase: &'static str,
}

/// 删除一版地图的请求。
#[derive(Debug, Clone, Deserialize)]
pub struct PublishDeleteReleaseRequest {
    pub owner: String,
    pub repository: String,
    pub release_id: u64,
    /// 删 Release 之后要不要顺手删同名 tag（标签模式下留孤儿 tag 会让
    /// 作者以为自己还发布了这一版）。
    pub tag: String,
    pub delete_tag: bool,
}

/// 一组 GitHub App 用户令牌（首次授权或刷新的结果），时刻都是 Unix 秒。
///
/// 只在后端流转，不序列化给前端：令牌本身不出后端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
}
