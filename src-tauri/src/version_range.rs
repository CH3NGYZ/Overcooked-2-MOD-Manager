//! 版本区间（pip 风格）语法的唯一实现。
//!
//! 关系表里的 `version = ">=3.10.2,<3.12.0"` 这条写法由本文件解析：
//! 逗号分隔的多个条件取 **AND**，每个条件是一个比较符加一个版本号。
//!
//! 这里**只做语法**，不排序。项目里的版本号比较有两套既有实现
//! （Rust 的 `releases::compare_release_versions` 与前端 `modStatus.ts`
//! 的 `compareVersions`），它们要处理四段 beta 序号、`v` 前缀、点号分隔的
//! `.beta4` 等历史写法；把排序规则搬进本文件会让 `tools/oc2-info`
//! （用 `#[path]` 复用本文件，且刻意不依赖 reqwest/regex）平白多出一份
//! 需要同步维护的排序实现。所以排序由调用方以回调传入。
//!
//! 支持的写法：
//! ```text
//! >=1.8.0            下限
//! >=1.8.0,<2.0.0     区间（推荐写法，替代 ~=）
//! <3.12.0            上限
//! ==3.10.2           精确
//! =3.10.2            同上（单个 = 也接受）
//! !=3.10.2           排除
//! *                  不限；留空同样不限
//! ```
//!
//! **裸版本号（`1.0`）不是合法写法**：老的两端写法里 `min_version = "1.0"`
//! 是「至少 1.0」，迁移时若把裸值按 `==` 解释，会把要求静默收窄成刚好 1.0。
//! 比较符必须写出来。
//!
//! 不支持 `~=`（兼容某段）与 `===`：本项目的版本号有 4 段写法
//! （`1.8.7.23` 是 `1.8.7` 的第 23 个 beta），`~=1.8.7` 到底锁到 1.8 还是
//! 1.8.7.x 有歧义，写清 `>=1.8.7,<1.9` 更直接。

use std::cmp::Ordering;

/// 单个条件的比较符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClauseOp {
    /// `==` 或 `=`：必须等于该版本
    Eq,
    /// `!=`：不得等于该版本
    NotEq,
    /// `>=`
    Ge,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `<`
    Lt,
}

impl ClauseOp {
    /// 反序列化用的字符串形态（`canonical_spec` 与错误提示共用）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::Ge => ">=",
            Self::Gt => ">",
            Self::Le => "<=",
            Self::Lt => "<",
        }
    }

    fn from_ordering(self, ordering: Ordering) -> bool {
        match self {
            Self::Eq => ordering == Ordering::Equal,
            Self::NotEq => ordering != Ordering::Equal,
            Self::Ge => ordering != Ordering::Less,
            Self::Gt => ordering == Ordering::Greater,
            Self::Le => ordering != Ordering::Greater,
            Self::Lt => ordering == Ordering::Less,
        }
    }
}

/// 一个条件：比较符 + 版本号（版本号保持调用方写的原文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionClause {
    pub op: ClauseOp,
    pub version: String,
}

/// 解析后的区间。`clauses` 为空表示「不限版本」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionRange {
    clauses: Vec<VersionClause>,
}

impl VersionRange {
    /// 解析 `>=1.8.0,<2.0.0` 这类写法。
    ///
    /// 空串与 `*` 都是「不限」；出现空条件（`>=1,,<2`）、未知比较符、
    /// 缺版本号都算错误 —— 静默把它当成「不限」会让一条写错的依赖
    /// 变成永远满足，比直接报错更难查。
    pub fn parse(spec: &str) -> Result<Self, String> {
        let trimmed = spec.trim();
        if trimmed.is_empty() || trimmed == "*" {
            return Ok(Self::default());
        }
        let mut clauses = Vec::new();
        for raw in trimmed.split(',') {
            let clause = raw.trim();
            if clause.is_empty() {
                return Err(format!("empty clause in `{trimmed}`"));
            }
            clauses.push(parse_clause(clause)?);
        }
        Ok(Self { clauses })
    }

    /// 是否「不限版本」（空串 / `*`）。
    pub fn is_any(&self) -> bool {
        self.clauses.is_empty()
    }

    #[allow(dead_code, reason = "tools/oc2-info 经 #[path] 复用本模块时使用")]
    pub fn clauses(&self) -> &[VersionClause] {
        &self.clauses
    }

    /// 版本是否满足全部条件。
    ///
    /// `compare(version, bound)` 的契约与 `compare_release_versions` 一致：
    /// 返回两侧的顺序，任一侧无法解析时返回 `None`。
    ///
    /// 两种「无法判定」都按**不满足**处理（宁可提示用户，也不要静默放过）：
    /// 条件里写了版本号但目标版本号为空，以及版本号解析失败。与改造前
    /// `versionInRange` 的行为一致。
    pub fn matches<F>(&self, version: &str, compare: F) -> bool
    where
        F: Fn(&str, &str) -> Option<Ordering>,
    {
        if self.clauses.is_empty() {
            return true;
        }
        if version.trim().is_empty() {
            return false;
        }
        self.clauses
            .iter()
            .all(|clause| match compare(version, &clause.version) {
                Some(ordering) => clause.op.from_ordering(ordering),
                None => false,
            })
    }

    /// 规范化的重写形态（`=1.2` → `==1.2`，多余空白去掉）。
    ///
    /// 只在需要展示或比较两条区间是否等价时使用；写回配置时保留作者原文，
    /// 免得用户保存一次就发现文件里每一行都被改写过。
    /// 区间里最高的「下限」版本（`>=` / `>` / `==`）；没有下限时返回 `None`。
    ///
    /// 覆盖表选择要用它排序「谁更具体」（见 `mods::select_release_relations`）：
    /// 两个都命中的区间里，下限更高的一条更贴合当前版本。
    /// 比较函数由调用方注入，模块自身仍然不依赖 crate 里的版本规则。
    pub fn lower_bound<F>(&self, compare: F) -> Option<&str>
    where
        F: Fn(&str, &str) -> Option<std::cmp::Ordering>,
    {
        let mut bound: Option<&str> = None;
        for clause in &self.clauses {
            if !matches!(clause.op, ClauseOp::Ge | ClauseOp::Gt | ClauseOp::Eq) {
                continue;
            }
            bound = Some(match bound {
                None => clause.version.as_str(),
                Some(current) => match compare(clause.version.as_str(), current) {
                    Some(std::cmp::Ordering::Greater) => clause.version.as_str(),
                    _ => current,
                },
            });
        }
        bound
    }

    /// 下限与上限是否自相矛盾（`>=2.0,<1.0`）：这种区间永远匹配不到版本。
    ///
    /// 配置校验用它拒绝「写反了的区间」——作者多半以为自己限定了范围，
    /// 实际却一条也命不中，界面上只会表现为依赖莫名失效。
    pub fn is_empty_interval<F>(&self, compare: F) -> bool
    where
        F: Fn(&str, &str) -> Option<std::cmp::Ordering>,
    {
        let mut lower: Option<&str> = None;
        let mut upper: Option<&str> = None;
        for clause in &self.clauses {
            match clause.op {
                ClauseOp::Ge | ClauseOp::Gt | ClauseOp::Eq => {
                    let candidate = clause.version.as_str();
                    lower = Some(match lower {
                        None => candidate,
                        Some(current) => match compare(candidate, current) {
                            Some(std::cmp::Ordering::Greater) => candidate,
                            _ => current,
                        },
                    });
                }
                ClauseOp::Le | ClauseOp::Lt => {
                    let candidate = clause.version.as_str();
                    upper = Some(match upper {
                        None => candidate,
                        Some(current) => match compare(candidate, current) {
                            Some(std::cmp::Ordering::Less) => candidate,
                            _ => current,
                        },
                    });
                }
                ClauseOp::NotEq => {}
            }
        }
        match (lower, upper) {
            (Some(low), Some(high)) => {
                matches!(compare(low, high), Some(std::cmp::Ordering::Greater))
            }
            _ => false,
        }
    }

    pub fn canonical(&self) -> String {
        if self.clauses.is_empty() {
            return String::new();
        }
        self.clauses
            .iter()
            .map(|clause| format!("{}{}", clause.op.as_str(), clause.version))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn parse_clause(clause: &str) -> Result<VersionClause, String> {
    // 先匹配两字符比较符，再匹配单字符，否则 `>=1.0` 会被拆成 `>` + `=1.0`
    for (token, op) in [
        (">=", ClauseOp::Ge),
        ("<=", ClauseOp::Le),
        ("!=", ClauseOp::NotEq),
        ("==", ClauseOp::Eq),
        (">", ClauseOp::Gt),
        ("<", ClauseOp::Lt),
        ("=", ClauseOp::Eq),
    ] {
        if let Some(rest) = clause.strip_prefix(token) {
            let version = rest.trim();
            validate_version_token(version)?;
            return Ok(VersionClause {
                op,
                version: version.to_owned(),
            });
        }
    }
    Err(format!(
        "missing comparator in `{clause}`, write `>={clause}` for a lower bound or `=={clause}` for an exact version"
    ))
}

/// 版本号本身是否长得像个版本号。
///
/// 只挡「明显写错」的情况（空、带空格、不含数字、不是以数字/v 开头），
/// 不做严格 semver 校验 —— 项目里合法存在 `0.10`、`1.8.7-beta1`、
/// `1.8.7.23` 这些写法，排序层本来就要宽容。
fn validate_version_token(token: &str) -> Result<(), String> {
    if token.is_empty() {
        return Err("missing version".into());
    }
    if token.chars().any(char::is_whitespace) {
        return Err(format!("version `{token}` contains whitespace"));
    }
    if token.starts_with("~=") {
        return Err(format!(
            "`~=` is not supported (`{token}`), write `>=x,<y` instead"
        ));
    }
    let body = token.strip_prefix(['v', 'V']).unwrap_or(token);
    if !body
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        return Err(format!("version `{token}` does not start with a digit"));
    }
    if !body.chars().any(|c| c.is_ascii_digit()) {
        return Err(format!("version `{token}` has no number"));
    }
    Ok(())
}

/// 把旧的两端写法折成新写法：`min_version` + `max_version` → `version`。
///
/// 老配置、老 Release note、老源文档里都是两端写法，必须继续读得懂；
/// 但写回时只写 `version`，一次保存就完成迁移。两端相等时给 `==`，
/// 只剩一端时给 `>=` / `<=`。
pub fn fold_bounds(min: &str, max: &str) -> String {
    let min = min.trim();
    let max = max.trim();
    match (min.is_empty(), max.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!(">={min}"),
        (true, false) => format!("<={max}"),
        (false, false) if min == max => format!("=={min}"),
        (false, false) => format!(">={min},<={max}"),
    }
}

#[cfg(test)]
#[path = "version_range/tests.rs"]
mod tests;
