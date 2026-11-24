use crate::app_update::distribution;
use crate::models::AppDistribution;
use std::sync::OnceLock;

static USER_AGENT: OnceLock<String> = OnceLock::new();

pub(crate) fn app_user_agent() -> &'static str {
    USER_AGENT.get_or_init(|| {
        format_user_agent(
            std::env::consts::OS,
            std::env::consts::ARCH,
            distribution::current().ok(),
        )
    })
}

fn format_user_agent(os: &str, arch: &str, distribution: Option<AppDistribution>) -> String {
    let os = match os {
        "windows" => "Windows",
        "macos" => "macOS",
        _ => os,
    };
    let arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => arch,
    };
    let channel = match distribution {
        Some(AppDistribution::MicrosoftStore) => "Store",
        Some(AppDistribution::Standalone) => "Standalone",
        None => "Unknown",
    };
    let version = env!("CARGO_PKG_VERSION");
    format!(
        "Overcooked 2 MOD Manager/{version} (client=desktop; os={os}; arch={arch}; tauri=2; channel={channel})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_has_the_same_shape_on_each_platform_and_channel() {
        for (os, arch, distribution, expected) in [
            (
                "windows",
                "x86_64",
                Some(AppDistribution::Standalone),
                "os=Windows; arch=x64; tauri=2; channel=Standalone",
            ),
            (
                "windows",
                "x86_64",
                Some(AppDistribution::MicrosoftStore),
                "os=Windows; arch=x64; tauri=2; channel=Store",
            ),
            (
                "windows",
                "aarch64",
                Some(AppDistribution::MicrosoftStore),
                "os=Windows; arch=arm64; tauri=2; channel=Store",
            ),
            (
                "macos",
                "x86_64",
                Some(AppDistribution::Standalone),
                "os=macOS; arch=x64; tauri=2; channel=Standalone",
            ),
            (
                "macos",
                "aarch64",
                Some(AppDistribution::Standalone),
                "os=macOS; arch=arm64; tauri=2; channel=Standalone",
            ),
            (
                "windows",
                "x86_64",
                None,
                "os=Windows; arch=x64; tauri=2; channel=Unknown",
            ),
        ] {
            assert_eq!(
                format_user_agent(os, arch, distribution),
                format!(
                    "Overcooked 2 MOD Manager/{} (client=desktop; {expected})",
                    env!("CARGO_PKG_VERSION")
                )
            );
        }
    }

    #[test]
    fn process_user_agent_uses_the_detected_distribution_and_target() {
        assert_eq!(
            app_user_agent(),
            format_user_agent(
                std::env::consts::OS,
                std::env::consts::ARCH,
                distribution::current().ok()
            )
        );
        assert!(std::ptr::eq(app_user_agent(), app_user_agent()));
    }
}
