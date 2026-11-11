//! 公开配置投影；隐藏密钥并叠加当前代理快照。

use super::ConfigService;
use super::availability::current_user_region;
use crate::models::*;

impl ConfigService {
    pub fn public_config(&self, config: &AppConfig) -> PublicConfig {
        let config = self.with_runtime(config.clone());
        let (community_discovery, gitee_community_discovery, gitcode_community_discovery) =
            crate::community::visible_discoveries(&config);
        let mut repositories = config.repositories.clone();
        for repo in &mut repositories {
            repo.github_token.clear();
            repo.gitee_token.clear();
        }
        PublicConfig {
            schema_version: config.schema_version,
            platform: std::env::consts::OS.into(),
            region: current_user_region().into(),
            ui_zoom: config.ui_zoom,
            hide_unidentified_mods: config.hide_unidentified_mods,
            recycle_deleted_mods: config.recycle_deleted_mods,
            mod_metadata_concurrency: config.mod_metadata_concurrency,
            map_info_concurrency: config.map_info_concurrency,
            map_compression_concurrency: config.map_compression_concurrency,
            diychef_preview_concurrency: config.diychef_preview_concurrency,
            tutorial_watched_steps: config.tutorial_watched_steps.clone(),
            game: config.game.clone(),
            network: config.network.clone(),
            launch: config.launch.clone(),
            official_api_only: config.auth.official_api_only,
            github_token_set: !config.auth.github_token.is_empty(),
            gitcode_token_set: self.api_token(&config, &Provider::Gitcode).is_some(),
            gitee_token_set: !config.auth.gitee_token.is_empty(),
            github_publish_token_set: !config.auth.github_publish_token.is_empty(),
            github_publish_login: config.auth.github_publish_login.clone(),
            repositories,
            repository_sources: config.repository_sources.clone(),
            community_discovery,
            gitee_community_discovery,
            gitcode_community_discovery,
            github_proxies: config.github_proxies,
            data_provider: config.data_provider,
        }
    }
}
