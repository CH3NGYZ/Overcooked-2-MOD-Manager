//! 仓库级 Release 列表缓存。
//!
//! 一个仓库的 Release 列表现在会被好几处读：MOD 仓库本身、地图条目（同一个
//! 合集仓里每张图占一个 tag），以及「文档放进 Release 资产」时的 sha256 校验。
//! 各请求一次会把匿名额度（GitHub 只有 60 次/小时）当场用光，所以按仓库收敛成
//! 一次请求。
//!
//! 两条复用规则，各自解决一件不同的事：
//!
//! - **同一个 pass**：一次用户点击 = 一个 pass id。同一 pass 内同一个仓库只请求
//!   一次 ——「点一次检查更新，只打一次 API」靠的是这个显式身份，不是靠时间猜；
//! - **5 分钟 TTL**：刚取回来的列表就是「现在的最新数据」，五分钟内再要一次没有
//!   任何信息增量。没有它，启动时的「先读源树、再刷条目」会把同一个仓库打两遍。
//!
//! 只缓存成功结果：失败写进去会让下一次重试直接拿到同一个错误。

pub(crate) mod freshness;

use crate::models::ReleaseInfo;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 没有 pass 时可用缓存的时长。
///
/// 与条目缓存使用同一份五分钟时限；bundle 各地图共享原始列表。
/// 右下角整批刷新通过 force 绕过 TTL，同 pass 内仍只请求一次。
pub const REPO_RELEASE_CACHE_TTL: Duration =
    Duration::from_millis(freshness::RELEASE_CACHE_TTL_MS as u64);

/// 过期条目保留多久才真正清掉。
///
/// 过期的不会被命中，留一会儿只为日志与调试；超过这个时长的条目在下次写入时
/// 顺手清掉，避免长期驻留。
const STALE_RETENTION: Duration = Duration::from_secs(600);

struct Entry {
    fetched_at: Instant,
    /// 写这条缓存的 pass。`None` = 没有 pass 的请求写的。
    pass_id: Option<String>,
    releases: Arc<Vec<ReleaseInfo>>,
}

#[derive(Default)]
pub struct RepoReleaseCache {
    entries: Mutex<HashMap<String, Entry>>,
    /// 每个仓库一把异步锁：并发进来的同一仓库请求只放行一个，其余的等它写完
    /// 再命中缓存。std 的 Mutex 不能跨 await 持有，所以另开一份。
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl RepoReleaseCache {
    #[cfg(test)]
    pub(crate) fn expire_for_test(&self, key: &str) {
        self.entries
            .lock()
            .unwrap()
            .get_mut(key)
            .unwrap()
            .fetched_at = Instant::now() - REPO_RELEASE_CACHE_TTL;
    }
    /// 取完整仓库列表；成功原子替换旧快照。force 跳过 TTL，但同 pass 仍合并。
    ///
    /// `fetch` 是 `FnOnce`：命中时它根本不会被调用，闭包里准备的客户端与令牌
    /// 也就白准备了 —— 这正是我们要的。
    ///
    /// `pass_id` 是「这一次用户动作」的身份，同一次动作里的所有读取都传同一个。
    pub async fn list<F, Fut>(
        &self,
        key: String,
        pass_id: Option<&str>,
        force: bool,
        fetch: F,
    ) -> Result<Arc<Vec<ReleaseInfo>>, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<ReleaseInfo>, String>>,
    {
        let gate = {
            let mut inflight = self.inflight.lock().unwrap_or_else(|error| {
                log::warn!("Release 缓存锁中毒，继续使用：{error}");
                error.into_inner()
            });
            inflight
                .entry(key.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        // 等锁期间前一个请求可能已经把结果写进去了，所以命中判定必须在拿到锁之后。
        let _guard = gate.lock().await;
        if let Some(hit) = self.hit(&key, pass_id, force) {
            log::debug!("Release 列表命中仓库缓存：仓库={}", key);
            return Ok(hit);
        }
        let releases = fetch().await?;
        let shared = Arc::new(releases);
        self.store(&key, pass_id.map(str::to_string), shared.clone());
        Ok(shared)
    }

    fn hit(&self, key: &str, pass_id: Option<&str>, force: bool) -> Option<Arc<Vec<ReleaseInfo>>> {
        let entries = self.entries.lock().unwrap_or_else(|error| {
            log::warn!("Release 缓存锁中毒，继续使用：{error}");
            error.into_inner()
        });
        let entry = entries.get(key)?;
        // `None == None` 不能算同一个 pass，否则无 pass 的调用会永远拿到旧列表。
        if pass_id.is_some() && entry.pass_id.as_deref() == pass_id {
            return Some(entry.releases.clone());
        }
        (!force && entry.fetched_at.elapsed() < REPO_RELEASE_CACHE_TTL)
            .then(|| entry.releases.clone())
    }

    fn store(&self, key: &str, pass_id: Option<String>, releases: Arc<Vec<ReleaseInfo>>) {
        let mut entries = self.entries.lock().unwrap_or_else(|error| {
            log::warn!("Release 缓存锁中毒，继续使用：{error}");
            error.into_inner()
        });
        entries.retain(|_, entry| entry.fetched_at.elapsed() < STALE_RETENTION);
        entries.insert(
            key.to_string(),
            Entry {
                fetched_at: Instant::now(),
                pass_id,
                releases,
            },
        );
    }
}

#[cfg(test)]
mod tests;
