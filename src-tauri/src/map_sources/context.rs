use crate::models::AppConfig;
use crate::models::MapRepositoryConfig;
use crate::models::Provider;

/// 地图源加载期间的外部依赖：缓存、HTTP 客户端、配置、令牌、刷新批次。
///
/// 源树遍历、Release 资产文档校验和条目刷新共用这些借用依赖；分页列表与
/// 普通列表用不同缓存键，同键在一次批次中收敛。不另建客户端或全局缓存。
pub struct MapSourceContext<'a> {
    pub cache: &'a crate::release_cache::RepoReleaseCache,
    pub client: &'a reqwest::Client,
    pub config: &'a AppConfig,
    pub github_token: Option<&'a str>,
    pub gitee_token: Option<&'a str>,
    /// 一次用户动作的批次身份：同一批次里同一个仓库只请求一次 Release API。
    pub pass_id: Option<&'a str>,
    /// 上一次加载出来的条目。
    ///
    /// 只给「需要联网才能变成列表」的源兜底：那些源取不到 API 时不能整块
    /// 消失（用户会以为作者下架了），沿用旧条目比清空诚实。文档子源不用它；
    /// 子源失败跳过，根索引失败的回退由 IPC 编排决定。
    pub previous: &'a [MapRepositoryConfig],
}

impl MapSourceContext<'_> {
    pub(super) fn token(&self, provider: &Provider) -> Option<&str> {
        match provider {
            Provider::Github => self.github_token,
            Provider::Gitee => self.gitee_token,
        }
    }

    pub(super) fn api_proxies(&self, provider: &Provider) -> &[crate::models::GitHubProxy] {
        match provider {
            Provider::Github => &self.config.github_proxies.github_api_proxies,
            Provider::Gitee => &self.config.github_proxies.gitee_api_proxies,
        }
    }

    /// 上一次成功生成、属于这个源的条目。
    pub(super) fn previous_for(&self, source_id: &str) -> Vec<MapRepositoryConfig> {
        self.previous
            .iter()
            .filter(|map| map.source_id.as_deref() == Some(source_id))
            .cloned()
            .collect()
    }
}
