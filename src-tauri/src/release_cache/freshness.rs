/// 仓库 Release 列表的缓存有效期。
///
/// 一次「刷新全部仓库源」会把整棵仓库树挨个打一遍 API：几十个仓库 × 每个
/// 若干次请求，光是打开页面就要等几十秒，而且极容易撞上 GitHub/Gitee 的
/// 速率限制。缓存 5 分钟后，重复刷新、切页再回来、重启管理器都直接读
/// 已有结果，只有真正过期或用户**主动**点刷新时才重新请求。
pub(crate) const RELEASE_CACHE_TTL_MS: i64 = 5 * 60 * 1000;

/// 当前 Unix 毫秒时间戳。
pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// 这份缓存是否仍在有效期内。
///
/// `None` 时间戳（旧配置、从未成功获取）一律视为过期 —— 宁可多请求一次，
/// 也不要拿一份来源不明的数据去判断「有没有更新」。Release 列表与程序更新
/// 检查共用这一份判定，时限各自由调用方传入（两者都按 5 分钟的约定）。
pub(crate) fn cache_is_fresh(cached_at: Option<i64>, now: i64, ttl_ms: i64) -> bool {
    match cached_at {
        // 时钟回拨（now < cached_at）也算过期：说明系统时间被改过，
        // 此时的「新鲜度」不可信。
        Some(at) => now >= at && now - at < ttl_ms,
        None => false,
    }
}

#[cfg(test)]
mod cache_ttl_tests;
