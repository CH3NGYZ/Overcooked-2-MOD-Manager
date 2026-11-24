use serde::{Deserialize, Serialize};

/// Official data entry points. The running session owns the selection; it is
/// initialized from the saved user region, rather than restored from disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataProvider {
    #[default]
    Github,
    Gitee,
    Gitcode,
    Cloudflare,
    #[serde(rename = "edgeone")]
    EdgeOne,
    Builtin,
}

impl DataProvider {
    pub(crate) const ALL: [Self; 6] = [
        Self::Github,
        Self::Gitee,
        Self::Gitcode,
        Self::Cloudflare,
        Self::EdgeOne,
        Self::Builtin,
    ];

    pub(crate) fn for_region(region: &str) -> Self {
        if crate::config::region_matches(Some(crate::config::REGION_CHINESE_MAINLAND), region) {
            Self::Gitcode
        } else {
            Self::Github
        }
    }

    /// Stable document identities; official files are read from one source
    /// archive, never fetched individually from these display/base URLs.
    pub(crate) fn document(self, path: &str) -> String {
        let root = match self {
            Self::Github => {
                "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/"
            }
            Self::Gitee => "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/",
            Self::Gitcode => {
                "https://raw.gitcode.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/"
            }
            Self::Cloudflare => {
                "https://oc2gh.ch3ng.top/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/"
            }
            Self::EdgeOne => {
                "https://oc2gh.5882778.xyz/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/"
            }
            Self::Builtin => "",
        };
        format!("{root}{path}")
    }

    pub(crate) fn archive_url(self) -> Option<&'static str> {
        match self {
            Self::Github => Some(
                "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
            Self::Gitee => Some(
                "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/repository/archive/main.zip",
            ),
            Self::Gitcode => Some(
                "https://raw.gitcode.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
            Self::Cloudflare => Some(
                "https://oc2gh.ch3ng.top/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
            Self::EdgeOne => Some(
                "https://oc2gh.5882778.xyz/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
            Self::Builtin => None,
        }
    }

    pub(crate) fn official_document_path(address: &str) -> Option<String> {
        let url = url::Url::parse(address).ok()?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return None;
        }
        let mut plain = url;
        plain.set_query(None);
        plain.set_fragment(None);
        let address = plain.as_str();
        let github_raw =
            "https://raw.githubusercontent.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/main/";
        if address
            .get(..github_raw.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(github_raw))
        {
            return Some(address[github_raw.len()..].to_owned());
        }
        for provider in Self::ALL
            .into_iter()
            .filter(|entry| *entry != Self::Builtin)
        {
            let root = provider.document("");
            if address
                .get(..root.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&root))
            {
                return Some(address[root.len()..].to_owned());
            }
        }
        None
    }

    pub(crate) fn fallback(self) -> Option<Self> {
        match self {
            Self::Gitcode => Some(Self::Gitee),
            Self::Github | Self::Gitee => Some(Self::Cloudflare),
            Self::Cloudflare => Some(Self::EdgeOne),
            Self::EdgeOne => Some(Self::Builtin),
            Self::Builtin => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_official_document_uses_the_same_selected_provider() {
        for path in [
            "mods.toml",
            "maps.toml",
            "proxies.toml",
            "network-plans.toml",
            "proxies/api.toml",
        ] {
            assert_eq!(
                DataProvider::Github.document(path),
                format!(
                    "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/{path}"
                )
            );
            assert_eq!(
                DataProvider::Gitee.document(path),
                format!("https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/{path}")
            );
            assert_eq!(DataProvider::Builtin.document(path), path);
            assert_eq!(
                DataProvider::Gitcode.document(path),
                format!(
                    "https://raw.gitcode.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/{path}"
                )
            );
            assert_eq!(
                DataProvider::Cloudflare.document(path),
                format!(
                    "https://oc2gh.ch3ng.top/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/{path}"
                )
            );
            assert_eq!(
                DataProvider::EdgeOne.document(path),
                format!(
                    "https://oc2gh.5882778.xyz/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/{path}"
                )
            );
        }
    }

    #[test]
    fn startup_selection_uses_computer_region_and_wire_values_are_closed() {
        assert_eq!(DataProvider::for_region("cn"), DataProvider::Gitcode);
        for region in ["global", "", "unknown"] {
            assert_eq!(DataProvider::for_region(region), DataProvider::Github);
        }
        for (value, provider) in [
            ("github", DataProvider::Github),
            ("gitee", DataProvider::Gitee),
            ("gitcode", DataProvider::Gitcode),
            ("cloudflare", DataProvider::Cloudflare),
            ("edgeone", DataProvider::EdgeOne),
            ("builtin", DataProvider::Builtin),
        ] {
            assert_eq!(serde_json::to_value(provider).unwrap(), value);
            assert_eq!(
                serde_json::from_value::<DataProvider>(value.into()).unwrap(),
                provider
            );
        }
        assert!(serde_json::from_value::<DataProvider>("custom-url".into()).is_err());
    }

    #[test]
    fn cloudflare_entry_matches_the_existing_oc2gh_raw_proxy_template() {
        let groups = crate::github_proxies::bundled_defaults();
        let routes = crate::github_proxies::proxy_urls(
            &groups.raw_proxies,
            &DataProvider::Github.document("mods.toml"),
            true,
            None,
        );
        assert!(routes.contains(&DataProvider::Cloudflare.document("mods.toml")));
    }

    #[test]
    fn official_archives_use_exact_download_addresses() {
        for (provider, expected) in [
            (
                DataProvider::Github,
                "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
            (
                DataProvider::Gitee,
                "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/repository/archive/main.zip",
            ),
            (
                DataProvider::Gitcode,
                "https://raw.gitcode.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
            (
                DataProvider::Cloudflare,
                "https://oc2gh.ch3ng.top/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
            (
                DataProvider::EdgeOne,
                "https://oc2gh.5882778.xyz/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/archive/refs/heads/main.zip",
            ),
        ] {
            assert_eq!(provider.archive_url(), Some(expected), "{provider:?}");
        }
        assert_eq!(DataProvider::Builtin.archive_url(), None);
    }

    #[test]
    fn regional_archive_chains_only_visit_the_authorized_providers() {
        for (region, expected) in [
            (
                "cn",
                vec![
                    DataProvider::Gitcode,
                    DataProvider::Gitee,
                    DataProvider::Cloudflare,
                    DataProvider::EdgeOne,
                    DataProvider::Builtin,
                ],
            ),
            (
                "global",
                vec![
                    DataProvider::Github,
                    DataProvider::Cloudflare,
                    DataProvider::EdgeOne,
                    DataProvider::Builtin,
                ],
            ),
        ] {
            let mut actual = Vec::new();
            let mut provider = Some(DataProvider::for_region(region));
            while let Some(current) = provider {
                actual.push(current);
                provider = current.fallback();
            }
            assert_eq!(actual, expected);
            for provider in actual {
                assert_eq!(
                    provider.archive_url().is_some(),
                    provider != DataProvider::Builtin
                );
            }
        }
    }

    #[test]
    fn official_document_addresses_resolve_to_one_package_path() {
        for provider in [
            DataProvider::Github,
            DataProvider::Gitee,
            DataProvider::Gitcode,
            DataProvider::Cloudflare,
            DataProvider::EdgeOne,
        ] {
            assert_eq!(
                DataProvider::official_document_path(
                    &provider.document("mods/with%20space.toml?ref=ignored#fragment")
                ),
                Some("mods/with%20space.toml".into())
            );
        }
        assert_eq!(
            DataProvider::official_document_path(
                "https://raw.githubusercontent.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/main/maps.toml"
            ),
            Some("maps.toml".into())
        );
        for address in [
            "https://github.com/CH3NGYZ/other/raw/refs/heads/main/mods.toml",
            "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main-extra/mods.toml",
            "https://user@gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/mods.toml",
            "https://raw.gitcode.com.attacker.test/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/mods.toml",
            "https://oc2gh.5882778.xyz/CH3NGYZ/other/raw/refs/heads/main/mods.toml",
            "https://oc2gh.5882778.xyz/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/dev/mods.toml",
            "http://oc2gh.5882778.xyz/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/mods.toml",
            "http://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/mods.toml",
            "mods.toml",
        ] {
            assert!(
                DataProvider::official_document_path(address).is_none(),
                "{address}"
            );
        }
    }
}
