use super::manifests::all_active_manifests;
use super::manifests::set_active_manifest;
use super::paths::file_key;
use super::paths::normalize_relative;
use super::selection::recycle_package_with_mods;
use crate::models::ModRecycleEntry;
use crate::mods;
use std::fs;
use std::path::Path;

/// 清单里的某个文件是否属于给定仓库。
///
/// 用扫描时「本地 MOD 绑定在线仓库」的同一套字段：BepInPlugin GUID、
/// PE 产品名、原始文件名。任一非空且与仓库配置相等即算命中。
///
/// 依赖 DLL 读不出这些字段，返回 false —— 但只要同一份清单里有主 MOD
/// 命中，整包就已经被判定为该仓库的旧版本了。
fn dll_matches_repository(
    game_root: &Path,
    relative: &str,
    repo: &crate::models::RepositoryConfig,
) -> bool {
    let configured = [
        repo.plugin_guid.trim(),
        repo.local_product_name.trim(),
        repo.local_original_filename.trim(),
    ];
    if configured.iter().all(|value| value.is_empty()) {
        return false;
    }
    let normalized = match normalize_relative(relative) {
        Ok(value) => value,
        Err(_) => return false,
    };
    let path = game_root.join(&normalized);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let plugin = mods::parse_bepin_plugin_metadata(&bytes);
    let metadata = crate::game::read_file_metadata(&path);
    let actual = [
        plugin
            .as_ref()
            .and_then(|item| item.guid.clone())
            .unwrap_or_default()
            .trim()
            .to_string(),
        metadata
            .product_name
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string(),
        metadata
            .original_filename
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string(),
    ];
    configured
        .iter()
        .zip(actual.iter())
        .any(|(expected, value)| !expected.is_empty() && value.eq_ignore_ascii_case(expected))
}

/// 两个游戏相对路径是否落在同一个**包目录**里。
///
/// 用于识别「同一个包的历史清单」：旧版本可能把内容解到子目录，
/// 文件路径与这次不同，但目录归属一致。
///
/// 必须排除 BepInEx 的系统容器目录：`BepInEx/plugins`、`BepInEx/patchers`
/// 下的散装 DLL 只是平铺放在一起，并不属于同一个包，认错会误删无关 MOD。
fn same_package_dir(left: &str, right: &str) -> bool {
    const SHARED_CONTAINERS: [&str; 3] = ["plugins", "patchers", "config"];
    let parent = |value: &str| -> Option<String> {
        let normalized = value.replace('\\', "/");
        let (dir, _) = normalized.rsplit_once('/')?;
        let dir = dir.trim_end_matches('/');
        if dir.is_empty() {
            return None;
        }
        // 目录本身是系统容器（或直接位于其中之一）时不算包目录
        let last = dir.rsplit_once('/').map_or(dir, |(_, tail)| tail);
        if SHARED_CONTAINERS.contains(&last.to_ascii_lowercase().as_str()) {
            return None;
        }
        Some(dir.to_lowercase())
    };
    match (parent(left), parent(right)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// 安装某个仓库的资产前，回收该仓库的已装包。
///
/// 「归属该仓库」按**身份**判定而不看渠道：
///   - `repository_id` 等于本次安装的包 ID（正常升级）；
///   - 或清单 binding 的 provider/owner/repository 与本次仓库一致
///     （历史安装用了别的包 ID，或仓库渠道、下载方式被改过）。
///
/// 之前用 `binding.matches_repository()` 判定，它额外要求 `channel` 相同。
/// 结果：仓库从 `all` 改成 `beta`（或反之）后，旧安装的 binding 不再匹配，
/// 旧包不被回收、文件留在磁盘上，扫描时就报「重复」。
/// 渠道只是「拉哪个 Release」的过滤条件，不改变「同一个仓库、同一批文件」的
/// 事实，所以这里必须按身份而非渠道判定。
///
/// 返回本次回收的全部记录；调用方在安装失败时据此整体回滚。
/// 任一包回收失败即中止（不继续安装），避免留下半清理状态。
pub fn recycle_repository_packages_before_install(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    repo: &crate::models::RepositoryConfig,
    replacement_files: &[String],
) -> Result<Vec<ModRecycleEntry>, String> {
    let manifests = all_active_manifests(manifests_dir)?;
    let mut targets = Vec::new();
    for manifest in &manifests {
        let same_id = manifest.repository_id == repo.id;
        let same_identity = manifest
            .repository_binding
            .as_ref()
            .is_some_and(|binding| binding.is_valid() && binding.matches_identity(repo));
        /*
         * 历史遗留或手动安装的清单：只要清单里**任一 DLL** 的身份能对上
         * 本次安装的仓库，整包都算这个仓库的旧版本。
         *
         * 判定用 GUID / 产品名 / 原始文件名 —— 与扫描时「本地 MOD 绑定在线
         * 仓库」用的是同一套字段。不用文件路径或目录来判断：
         *   · 包内依赖 DLL 读不出 GUID，靠主 MOD 命中即可整包归属；
         *   · 管理器解压 ZIP 时所有文件都写进同一份清单，所以命中一个就等于
         *     命中整包，与它们散落在哪个子目录无关；
         *   · 同目录判定会误伤 plugins 下平铺的无关插件。
         */
        let identity_match = manifest
            .files
            .iter()
            .any(|file| dll_matches_repository(game_root, file, repo));
        let overlaps_install = manifest.repository_id.starts_with("manual-")
            && manifest.files.iter().any(|file| {
                replacement_files.iter().any(|target| {
                    file_key(target) == file_key(file) || same_package_dir(target, file)
                })
            });
        if same_id || same_identity || identity_match || overlaps_install {
            targets.push(manifest.repository_id.clone());
        }
    }
    targets.sort();
    targets.dedup();

    let mut entries = Vec::new();
    // 同批要一起回收的包 ID 全量列表：它们彼此不能算「其他包」，
    // 否则 A 的清单会让 B 的文件被当成共享文件而跳过回收，装完仍留旧副本。
    let siblings = targets.clone();
    for package_id in targets {
        let entry = recycle_package_with_mods(
            root,
            manifests_dir,
            game_root,
            &package_id,
            None,
            None,
            None,
            replacement_files,
            &siblings,
        )?;
        if let Some(entry) = entry {
            log::info!(
                "安装前回收同仓库旧包：仓库={}，包={}，回收记录={}，文件数={}",
                repo.id,
                package_id,
                entry.id,
                entry.file_count
            );
            entries.push(entry);
            continue;
        }
        /*
         * 没有可回收文件：这批文件已被同批的另一个包顺带移走
         * （它们登记在同一批文件上）。此时必须把这个包的清单也移除，
         * 否则活动清单里留下一条指向已不存在文件的记录，扫描仍会多出一行。
         *
         * 只有在同批确实有其它包存在时才这样做 —— 单独回收一个已经空了的
         * 包时，保留清单是既有语义（用户删除时由调用方报错）。
         */
        if siblings.len() > 1 {
            set_active_manifest(manifests_dir, &package_id, None)?;
            log::info!(
                "旧包文件已由同批回收清空，移除其清单：仓库={}，包={}",
                repo.id,
                package_id
            );
        }
    }
    Ok(entries)
}
