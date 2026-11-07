use regex::Regex;

/// 版本号解析：把各种宽写 tag 归一到三段 semver。
///
///   - 不足三段补 0：`0.4` → `0.4.0`；
///   - 第 4 段及以后丢弃（semver 只能表达三段）：
///     `0.4.0.5`、`0.4.0.0` → `0.4.0`；
///   - 保留 `-`/`+` 后缀作为预发布/构建元数据。
///
/// **MOD 更新判定不要用这个函数的返回值**，用 `compare_release_versions`：
/// 后者保留全部数字段（本项目有仓库靠第 4 段发版）。
pub fn parse_version(tag: &str) -> Option<semver::Version> {
    let cleaned = tag.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(cleaned).ok().or_else(|| {
        let re = Regex::new(r"(\d+(?:\.\d+){1,7}(?:[-+][0-9A-Za-z.-]+)?)").ok()?;
        let value = re.captures(cleaned)?.get(1)?.as_str();
        let numeric = value.split(['-', '+']).next()?;
        let mut parts = numeric
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let suffix = value.get(numeric.len()..).unwrap_or_default();
        parts.truncate(3);
        let normalized = match parts.as_slice() {
            [major, minor] => format!("{major}.{minor}.0{suffix}"),
            [major, minor, patch] => format!("{major}.{minor}.{patch}{suffix}"),
            _ => return None,
        };
        semver::Version::parse(&normalized).ok()
    })
}

/// 保留 DLL 四段号与 Release beta tag 的等价关系，而非只比较显示用三段号。
/// 前三段缺位补零；其余非全零数字段并入预发布序列，
/// 因此 1.8.7 > 1.8.7.23 > 1.8.7.21，尾部全零仍等于正式版。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SegmentVersion {
    numbers: Vec<u64>,
    prerelease: Vec<String>,
}

/// 把版本号尾部的「非数字部分」归一成预发布标识。
///
/// 保留 pre/beta 等标识符差异，同时识别紧凑写法：
/// ```text
/// 1.8.5-pre.01      → [pre, 01]
/// 1.9.7-beta.4      → [beta, 4]
/// 1.8.7-beta1       → [beta, 1]
/// 1.8.7.beta4       → [beta, 4]
/// ```
/// 这里也把 `+` 后的内容并入预发布，不同于 semver 标准忽略构建元数据的优先级。
///
/// 调用方已用贪婪数字正则吃掉了所有前导数字段（`1.8.7.23` 的 `23` 属于
/// 数字段，不在 rest 里），所以这里剩下的内容整体都是标识符 ——
/// 其中的纯数字段（`-beta.4` 的 `4`）是 semver 的预发布序号，必须保留。
///
/// 空结果表示正式版。
fn suffix_identifiers(rest: &str) -> Vec<String> {
    rest.trim_start_matches(['-', '+', '.'])
        .split(['-', '+', '.'])
        .filter(|part| !part.is_empty())
        .flat_map(split_alpha_numeric)
        .collect()
}

/// 把 `beta1` 这类「字母 + 数字」的标识符拆成两段（`beta`、`1`）。
///
/// 语义上等同于 `beta.1`，让序号按数值比较。
///
/// 为什么必须拆：严格按 semver，`-beta1` 是**单个**字母数字标识符，
/// 按 ASCII 字典序比较，于是 `beta10 < beta9`（`'1' < '9'`）——
/// 发到第 10 个测试版时更新检测会静默失效。规范自己举的例子是
/// `-beta.2 < -beta.11`，就是因为点号分隔才能按数值比较。
///
/// 本项目实际用 `-beta1` 这种紧凑写法，所以在解析层统一拆开，
/// 让两种写法得到同一结论。这是对规范的**有意放宽**，不是误读。
fn split_alpha_numeric(part: &str) -> Vec<String> {
    let split_at = part.find(|c: char| c.is_ascii_digit());
    let Some(index) = split_at else {
        return vec![part.to_owned()];
    };
    let (head, tail) = part.split_at(index);
    let valid = !head.is_empty()
        && head.chars().all(|c| c.is_ascii_alphabetic())
        && tail.chars().all(|c| c.is_ascii_digit());
    if !valid {
        return vec![part.to_owned()];
    }
    // 去掉前导零：semver 禁止它们，但真实仓库出现过 `pre.01`
    let serial = tail.trim_start_matches('0');
    vec![
        head.to_owned(),
        if serial.is_empty() { "0" } else { serial }.to_owned(),
    ]
}

impl SegmentVersion {
    fn parse(value: &str) -> Option<Self> {
        let cleaned = value.trim().trim_start_matches(['v', 'V']);
        /*
         * 先切出「前导数字段」和「其余部分」。
         *
         * 用 `^(\d+(\.\d+)*)` 而不是找第一个 `-`：本项目的 beta 写法不统一，
         * 除了标准 semver 的 `-beta.4`，还有 `1.8.7-beta1`（无分隔符）
         * 和 `1.8.7.beta4`（点号分隔）—— 后者用旧的 `-` 正则会把整个号
         * 解析失败，于是它既不参与比较也判不出预发布。
         */
        let re = Regex::new(r"^(\d+(?:\.\d+)*)(.*)$").ok()?;
        let caps = re.captures(cleaned)?;
        let parts = caps
            .get(1)?
            .as_str()
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let numbers = parts.iter().take(3).copied().collect::<Vec<_>>();
        let suffix = suffix_identifiers(caps.get(2).map_or("", |value| value.as_str()));
        /*
         * 第 4 段及以后是前三位的 beta 序号：`1.8.7.23` ≡ `1.8.7-beta.23`。
         * 全 0 视为正式版（1.8.7.0 与 1.8.7 相同）；有非 0 段则拼进预发布标识，
         * 数字段参与 semver 的预发布比较，所以 beta 23 > beta 21。
         */
        let build = parts.iter().skip(3).map(u64::to_string).collect::<Vec<_>>();
        let has_build = build.iter().any(|part| part != "0");
        let prerelease = if !suffix.is_empty() {
            let mut merged = suffix;
            if has_build {
                merged.extend(build);
            }
            merged
        } else if has_build {
            let mut merged = vec!["beta".to_string()];
            merged.extend(build);
            merged
        } else {
            Vec::new()
        };
        Some(Self {
            numbers,
            prerelease,
        })
    }
}

impl Ord for SegmentVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        // 三段比较，缺的段按 0：0.4 == 0.4.0
        let length = self.numbers.len().max(other.numbers.len());
        for index in 0..length {
            let left = self.numbers.get(index).copied().unwrap_or(0);
            let right = other.numbers.get(index).copied().unwrap_or(0);
            match left.cmp(&right) {
                Ordering::Equal => continue,
                ordering => return ordering,
            }
        }
        // 数字段相同：正式版高于预发布；两个预发布按标识符逐段比较
        // （第 4 段已并入预发布，所以 beta.23 > beta.21 在这里判定）
        match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => compare_prerelease_parts(&self.prerelease, &other.prerelease),
        }
    }
}

impl PartialOrd for SegmentVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease_parts(left: &[String], right: &[String]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let count = left.len().max(right.len());
    for index in 0..count {
        match (left.get(index), right.get(index)) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => {
                let ordering = match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    // 数字标识符低于字母标识符
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => a.cmp(b),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
    Ordering::Equal
}

/// 比较两个 Release 标签/版本号，用于判断 MOD 是否有更新。
///
/// 使用历史兼容比较：前三段核心、额外非零数字段映射 beta、紧凑标识拆分。
/// 不等同于 semver 库排序，也不等同于本地重复 DLL 的四整数比较。
/// 任一无法解析时返回 `None`，调用方应显示「版本未知」而不是臆断更新。
pub fn compare_release_versions(remote: &str, local: &str) -> Option<std::cmp::Ordering> {
    Some(SegmentVersion::parse(remote)?.cmp(&SegmentVersion::parse(local)?))
}

/// 检查本项目三字段发布语法（`x.y.z[-tag.w][+build]`，允许单个 v/V 前缀）。
/// 构建字段沿用同一标识检查，会拒绝纯数字前导零，比 semver 2.0.0 更严格。
///
/// **这是发布契约，不是解析门槛。** 解析器（`SegmentVersion::parse`）继续
/// 宽容接受历史写法（四段序号、`v0.10` 这类两段号、点号分隔的 `.beta4`），
/// 因为已经发出去的 tag 不会改变，拒绝它们只会让老用户看到「版本未知」。
/// 这个函数回答的是「新发布是否合规」：
///
/// ```text
/// 1.9.8              ✅
/// 1.9.8-beta.1       ✅   首选：beta / alpha / rc 统一写成 x.y.z-tag.w
/// 1.9.8-rc.2         ✅
/// 1.9.8-beta1        ⚠️   语法合法（单个字母数字标识符），但不推荐
/// 1.9.8.1            ❌   四段是历史写法，semver 只允许三段
/// 1.9.8.beta.1       ❌   预发布必须跟在 `-` 后
/// v0.10              ❌   必须三段
/// 01.9.8             ❌   数字段不许前导零
/// ```
///
/// `-beta1` 为什么标 ⚠️ 而不是 ❌：它在语法上完全合法，但作为**单个**
/// 标识符要按 ASCII 字典序比较（`beta9 > beta10`）。本项目在解析层把
/// 「字母+数字」拆开规避了这个问题，但新发布统一用 `-beta.1` 更清楚，
/// 也与规范 §11 自己的示例一致（`1.0.0-beta.2 < 1.0.0-beta.11`）。
///
/// 三条硬性规则：
///   1. 恰好三段，每段无前导零；
///   2. 预发布用 `-` 分隔，标识符逐段点号分隔，数字段无前导零；
///   3. 标识符只含 `[0-9A-Za-z-]`。
pub fn is_strict_semver(value: &str) -> bool {
    let trimmed = value.trim();
    // 允许常见的 `v` 前缀（GitHub/Gitee tag 惯例），但版本本身仍须严格
    let text = trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed);
    // 此处只检查格式；历史比较器如何处理 + 由 SegmentVersion 决定。
    let (without_build, build) = match text.split_once('+') {
        Some((core, build)) => (core, Some(build)),
        None => (text, None),
    };
    if let Some(build) = build {
        if !is_valid_identifier_sequence(build) {
            return false;
        }
    }
    let (core, prerelease) = match without_build.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (without_build, None),
    };
    // 恰好三段
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| !is_valid_numeric_segment(part)) {
        return false;
    }
    // 预发布：至少一段合法标识符
    match prerelease {
        None => true,
        Some(pre) => {
            !pre.is_empty()
                && pre
                    .split('.')
                    .all(|part| !part.is_empty() && is_valid_identifier(part))
        }
    }
}

/// 数字段：纯数字且无前导零（`0` 本身允许）。
fn is_valid_numeric_segment(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|c| c.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

/// 预发布标识符：纯数字时同样不许前导零；含字母时只允许字母数字与连字符。
fn is_valid_identifier(value: &str) -> bool {
    let numeric = value.chars().all(|c| c.is_ascii_digit());
    if numeric {
        return is_valid_numeric_segment(value);
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn is_valid_identifier_sequence(value: &str) -> bool {
    !value.is_empty()
        && value
            .split('.')
            .all(|part| !part.is_empty() && is_valid_identifier(part))
}

/// 版本号本身是否带预发布标识（`-beta`、`-rc.1` 等）。
///
/// 与 Release 元数据里的 `prerelease` 标记是**或**的关系：作者没勾标记，
/// 但版本叫 `0.4.0-beta`、`0.4.0.5-beta` 时同样按预发布对待。
///
/// 四段版本按 semver 的 beta 序号处理：`1.8.7.23` ≡ `1.8.7-beta.23`，
/// 因此**是预发布**。这条规则必须全局一致，否则同一版本在比较、
/// 渠道筛选和界面提示里会得到三种答案。
pub fn is_prerelease_version(version: &str) -> bool {
    SegmentVersion::parse(version).is_some_and(|parsed| !parsed.prerelease.is_empty())
}

#[cfg(test)]
mod tests;
