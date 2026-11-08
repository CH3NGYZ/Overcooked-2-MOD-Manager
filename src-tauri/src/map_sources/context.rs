use crate::models::AppConfig;
use crate::models::MapRepositoryConfig;
use crate::models::Provider;

/// 地图源加载期间的外部依赖：缓存、HTTP 客户端、配置、令牌、刷新批次。
///
/// 源树遍历、Release 资产文档校验和条目刷新共用这些借用依赖；统一完整分页，
/// 同仓库共用缓存键并在一次批次中收敛。不另建客户端或全局缓存。
pub struct MapSourceContext<'a> {
    pub observer: Option<&'a super::progress::SourceObserver>,
    pub cache: &'a crate::release_cache::RepoReleaseCache,
    pub client: &'a reqwest::Client,
    pub config: &'a AppConfig,
    pub github_token: Option<&'a str>,
    pub gitcode_token: Option<&'a str>,
    pub gitee_token: Option<&'a str>,
    /// 一次用户动作的批次身份：同一批次里同一个仓库只请求一次 Release API。
    pub pass_id: Option<&'a str>,
    /// Only full refresh bypasses TTL; same-pass readers still share one snapshot.
    pub force: bool,
    /// 上次声明快照：局部刷新保留其他分支，Release 派生源据此识别已安装优先级。
    /// 失败源不恢复旧卡片，错误保留在源节点供用户重试。
    pub previous: &'a [MapRepositoryConfig],
}

impl MapSourceContext<'_> {
    pub(super) fn token(&self, provider: &Provider) -> Option<&str> {
        match provider {
            Provider::Github => self.github_token,
            Provider::Gitee => self.gitee_token,
            Provider::Gitcode => self.gitcode_token,
        }
    }

    pub(super) fn api_proxies(&self, provider: &Provider) -> &[crate::models::GitHubProxy] {
        crate::providers::api_proxies(self.config, provider)
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
