use super::{RELEASE_CACHE_TTL_MS, cache_is_fresh, now_millis};
use crate::app_update::cache::APP_UPDATE_CACHE_TTL_MS;

#[test]
fn every_network_cache_uses_five_minutes() {
    assert_eq!(RELEASE_CACHE_TTL_MS, 5 * 60 * 1000);
    assert_eq!(APP_UPDATE_CACHE_TTL_MS, 5 * 60 * 1000);
}

#[test]
fn fresh_within_five_minutes() {
    let now = 1_700_000_000_000;
    assert!(
        cache_is_fresh(Some(now), now, RELEASE_CACHE_TTL_MS),
        "刚写入应命中"
    );
    assert!(
        cache_is_fresh(
            Some(now - RELEASE_CACHE_TTL_MS + 1),
            now,
            RELEASE_CACHE_TTL_MS
        ),
        "差 1 毫秒到期仍算新鲜"
    );
}

#[test]
fn expired_at_and_after_ttl() {
    let now = 1_700_000_000_000;
    assert!(
        !cache_is_fresh(Some(now - RELEASE_CACHE_TTL_MS), now, RELEASE_CACHE_TTL_MS),
        "正好到期即失效"
    );
    assert!(!cache_is_fresh(
        Some(now - RELEASE_CACHE_TTL_MS - 1),
        now,
        RELEASE_CACHE_TTL_MS
    ));
    assert!(
        !cache_is_fresh(Some(now - 86_400_000), now, RELEASE_CACHE_TTL_MS),
        "隔天必失效"
    );
}

#[test]
fn missing_timestamp_is_expired() {
    // 旧版本配置没有这个字段，或从未成功获取过 —— 必须重新请求，
    // 不能拿一份来源不明的列表去判断「有没有更新」。
    assert!(!cache_is_fresh(
        None,
        1_700_000_000_000,
        RELEASE_CACHE_TTL_MS
    ));
    assert!(!cache_is_fresh(
        None,
        1_700_000_000_000,
        APP_UPDATE_CACHE_TTL_MS
    ));
}

#[test]
fn clock_going_backwards_is_treated_as_expired() {
    // 系统时间被调早：此时 now < cached_at，差值算出来是负数，
    // 如果不判方向就会永远命中缓存。
    let now = 1_700_000_000_000;
    assert!(!cache_is_fresh(
        Some(now + 60_000),
        now,
        RELEASE_CACHE_TTL_MS
    ));
    assert!(!cache_is_fresh(
        Some(now + 60_000),
        now,
        APP_UPDATE_CACHE_TTL_MS
    ));
}

#[test]
fn now_millis_is_plausible() {
    // 2023-11 之后、2100 之前：防止单位错成秒导致缓存永不过期
    let now = now_millis();
    assert!(now > 1_700_000_000_000, "时间戳应以毫秒为单位：{now}");
    assert!(now < 4_102_444_800_000, "时间戳明显异常：{now}");
}
