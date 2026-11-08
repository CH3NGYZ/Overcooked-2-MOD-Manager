//! Hosting platform infrastructure. Business domains consume validated coordinates
//! and capabilities rather than inferring a platform from an unrelated default.
mod authentication;
mod coordinates;
pub(crate) use authentication::api_request;
mod gitcode;
mod gitee;
mod github;

pub(crate) use coordinates::{
    ReleaseAssetReference, release_asset_reference, repository_cache_key, repository_coordinates,
    repository_file_name, repository_file_url, repository_release_api_url,
    repository_release_api_url_page,
};

pub(crate) struct Infrastructure {
    pub homepage: &'static str,
    pub repository_api: &'static str,
    pub hosts: &'static [&'static str],
}

pub(crate) fn infrastructure(provider: &crate::models::Provider) -> &'static Infrastructure {
    match provider {
        crate::models::Provider::Github => &github::INFRASTRUCTURE,
        crate::models::Provider::Gitee => &gitee::INFRASTRUCTURE,
        crate::models::Provider::Gitcode => &gitcode::INFRASTRUCTURE,
    }
}

pub(crate) fn provider_for_host(host: &str) -> Option<crate::models::Provider> {
    use crate::models::Provider;
    [Provider::Github, Provider::Gitee, Provider::Gitcode]
        .into_iter()
        .find(|provider| {
            infrastructure(provider)
                .hosts
                .iter()
                .any(|known| host.eq_ignore_ascii_case(known))
        })
}

pub(crate) fn api_proxies<'a>(
    config: &'a crate::models::AppConfig,
    provider: &crate::models::Provider,
) -> &'a [crate::models::GitHubProxy] {
    match provider {
        crate::models::Provider::Github => &config.github_proxies.github_api_proxies,
        crate::models::Provider::Gitee => &config.github_proxies.gitee_api_proxies,
        crate::models::Provider::Gitcode => &config.github_proxies.gitcode_api_proxies,
    }
}

pub(crate) use gitcode::is_generated_asset;
