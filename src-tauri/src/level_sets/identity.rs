//! 地图身份与目录名消歧规则。

use crate::asset_bundle::LevelSetMetadata;

/// 两个关卡集是否是同一张地图。
///
/// 身份由三段构成：`levelSetName`（英文集名）+ `levelSetNameZH`（中文名）+
/// `author`。只用前两段不够 —— 实测「胡闹糕中」与「胡闹达学」是同一个作者的
/// 两张不同地图，英文集名与作者都写成 `GAODA / GAODA`，只有中文名不同。
///
/// 比较规则：
/// - 英文集名：trim 后忽略大小写，**空名永不匹配**（空名说明 `info` 没解析
///   出来，不能把所有坏目录都算成同一张图）。
/// - 作者：trim 后忽略大小写，空对空算相同（官方导出的 `info` 常不写作者）。
/// - 中文名：**两边都非空才比较**。地图源 TOML 可以只写英文集名与作者（老源
///   都没有中文名），此时退回前两段的判定，与加入中文名之前的行为一致。
pub fn level_set_identity_matches(
    level_set_name: &str,
    name_zh: Option<&str>,
    author: &str,
    other_level_set_name: &str,
    other_name_zh: Option<&str>,
    other_author: &str,
) -> bool {
    let name = level_set_name.trim();
    let other = other_level_set_name.trim();
    if name.is_empty() || other.is_empty() {
        return false;
    }
    if !name.eq_ignore_ascii_case(other) {
        return false;
    }
    if !author.trim().eq_ignore_ascii_case(other_author.trim()) {
        return false;
    }
    match (
        name_zh.map(str::trim).filter(|value| !value.is_empty()),
        other_name_zh
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    ) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => true,
    }
}

/// 两张关卡集是否同一张图：`levelSetName + levelSetNameZH + author`。
pub(super) fn same_level_set(left: &LevelSetMetadata, right: &LevelSetMetadata) -> bool {
    level_set_identity_matches(
        &left.name,
        left.name_zh.as_deref(),
        &left.author,
        &right.name,
        right.name_zh.as_deref(),
        &right.author,
    )
}

/// 冲突行展示名：英文集名优先（界面其余地方也按它显示），空则中文名。
pub(super) fn level_set_display_name(metadata: &LevelSetMetadata) -> String {
    let name = metadata.name.trim();
    if name.is_empty() {
        metadata.name_zh.clone().unwrap_or_default()
    } else {
        metadata.name.clone()
    }
}

/// 把集名变成合法目录名。
///
/// 去掉 Windows 不允许的字符与空白；结果为空时返回 `None`，由调用方换用
/// 下一个候选（ZIP 目录名 / 报错），不静默落一个空名字。
pub(super) fn sanitize_dir_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// 单个目录名的字符数上限。
///
/// 关卡集目录位于 `<游戏>/BepInEx/plugins/OC2DIYLevel/levels/` 下，Windows
/// 默认 260 字符的路径上限还要留给里面的文件名（Unity 的 `.resS` 常与主文件同名同长）。
/// 正常集名远低于这个值，这里只拦病态长名。
pub(super) const MAX_DIR_NAME_CHARS: usize = 48;

/// 给关卡集挑一个在本批里不重名的目录名。
///
/// 只用英文集名不够：不同地图可能共用同一个集名（实测「胡闹糕中」与
/// 「胡闹达学」都写 `GAODA`，作者也都是 `GAODA`）。撞车时按可读性依次追加
/// 中文名、作者、uid 前缀，最后才退到数字后缀。
///
/// 中文名排第一是因为它区分度最高且**不随版本变化** —— 同一张图升级时目录名
/// 保持不变，回收覆盖才能命中同一处。`uid` 是 `MD5(baseUID + version)`，每发
/// 一版就变，只能当最后的兜底。
///
/// `allow_base` 为 `false` 时跳过基础名本身。同一批里有多张图共用这个基础名
/// 时，调用方会对**每一张**都置 `false`，让它们一起带后缀，而不是「第一个
/// 占住干净名字、其余加后缀」—— 后者会让目录名取决于条目顺序。
pub(super) fn allocate_dir_name(
    base: &str,
    metadata: &LevelSetMetadata,
    taken: &mut std::collections::HashSet<String>,
    allow_base: bool,
) -> String {
    let shorten = |value: &str| -> Option<String> {
        let cleaned = sanitize_dir_name(value)?;
        let shortened: String = cleaned.chars().take(MAX_DIR_NAME_CHARS).collect();
        let shortened = shortened.trim_matches(['.', ' ']).to_string();
        if shortened.is_empty() {
            None
        } else {
            Some(shortened)
        }
    };

    let base = shorten(base).unwrap_or_else(|| "LevelSet".to_string());
    let mut candidates: Vec<String> = Vec::new();
    if allow_base {
        candidates.push(base.clone());
    }
    if let Some(name_zh) = metadata.name_zh.as_deref().and_then(shorten) {
        candidates.push(format!("{base}_{name_zh}"));
    }
    if let Some(author) = shorten(&metadata.author) {
        candidates.push(format!("{base}_{author}"));
    }
    let uid: String = metadata.uid.chars().take(8).collect();
    if !uid.trim().is_empty() {
        candidates.push(format!("{base}_{uid}"));
    }

    /*
     * 按小写比较：Windows 与 macOS 的默认文件系统都不区分大小写，只比原串会
     * 放过 `GAODA` 与 `gaoda` 这种「看起来不同、实际撞同一个目录」的组合。
     */
    for candidate in candidates {
        if taken.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    for suffix in 2u32.. {
        let candidate = format!("{base}_{suffix}");
        if taken.insert(candidate.to_lowercase()) {
            return candidate;
        }
    }
    unreachable!("数字后缀必然能找到一个空位")
}
