//! ZIP 与拍平共用的随包插件归集规则。

use super::zip_info::split_entry;
use crate::asset_bundle::LevelSetMetadata;
use std::collections::BTreeMap;

/// 目录名是否形如 `levels`（容忍 `levels包`、`LEVELS2` 这类作者写法）。
fn is_levels_dir(name: &str) -> bool {
    name.to_lowercase().starts_with("levels")
}

/// 目录名是否形如 `plugins`（容忍 `plugins包`、`Plugins`）。
fn is_plugins_dir(name: &str) -> bool {
    name.to_lowercase().starts_with("plugins")
}

/// 一个关卡集所属的「包单位」根：作者把地图与插件打在一起的那层目录。
///
/// 实测真实包都长这样：
/// ```text
/// jonny职业鼠鼠的图/
///   ├─ levels包/jonnylevelset3.4.1/{info_jonny, s_jonny_*}
///   └─ plugins包/Jonny_dll/{JonnyLevelLoaderPlugin.dll, JonnyLevelScripts.dll}
/// ```
/// 规则：从集目录往上找第一层 `levels*`，它的父目录就是单位根；路径里没有
/// `levels*`（ZIP 根下直接就是集目录）时退回「集目录的父目录」。
pub(super) fn package_unit_root(directory: &str) -> String {
    let parts: Vec<&str> = directory
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    if let Some(index) = (0..parts.len().saturating_sub(1))
        .rev()
        .find(|index| is_levels_dir(parts[*index]))
    {
        return parts[..index].join("/");
    }
    parts[..parts.len() - 1].join("/")
}

/// 一个 DLL 条目所属的「包单位」根：它上方最近一层 `plugins*` 的父目录。
///
/// 这是**第二级**归属规则，只处理「插件与地图分开打包」的情况 —— 与 `info`
/// 同父目录的 DLL 在 [`assign_plugins`] 里已被第一级规则判掉。既不在
/// `plugins*` 子树、也不在集目录里的 DLL 归属无从判断，猜错就会把无关插件
/// 塞进某张地图，宁可不认。
pub(super) fn plugin_unit_root(entry: &str) -> Option<String> {
    let parts: Vec<&str> = entry.split('/').filter(|part| !part.is_empty()).collect();
    // 最后一段是文件名
    let dirs = &parts[..parts.len().saturating_sub(1)];
    let index = (0..dirs.len())
        .rev()
        .find(|index| is_plugins_dir(dirs[*index]))?;
    Some(dirs[..index].join("/"))
}

/// 归一化名字：小写 + 只留字母数字（CJK 也算字母，作者的中文目录名因此保留）。
fn normalize_name(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// 两个归一化名字的公共前缀长度（字符数）。
fn common_prefix_len(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count()
}

/// 公共前缀达到这个长度才认为「DLL 与这张地图有关」。
///
/// 4 是实测折中：`shz` 只有 3 个字符，但 `SHZTargetDrop.dll` 的父目录
/// `shzhogwafflesdll` 与集名共享 13 个字符，仍能命中；而 3 个字符（`mod`、
/// `fix`）几乎任何包都会误配。
const PLUGIN_NAME_MATCH_MIN: usize = 4;

/// 一个插件该归哪个关卡集。`member` 必须非空且已按目录名排序。
///
/// 只有一张集时直接归它（实测三个真实包都是这种）；多集时按名字相关性挑，
/// 挑不出来就归单位内排序第一张 —— 不复制给多张，同一个 DLL 落两份会让
/// BepInEx 加载两次。
pub(super) fn pick_level_set_for_plugin(
    entry: &str,
    sets: &[(String, LevelSetMetadata, String)],
    member: &[usize],
) -> usize {
    if member.len() <= 1 {
        return member[0];
    }
    let (directory, file_name) = split_entry(entry);
    let stem = file_name
        .rsplit_once('.')
        .map(|(head, _)| head.to_string())
        .unwrap_or_else(|| file_name.clone());
    let folder = directory.rsplit('/').next().unwrap_or("").to_string();
    let probes = [normalize_name(&stem), normalize_name(&folder)];
    let mut scores: Vec<(usize, usize)> = member
        .iter()
        .map(|index| {
            let (set_directory, metadata, _) = &sets[*index];
            let set_folder = set_directory.rsplit('/').next().unwrap_or("");
            let names = [
                normalize_name(set_folder),
                normalize_name(&metadata.name),
                normalize_name(metadata.name_zh.as_deref().unwrap_or("")),
            ];
            let score = probes
                .iter()
                .flat_map(|probe| names.iter().map(move |name| common_prefix_len(probe, name)))
                .max()
                .unwrap_or(0);
            (*index, score)
        })
        .collect();
    let best = scores.iter().map(|(_, score)| *score).max().unwrap_or(0);
    if best >= PLUGIN_NAME_MATCH_MIN {
        let winners: Vec<usize> = scores
            .iter()
            .filter(|(_, score)| *score == best)
            .map(|(index, _)| *index)
            .collect();
        if winners.len() == 1 {
            return winners[0];
        }
    }
    // 挑不出来：按目录名排序的第一张（`member` 已排序，结果不随 ZIP 条目顺序变）
    scores.sort_by_key(|(index, _)| *index);
    member[0]
}

/// 把 ZIP 里的插件归到各关卡集的目录：`集目录 → 归属的 DLL 条目名`。
///
/// 归过去之后它们会随 `files` 一起落进 `<levels>/<集名>/`，于是「启用/停用
/// 整目录 rename」天然把作者的修复插件一起开关 —— 这正是「地图和 DLL 一起搬」。
pub(super) fn assign_plugins(
    all_entries: &[String],
    sets: &[(String, LevelSetMetadata, String)],
) -> BTreeMap<String, Vec<String>> {
    let mut owners: BTreeMap<String, Vec<String>> = BTreeMap::new();

    /*
     * 第一级（确定绑定）：DLL 与某张地图集的 `info` 同父目录，直接归它。
     *
     * 这是「地图 + 自己的修复 DLL」最直接的写法，也是导入后的磁盘形态（插件
     * 落在集目录根）—— 不猜名字、不打分，看到就绑。导出地图包时整目录打包，
     * 重新导入还会命中同一条规则，所以绑定关系不需要额外存一份。
     */
    let mut unresolved: Vec<&String> = Vec::new();
    for entry in all_entries {
        if !entry.to_lowercase().ends_with(".dll") {
            continue;
        }
        let (directory, _) = split_entry(entry);
        match sets
            .iter()
            .position(|(set_directory, _, _)| set_directory == &directory)
        {
            Some(index) => owners
                .entry(sets[index].0.clone())
                .or_default()
                .push(entry.clone()),
            None => unresolved.push(entry),
        }
    }

    /*
     * 第二级（推断绑定）：DLL 在 `plugins*` 子树里，按包单位归集再按名字打分。
     * 作者把插件和地图分开打包时的兜底，猜不出来就归单位内排序第一张。
     */
    // 单位根 → 该单位下的集
    let mut units: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, (directory, _, _)) in sets.iter().enumerate() {
        units
            .entry(package_unit_root(directory))
            .or_default()
            .push(index);
    }
    // 单位根 → 该单位下的 DLL
    let mut pending: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in unresolved {
        if let Some(unit) = plugin_unit_root(entry) {
            pending.entry(unit).or_default().push(entry.clone());
        }
    }

    for (unit, entries) in pending {
        let mut member: Vec<usize> = units.get(&unit).cloned().unwrap_or_default();
        if member.is_empty() {
            // 单位根对不上（集目录与 plugins 不在同一层包装里）：退化成
            // 「集目录在 unit 之下」也算成员
            member = sets
                .iter()
                .enumerate()
                .filter(|(_, (directory, _, _))| {
                    unit.is_empty()
                        || directory == &unit
                        || directory.starts_with(&format!("{unit}/"))
                })
                .map(|(index, _)| index)
                .collect();
        }
        if member.is_empty() {
            log::warn!("ZIP 里的插件找不到所属关卡集，已忽略：单位={}", unit);
            continue;
        }
        member.sort_by(|left, right| sets[*left].0.cmp(&sets[*right].0));
        for entry in entries {
            let target = pick_level_set_for_plugin(&entry, sets, &member);
            owners
                .entry(sets[target].0.clone())
                .or_default()
                .push(entry);
        }
    }
    for list in owners.values_mut() {
        list.sort();
        list.dedup();
    }
    owners
}
