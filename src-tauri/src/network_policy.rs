//! Trusted network configuration. Parse the entire registry/plan closure before
//! publishing it; source documents may reference plans but cannot define proxies.
mod validation;

use crate::models::{GitHubProxy, Provider};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Intent {
    Api,
    Release,
    Raw,
    Avatar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Access {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenTransport {
    None,
    MatchingPlatformToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Proxy {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    pub intent: Intent,
    pub access: Access,
    pub template: String,
    pub token_transport: TokenTransport,
    #[serde(default)]
    pub description: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub regions: Vec<String>,
}

fn enabled() -> bool {
    true
}

impl Proxy {
    pub(crate) fn legacy_entry(&self) -> GitHubProxy {
        GitHubProxy {
            id: self.id.clone(),
            name: self.name.clone(),
            template: self.template.clone(),
            regions: self.regions.clone(),
            description: self.description.clone(),
            enabled: self.enabled,
            builtin: true,
            remote: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Step {
    Official,
    Direct,
    Proxy { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Route {
    pub route: Vec<Step>,
    pub max_parallelism: u32,
    /// Explicitly race this whole route group; otherwise try in listed order.
    #[serde(default)]
    pub race: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestPlan {
    pub id: String,
    pub provider: Provider,
    pub access: Access,
    pub api: Route,
    pub release: Route,
    pub raw: Route,
}

impl RequestPlan {
    pub(crate) fn route(&self, intent: Intent) -> Result<&Route, String> {
        match intent {
            Intent::Api => Ok(&self.api),
            Intent::Release => Ok(&self.release),
            Intent::Raw => Ok(&self.raw),
            Intent::Avatar => Err("proxy.validationFailed".into()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkPolicy {
    pub proxies: BTreeMap<String, Proxy>,
    pub plans: BTreeMap<String, RequestPlan>,
    proxy_order: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProxyIndex {
    schema_version: u32,
    #[serde(default)]
    proxy_sources: Vec<ProxySource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProxySource {
    id: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProxyDocument {
    schema_version: u32,
    #[serde(default)]
    proxies: Vec<Proxy>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanDocument {
    schema_version: u32,
    #[serde(default)]
    request_plans: Vec<RequestPlan>,
}

fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, String> {
    if text.len() > 1024 * 1024 {
        return Err("proxy.manifestTooLarge".into());
    }
    toml::from_str(text).map_err(|_| "proxy.validationFailed".into())
}

/// Return safe relative paths only. An index cannot redirect configuration
/// loading to another origin, a local file, or a parent directory.
pub(crate) fn proxy_documents(index: &str) -> Result<Vec<String>, String> {
    let index: ProxyIndex = parse(index)?;
    let mut ids = std::collections::HashSet::new();
    let mut paths = std::collections::HashSet::new();
    if index.schema_version != 1 || index.proxy_sources.len() > 32 {
        return Err("proxy.validationFailed".into());
    }
    index
        .proxy_sources
        .into_iter()
        .map(|source| {
            let path = &source.url;
            if !validation::valid_id(&source.id)
                || !ids.insert(source.id)
                || !path.starts_with("proxies/")
                || !path.ends_with(".toml")
                || path
                    .chars()
                    .any(|c| c.is_control() || matches!(c, ':' | '\\' | '?' | '#' | '%'))
                || path
                    .split('/')
                    .any(|part| part.is_empty() || matches!(part, "." | ".."))
                || !paths.insert(path.clone())
            {
                return Err("proxy.validationFailed".into());
            }
            Ok(source.url)
        })
        .collect()
}

impl NetworkPolicy {
    /// Both local embedding and remote refresh use this same closure parser.
    /// The caller must fetch all declared documents before invoking it.
    pub(crate) fn parse(
        index: &str,
        documents: &BTreeMap<String, String>,
        plans: &str,
    ) -> Result<Self, String> {
        let paths = proxy_documents(index)?;
        if paths.len() != documents.len() {
            return Err("proxy.validationFailed".into());
        }
        let mut snapshot = Self::default();
        for path in paths {
            let document: ProxyDocument =
                parse(documents.get(&path).ok_or("proxy.validationFailed")?)?;
            if document.schema_version != 1 || document.proxies.len() > 128 {
                return Err("proxy.validationFailed".into());
            }
            for proxy in document.proxies {
                validation::proxy(&proxy)?;
                snapshot.proxy_order.push(proxy.id.clone());
                if snapshot.proxies.insert(proxy.id.clone(), proxy).is_some() {
                    return Err("proxy.validationFailed".into());
                }
                if snapshot.proxies.len() > 128 {
                    return Err("proxy.validationFailed".into());
                }
            }
        }
        let plans: PlanDocument = parse(plans)?;
        if plans.schema_version != 1 || plans.request_plans.len() > 128 {
            return Err("proxy.validationFailed".into());
        }
        for plan in plans.request_plans {
            validation::plan(&snapshot, &plan)?;
            if snapshot.plans.insert(plan.id.clone(), plan).is_some() {
                return Err("proxy.validationFailed".into());
            }
        }
        Ok(snapshot)
    }

    pub(crate) fn plan(&self, id: &str, provider: &Provider) -> Result<&RequestPlan, String> {
        self.plans
            .get(id)
            .filter(|plan| &plan.provider == provider)
            .ok_or_else(|| "proxy.validationFailed".into())
    }

    pub(crate) fn bundled() -> Result<Self, String> {
        let index = crate::repository_sources::SourceLocation::parse("proxies.toml")
            .map_err(|_| "proxy.validationFailed")?;
        let text = index.bundled_text().map_err(|_| "proxy.validationFailed")?;
        let mut documents = BTreeMap::new();
        for path in proxy_documents(text)? {
            let location = crate::repository_sources::SourceLocation::parse(&path)
                .map_err(|_| "proxy.validationFailed")?;
            documents.insert(
                path,
                location
                    .bundled_text()
                    .map_err(|_| "proxy.validationFailed")?
                    .into(),
            );
        }
        let plans = crate::repository_sources::SourceLocation::parse("network-plans.toml")
            .map_err(|_| "proxy.validationFailed")?;
        Self::parse(
            text,
            &documents,
            plans.bundled_text().map_err(|_| "proxy.validationFailed")?,
        )
    }

    /// Transitional projection for existing consumers. It only selects proxy
    /// groups; API credential grants and source-specific routes are not implied
    /// by this view. The typed closure is validated before any group is exposed.
    pub(crate) fn bundled_proxy_groups(&self) -> Result<crate::models::GitHubProxyConfig, String> {
        let entries =
            |id: &str, provider: &Provider, intent: Intent| -> Result<Vec<GitHubProxy>, String> {
                self.plan(id, provider)?
                    .route(intent)?
                    .route
                    .iter()
                    .filter_map(|step| match step {
                        Step::Proxy { id } => Some(
                            self.proxies
                                .get(id)
                                .map(Proxy::legacy_entry)
                                .ok_or_else(|| "proxy.validationFailed".to_string()),
                        ),
                        _ => None,
                    })
                    .collect()
            };
        Ok(crate::models::GitHubProxyConfig {
            release_proxies: entries("github-mainland-mod", &Provider::Github, Intent::Release)?,
            raw_proxies: entries("github-mainland-mod", &Provider::Github, Intent::Raw)?,
            github_api_proxies: entries("github-public-mod", &Provider::Github, Intent::Api)?,
            gitee_api_proxies: entries("gitee-public-mod", &Provider::Gitee, Intent::Api)?,
            gitcode_api_proxies: entries("gitcode-public-mod", &Provider::Gitcode, Intent::Api)?,
            avatar_proxies: self
                .proxy_order
                .iter()
                .filter_map(|id| self.proxies.get(id))
                .filter(|proxy| {
                    proxy.provider == Provider::Github
                        && proxy.intent == Intent::Avatar
                        && proxy.access == Access::Public
                })
                .map(Proxy::legacy_entry)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
