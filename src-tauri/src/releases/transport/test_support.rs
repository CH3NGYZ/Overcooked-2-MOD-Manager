use super::*;

pub async fn fetch_release_api(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let value = fetch_api_json(client, url, provider, token).await?;
    parse_release_json(&value, provider)
}
