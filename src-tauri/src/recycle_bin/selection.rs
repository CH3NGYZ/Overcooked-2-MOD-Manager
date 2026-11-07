use super::errors::INVALID_RECORD;
use super::errors::NO_FILES;
use super::errors::io_error;
use super::manifests::all_active_manifests;
use super::manifests::read_active_manifest;
use super::paths::checked_path;
use super::paths::existing_variants;
use super::paths::file_key;
use super::paths::is_link;
use super::paths::normalize_relative;
use super::records::RelatedManifest;
use super::staging::stage;
use super::staging::stage_with_related_manifests;
use crate::archive;
use crate::models::ModRecycleEntry;
use crate::mods;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// None selects the full package; Some selects only its canonical DLL.
pub fn recycle_package(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: Option<&str>,
) -> Result<ModRecycleEntry, String> {
    recycle_package_with_mods(
        root,
        manifests_dir,
        game_root,
        package_id,
        dll,
        None,
        None,
        &[],
        &[],
    )?
    .ok_or_else(|| NO_FILES.into())
}

pub fn recycle_mod_dll(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: &str,
    installed: &[crate::models::InstalledMod],
) -> Result<ModRecycleEntry, String> {
    recycle_package_with_mods(
        root,
        manifests_dir,
        game_root,
        package_id,
        Some(dll),
        Some(installed),
        None,
        &[],
        &[],
    )?
    .ok_or_else(|| NO_FILES.into())
}

/// A local delete explicitly selects this DLL, even when recycling its package.
pub fn recycle_selected_mod(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: &str,
    whole_package: bool,
) -> Result<ModRecycleEntry, String> {
    recycle_package_with_mods(
        root,
        manifests_dir,
        game_root,
        package_id,
        (!whole_package).then_some(dll),
        None,
        Some(dll),
        &[],
        &[],
    )?
    .ok_or_else(|| NO_FILES.into())
}

fn has_safe_replacement(
    game_root: &Path,
    installed: &[crate::models::InstalledMod],
    dll: &str,
) -> Result<bool, String> {
    let targets = installed
        .iter()
        .filter(|item| file_key(&item.dll_file) == file_key(dll))
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(false);
    }
    // Both enabled and disabled variants can exist at the selected path.
    // A replacement must safely cover every variant that will be staged.
    for peer in installed.iter().filter(|peer| {
        targets
            .iter()
            .all(|target| mods::is_safe_replacement_mod(target, peer))
    }) {
        let path = checked_path(game_root, &mods::canonical_dll_name(&peer.dll_file))?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !is_link(&metadata) => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn recycle_package_with_mods(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    package_id: &str,
    dll: Option<&str>,
    installed: Option<&[crate::models::InstalledMod]>,
    explicit_dll: Option<&str>,
    replacement_files: &[String],
    sibling_packages: &[String],
) -> Result<Option<ModRecycleEntry>, String> {
    let (manifest, original) =
        read_active_manifest(manifests_dir, package_id)?.ok_or("mods.unknownFiles")?;
    let selected = dll.map(file_key);
    let explicit = explicit_dll.map(file_key);
    let replacement_keys = replacement_files
        .iter()
        .map(|file| file_key(file))
        .collect::<HashSet<_>>();
    if explicit.as_ref().is_some_and(|selected| {
        !manifest
            .files
            .iter()
            .any(|file| file_key(file) == *selected)
    }) {
        return Err("mods.dllNotInPackage".into());
    }
    let removed_files = manifest
        .files
        .iter()
        .filter(|file| {
            selected
                .as_ref()
                .is_none_or(|selected| file_key(file) == *selected)
        })
        .map(|file| normalize_relative(file))
        .collect::<Result<Vec<_>, _>>()?;
    if removed_files.is_empty() && dll.is_some() {
        return Err("mods.dllNotInPackage".into());
    }
    if let Some(dll) = explicit_dll {
        let relative = normalize_relative(&mods::canonical_dll_name(dll))?;
        if existing_variants(game_root, &relative)?.is_empty() {
            return Err("mods.dllNotFound".into());
        }
    }
    let peer_manifests = all_active_manifests(manifests_dir)?
        .into_iter()
        // 同批回收的包不算 peer：它们会和本包一起被移走，
        // 若把它们当「其他包」保护共享文件，两边都会跳过回收，旧副本留在磁盘上。
        .filter(|peer| {
            peer.repository_id != package_id && !sibling_packages.contains(&peer.repository_id)
        })
        .collect::<Vec<_>>();
    let peer_files = peer_manifests
        .iter()
        .flat_map(|peer| peer.files.iter().map(|file| file_key(file)))
        .collect::<HashSet<_>>();
    let safe_duplicate = match (dll, installed) {
        (Some(dll), Some(installed)) => has_safe_replacement(game_root, installed, dll)?,
        _ => false,
    };
    let mut actual = Vec::new();
    for relative in &removed_files {
        if peer_files.contains(&file_key(relative)) {
            if explicit
                .as_ref()
                .is_some_and(|selected| file_key(relative) == *selected)
                || replacement_keys.contains(&file_key(relative))
            {
                actual.extend(existing_variants(game_root, relative)?);
                continue;
            }
            if dll.is_some() {
                if !safe_duplicate {
                    return Err("recycle.fileSharedByOtherPackage".into());
                }
                actual.extend(existing_variants(game_root, relative)?);
                continue;
            }
            log::debug!(
                "回收包保留其他包共享文件：包={}，文件={}",
                package_id,
                relative
            );
            continue;
        }
        actual.extend(existing_variants(game_root, relative)?);
    }
    if actual.is_empty() {
        log::debug!(
            "MOD 包没有可回收文件，保留安装清单：包={}，清单文件数={}",
            package_id,
            removed_files.len()
        );
        return Ok(None);
    }
    // 清单只剩多语言快照，单语名取主语言值作为兜底显示名
    let name = {
        let from_i18n = archive::primary_translation(&manifest.package_name_i18n);
        if from_i18n.trim().is_empty() {
            package_id.to_owned()
        } else {
            from_i18n
        }
    };
    let name = dll
        .map(|relative| {
            Path::new(relative)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(&name)
                .to_owned()
        })
        .unwrap_or(name);
    let remaining = if dll.is_some() {
        let mut updated = manifest.clone();
        updated.files.retain(|file| {
            !removed_files
                .iter()
                .any(|removed| file_key(file) == file_key(removed))
        });
        (!updated.files.is_empty()).then_some(updated)
    } else {
        None
    };
    let mut related_manifests = Vec::new();
    if safe_duplicate || explicit.is_some() || !replacement_keys.is_empty() {
        for peer in peer_manifests {
            let removed = peer
                .files
                .iter()
                .filter(|file| {
                    removed_files
                        .iter()
                        .any(|removed| file_key(file) == file_key(removed))
                        && (safe_duplicate
                            || replacement_keys.contains(&file_key(file))
                            || explicit
                                .as_ref()
                                .is_some_and(|selected| file_key(file) == *selected))
                })
                .cloned()
                .collect::<Vec<_>>();
            if removed.is_empty() {
                continue;
            }
            let (_, bytes) =
                read_active_manifest(manifests_dir, &peer.repository_id)?.ok_or(INVALID_RECORD)?;
            related_manifests.push(RelatedManifest {
                package_id: peer.repository_id,
                original: String::from_utf8(bytes).map_err(|_| INVALID_RECORD.to_string())?,
                removed_files: removed,
            });
        }
    }
    stage_with_related_manifests(
        root,
        manifests_dir,
        game_root,
        name,
        Some(&manifest),
        Some(&original),
        removed_files,
        actual,
        remaining.as_ref(),
        related_manifests,
        &mut || Ok(()),
    )
    .map(Some)
}

pub fn recycle_unmanaged(
    root: &Path,
    game_root: &Path,
    dll: &str,
) -> Result<ModRecycleEntry, String> {
    let relative = normalize_relative(&mods::canonical_dll_name(dll))?;
    let parts = relative.split('/').collect::<Vec<_>>();
    if parts.len() < 3
        || !parts[0].eq_ignore_ascii_case("BepInEx")
        || !parts[1].eq_ignore_ascii_case("plugins")
        || !Path::new(&relative)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
    {
        return Err("mods.notInPlugins".into());
    }
    let actual = existing_variants(game_root, &relative)?;
    if actual.is_empty() {
        return Err("mods.dllNotFound".into());
    }
    let name = Path::new(&relative)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("MOD")
        .to_owned();
    stage(
        root,
        root,
        game_root,
        name,
        None,
        None,
        Vec::new(),
        actual,
        None,
        &mut || Ok(()),
    )
}
