use serde::{Deserialize, Serialize};

/// Official data entry points. The running session owns the selection; it is
/// initialized from the computer's region once, rather than restored from disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataProvider {
    #[default]
    Github,
    Gitee,
    Builtin,
}

impl DataProvider {
    pub(crate) fn for_region(region: &str) -> Self {
        if crate::config::region_matches(Some(crate::config::REGION_CHINESE_MAINLAND), region) {
            Self::Gitee
        } else {
            Self::Github
        }
    }

    /// All four official data kinds use the same repository and relative paths.
    pub(crate) fn document(self, path: &str) -> String {
        let root = match self {
            Self::Github => {
                "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/"
            }
            Self::Gitee => "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/",
            Self::Builtin => "",
        };
        format!("{root}{path}")
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
        }
    }

    #[test]
    fn startup_selection_uses_computer_region_and_wire_values_are_closed() {
        assert_eq!(DataProvider::for_region("cn"), DataProvider::Gitee);
        for region in ["global", "", "unknown"] {
            assert_eq!(DataProvider::for_region(region), DataProvider::Github);
        }
        for (value, provider) in [
            ("github", DataProvider::Github),
            ("gitee", DataProvider::Gitee),
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
}
