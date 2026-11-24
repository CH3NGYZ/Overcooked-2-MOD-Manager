use super::*;

pub async fn fetch_release_api(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let target = RepositoryApiTarget::new(url, url, provider, token, None);
    let value = fetch_api_json(client, &target).await?;
    parse_release_json(&value, provider)
}
