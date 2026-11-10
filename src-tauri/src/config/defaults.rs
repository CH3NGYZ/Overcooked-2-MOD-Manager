//! 默认配置与跨层共享的界面缩放默认值。

use super::availability::current_user_region;
use super::sources::builtin_repository_sources_for_region;
use crate::models::*;

/// 默认配置。
///
/// 测试也需要一份结构完整的 `AppConfig`（字段多且部分无 serde 默认值），
/// 所以对 crate 内可见 —— 复制一份字段列表到测试里，新增字段时就会漏。
/// 配置文件的当前结构版本。
///
/// 只在「需要区分旧写法」时递增：加载时按版本号跑一次性迁移，然后把新版本
/// 写回，迁移不会重复执行。历史：1 = V2 初期两个 `builtin_*_sources_only`
/// 默认开启；2 = 两者改为默认关闭，跟随远端索引。
pub(crate) const CONFIG_SCHEMA_VERSION: u32 = 2;

/// 与前端的启动兜底、恢复默认共用一份编译期资源，避免跨层默认值漂移。
pub(crate) fn default_ui_zoom() -> f64 {
    include_str!("../../../src/uiZoomDefault.json")
        .trim()
        .parse()
        .expect("bundled UI zoom default must be numeric")
}

pub(crate) fn default_config() -> AppConfig {
    AppConfig {
        schema_version: CONFIG_SCHEMA_VERSION,
        ui_zoom: default_ui_zoom(),
        hide_unidentified_mods: true,
        // 默认开启：删除的 MOD 先进回收站，用户能反悔。
        recycle_deleted_mods: true,
        // 两类元数据默认都用全部 CPU 核心，线程数由 metadata_read 解析。
        mod_metadata_concurrency: crate::concurrency::AUTO_CONCURRENCY,
        map_info_concurrency: crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY,
        map_compression_concurrency: crate::concurrency::AUTO_CONCURRENCY,
        // 全新配置没看过任何教程步骤，首次启动会完整播一遍。
        tutorial_watched_steps: Vec::new(),
        game: GameConfig {
            path: String::new(),
            // 与 serde 的字段默认值同一处定义：macOS 默认也走原生。
            runtime: GameRuntime::default(),
        },
        network: NetworkConfig {
            timeout_seconds: 20,
            download_threads: 32,
            max_retry_count: 3,
        },
        launch: LaunchConfig {
            ask_user_to_start_from_steam: true,
            auto_restore_winhttp: true,
        },
        auth: AuthConfig {
            gitcode_publish: Default::default(),
            github_token: String::new(),
            gitee_token: String::new(),
            gitcode_token: String::new(),
            official_api_only: true,
            github_publish_token: String::new(),
            github_publish_refresh_token: String::new(),
            github_publish_token_expires_at: None,
            github_publish_refresh_expires_at: None,
            github_publish_login: String::new(),
            github_publish_installation_id: 0,
        },
        repositories: Vec::new(),
        /*
         * 新配置的官方根源按地区指向远端索引，与下面对应的开关保持同一个状态。
         *
         * 两个「仅使用内置源」开关默认关闭：内置快照退居为远程索引读不到时的
         * 回退数据（见 `refresh_sources` / `refresh_map_sources`），正常情况下
         * 用远端的最新条目。
         */
        repository_sources: builtin_repository_sources_for_region(current_user_region(), false),
        community_discovery: None,
        gitee_community_discovery: None,
        gitcode_community_discovery: None,
        community_map_repositories: Vec::new(),
        source_repository_ids: Default::default(),
        staged_release_cache: Default::default(),
        /*
         * 新配置直接带上内置地图快照。
         *
         * 放在这里而不是「第一次读地图列表时补」：`get_maps` 是只读命令，
         * 在里面写盘会绕开短操作锁，与「整份配置读取、修改与保存须持锁」
         * 的边界冲突。内置快照是编译期资源，同步构造没有副作用。
         *
         * 快照不再是默认数据来源：`builtin_map_sources_only` 默认关闭，刷新
         * 先读远端索引，失败时才用这份快照兜底。
         */
        map_repositories: crate::map_sources::bundled_map_tree(),
        builtin_map_sources_only: false,
        builtin_proxies_only: false,
        builtin_network_plans_only: false,
        network_policy: None,
        github_proxies: crate::github_proxies::bundled_defaults(),
        mod_notes: Vec::new(),
        builtin_sources_only: false,
    }
}
