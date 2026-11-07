use crate::app_state::AppState;
use crate::app_state::CachedAppUpdateCheck;
use crate::app_update::channels::InstallerRegion;
use crate::models::AppUpdateCheckResult;
use crate::release_cache::freshness::cache_is_fresh;
use crate::release_cache::freshness::now_millis;

/// 程序更新检查结果的缓存时限。
///
/// 启动时恒检测，所以「启动 → 立刻点检查 → 再切回设置页」会在几十秒内
/// 反复打同一份安装器清单；5 分钟内重复调用直接复用上一次的成功结果。
/// 手动检测传 `force = true` 绕过（与仓库刷新同一条规则）。
pub(crate) const APP_UPDATE_CACHE_TTL_MS: i64 = 5 * 60 * 1000;

/// 命中的缓存结果；地区不一致或已过期都算未命中。
pub(crate) fn app_update_cache_get(
    state: &AppState,
    region: InstallerRegion,
) -> Option<AppUpdateCheckResult> {
    let guard = state.app_update_check.lock().ok()?;
    let cached = guard.as_ref()?;
    if cached.region != region.as_str() {
        return None;
    }
    if !cache_is_fresh(
        Some(cached.checked_at),
        now_millis(),
        APP_UPDATE_CACHE_TTL_MS,
    ) {
        return None;
    }
    Some(cached.result.clone())
}

pub(crate) fn app_update_cache_store(state: &AppState, result: &AppUpdateCheckResult) {
    let Ok(mut guard) = state.app_update_check.lock() else {
        return;
    };
    *guard = Some(CachedAppUpdateCheck {
        region: result.region.clone(),
        checked_at: now_millis(),
        result: result.clone(),
    });
}
