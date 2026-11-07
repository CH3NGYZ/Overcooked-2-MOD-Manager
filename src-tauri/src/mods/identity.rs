use super::file_state::canonical_dll_name;
use crate::models::InstalledMod;
use std::cmp::Ordering;

pub(super) fn note_key_for_values(
    guid: Option<&str>,
    product_name: Option<&str>,
    dll_file: &str,
) -> String {
    if let Some(guid) = guid.filter(|value| !value.is_empty()) {
        return format!("guid:{}", guid.to_ascii_lowercase());
    }
    if let Some(name) = product_name.filter(|value| !value.is_empty()) {
        return format!("name:{}", name.to_ascii_lowercase());
    }
    format!("file:{}", dll_file.to_ascii_lowercase())
}

pub fn note_key(mod_info: &InstalledMod) -> String {
    note_key_for_values(
        mod_info.guid.as_deref(),
        mod_info.product_name.as_deref(),
        &mod_info.dll_file,
    )
}

pub(super) fn mark_duplicate_guids(mods: &mut [InstalledMod]) {
    for index in 0..mods.len() {
        if !mods[index].enabled {
            continue;
        }
        let peers = mods
            .iter()
            .enumerate()
            .filter(|(peer_index, item)| {
                *peer_index != index && item.enabled && mods_share_identity(&mods[index], item)
            })
            .map(|(peer_index, _)| peer_index)
            .collect::<Vec<_>>();
        if peers.is_empty() {
            continue;
        }
        log::warn!(
            "发现重复 MOD 身份：名称={}，GUID={}，重复数量={}",
            mods[index].name,
            mods[index].guid.as_deref().unwrap_or("未知"),
            peers.len()
        );
        mods[index].duplicate_guid = true;
        let duplicate_version = peers
            .iter()
            .any(|peer| mods_share_guid_and_version(&mods[index], &mods[*peer]));
        mods[index].duplicate_version = duplicate_version;
        let Some(current) = mods[index].version.as_deref() else {
            continue;
        };
        let has_higher = peers.iter().any(|peer| {
            mods[*peer]
                .version
                .as_deref()
                .and_then(|higher| compare_mod_versions(higher, current))
                == Some(Ordering::Greater)
        });
        if has_higher {
            log::warn!(
                "重复 MOD 中存在更高版本：名称={}，当前版本={}",
                mods[index].name,
                current
            );
            mods[index].load_warning = Some("mods.higherVersionPresent".into());
            // BepInEx loads the higher local copy, so the shadowed DLL is a
            // cleanup candidate rather than another pending online update.
            mods[index].update_available = false;
            if mods[index].status != "not_release_asset" {
                mods[index].status = "duplicate_lower_version".into();
            }
        }
    }
}

fn mods_share_guid_and_version(left: &InstalledMod, right: &InstalledMod) -> bool {
    let same_guid = left
        .guid
        .as_deref()
        .zip(right.guid.as_deref())
        .is_some_and(|(left, right)| {
            !left.trim().is_empty()
                && !right.trim().is_empty()
                && left.trim().eq_ignore_ascii_case(right.trim())
        });
    let same_version = left
        .version
        .as_deref()
        .zip(right.version.as_deref())
        .and_then(|(left, right)| compare_mod_versions(left, right))
        == Some(Ordering::Equal);
    same_guid && same_version
}

fn mods_share_identity(left: &InstalledMod, right: &InstalledMod) -> bool {
    let same_guid = left
        .guid
        .as_deref()
        .zip(right.guid.as_deref())
        .is_some_and(|(left, right)| left.trim().eq_ignore_ascii_case(right.trim()));
    let same_name = {
        let left = left.name.trim();
        let right = right.name.trim();
        !left.is_empty() && !right.is_empty() && left.eq_ignore_ascii_case(right)
    };
    same_guid || same_name
}

pub(crate) fn is_safe_replacement_mod(target: &InstalledMod, peer: &InstalledMod) -> bool {
    let target_path = canonical_dll_name(&target.dll_file.replace('\\', "/"));
    let peer_path = canonical_dll_name(&peer.dll_file.replace('\\', "/"));
    !target_path.eq_ignore_ascii_case(&peer_path)
        && peer.enabled
        && peer.load_warning.is_none()
        && target
            .guid
            .as_deref()
            .zip(peer.guid.as_deref())
            .is_some_and(|(target, peer)| {
                !target.trim().is_empty() && target.trim().eq_ignore_ascii_case(peer.trim())
            })
        && target
            .version
            .as_deref()
            .zip(peer.version.as_deref())
            .is_some_and(|(target, peer)| {
                compare_mod_versions(peer, target).is_some_and(|order| order != Ordering::Less)
            })
}

/// 比较两个**本地** DLL 的版本号，用于同 GUID 副本去重。
///
/// 这里刻意用「纯数字段逐段比较」，**不是** semver 语义 —— 与
/// `releases::compare_release_versions`（线上更新判定）是两套规则，
/// 不要试图统一。
///
/// 为什么必须不同：BepInEx 加载同 GUID 的多份 DLL 时，比的是程序集版本号
/// 的**数字大小**，`1.9.8.1`（四段）严格大于 `1.9.8`。如果这里改用 semver
/// （beta 低于同号正式版），管理器会把 `1.9.8` 判为更高并建议卸载
/// `1.9.8.1` —— 而实际被游戏加载的恰恰是 `1.9.8.1`，建议卸载正在生效的
/// 文件是最坏的结果。
///
/// 两者各司其职：
///   · 本地副本谁在生效 → 纯数字段比较（跟随 BepInEx）
///   · 线上是否有新版   → semver（`1.9.8` 确实是 `1.9.8-beta.1` 的升级）
///
/// `mod_preview::find_install_conflicts` 判断「本次要装的版本与本地已装是否
/// 相同」也用它：同一份 DLL 换不换版本，该跟 BepInEx 的判定一致。
pub(crate) fn compare_mod_versions(left: &str, right: &str) -> Option<Ordering> {
    Some(parse_mod_version_components(left)?.cmp(&parse_mod_version_components(right)?))
}

fn parse_mod_version_components(value: &str) -> Option<[u64; 4]> {
    let numeric = value
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()?;
    let parts = numeric.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut components = [0_u64; 4];
    for (index, part) in parts.into_iter().enumerate() {
        components[index] = part.parse().ok()?;
    }
    Some(components)
}
