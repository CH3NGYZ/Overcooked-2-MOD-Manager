use crate::config::ConfigService;
use crate::models::{AppUpdateCheckResult, GameInfo};
use std::sync::{Arc, Mutex};

/// 应用状态：配置、串行化锁与几个进程内缓存。
///
/// `Clone` 是给重活用的：命令层把 `State` 克隆一份带进 `spawn_blocking`，
/// 闭包里既能拿配置也能拿操作锁，不必为每个命令重写一遍参数列表。克隆的
/// 都是句柄（配置服务与各 `Arc`），共享同一份数据，不是拷贝。
#[derive(Clone)]
pub struct AppState {
    pub config: ConfigService,
    pub(crate) local_mod_snapshot: Arc<Mutex<crate::mods::snapshot::LocalModSnapshots>>,
    pub(crate) diychef_sessions: Arc<Mutex<crate::diychef::session::ChefSessions>>,
    /*
     * 串行化「写游戏目录」的操作（安装、卸载、开关、回收站）。
     *
     * `Arc` 而不是裸 `Mutex`：重活（解压落盘、全量扫描）必须搬到
     * `spawn_blocking` 里跑，锁的所有权也要跟着进闭包；同步命令在 IPC 线程
     * 上内联执行，几秒的写盘就是几秒的窗口冻结。
     */
    pub operation: Arc<Mutex<()>>,
    pub repository_cache: Arc<tokio::sync::Mutex<()>>,
    pub community: Arc<crate::community::CommunitySearch>,
    pub readme: Arc<crate::repository_readme::ReadmeDownloads>,
    // Display-only snapshot; file operations always inspect the live process.
    pub game_state: Arc<Mutex<Option<GameInfo>>>,
    /// 关卡集目录监听：外部改动后通知前端重读（见 `level_watch`）。
    pub level_watch: Arc<crate::level_watch::LevelWatcher>,
    /// 按仓库收敛的 Release 列表缓存（见 `release_cache`）。
    ///
    /// 一个合集仓的 Release 列表会被它下面的每张地图读到，还会被「文档放在
    /// Release 资产里」时的 sha256 校验读到 —— 一次刷新里只该请求一次 API。
    pub(crate) map_source_registry: Arc<crate::map_sources::progress::SourceRegistry>,
    pub release_cache: Arc<crate::release_cache::RepoReleaseCache>,
    /// 程序更新检查的 5 分钟缓存（启动时恒检测，见 `check_app_update`）。
    pub app_update_check: Arc<Mutex<Option<CachedAppUpdateCheck>>>,
}

/// 上一次成功的程序更新检查结果。
///
/// 只缓存成功结果：全部通道失败或超时的那一份不能占住缓存，否则用户在这
/// 5 分钟里连手动重试都会拿到同一份失败结论。
pub struct CachedAppUpdateCheck {
    /// 结果对应的更新渠道地区；地区变了旧结果没有意义（清单地址不同）。
    pub(crate) region: String,
    pub(crate) checked_at: i64,
    pub(crate) result: AppUpdateCheckResult,
}

impl AppState {
    /// Call immediately before a plugin/game-root mutation while operation is held.
    pub(crate) fn invalidate_local_mod_snapshot(&self) -> Result<(), String> {
        self.local_mod_snapshot
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?
            .invalidate();
        Ok(())
    }
    pub fn new() -> Result<Self, String> {
        log::debug!("创建应用状态");
        Ok(Self::with_config(
            ConfigService::new().map_err(|e| e.to_string())?,
        ))
    }

    pub(crate) fn with_config(config: ConfigService) -> Self {
        Self {
            config,
            local_mod_snapshot: Arc::default(),
            diychef_sessions: Arc::default(),
            operation: Arc::new(Mutex::new(())),
            repository_cache: Arc::new(tokio::sync::Mutex::new(())),
            community: Arc::new(crate::community::CommunitySearch::default()),
            readme: Arc::new(crate::repository_readme::ReadmeDownloads::default()),
            game_state: Arc::new(Mutex::new(None)),
            level_watch: Arc::new(crate::level_watch::LevelWatcher::default()),
            map_source_registry: Arc::default(),
            release_cache: Arc::new(crate::release_cache::RepoReleaseCache::default()),
            app_update_check: Arc::new(Mutex::new(None)),
        }
    }
}

#[cfg(test)]
mod tests;
