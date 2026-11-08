use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::models::Provider;

/**
 * 匿名 API 限流闸门。
 *
 * 目标：一旦确认「匿名请求被限流」，就不再继续打官方接口，直到限流窗口结束。
 * 带 Token 的请求走独立闸门：Token 有独立配额，不受匿名限流影响，
 * 但 Token 自己也可能被限流，同样要挂起。
 *
 * 两把锁对应两类凭据：
 *   anonymous —— 不带 Token 的官方直连
 *   token     —— 带个人 Token 的官方请求
 * 代理请求不参与限流（它们不消耗官方配额）。
 */

/// Gitee 匿名 403 + 正文含 “Rate Limit Exceeded” 后锁定的时长。
const GITEE_ANONYMOUS_BLOCK: Duration = Duration::from_secs(5 * 60);
/// GitHub 既没有 x-ratelimit-reset 也没有 retry-after 时的保守等待。
const GITHUB_FALLBACK_BLOCK: Duration = Duration::from_secs(60);
/// 二级限流的指数退避上限。
const SECONDARY_BACKOFF_CAP: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    /// 不带 Token 的官方直连。
    Anonymous,
    /// 带个人 Token 的官方请求。
    Token,
    /// 代理请求：不消耗官方配额，不受闸门约束。
    Proxy,
}

/// 闸门状态：某 provider 某类凭据在什么时间点之前不允许再请求。
#[derive(Default)]
struct GateState {
    gitee_anonymous: Option<Instant>,
    gitee_token: Option<Instant>,
    github_anonymous: Option<Instant>,
    github_token: Option<Instant>,
    gitcode_anonymous: Option<Instant>,
    gitcode_token: Option<Instant>,
    /// 连续二级限流次数，用于指数退避。
    github_secondary_hits: u32,
}

#[derive(Default)]
pub struct ApiRateGate {
    state: Mutex<GateState>,
}

/// 一次 403 的判定结果，来自响应头与正文。
#[derive(Debug, Default, Clone)]
pub struct RateLimitSignal {
    /// 正文是否包含限流文案（Gitee “Rate Limit Exceeded” / GitHub 二级限流提示）。
    pub body_limit_message: bool,
    /// x-ratelimit-remaining 是否为 0。
    pub remaining_exhausted: bool,
    /// x-ratelimit-reset（UTC 纪元秒）。
    pub reset_at: Option<u64>,
    /// retry-after（秒）。
    pub retry_after: Option<u64>,
}

impl RateLimitSignal {
    /// 是否构成「触发限流锁」。
    ///
    /// 只认三类证据：正文限流文案、remaining=0、retry-after。
    /// 单纯的 403（例如私有仓库无权限）不应锁住整条通道。
    pub fn is_rate_limited(&self) -> bool {
        self.body_limit_message || self.remaining_exhausted || self.retry_after.is_some()
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

impl ApiRateGate {
    /// 该请求现在能不能发：返回 Ok 或需要等待的时长。
    pub fn check(&self, provider: &Provider, kind: GateKind) -> Result<(), Duration> {
        if kind == GateKind::Proxy {
            return Ok(());
        }
        let now = Instant::now();
        let state = match self.state.lock() {
            Ok(state) => state,
            // 锁中毒时宁可放行，也不要卡死所有网络请求
            Err(_) => return Ok(()),
        };
        let until = match (provider, kind) {
            (Provider::Gitee, GateKind::Anonymous) => state.gitee_anonymous,
            (Provider::Gitee, GateKind::Token) => state.gitee_token,
            (Provider::Github, GateKind::Anonymous) => state.github_anonymous,
            (Provider::Github, GateKind::Token) => state.github_token,
            (Provider::Gitcode, GateKind::Anonymous) => state.gitcode_anonymous,
            (Provider::Gitcode, GateKind::Token) => state.gitcode_token,
            (_, GateKind::Proxy) => None,
        };
        match until {
            Some(deadline) if deadline > now => Err(deadline - now),
            _ => Ok(()),
        }
    }

    /// 403 之后调用：符合限流特征就上锁，返回锁定到什么时候（None 表示不是限流）。
    pub fn record_forbidden(
        &self,
        provider: &Provider,
        kind: GateKind,
        signal: &RateLimitSignal,
    ) -> Option<Duration> {
        if kind == GateKind::Proxy || !signal.is_rate_limited() {
            return None;
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return None,
        };
        let block = match provider {
            Provider::Gitcode => {
                Duration::from_secs(signal.retry_after.unwrap_or(60).clamp(1, 900))
            }
            Provider::Gitee => {
                // Gitee 只认正文里的限流文案；命中后固定 5 分钟
                if !signal.body_limit_message {
                    return None;
                }
                GITEE_ANONYMOUS_BLOCK
            }
            Provider::Github => {
                // GitHub：优先 retry-after，其次 x-ratelimit-reset，最后保守 60 秒；
                // 连续命中做指数退避
                state.github_secondary_hits = state.github_secondary_hits.saturating_add(1);
                let base = if let Some(seconds) = signal.retry_after {
                    Duration::from_secs(seconds)
                } else if let Some(reset_at) = signal.reset_at {
                    // reset 是绝对时间点；已经过去就按 60 秒保守处理
                    Duration::from_secs(reset_at.saturating_sub(now_epoch_secs()).max(1))
                } else if signal.remaining_exhausted {
                    GITHUB_FALLBACK_BLOCK
                } else {
                    GITHUB_FALLBACK_BLOCK
                };
                let factor = 1u32 << state.github_secondary_hits.saturating_sub(1).min(4);
                (base * factor).min(SECONDARY_BACKOFF_CAP)
            }
        };
        let deadline = Instant::now() + block;
        match (provider, kind) {
            (Provider::Gitee, GateKind::Anonymous) => state.gitee_anonymous = Some(deadline),
            (Provider::Gitee, GateKind::Token) => state.gitee_token = Some(deadline),
            (Provider::Github, GateKind::Anonymous) => state.github_anonymous = Some(deadline),
            (Provider::Github, GateKind::Token) => state.github_token = Some(deadline),
            (Provider::Gitcode, GateKind::Anonymous) => state.gitcode_anonymous = Some(deadline),
            (Provider::Gitcode, GateKind::Token) => state.gitcode_token = Some(deadline),
            _ => {}
        }
        Some(block)
    }

    /// 请求成功：清掉对应通道的退避计数（恢复正常节流节奏）。
    pub fn record_success(&self, provider: &Provider, kind: GateKind) {
        if kind == GateKind::Proxy {
            return;
        }
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        match (provider, kind) {
            (Provider::Gitee, GateKind::Anonymous) => state.gitee_anonymous = None,
            (Provider::Gitee, GateKind::Token) => state.gitee_token = None,
            (Provider::Gitcode, GateKind::Anonymous) => state.gitcode_anonymous = None,
            (Provider::Gitcode, GateKind::Token) => state.gitcode_token = None,
            (Provider::Github, GateKind::Anonymous) => {
                state.github_anonymous = None;
                state.github_secondary_hits = 0;
            }
            (Provider::Github, GateKind::Token) => {
                state.github_token = None;
                state.github_secondary_hits = 0;
            }
            _ => {}
        }
    }

    /// 当前剩余的锁定时间（供测试查询）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remaining(&self, provider: &Provider, kind: GateKind) -> Option<Duration> {
        self.check(provider, kind).err()
    }
}

/// 从正文里识别限流文案。
pub fn body_reports_rate_limit(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    lowered.contains("rate limit exceeded")
        || lowered.contains("secondary rate limit")
        || lowered.contains("api rate limit")
}

/// 进程级单例：所有 Release API 请求共用同一套闸门状态。
pub fn global_gate() -> &'static ApiRateGate {
    static GATE: std::sync::OnceLock<ApiRateGate> = std::sync::OnceLock::new();
    GATE.get_or_init(ApiRateGate::default)
}

#[cfg(test)]
mod tests;
