//! 仓库 owner 头像：磁盘缓存 + 竞速下载 + 同一 owner 单飞。
//!
//! 头像不是关键数据，但有几个约束必须守住：
//!
//! 1. **缓存优先，但有有效期**。头像几乎不变，每次都联网既慢又浪费流量；
//!    落盘后 12 小时内直接读文件、不发请求。超过 12 小时当作未命中重新
//!    下载（用户偶尔会换头像，永久缓存会让界面一直显示旧图）。
//! 2. **竞速下载**。GitHub 直连在部分网络下不可达，必须按现有 Raw 代理
//!    候选竞速 —— 与 MOD 下载共用同一套候选顺序与代理配置，不另写一套。
//! 3. **同一 owner 单飞**。一次刷新会同时刷出几十个仓库，其中很多属于
//!    同一个 owner（例如所有 BepInEx 插件）。若每个仓库各起一次竞速下载，
//!    同一个人会被并发下载几十次。这里用进程内的 in-flight 表把同一
//!    owner 的请求合并成一次，其余调用者等同一个结果。
//! 4. **失败就回落到平台图标**（由前端处理），不要用占位图或旧图凑数。
//!
//! 缓存文件名用 `owner + provider` 的哈希，避免用户名里的特殊字符变成
//! 路径问题，也避免不同平台的同名 owner 互相覆盖。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::models::{AppConfig, Provider};

/// 单个头像的大小上限。
///
/// 头像是 60~128px 的小图，正常几十 KB。超过 512 KiB 说明拿到的不是头像
/// （错误页 HTML、被代理注入的页面），直接丢弃。
const MAX_AVATAR_BYTES: usize = 512 * 1024;

/// 头像下载超时：比源文档短，因为它是纯装饰性内容，不值得让界面等太久。
const AVATAR_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 头像磁盘缓存有效期：12 小时。
///
/// 头像是「几乎不变但偶尔会变」的数据：永久缓存会让换了头像的用户一直看到
/// 旧图，完全不做缓存又每次启动都重下几十张。12 小时是这两者之间的折中 ——
/// 一天顶多多下一次，换头像当天也能恢复。
///
/// 过期**不删文件**：只当作未命中重新下载，成功后覆盖原文件。这样下载失败
/// 时文件还在（下次仍可尝试），也不会在缓存目录里留下一堆半删状态。
const AVATAR_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(12 * 60 * 60);

/// 缓存子目录名。
const AVATAR_DIR: &str = "avatars";

/// 进程内的「正在下载」表：同一 owner+provider 只跑一次。
///
/// 值为 `None` 表示后来者只需等锁；具体结果由先到的那个写进缓存文件，
/// 后来者拿到锁后直接命中缓存。
fn in_flight() -> &'static tokio::sync::Mutex<HashMap<String, ()>> {
    static TABLE: OnceLock<tokio::sync::Mutex<HashMap<String, ()>>> = OnceLock::new();
    TABLE.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

/// 缓存键：`{provider}-{owner}` 的稳定哈希。
///
/// 用哈希而不是原始用户名，是为了让任何字符都能安全落盘；
/// 前缀保留 provider，避免 GitHub/Gitee 同名 owner 撞车。
fn cache_key(provider: &Provider, owner: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    owner.trim().to_lowercase().hash(&mut hasher);
    let provider = match provider {
        Provider::Github => "gh",
        Provider::Gitee => "gt",
        Provider::Gitcode => "gc",
    };
    format!("{provider}-{:016x}", hasher.finish())
}

/// 批量命令返回给前端的键：`{provider}:{owner_lowercase}`。
///
/// **必须与前端 `avatars.ts::avatarKey()` 逐字一致**。两边各写一份是这次
/// bug 的来源：后端用了原样大小写的 owner，前端用小写，于是 `CH3NGY2`、
/// `QingyuQingxue` 这类含大写的 owner 明明下载成功却查不到，卡片回落成
/// 平台图标，而日志显示「可用=8/8」——现象是「只有这几个没有头像」。
pub fn response_key(provider: &Provider, owner: &str) -> String {
    let provider = match provider {
        Provider::Github => "github",
        Provider::Gitee => "gitee",
        Provider::Gitcode => "gitcode",
    };
    format!("{provider}:{}", owner.trim().to_lowercase())
}

/// 按文件头判断 MIME，供前端拼 data URL。
///
/// 不信任响应里的 `Content-Type`：代理经常返回 `text/html` 或
/// `application/octet-stream`，用真实字节判断更可靠。
pub fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        "image/svg+xml"
    } else {
        // 校验通过的内容一定是图片之一，这里只是兜底
        "application/octet-stream"
    }
}

/// 头像缓存路径。
pub fn cache_path(cache_dir: &std::path::Path, provider: &Provider, owner: &str) -> PathBuf {
    cache_dir
        .join(AVATAR_DIR)
        .join(format!("{}.img", cache_key(provider, owner)))
}

/// 头像的直链候选地址（不含代理）。
///
/// GitHub 与 Gitee 都支持 `/{owner}.png` 这个入口，会 302 到真正的头像
/// 地址（GitHub → `avatars.githubusercontent.com`，Gitee → `foruda.gitee.com`）。
/// 用这条规律可以省掉一次 API 请求 —— 仓库配置里本来只有 owner 名。
fn direct_url(provider: &Provider, owner: &str) -> String {
    let encoded: String = owner.trim().bytes().fold(String::new(), |mut out, byte| {
        // 只保留 URI 里安全且用户名可能出现的字符，其余百分号编码
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
        out
    });
    match provider {
        Provider::Github => format!("https://github.com/{encoded}.png?size=64"),
        Provider::Gitee => format!("https://gitee.com/{encoded}.png?size=64"),
        // No verified owner-only avatar endpoint is available for GitCode yet.
        // Let the card use its provider icon instead of requesting a guessed URL.
        Provider::Gitcode => String::new(),
    }
}

/// 缓存文件是否仍在有效期内（按修改时间算）。
///
/// 取不到修改时间（文件刚被外部工具改写、文件系统不提供）时按**过期**处理：
/// 多下一次是可接受的代价，拿一份无法判定年龄的图当缓存更糟。
/// 时钟回拨（文件修改时间早于系统时间很多）同理 —— 此时「年龄」不可信。
fn cache_is_fresh(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let Ok(age) = std::time::SystemTime::now().duration_since(modified) else {
        return false;
    };
    age < AVATAR_CACHE_TTL
}

/// 读缓存；不存在、为空或已过有效期时返回 `None`。
pub fn read_cached(
    cache_dir: &std::path::Path,
    provider: &Provider,
    owner: &str,
) -> Option<Vec<u8>> {
    let path = cache_path(cache_dir, provider, owner);
    if !cache_is_fresh(&path) {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(bytes)
}

/// 拿到一个 owner 的头像字节：缓存优先，未命中则竞速下载并落盘。
///
/// 同一 `(provider, owner)` 并发调用只会产生**一次**实际下载：先到的负责
/// 下载，其余等待后重新读缓存。这样一次刷新里几十个同 owner 的仓库
/// 不会各自发起竞速。
pub async fn load(
    config: &AppConfig,
    cache_dir: &std::path::Path,
    provider: &Provider,
    owner: &str,
) -> Result<Vec<u8>, String> {
    let owner = owner.trim();
    if owner.is_empty() {
        return Err("avatar.ownerRequired".into());
    }
    if let Some(bytes) = read_cached(cache_dir, provider, owner) {
        log::debug!("头像命中缓存：provider={provider:?} owner={owner}");
        return Ok(bytes);
    }

    let key = cache_key(provider, owner);
    {
        // 单飞：拿到这张表项的独占权才继续；拿不到说明别的任务正在下载，
        // 等它结束后直接读缓存（下面的 read_cached 会命中）。
        let mut table = in_flight().lock().await;
        match table.entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => {
                drop(table);
                log::debug!("头像已有下载在进行，等待结果：provider={provider:?} owner={owner}");
                return Ok(wait_for_cache(cache_dir, provider, owner).await?);
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(());
            }
        }
    }

    let result = download_and_store(config, cache_dir, provider, owner).await;
    // 无论成功失败都要释放表项，否则后续请求会永远等一个不会完成的任务
    in_flight().lock().await.remove(&key);
    result
}

/// 等另一个任务把缓存写出来；超时后按失败处理（回落到平台图标）。
async fn wait_for_cache(
    cache_dir: &std::path::Path,
    provider: &Provider,
    owner: &str,
) -> Result<Vec<u8>, String> {
    let deadline = std::time::Instant::now() + AVATAR_TIMEOUT;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        if let Some(bytes) = read_cached(cache_dir, provider, owner) {
            return Ok(bytes);
        }
    }
    Err("avatar.waitTimeout".into())
}

/// 按 Raw 代理候选竞速下载，成功后落盘。
async fn download_and_store(
    config: &AppConfig,
    cache_dir: &std::path::Path,
    provider: &Provider,
    owner: &str,
) -> Result<Vec<u8>, String> {
    let url = direct_url(provider, owner);
    if url.is_empty() {
        return Err("avatar.noSource".into());
    }
    let mode = crate::download::planning::selected_asset_proxy_mode(
        None,
        provider,
        crate::config::current_user_region(),
    );
    /*
     * 用**头像专用**代理组，不用 raw 组。
     *
     * 实测 8 个 raw 代理全部取不到 `github.com/{owner}.png`（403/404）——
     * 它们的模板都按 `{owner}/{repo}/{branch}/{path}` 设计，头像 URL 没有
     * 这些分段。真正能取到的是「纯 {url} 包装」型代理，所以头像有独立
     * 名单 `avatar_proxies`，与其他分组一起由同一份清单替换。
     *
     * `is_raw_file = false` 同样必须：raw 模式会把目标重写成
     * `raw.githubusercontent.com/{owner}/{repo}/main/{path}`，头像拆不出
     * repo，会拼出 `.../gua248.png//main/` 这种必然失败的地址。
     */
    let proxies = crate::download::planning::asset_proxy_urls(
        &config.github_proxies.avatar_proxies,
        &url,
        false,
        "",
        "",
        mode,
        None,
    );
    let candidates =
        crate::download::planning::ordered_asset_download_candidates(mode, &url, proxies);
    if candidates.is_empty() {
        log::warn!("头像没有可用下载候选：provider={provider:?} owner={owner}");
        return Err("avatar.noSource".into());
    }
    log::info!(
        "开始下载头像：provider={provider:?} owner={owner} 候选数={}",
        candidates.len()
    );

    let client = crate::releases::build_download_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let mut candidates_iter = candidates.into_iter();
    // 逐个尝试而不是竞速：头像是几十 KB 的小文件，竞速的并发开销比省下的
    // 时间更贵，而且一次刷新里会有很多 owner 同时请求，再竞速会把连接数
    // 放大好几倍（用户明确要求「同一 owner 只做一次」，同样适用于彼此）。
    let mut last_error = String::new();
    while let Some(candidate) = candidates_iter.next() {
        match fetch_avatar(&client, &candidate.url).await {
            Ok(bytes) => {
                if let Err(error) = store(cache_dir, provider, owner, &bytes) {
                    // 落盘失败不影响本次显示，只是下次还要重新下载
                    log::warn!("头像缓存写入失败：owner={owner} 原因={error}");
                }
                return Ok(bytes);
            }
            Err(error) => {
                log::debug!("头像下载失败，尝试下一个候选：owner={owner} 原因={error}");
                last_error = error;
            }
        }
    }
    log::warn!("头像全部候选失败：provider={provider:?} owner={owner} 原因={last_error}");
    Err(last_error)
}

/// 下载单个候选并做基本校验（状态码、大小、看起来像图片）。
async fn fetch_avatar(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = tokio::time::timeout(
        AVATAR_TIMEOUT,
        client
            .get(url)
            .header("User-Agent", crate::releases::APP_USER_AGENT)
            .send(),
    )
    .await
    .map_err(|_| "avatar.requestTimeout".to_string())?
    .map_err(|error| error.without_url().to_string())?
    .error_for_status()
    .map_err(|error| error.to_string())?;

    let bytes = tokio::time::timeout(AVATAR_TIMEOUT, response.bytes())
        .await
        .map_err(|_| "avatar.readTimeout".to_string())?
        .map_err(|error| error.to_string())?
        .to_vec();
    validate(&bytes).map(|_| bytes)
}

/// 头像必须是图片，且不超过上限。
///
/// 代理失败时常见的是返回一页 HTML（200 状态码），若不校验就会把 HTML
/// 缓存成图片，界面上变成破图 —— 那比回落到平台图标更糟。
fn validate(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("avatar.emptyContent".into());
    }
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err(crate::locale_tag::with_params(
            "avatar.tooLarge",
            &[("bytes", &bytes.len().to_string())],
        ));
    }
    let is_image = bytes.starts_with(&[0xFF, 0xD8]) // JPEG
        || bytes.starts_with(&[0x89, b'P', b'N', b'G']) // PNG
        || bytes.starts_with(b"GIF8")
        || (bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"<svg")
        || bytes.starts_with(b"<?xml");
    if !is_image {
        return Err("avatar.notImage".into());
    }
    Ok(())
}

/// 原子落盘：先写临时文件再改名，避免半截文件被当成有效缓存。
fn store(
    cache_dir: &std::path::Path,
    provider: &Provider,
    owner: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let path = cache_path(cache_dir, provider, owner);
    let dir = path.parent().ok_or("avatar.cachePathInvalid")?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    std::fs::rename(&temp, &path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        error.to_string()
    })
}

/// 清空头像缓存（供设置页或排查使用）。
pub fn clear_cache(cache_dir: &std::path::Path) -> Result<usize, String> {
    let dir = cache_dir.join(AVATAR_DIR);
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut removed = 0usize;
    for entry in std::fs::read_dir(&dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.path().is_file() && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
#[path = "avatars/tests.rs"]
mod tests;
