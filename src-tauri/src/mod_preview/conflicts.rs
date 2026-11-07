use super::model::{ModPreview, ModPreviewEntry};
use crate::app_state::AppState;
use crate::models::{DllRole, InstallConflict, InstalledMod};
use crate::mods::compare_mod_versions;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;

/// 找出「这次要装的 MOD」与本地已装副本的身份冲突。
///
/// 只报告、不卸载：卸载由前端确认后逐个走 `uninstall_local_mod` 的整包语义
/// （它按安装清单回收整包），安装事务里不掺卸载。同一个 MOD 不管版本相同、
/// 升级还是降级都算同一个东西 —— 并存只会得到两份互相顶掉的 DLL。
///
/// 匹配规则（唯一实现，前端不要另判）：
///   · 只看 `role == DllRole::Mod` 的条目：纯依赖不算冲突源，依赖是随包带着
///     的 DLL，装成依赖不会顶掉本地同一个插件；
///   · 两边都有 GUID 时只比 GUID：它是 BepInPlugin 的稳定身份，不同作者可能
///     起同名 DLL，但不会共用 GUID；
///   · 任一方缺 GUID（读不出元数据）才退回比名字，忽略大小写与首尾空白；
///     两边名字都空时不算命中，否则「读不出元数据」会把所有同类包都判成冲突。
///
/// 命中项按安装包去重：同一个包里的多个 DLL 属于同一次卸载（`uninstall_local_mod`
/// 是整包语义），每组只报一条，`handle` 取该组第一份命中的 `dll_file`。
/// 顺序沿用 `installed` 的顺序，不排序 —— 界面按列表顺序展示才与 MOD 管理页一致。
pub fn find_install_conflicts(
    installed: &[InstalledMod],
    entries: &[ModPreviewEntry],
) -> Vec<InstallConflict> {
    let candidates: Vec<&ModPreviewEntry> = entries
        .iter()
        .filter(|entry| entry.role == DllRole::Mod)
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    let mut groups: HashSet<String> = HashSet::new();
    let mut conflicts = Vec::new();
    for existing in installed {
        let Some(entry) = candidates
            .iter()
            .find(|entry| same_plugin(entry, existing))
            .copied()
        else {
            continue;
        };
        // 包 ID 是「同一次卸载」的粒度；没有包 ID（单 MOD）才按 DLL 分组。
        let group = trimmed(existing.package_id.as_deref())
            .map(str::to_string)
            .unwrap_or_else(|| existing.dll_file.clone());
        if !groups.insert(group) {
            continue;
        }
        conflicts.push(InstallConflict {
            name: conflict_name(existing),
            version: existing.version.clone().unwrap_or_default(),
            handle: existing.dll_file.clone(),
            enabled: existing.enabled,
            same_version: same_version(entry.version.as_deref(), existing.version.as_deref()),
        });
    }
    conflicts
}

/// 给预览补上冲突列表（安装入口在 `build_preview` 之后、`stage_local` 之前调）。
///
/// 探测不到游戏目录时留空：没装游戏就没有已装副本。扫描走
/// `mods::scan_mods_with_manifests`（与 MOD 管理页同一份），所以「包内副本」
/// 这类整包视角与界面显示的完全一致。
pub(super) fn attach_install_conflicts(state: &AppState, preview: &mut ModPreview) {
    let config = state.config.load_or_default();
    let Some(game_root) = crate::game::detect_game(&config).path else {
        return;
    };
    let installed = crate::mods::scan_mods_with_manifests(
        Path::new(&game_root),
        &config,
        Some(&state.config.manifests_dir()),
    );
    preview.conflicts = find_install_conflicts(&installed, &preview.entries);
}

/// 预览条目与本地已装 MOD 是否指同一个插件。
fn same_plugin(entry: &ModPreviewEntry, installed: &InstalledMod) -> bool {
    if let (Some(entry_guid), Some(installed_guid)) = (
        trimmed(entry.guid.as_deref()),
        trimmed(installed.guid.as_deref()),
    ) {
        return entry_guid.eq_ignore_ascii_case(installed_guid);
    }
    let Some(entry_name) = trimmed(entry.name.as_deref()) else {
        return false;
    };
    let installed_name = installed.name.trim();
    !installed_name.is_empty() && entry_name.eq_ignore_ascii_case(installed_name)
}

/// 冲突行的展示名：包名优先（一个包里多个 DLL 时用户认包），缺省回落插件名。
fn conflict_name(installed: &InstalledMod) -> String {
    trimmed(installed.package_name.as_deref())
        .unwrap_or(installed.name.trim())
        .to_string()
}

/// 本次要装的版本与本地已装是否相同。
///
/// 两边都有版本号时先按 BepInEx 的数字段比较（`mods::compare_mod_versions`，
/// 与「本地谁在生效」同一把尺子）；读不出数字段说明作者写的不是版本号，
/// 退回原文比较。缺失方按空串参与比较 —— 两边都读不出时算相同。
pub(super) fn same_version(target: Option<&str>, local: Option<&str>) -> bool {
    let target = target.unwrap_or_default().trim();
    let local = local.unwrap_or_default().trim();
    if target.is_empty() || local.is_empty() {
        return target == local;
    }
    match compare_mod_versions(target, local) {
        Some(ordering) => ordering == Ordering::Equal,
        None => target == local,
    }
}

/// `Option<&str>` 去掉首尾空白后的非空值；空白字符串等同于「没有」。
pub(super) fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
