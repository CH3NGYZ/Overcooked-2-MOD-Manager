//! 读取和发布令牌的保护、读取与更新。

use super::ConfigService;
use crate::models::*;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use std::io;

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

impl ConfigService {
    pub fn set_tokens(
        &self,
        config: &mut AppConfig,
        github: &str,
        gitee: &str,
        gitcode: &str,
    ) -> io::Result<()> {
        // Protect every supplied value before mutating any field.
        let github = (!github.trim().is_empty())
            .then(|| protect_secret(github.trim()))
            .transpose()?;
        let gitee = (!gitee.trim().is_empty())
            .then(|| protect_secret(gitee.trim()))
            .transpose()?;
        let gitcode = (!gitcode.trim().is_empty())
            .then(|| protect_secret(gitcode.trim()))
            .transpose()?;
        if let Some(value) = github {
            config.auth.github_token = value;
        }
        if let Some(value) = gitee {
            config.auth.gitee_token = value;
        }
        if let Some(value) = gitcode {
            config.auth.gitcode_token = value;
        }
        Ok(())
    }

    pub fn set_repository_tokens(&self, repo: &mut RepositoryConfig, github: &str, gitee: &str) {
        if !github.is_empty() {
            repo.github_token = protect_secret(github).unwrap_or_default();
        }
        if !gitee.is_empty() {
            repo.gitee_token = protect_secret(gitee).unwrap_or_default();
        }
    }

    pub fn repository_token(&self, repo: &RepositoryConfig) -> Option<String> {
        match repo.provider {
            Provider::Github => reveal_secret(&repo.github_token),
            Provider::Gitee => reveal_secret(&repo.gitee_token),
            Provider::Gitcode => None,
        }
    }

    pub(crate) fn repository_api_token(
        &self,
        config: &AppConfig,
        repo: &RepositoryConfig,
    ) -> Option<String> {
        self.repository_token(repo)
            .or_else(|| self.api_token(config, &repo.provider))
    }

    pub(crate) fn api_token(&self, config: &AppConfig, provider: &Provider) -> Option<String> {
        reveal_secret(match provider {
            Provider::Github => &config.auth.github_token,
            Provider::Gitee => &config.auth.gitee_token,
            Provider::Gitcode => &config.auth.gitcode_token,
        })
    }

    /// 地图发布页的 access token：GitHub App 授权拿到的那一份，读路径不受影响。
    pub(crate) fn publish_token(&self, config: &AppConfig) -> Option<String> {
        reveal_secret(&config.auth.github_publish_token)
    }

    /// 与 access token 配对的 refresh token；App 关闭了令牌过期时没有。
    pub(crate) fn publish_refresh_token(&self, config: &AppConfig) -> Option<String> {
        reveal_secret(&config.auth.github_publish_refresh_token)
    }

    /// 写入一组发布令牌（首次授权或刷新后的轮换结果）。
    ///
    /// 加密失败必须报错：吞掉错误会写出空令牌，界面却显示「登录成功」，下一次
    /// 进页面才莫名其妙地掉线。refresh token 一并覆盖 —— 刷新后旧的已经作废。
    pub fn set_publish_tokens(
        &self,
        config: &mut AppConfig,
        tokens: &PublishTokens,
        login: &str,
    ) -> io::Result<()> {
        let access = protect_secret(tokens.access_token.trim())?;
        let refresh = match tokens.refresh_token.as_deref().map(str::trim) {
            Some(value) if !value.is_empty() => protect_secret(value)?,
            _ => String::new(),
        };
        config.auth.github_publish_token = access;
        config.auth.github_publish_refresh_token = refresh;
        config.auth.github_publish_token_expires_at = tokens.expires_at;
        config.auth.github_publish_refresh_expires_at = tokens.refresh_expires_at;
        config.auth.github_publish_login = login.trim().to_string();
        Ok(())
    }

    pub fn clear_publish_token(&self, config: &mut AppConfig) {
        config.auth.github_publish_token.clear();
        config.auth.github_publish_refresh_token.clear();
        config.auth.github_publish_token_expires_at = None;
        config.auth.github_publish_refresh_expires_at = None;
        config.auth.github_publish_login.clear();
    }

    /// 上次装 App 的 installation id；0 表示还没装过（或记录已失效被清掉）。
    pub(crate) fn publish_installation_id(&self, config: &AppConfig) -> u64 {
        config.auth.github_publish_installation_id
    }

    pub fn set_publish_installation_id(&self, config: &mut AppConfig, id: u64) {
        config.auth.github_publish_installation_id = id;
    }
}
