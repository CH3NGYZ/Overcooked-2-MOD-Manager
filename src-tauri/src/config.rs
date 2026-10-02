use crate::models::*;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chrono::Utc;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const CONFIG_DIR: &str = "OC2ModManager";
const CONFIG_FILE: &str = "config.toml";
const BUNDLE_FORMAT: &str = "oc2-mod-repository-bundle";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("配置目录不可用: {0}")]
    Directory(String),
    #[error("配置读取失败: {0}")]
    Read(String),
    #[error("配置解析失败: {0}")]
    Parse(String),
    #[error("配置写入失败: {0}")]
    Write(String),
    #[error("配置格式无效: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug)]
pub struct ConfigService {
    root: PathBuf,
}

impl ConfigService {
    pub fn new() -> Result<Self, ConfigError> {
        let root = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::temp_dir().join(CONFIG_DIR));
        fs::create_dir_all(&root).map_err(|e| ConfigError::Directory(e.to_string()))?;
        Ok(Self { root })
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn manifests_dir(&self) -> PathBuf {
        self.root.join("manifests")
    }

    pub fn backups_dir(&self) -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| self.root.clone())
            .join("backups")
    }

    pub fn load(&self) -> Result<AppConfig, ConfigError> {
        let path = self.config_path();
        let previous = self.root.join("config.previous.toml");
        let source = if path.exists() {
            path
        } else if previous.exists() {
            previous
        } else {
            let config = default_config();
            self.save(&config)?;
            return Ok(config);
        };
        let text = fs::read_to_string(&source).map_err(|e| ConfigError::Read(e.to_string()))?;
        match toml::from_str::<AppConfig>(&text) {
            Ok(config) => Ok(config),
            Err(error) => {
                let backup = self.root.join(format!(
                    "config.corrupt-{}.toml",
                    Utc::now().format("%Y%m%d%H%M%S")
                ));
                let _ = fs::rename(&source, backup);
                let config = default_config();
                self.save(&config)?;
                Err(ConfigError::Parse(error.to_string()))
            }
        }
    }

    pub fn load_or_default(&self) -> AppConfig {
        self.load().unwrap_or_else(|_| default_config())
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
        fs::create_dir_all(&self.root).map_err(|e| ConfigError::Directory(e.to_string()))?;
        let text = toml::to_string_pretty(config).map_err(|e| ConfigError::Write(e.to_string()))?;
        let temp = self.root.join(format!("config.{}.tmp", Uuid::new_v4()));
        fs::write(&temp, text).map_err(|e| ConfigError::Write(e.to_string()))?;
        let target = self.config_path();
        let previous = self.root.join("config.previous.toml");
        if target.exists() {
            let _ = fs::remove_file(&previous);
            fs::rename(&target, &previous).map_err(|e| ConfigError::Write(e.to_string()))?;
        }
        if let Err(error) = fs::rename(&temp, &target) {
            let _ = fs::remove_file(&temp);
            if previous.exists() {
                let _ = fs::rename(&previous, &target);
            }
            return Err(ConfigError::Write(error.to_string()));
        }
        let _ = fs::remove_file(previous);
        Ok(())
    }

    pub fn public_config(&self, config: &AppConfig) -> PublicConfig {
        let mut repositories = config.repositories.clone();
        for repo in &mut repositories {
            repo.github_token.clear();
            repo.gitee_token.clear();
        }
        PublicConfig {
            schema_version: config.schema_version,
            game: config.game.clone(),
            network: config.network.clone(),
            launch: config.launch.clone(),
            official_api_only: config.auth.official_api_only,
            github_token_set: !config.auth.github_token.is_empty(),
            gitee_token_set: !config.auth.gitee_token.is_empty(),
            repositories,
        }
    }

    pub fn set_tokens(&self, config: &mut AppConfig, github: &str, gitee: &str) {
        if !github.is_empty() {
            config.auth.github_token = protect_secret(github).unwrap_or_default();
        }
        if !gitee.is_empty() {
            config.auth.gitee_token = protect_secret(gitee).unwrap_or_default();
        }
    }

    pub fn set_repository_tokens(&self, repo: &mut RepositoryConfig, github: &str, gitee: &str) {
        if !github.is_empty() { repo.github_token = protect_secret(github).unwrap_or_default(); }
        if !gitee.is_empty() { repo.gitee_token = protect_secret(gitee).unwrap_or_default(); }
    }

    pub fn repository_token(&self, repo: &RepositoryConfig) -> Option<String> {
        match repo.provider {
            Provider::Github => reveal_secret(&repo.github_token),
            Provider::Gitee => reveal_secret(&repo.gitee_token),
        }
    }

    pub fn set_mod_note(&self, config: &mut AppConfig, key: &str, note: &str) {
        config.mod_notes.retain(|item| item.key != key);
        let note = note.trim();
        if !note.is_empty() {
            config.mod_notes.push(ModNote {
                key: key.to_string(),
                note: note.to_string(),
            });
        }
    }

    pub fn github_token(&self, config: &AppConfig) -> Option<String> {
        reveal_secret(&config.auth.github_token)
    }

    pub fn gitee_token(&self, config: &AppConfig) -> Option<String> {
        reveal_secret(&config.auth.gitee_token)
    }

    pub fn export_bundle(
        &self,
        config: &AppConfig,
        ids: Option<Vec<String>>,
        include_builtin: bool,
    ) -> Result<String, ConfigError> {
        let selected = config
            .repositories
            .iter()
            .filter(|repo| include_builtin || !repo.builtin)
            .filter(|repo| {
                ids.as_ref()
                    .map(|list| list.contains(&repo.id))
                    .unwrap_or(true)
            })
            .map(|repo| RepositoryExport {
                name: repo.name.clone(),
                provider: repo.provider.clone(),
                url: repo.url.clone(),
                channel: repo.channel.clone(),
                asset_rules: repo.asset_rules.clone(),
                install_target: repo.install_target.clone(),
                plugin_guid: repo.plugin_guid.clone(),
                download_threads: repo.download_threads,
                api_proxy_mode: repo.api_proxy_mode.clone(),
                api_proxy_url: repo.api_proxy_url.clone(),
                release_proxy_mode: repo.release_proxy_mode.clone(),
                release_proxy_url: repo.release_proxy_url.clone(),
                enabled: repo.enabled,
            })
            .collect::<Vec<_>>();
        let bundle = RepositoryBundle {
            format: BUNDLE_FORMAT.to_string(),
            schema_version: 1,
            exported_at: Utc::now().to_rfc3339(),
            repositories: selected,
        };
        toml::to_string_pretty(&bundle).map_err(|e| ConfigError::Write(e.to_string()))
    }

    pub fn validate_bundle(&self, text: &str) -> Result<RepositoryBundle, ConfigError> {
        if text.len() > 1024 * 1024 {
            return Err(ConfigError::Invalid("导入文件超过 1 MiB".into()));
        }
        let bundle: RepositoryBundle = toml::from_str(text)
            .map_err(|e| ConfigError::Invalid(format!("TOML 解析失败: {e}")))?;
        if bundle.format != BUNDLE_FORMAT {
            return Err(ConfigError::Invalid(
                "format 不是 oc2-mod-repository-bundle".into(),
            ));
        }
        if bundle.schema_version != 1 {
            return Err(ConfigError::Invalid(format!(
                "不支持 schema_version {}",
                bundle.schema_version
            )));
        }
        for repo in &bundle.repositories {
            validate_repository(repo)?;
        }
        Ok(bundle)
    }

    pub fn preview_bundle(&self, config: &AppConfig, bundle: &RepositoryBundle) -> ImportPreview {
        let mut preview = ImportPreview {
            valid: true,
            errors: Vec::new(),
            additions: Vec::new(),
            duplicates: Vec::new(),
            conflicts: Vec::new(),
        };
        for item in &bundle.repositories {
            let key = export_key(item);
            let matching = config
                .repositories
                .iter()
                .find(|repo| repo_key(repo) == key);
            if matching.is_some_and(|repo| repo.builtin) {
                preview.duplicates.push(item.name.clone());
                continue;
            }
            let same_name = config
                .repositories
                .iter()
                .any(|repo| repo.name == item.name);
            if matching.is_some() {
                preview.duplicates.push(item.name.clone());
            } else if same_name {
                preview.conflicts.push(item.name.clone());
            } else {
                preview.additions.push(item.name.clone());
            }
        }
        preview
    }

    pub fn import_bundle(
        &self,
        config: &mut AppConfig,
        bundle: &RepositoryBundle,
        conflict: &str,
    ) -> ImportPreview {
        let mut preview = self.preview_bundle(config, bundle);
        for item in &bundle.repositories {
            let key = export_key(item);
            if let Some(existing) = config
                .repositories
                .iter_mut()
                .find(|repo| repo_key(repo) == key)
            {
                if existing.builtin {
                    continue;
                }
                if conflict == "overwrite" {
                    let id = existing.id.clone();
                    *existing = repository_from_export(item, id, false);
                }
                continue;
            }
            let mut name = item.name.clone();
            if config.repositories.iter().any(|repo| repo.name == name) {
                match conflict {
                    "skip" => continue,
                    "keep_both" => {
                        name = format!("{} (导入)", name);
                    }
                    "overwrite" => {
                        config.repositories.retain(|repo| repo.name != name);
                    }
                    _ => continue,
                }
            }
            let mut export = item.clone();
            export.name = name;
            config.repositories.push(repository_from_export(
                &export,
                Uuid::new_v4().to_string(),
                false,
            ));
        }
        preview.valid = true;
        preview
    }
}

fn default_config() -> AppConfig {
    AppConfig {
        schema_version: 1,
        game: GameConfig {
            path: String::new(),
            auto_detect: true,
        },
        network: NetworkConfig {
            timeout_seconds: 20,
            download_threads: 32,
            max_retry_count: 3,
            check_updates_on_start: true,
            max_release_count: 10,
        },
        launch: LaunchConfig {
            ask_user_to_start_from_steam: true,
            auto_restore_winhttp: true,
        },
        auth: AuthConfig {
            github_token: String::new(),
            gitee_token: String::new(),
            official_api_only: true,
        },
        repositories: builtin_repositories(),
        mod_notes: Vec::new(),
    }
}

fn builtin_repositories() -> Vec<RepositoryConfig> {
    [
        (
            "hostutilities-stable",
            "HostUtilities 正式版",
            Provider::Gitee,
            "https://gitee.com/ch3ngyz/Overcooked-2-HostUtilities-Stable-Releases/releases",
            Channel::All,
        ),
        (
            "hostutilities-beta",
            "HostUtilities 测试版",
            Provider::Gitee,
            "https://gitee.com/ch3ngyz/Overcooked-2-HostUtilities-Beta-Releases/releases",
            // Gitee marks this beta repository's releases as non-prerelease.
            Channel::All,
        ),
    ]
    .into_iter()
    .map(|(id, name, provider, url, channel)| RepositoryConfig {
        id: id.into(),
        name: name.into(),
        provider: provider.clone(),
        url: url.into(),
        channel,
        asset_rules: vec![AssetRule {
            rule_type: AssetRuleType::Exact,
            value: "HostUtilities.zip".into(),
        }],
        install_target: "BepInEx/plugins".into(),
        plugin_guid: "com.ch3ngyz.plugin.HostUtilities".into(),
        download_threads: 1,
        api_proxy_mode: ProxyMode::Auto,
        api_proxy_url: "https://oc2gtapi.ch3ng.top".into(),
        release_proxy_mode: ProxyMode::Direct,
        release_proxy_url: String::new(),
        github_token: String::new(),
        gitee_token: String::new(),
        official_api_only: true,
        enabled: true,
        builtin: true,
        pinned_version: None,
        ignored_version: None,
        etag: None,
        cached_releases: Vec::new(),
        last_error: None,
    })
    .collect()
}

fn validate_repository(repo: &RepositoryExport) -> Result<(), ConfigError> {
    let parsed = url::Url::parse(&repo.url).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err(ConfigError::Invalid(format!(
            "{} 的 URL 协议无效",
            repo.name
        )));
    }
    for proxy_url in [&repo.api_proxy_url, &repo.release_proxy_url] {
        if proxy_url.trim().is_empty() {
            continue;
        }
        let parsed = url::Url::parse(proxy_url)
            .map_err(|e| ConfigError::Invalid(format!("{} 的代理 URL 无效: {e}", repo.name)))?;
        if parsed.scheme() != "https" && parsed.scheme() != "http" {
            return Err(ConfigError::Invalid(format!(
                "{} 的代理 URL 协议无效",
                repo.name
            )));
        }
    }
    if repo.install_target.is_empty()
        || Path::new(&repo.install_target).is_absolute()
        || repo
            .install_target
            .split(['/', '\\'])
            .any(|part| part == "..")
    {
        return Err(ConfigError::Invalid(format!(
            "{} 的安装目录不安全",
            repo.name
        )));
    }
    if repo.asset_rules.is_empty()
        || repo
            .asset_rules
            .iter()
            .any(|rule| rule.value.trim().is_empty())
    {
        return Err(ConfigError::Invalid("请创建至少一个有效的资产规则".into()));
    }
    Ok(())
}

fn repo_key(repo: &RepositoryConfig) -> String {
    let export = RepositoryExport {
        name: String::new(),
        provider: repo.provider.clone(),
        url: repo.url.clone(),
        channel: repo.channel.clone(),
        asset_rules: repo.asset_rules.clone(),
        install_target: repo.install_target.clone(),
        plugin_guid: repo.plugin_guid.clone(),
        download_threads: repo.download_threads,
        api_proxy_mode: repo.api_proxy_mode.clone(),
        api_proxy_url: repo.api_proxy_url.clone(),
        release_proxy_mode: repo.release_proxy_mode.clone(),
        release_proxy_url: repo.release_proxy_url.clone(),
        enabled: repo.enabled,
    };
    export_key(&export)
}

fn export_key(repo: &RepositoryExport) -> String {
    format!(
        "{:?}|{}|{:?}|{}|{:?}|{}|{:?}|{}|{}",
        repo.provider,
        repo.url.trim_end_matches('/').to_ascii_lowercase(),
        repo.asset_rules,
        repo.install_target,
        repo.api_proxy_mode,
        repo.api_proxy_url,
        repo.release_proxy_mode,
        repo.release_proxy_url,
        repo.enabled
    )
}

fn repository_from_export(repo: &RepositoryExport, id: String, builtin: bool) -> RepositoryConfig {
    let download_threads = match &repo.provider {
        Provider::Github => if repo.download_threads == 0 { 16 } else { repo.download_threads.clamp(1, 32) },
        Provider::Gitee => 1,
    };
    RepositoryConfig {
        id,
        name: repo.name.clone(),
        provider: repo.provider.clone(),
        url: repo.url.clone(),
        channel: repo.channel.clone(),
        asset_rules: repo.asset_rules.clone(),
        install_target: repo.install_target.clone(),
        plugin_guid: repo.plugin_guid.clone(),
        download_threads,
        api_proxy_mode: repo.api_proxy_mode.clone(),
        api_proxy_url: repo.api_proxy_url.clone(),
        release_proxy_mode: repo.release_proxy_mode.clone(),
        release_proxy_url: repo.release_proxy_url.clone(),
        github_token: String::new(),
        gitee_token: String::new(),
        official_api_only: true,
        enabled: repo.enabled,
        builtin,
        pinned_version: None,
        ignored_version: None,
        etag: None,
        cached_releases: Vec::new(),
        last_error: None,
    }
}

fn protect_secret(value: &str) -> io::Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    #[cfg(windows)]
    unsafe {
        use std::ptr::null_mut;
        use winapi::um::dpapi::CryptProtectData;
        use winapi::um::winbase::LocalFree;
        use winapi::um::wincrypt::DATA_BLOB;
        let mut input = DATA_BLOB {
            cbData: value.len() as u32,
            pbData: value.as_ptr() as *mut u8,
        };
        let mut output = DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        if CryptProtectData(
            &mut input,
            std::ptr::null(),
            null_mut(),
            null_mut(),
            null_mut(),
            0,
            &mut output,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let encrypted = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let encoded = format!("dpapi:{}", B64.encode(encrypted));
        LocalFree(output.pbData as *mut _);
        return Ok(encoded);
    }
    #[cfg(not(windows))]
    Ok(format!("plain:{}", B64.encode(value.as_bytes())))
}

fn reveal_secret(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let encoded = value.split_once(':').map(|(_, data)| data).unwrap_or(value);
    let encrypted = B64.decode(encoded).ok()?;
    #[cfg(windows)]
    unsafe {
        use std::ptr::null_mut;
        use winapi::um::dpapi::CryptUnprotectData;
        use winapi::um::winbase::LocalFree;
        use winapi::um::wincrypt::DATA_BLOB;
        let mut input = DATA_BLOB {
            cbData: encrypted.len() as u32,
            pbData: encrypted.as_ptr() as *mut u8,
        };
        let mut output = DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        if CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            0,
            &mut output,
        ) == 0
        {
            return None;
        }
        let result = String::from_utf8_lossy(std::slice::from_raw_parts(
            output.pbData,
            output.cbData as usize,
        ))
        .to_string();
        LocalFree(output.pbData as *mut _);
        Some(result)
    }
    #[cfg(not(windows))]
    {
        String::from_utf8(encrypted).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_repositories_are_present() {
        let config = default_config();
        assert_eq!(config.repositories.len(), 2);
        assert!(config
            .repositories
            .iter()
            .all(|repo| repo.api_proxy_url == "https://oc2gtapi.ch3ng.top"));
        assert!(config.repositories.iter().all(|repo| {
            repo.release_proxy_mode == ProxyMode::Direct && repo.release_proxy_url.is_empty()
        }));
        assert!(config.repositories.iter().all(|repo| {
            repo.plugin_guid == "com.ch3ngyz.plugin.HostUtilities"
                && repo.channel == Channel::All
        }));
        let serialized = toml::to_string(&config).unwrap();
        assert!(!serialized.contains("\n[proxies"));
        assert!(serialized.contains("api_proxy_url = \"https://oc2gtapi.ch3ng.top\""));
    }

    #[test]
    fn bundle_round_trip_is_toml() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let config = default_config();
        let text = service.export_bundle(&config, None, false).unwrap();
        assert!(text.contains("format = \"oc2-mod-repository-bundle\""));
        let bundle = service.validate_bundle(&text).unwrap();
        assert_eq!(bundle.repositories.len(), 0);
    }

    #[test]
    fn unsafe_install_target_is_rejected() {
        let result = validate_repository(&RepositoryExport {
            name: "bad".into(),
            provider: Provider::Github,
            url: "https://github.com/a/b/releases".into(),
            channel: Channel::Stable,
            asset_rules: vec![AssetRule {
                rule_type: AssetRuleType::Exact,
                value: "a.zip".into(),
            }],
            install_target: "../plugins".into(),
            plugin_guid: String::new(),
            download_threads: 1,
            api_proxy_mode: ProxyMode::Auto,
            api_proxy_url: String::new(),
            release_proxy_mode: ProxyMode::Direct,
            release_proxy_url: String::new(),
            enabled: true,
        });
        assert!(result.is_err());
    }
}
