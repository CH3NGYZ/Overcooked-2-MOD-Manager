use crate::models::Provider;
use crate::releases::ReleaseError;
use reqwest::{Client, RequestBuilder};
use url::Url;

fn is_official_gitee_api_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.host_str(), Some("gitee.com" | "www.gitee.com"))
        && url.path().starts_with("/api/v5/")
}

fn api_url_with_query_token(url: &str, token: &str) -> Result<String, ReleaseError> {
    let mut parsed =
        Url::parse(url).map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    parsed.query_pairs_mut().append_pair("access_token", token);
    Ok(parsed.into())
}

/// API authentication only. File requests never pass through this capability.
pub(crate) fn api_request(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<RequestBuilder, ReleaseError> {
    let request_url = match (provider, token) {
        (Provider::Gitcode, Some(token))
            if Url::parse(url).ok().is_some_and(|url| {
                url.host_str() == Some("api.atomgit.com") && url.path().starts_with("/api/v5/")
            }) =>
        {
            api_url_with_query_token(url, token)?
        }
        (Provider::Gitee, Some(token)) if is_official_gitee_api_url(url) => {
            api_url_with_query_token(url, token)?
        }
        _ => url.to_owned(),
    };
    let mut request = client.get(request_url).header(
        reqwest::header::ACCEPT,
        if *provider == Provider::Github {
            "application/vnd.github+json"
        } else {
            "application/json"
        },
    );
    if let Some(token) = token {
        if *provider == Provider::Github {
            request = request.bearer_auth(token);
        }
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitee_token_uses_access_token_query_on_official_api_only() {
        // Gitee 的 access_token 走 query 参数（官方约定），且只附加在官方地址上
        let official = "https://gitee.com/api/v5/repos/a/b/releases?per_page=10";
        assert_eq!(
            api_url_with_query_token(official, "secret token").unwrap(),
            "https://gitee.com/api/v5/repos/a/b/releases?per_page=10&access_token=secret+token"
        );
        // 代理地址不识别为官方接口，因此不会附加凭据
        assert!(!is_official_gitee_api_url(
            "https://oc2gtapi.ch3ng.top/api/v5/repos/a/b/releases"
        ));
    }
    #[test]
    fn official_query_auth_is_encoded_and_other_hosts_do_not_receive_it() {
        assert_eq!(
            api_url_with_query_token(
                "https://gitee.com/api/v5/repos/a/b/releases",
                "secret token"
            )
            .unwrap(),
            "https://gitee.com/api/v5/repos/a/b/releases?access_token=secret+token"
        );
        assert!(!is_official_gitee_api_url(
            "https://gitee.com.evil.example/api/v5/repos/a/b/releases"
        ));
        let client = Client::new();
        let proxy = api_request(
            &client,
            "https://proxy.example/api",
            &Provider::Gitee,
            Some("secret"),
        )
        .unwrap()
        .build()
        .unwrap();
        assert!(proxy.url().query().is_none());
        assert!(!proxy.headers().contains_key(reqwest::header::AUTHORIZATION));
    }
}

#[cfg(test)]
mod gitcode_tests {
    use super::*;
    #[test]
    fn gitcode_query_token_is_confined_to_official_api() {
        let client = Client::new();
        for (url, allowed) in [
            ("https://api.atomgit.com/api/v5/repos/a/b/releases", true),
            (
                "https://api.atomgit.com.evil.example/api/v5/repos/a/b/releases",
                false,
            ),
            ("https://raw.atomgit.com/a/b/raw/main/file", false),
            ("https://proxy.example/api/v5/repos/a/b/releases", false),
        ] {
            let request = api_request(&client, url, &Provider::Gitcode, Some("fixture token"))
                .unwrap()
                .build()
                .unwrap();
            assert_eq!(
                request
                    .url()
                    .query_pairs()
                    .any(|(key, value)| key == "access_token" && value == "fixture token"),
                allowed
            );
            assert!(
                !request
                    .headers()
                    .contains_key(reqwest::header::AUTHORIZATION)
            );
        }
    }
}
