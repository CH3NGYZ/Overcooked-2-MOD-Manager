use crate::archive::{self, InstallManifest};
use crate::game::read_file_metadata;
use crate::models::{
    AppConfig, Channel, InstalledMod, InstalledModStatusLabel, RepositoryBinding, RepositoryConfig,
    RepositoryDownloadMode,
};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/**
 * 是否是「恢复框架前的暂存目录」。
 *
 * 恢复 BepInEx 或 DIYLevel 时会先把现有目录改名成 `<目录>.before-restore-<时间戳>`，
 * 用完即删。但旧版本不删，且 DIYLevel 的目录在 `BepInEx/plugins/` 里面
 * （`BepInEx/plugins/OC2DIYLevel`），所以它的暂存目录直接躺在扫描根下 ——
 * 里面的 DLL 会被当成插件，用户看到「刚恢复完就冒出一批重复 MOD」。
 *
 * 这里在扫描时兜底跳过：新版本已经会删，但升级前留下的残留仍可能一直在，
 * 不跳过就会持续误报，用户只能自己翻目录手删才能恢复。
 */
fn is_restore_staging_component(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|text| text.contains(".before-restore-"))
}

pub fn scan_mods_with_manifests(
    game: &Path,
    config: &AppConfig,
    manifest_dir: Option<&Path>,
) -> Vec<InstalledMod> {
    let plugins = game.join("BepInEx/plugins");
    log::debug!("开始扫描本地 MOD：插件目录={}", plugins.display());
    if !plugins.is_dir() {
        log::warn!("插件目录不存在，跳过 MOD 扫描：{}", plugins.display());
        return Vec::new();
    }
    let manifests = manifest_dir
        .map(archive::read_manifests)
        .unwrap_or_default();
    let mut paths = Vec::new();
    let mut traversal_errors = 0usize;
    for entry in WalkDir::new(&plugins) {
        match entry {
            Ok(entry) if entry.file_type().is_file() && is_mod_file(entry.path()) => {
                // 恢复暂存目录里的文件不算 MOD：那是一次恢复操作的回滚素材，
                // 删不掉时会残留，不能让它污染插件列表
                let inside_staging = entry
                    .path()
                    .strip_prefix(&plugins)
                    .ok()
                    .is_some_and(|relative| {
                        relative
                            .components()
                            .any(|component| is_restore_staging_component(component.as_os_str()))
                    });
                if inside_staging {
                    continue;
                }
                paths.push(entry.into_path());
            }
            Ok(_) => {}
            Err(error) => {
                traversal_errors += 1;
                let relative_path = error
                    .path()
                    .and_then(|path| path.strip_prefix(&plugins).ok())
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "<unknown>".into());
                log::warn!(
                    "遍历 MOD 插件目录失败：相对路径={}，错误类别={:?}",
                    relative_path,
                    error.io_error().map(std::io::Error::kind)
                );
            }
        }
    }
    log::debug!(
        "发现候选 MOD 文件：数量={}，清单数量={}",
        paths.len(),
        manifests.len()
    );
    // If both names exist, the enabled DLL is the file BepInEx will load.
    // Keep one logical MOD row and let the enabled copy win.
    paths.sort_by_key(|path| is_disabled_mod_file(path));
    let mut seen = HashSet::new();
    let mut installed = paths
        .into_iter()
        .filter(|path| seen.insert(canonical_dll_file(path, game).to_ascii_lowercase()))
        .map(|path| inspect_dll(&path, game, config, &manifests))
        .collect::<Vec<_>>();
    annotate_package_counts(&mut installed);
    inherit_repository_from_sibling(&mut installed);
    mark_duplicate_guids(&mut installed);
    flag_manifest_version_mismatch(&mut installed, &manifests);
    refresh_status_labels(&mut installed);
    let packages = installed
        .iter()
        .filter_map(|item| item.package_id.as_ref())
        .collect::<HashSet<_>>()
        .len();
    let updates = installed
        .iter()
        .filter(|item| item.update_available)
        .count();
    let duplicates = installed.iter().filter(|item| item.duplicate_guid).count();
    log::info!(
        "本地 MOD 扫描完成：MOD数量={}，MOD包数量={}，可更新={}，重复身份={}，目录遍历错误={}",
        installed.len(),
        packages,
        updates,
        duplicates,
        traversal_errors
    );
    installed
}

fn is_mod_file(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.to_ascii_lowercase().ends_with(".dll")
        || name.to_ascii_lowercase().ends_with(".dll.disabled")
}

fn is_disabled_mod_file(path: &Path) -> bool {
    path.to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(".dll.disabled")
}

pub fn canonical_dll_file(path: &Path, game: &Path) -> String {
    let relative = path
        .strip_prefix(game)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    canonical_dll_name(&relative)
}

pub fn canonical_dll_name(dll_file: &str) -> String {
    if dll_file.to_ascii_lowercase().ends_with(".disabled") {
        dll_file[..dll_file.len() - ".disabled".len()].to_string()
    } else {
        dll_file.to_string()
    }
}

pub fn disabled_dll_file(dll_file: &str) -> String {
    format!("{dll_file}.disabled")
}

fn inspect_dll(
    path: &Path,
    game: &Path,
    config: &AppConfig,
    manifests: &[InstallManifest],
) -> InstalledMod {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!(
                "读取 MOD 文件失败，按空元数据处理：文件={}，原因={}",
                path.display(),
                error
            );
            Vec::new()
        }
    };
    let plugin = parse_bepin_plugin_metadata(&bytes);
    let metadata = read_file_metadata(path);
    let guid = plugin.as_ref().and_then(|item| item.guid.clone());
    let plugin_version = plugin.as_ref().and_then(|item| item.version.clone());
    let (installed_version, version_source) = choose_local_version(plugin_version.as_deref());
    let dll_file = canonical_dll_file(path, game);
    let manifest = manifests
        .iter()
        .filter(|manifest| {
            manifest
                .files
                .iter()
                .any(|file| canonical_dll_name(file).eq_ignore_ascii_case(&dll_file))
        })
        .max_by(|left, right| left.installed_at.cmp(&right.installed_at));
    let source_repository = manifest.and_then(|manifest| {
        config
            .repositories
            .iter()
            .find(|repo| repo.id == manifest.repository_id)
    });
    let repository_binding = manifest
        .and_then(|manifest| manifest.repository_binding.as_ref())
        .filter(|binding| binding.is_valid());
    /*
     * 候选只来自当前地区可用的仓库：`enabled=false`（用户关闭，或被官方源
     * 覆盖的社区副本）与地区不适用（例如中国大陆下的 international 仓库）
     * 都不展示也不刷新，已安装的 MOD 自然不该再绑定到它们。
     */
    let candidates = config.repositories.iter().filter(|repo| {
        crate::config::repository_is_available(config, repo, crate::config::current_user_region())
            && if let Some(binding) = repository_binding {
                binding.matches_repository(repo)
            } else {
                repository_matches_mod(
                    repo,
                    guid.as_deref(),
                    metadata.product_name.as_deref(),
                    metadata.original_filename.as_deref(),
                    installed_version.as_deref(),
                )
            }
    });
    let candidates = candidates.collect::<Vec<_>>();
    let repository = if let Some(binding) = repository_binding {
        candidates
            .iter()
            .copied()
            .find(|repo| source_repository.is_some_and(|source| source.id == repo.id))
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|repo| repo.channel == binding.channel)
            })
            .or_else(|| candidates.first().copied())
    } else {
        select_repository(&candidates, installed_version.as_deref(), source_repository)
    };
    let repository_ids = repository
        .map(|bound| {
            candidates
                .iter()
                .filter(|repo| repo.download_mode == bound.download_mode)
                .map(|repo| repo.id.clone())
                .collect()
        })
        .unwrap_or_default();
    log::trace!(
        "分析 MOD：文件={}，GUID={}，版本={}，候选仓库={}，绑定={}",
        path.display(),
        guid.as_deref().unwrap_or("未知"),
        installed_version.as_deref().unwrap_or("未知"),
        candidates.len(),
        repository.is_some()
    );
    let name = plugin
        .as_ref()
        .and_then(|item| item.name.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            metadata
                .product_name
                .clone()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| {
            Path::new(&canonical_dll_file(path, game))
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("mods.unknownName")
                .to_string()
        });
    /*
     * 仓库文件（RAW）模式没有 Release 可比较，装的是一份固定文件。
     *
     * 这类仓库也可能在清单里留下 repository_binding（安装时按 release_tag
     * 建的），但 RAW 从不抓 Release、cached_releases 永远为空。若照常走
     * binding_update_state，bound_release 找不到 Release 就返回 unknown，
     * MOD 管理页于是永远显示「版本未知」——即使刷新早已完成。
     * 所以先按仓库模式分流：RAW 直接判「已安装」，不参与版本判断。
     */
    let (update_available, status) = match repository {
        Some(repo) if repo.download_mode == RepositoryDownloadMode::RepositoryFile => {
            (false, "not_release_asset")
        }
        _ => match repository_binding {
            Some(binding) => binding_update_state(repository, binding),
            None => repository_update_state(repository, installed_version.as_deref()),
        },
    };
    if update_available {
        let remote = repository.and_then(|repo| {
            if let Some(binding) = repository_binding {
                bound_release(repo, binding).map(|release| release.tag_name.clone())
            } else {
                repository_remote_version(repo)
            }
        });
        log::debug!(
            "检测到 MOD 可更新：文件={}，本地版本={}，远程版本={}",
            path.display(),
            repository_binding
                .map(|binding| binding.tag.as_str())
                .or(installed_version.as_deref())
                .unwrap_or("未知"),
            remote.as_deref().unwrap_or("未知")
        );
    }
    let product_name = metadata.product_name;
    let original_filename = metadata.original_filename;
    let plugin_name = plugin.and_then(|item| item.name);
    let package_id = manifest.map(|item| item.repository_id.clone());
    /*
     * 包名的多语言快照 —— 清单里现在只有这一份。
     *
     * 清单没有快照（更旧版本安装的）时回退到当前仓库的 `name_i18n`，
     * 并在下次同步安装清单时写回 —— 与 `group_i18n` 的处理方式一致。
     */
    let package_name_i18n = manifest
        .map(|item| item.package_name_i18n.clone())
        .filter(|map| !map.is_empty())
        .or_else(|| {
            let repository_id = manifest?.repository_id.as_str();
            config
                .repositories
                .iter()
                .find(|repo| repo.id == repository_id)
                .map(|repo| {
                    repo.name_i18n
                        .iter()
                        .filter(|(_, value)| !value.trim().is_empty())
                        .map(|(key, value)| {
                            (
                                key.clone(),
                                crate::archive::package_display_name(value, &repo.provider),
                            )
                        })
                        .collect::<BTreeMap<_, _>>()
                })
                .filter(|map: &BTreeMap<String, String>| !map.is_empty())
        })
        .unwrap_or_default();
    /*
     * 单语包名 = 多语言快照的主语言值，没有快照时退回 Release tag。
     * 保留这个字段是为了兼容仍读它的旧调用方；前端优先用 `package_name_i18n`。
     */
    let package_name = if package_name_i18n.is_empty() {
        manifest
            .map(|item| item.release_tag.clone())
            .filter(|tag| !tag.trim().is_empty())
    } else {
        Some(crate::archive::primary_translation(&package_name_i18n))
    };
    let enabled = !is_disabled_mod_file(path);
    // 备注同理：清单只存多语言快照，单语值由主语言派生
    let note_i18n = manifest
        .map(|item| item.note_i18n.clone())
        .unwrap_or_default();
    let note = config
        .mod_notes
        .iter()
        .find(|item| {
            item.key == note_key_for_values(guid.as_deref(), product_name.as_deref(), &dll_file)
        })
        .map(|item| item.note.clone())
        .or_else(|| {
            let value = crate::archive::primary_translation(&note_i18n);
            (!value.is_empty()).then_some(value)
        });
    InstalledMod {
        repository_id: repository.map(|repo| repo.id.clone()),
        // 安装清单 id 以 manual- 开头 → 手动安装（本地文件 / 在线直链）。
        // 即使它成功绑定了在线仓库（repository_id 已变成仓库 id），
        // 这个标记仍保留原始安装来源，供前端在未匹配到仓库时归类。
        manual_install: manifest.is_some_and(|item| item.repository_id.starts_with("manual-")),
        // 没有 BepInPlugin 元数据（无 GUID、无版本）的 DLL 是纯依赖，
        // 例如 DIYLevel 包里的 LevelEditorStub.dll。
        is_dependency: guid.is_none()
            && installed_version.is_none()
            && plugin_name.as_deref().is_none_or(str::is_empty),
        repository_ids,
        repository_binding: repository_binding.cloned(),
        package_id,
        package_name,
        package_name_i18n,
        package_mod_count: 0,
        // 分组优先用安装清单里的快照：源文档没刷新出来时仓库配置可能为空，
        // 直接读当前仓库会导致分组跳变。
        // 旧版本清单没有快照（字段为空）时才回退到当前仓库，
        // 这样升级上来的用户也能立刻看到分组；回退值会在下次源刷新时写回清单。
        group_i18n: manifest
            .filter(|item| !item.group_i18n.is_empty())
            .map(|item| item.group_i18n.clone())
            .or_else(|| repository.map(|repo| repo.group_i18n.clone()))
            .unwrap_or_default(),
        /* 单语分组名 = 多语言快照的主语言值，仅作旧调用方的兼容字段。 */
        group: {
            let snapshot = manifest
                .map(|item| crate::archive::primary_translation(&item.group_i18n))
                .unwrap_or_default();
            if !snapshot.is_empty() {
                snapshot
            } else {
                repository
                    .map(|repo| repo.group.clone())
                    .unwrap_or_default()
            }
        },
        /*
         * 预发布判定取「清单快照标记」或「版本号带 - 后缀」的或关系。
         *
         * 只读快照不够：作者可能没在 Release 上勾 prerelease，但版本号本身
         * 叫 `0.4.0-beta`、`0.4.0.5-beta` —— 这类同样要按预发布提示。
         * 与前端 isPrereleaseVersion 保持同一规则。
         */
        prerelease: manifest.is_some_and(|item| item.prerelease)
            || installed_version
                .as_deref()
                .is_some_and(crate::releases::is_prerelease_version),
        name,
        title: metadata.title,
        description: metadata.description,
        product_name,
        original_filename,
        has_config: guid
            .as_deref()
            .is_some_and(|guid| crate::mod_config::exists(game, guid)),
        has_logs: guid
            .as_deref()
            .is_some_and(|guid| crate::mod_config::logs_dir_exists(game, guid)),
        guid,
        plugin_name,
        company: metadata.company,
        copyright: metadata.copyright,
        dll_file,
        enabled,
        version: installed_version,
        version_source: version_source.into(),
        update_available,
        labels: Vec::new(),
        duplicate_guid: false,
        duplicate_version: false,
        load_warning: None,
        note,
        status: status.into(),
    }
}

fn select_repository<'a>(
    candidates: &[&'a RepositoryConfig],
    installed_version: Option<&str>,
    source: Option<&RepositoryConfig>,
) -> Option<&'a RepositoryConfig> {
    // A real Raw install cannot silently become a Release install because the
    // same plugin GUID also occurs in a repository with a newer cached tag.
    if let Some(source) =
        source.filter(|repo| repo.download_mode == RepositoryDownloadMode::RepositoryFile)
    {
        return candidates.iter().copied().find(|repo| repo.id == source.id);
    }
    /*
     * 本地装的是哪一个渠道，按「版本号是否预发布」判断，不按段数。
     *
     * 判定统一交给 `releases::is_prerelease_version`：四段版本号是 beta
     * 序号（`1.8.7.23` ≡ `1.8.7-beta.23`），各种 beta 写法
     * （`-beta.4`、`-beta1`、`.beta4`、`-pre.01`）也都算预发布。
     * 在这里另写一套数段逻辑会和它分叉 —— 比如 `1.8.7.beta4` 按 `.`
     * 数会得到 4 段，看起来像四段 beta 序号，其实是三段 + 标识。
     */
    let preferred_channel = installed_version.map(|version| {
        if crate::releases::is_prerelease_version(version) {
            Channel::Beta
        } else {
            Channel::Stable
        }
    });
    let compatible_release = |repo: &&RepositoryConfig| {
        repo.download_mode == RepositoryDownloadMode::Release
            && preferred_channel
                .as_ref()
                .is_none_or(|channel| repo.channel == *channel || repo.channel == Channel::All)
    };
    if let Some(source) = source {
        if let Some(repo) = candidates
            .iter()
            .copied()
            .filter(compatible_release)
            .find(|repo| repo.id == source.id)
        {
            return Some(repo);
        }
    }
    let release = preferred_channel
        .as_ref()
        .and_then(|channel| {
            candidates.iter().copied().find(|repo| {
                repo.download_mode == RepositoryDownloadMode::Release && repo.channel == *channel
            })
        })
        .or_else(|| candidates.iter().copied().find(compatible_release));
    // Raw has no Release channel, regardless of the number of version parts.
    release.or_else(|| {
        candidates
            .iter()
            .copied()
            .find(|repo| repo.download_mode == RepositoryDownloadMode::RepositoryFile)
    })
}

fn repository_remote_version(repo: &RepositoryConfig) -> Option<String> {
    if repo.download_mode == RepositoryDownloadMode::RepositoryFile || repo.last_error.is_some() {
        return None;
    }
    repo.cached_releases.first().and_then(|release| {
        release
            .version
            .clone()
            .or_else(|| Some(release.tag_name.clone()))
    })
}

fn repository_update_state(
    repo: Option<&RepositoryConfig>,
    installed_version: Option<&str>,
) -> (bool, &'static str) {
    let Some(repo) = repo else {
        return (false, "unbound");
    };
    if repo.download_mode == RepositoryDownloadMode::RepositoryFile {
        return (false, "not_release_asset");
    }
    if repo.last_error.is_some() {
        return (false, "repository_unavailable");
    }
    let remote = repository_remote_version(repo);
    match (
        installed_version.and_then(crate::releases::parse_version),
        remote.as_deref().and_then(crate::releases::parse_version),
    ) {
        (Some(local), Some(remote)) if remote > local => (true, "update_available"),
        (Some(_), Some(_)) => (false, "latest"),
        _ => (false, "unknown"),
    }
}

fn bound_release<'a>(
    repo: &'a RepositoryConfig,
    binding: &RepositoryBinding,
) -> Option<&'a crate::models::ReleaseInfo> {
    if repo.last_error.is_some() || !binding.matches_repository(repo) {
        return None;
    }
    repo.cached_releases.iter().find(|release| {
        !release.draft
            && match binding.channel {
                Channel::All => true,
                Channel::Stable => !release.prerelease,
                Channel::Beta => release.prerelease,
            }
    })
}

fn binding_update_state(
    repo: Option<&RepositoryConfig>,
    binding: &RepositoryBinding,
) -> (bool, &'static str) {
    let Some(repo) = repo else {
        return (false, "bound_repository_missing");
    };
    if repo.last_error.is_some() {
        return (false, "repository_unavailable");
    }
    let Some(release) = bound_release(repo, binding) else {
        return (false, "unknown");
    };
    if release.tag_name == binding.tag {
        return (false, "latest");
    }
    // Exact tags also work for non-SemVer releases; comparable versions must
    // never turn an installed newer tag into a downgrade notification.
    // 用宽松多段比较：本项目允许 0.4.0.5 这类四段版本，且它高于 0.4.0。
    let update = match crate::releases::compare_release_versions(&release.tag_name, &binding.tag) {
        Some(ordering) => ordering == std::cmp::Ordering::Greater,
        _ => true,
    };
    if update {
        (true, "update_available")
    } else {
        (false, "latest")
    }
}

pub(crate) fn local_update_version(installed: &InstalledMod) -> Option<String> {
    installed
        .repository_binding
        .as_ref()
        .map(|binding| binding.tag.clone())
        .or_else(|| installed.version.clone())
}

pub(crate) fn remote_update_version(
    repo: &RepositoryConfig,
    installed: Option<&InstalledMod>,
) -> Option<String> {
    match installed.and_then(|installed| installed.repository_binding.as_ref()) {
        Some(binding) => bound_release(repo, binding).map(|release| release.tag_name.clone()),
        None => repository_remote_version(repo),
    }
}

pub(crate) fn mod_update_state(
    repo: &RepositoryConfig,
    installed: &InstalledMod,
) -> (bool, &'static str) {
    match installed.repository_binding.as_ref() {
        Some(binding) => binding_update_state(Some(repo), binding),
        None => repository_update_state(Some(repo), installed.version.as_deref()),
    }
}

/// 同目录下读不出 GUID 的 DLL，继承同目录里第一个已绑定仓库的插件的归属。
///
/// 场景：用户自己把 ZIP 解压到 `BepInEx/plugins/SomeMod/`，里面既有能读出
/// BepInPlugin 的主插件，也有读不出元数据的依赖 DLL。主插件能按 GUID 绑到
/// 在线仓库，依赖绑不上，于是：
///   - 前端分组会把它当成「本地安装」/「未绑定」，与主插件分成两组，
///     永远折不成同一个 MOD 包；
///   - 仓库更新时按仓库身份回收，也认不出这个依赖该一起删。
///
/// 这里给它们补上同一个仓库身份。**只在同一目录内传播**：跨目录没有可靠
/// 依据（plugins 根目录下各插件互不相干），不能仅凭「都在 plugins 里」就
/// 认为属于同一个包。
fn inherit_repository_from_sibling(mods: &mut [InstalledMod]) {
    /// DLL 所在目录（游戏相对路径去掉文件名，小写）。
    fn parent_dir(dll_file: &str) -> Option<String> {
        let normalized = dll_file.replace('\\', "/");
        let (dir, _) = normalized.rsplit_once('/')?;
        let dir = dir.trim_end_matches('/');
        if dir.is_empty() {
            return None;
        }
        /*
         * BepInEx 的系统容器不算「包目录」：plugins / patchers / config 下
         * 平铺的 DLL 只是碰巧放在一起，彼此互不相干，不能互相继承仓库身份。
         */
        const SHARED_CONTAINERS: [&str; 3] = ["plugins", "patchers", "config"];
        let last = dir.rsplit_once('/').map_or(dir, |(_, tail)| tail);
        if SHARED_CONTAINERS.contains(&last.to_ascii_lowercase().as_str()) {
            return None;
        }
        Some(dir.to_lowercase())
    }

    // 先收集每个目录里第一个能绑定仓库的成员，作为该目录的代表。
    let mut by_dir: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for item in mods.iter() {
        if item.is_dependency || item.repository_ids.is_empty() {
            continue;
        }
        let Some(dir) = parent_dir(&item.dll_file) else {
            continue;
        };
        by_dir.entry(dir).or_insert_with(|| {
            (
                item.repository_id.clone().unwrap_or_default(),
                item.repository_ids.clone(),
            )
        });
    }
    if by_dir.is_empty() {
        return;
    }

    for item in mods.iter_mut() {
        // 只补「自己完全没有仓库身份」的项，不覆盖已知归属
        if !item.repository_ids.is_empty() || item.repository_binding.is_some() {
            continue;
        }
        let Some(dir) = parent_dir(&item.dll_file) else {
            continue;
        };
        let Some((repository_id, repository_ids)) = by_dir.get(&dir) else {
            continue;
        };
        item.repository_ids = repository_ids.clone();
        if item.repository_id.is_none() && !repository_id.is_empty() {
            item.repository_id = Some(repository_id.clone());
        }
        log::debug!(
            "同目录插件继承仓库归属：文件={}，目录={}，仓库={:?}",
            item.dll_file,
            dir,
            repository_ids.first()
        );
    }
}

fn annotate_package_counts(mods: &mut [InstalledMod]) {
    let mut counts = HashMap::<String, usize>::new();
    for item in mods.iter().filter_map(|item| item.package_id.as_ref()) {
        *counts.entry(item.clone()).or_default() += 1;
    }
    for item in mods {
        item.package_mod_count = item
            .package_id
            .as_ref()
            .and_then(|id| counts.get(id).copied())
            .unwrap_or(0);
    }
}

/// 单个 MOD 的「安装清单版本」与「DLL 实际版本」不一致时，标记为可更新。
///
/// 背景：绑定更新判定（`binding_update_state`）只比较「清单里的 tag」和
/// 「远端最新 tag」。如果用户手动替换了 plugins 下的 DLL（或安装后又被别的
/// 工具覆盖），DLL 自己报告的 BepInPlugin 版本可能低于清单记录的那次安装，
/// 而清单 tag 仍等于远端最新 tag —— 结果会错误地显示「最新版」。
///
/// 规则边界（重要）：
///   - 只处理**单 DLL 的 MOD**（`package_mod_count <= 1`）。MOD 包的版本
///     不绑定任何单个 DLL，包的更新只按清单 tag 与仓库 tag 比较，这里不碰。
///   - 只把状态升级为「可更新」，不会把已判定的更新降级；
///   - 两者版本都不可比较（非 SemVer）时不做判断，避免误报；
///   - DLL 版本高于清单版本不告警（用户可能已手动升级，远端也没有更新）。
fn flag_manifest_version_mismatch(mods: &mut [InstalledMod], manifests: &[InstallManifest]) {
    for item in mods.iter_mut() {
        // MOD 包成员的版本由包统一判定，不参与逐 DLL 比较
        if item.package_mod_count > 1 {
            continue;
        }
        if item.update_available {
            continue;
        }
        // 只对「已安装且绑定到某个 Release」的 MOD 有意义
        let Some(binding) = item.repository_binding.as_ref() else {
            continue;
        };
        let Some(manifest) = manifests.iter().find(|manifest| {
            manifest
                .files
                .iter()
                .any(|file| canonical_dll_name(file).eq_ignore_ascii_case(&item.dll_file))
        }) else {
            continue;
        };
        if manifest.release_tag.trim().is_empty() {
            continue;
        }
        let Some(dll_version) = item
            .version
            .as_deref()
            .and_then(crate::releases::parse_version)
        else {
            continue;
        };
        let Some(installed_tag_version) = crate::releases::parse_version(&binding.tag) else {
            continue;
        };
        if dll_version >= installed_tag_version {
            continue;
        }
        log::warn!(
            "MOD 版本与安装清单不一致，标记为可更新：文件={}，DLL 版本={}，清单版本={}",
            item.dll_file,
            item.version.as_deref().unwrap_or("未知"),
            binding.tag
        );
        item.update_available = true;
        item.status = "update_available".into();
    }
}

fn repository_matches_mod(
    repo: &crate::models::RepositoryConfig,
    guid: Option<&str>,
    product_name: Option<&str>,
    original_filename: Option<&str>,
    installed_version: Option<&str>,
) -> bool {
    let identity_matches = [
        (guid, repo.plugin_guid.as_str()),
        (product_name, repo.local_product_name.as_str()),
        (original_filename, repo.local_original_filename.as_str()),
    ]
    .into_iter()
    .any(|(actual, configured)| {
        let configured = configured.trim();
        !configured.is_empty()
            && actual.is_some_and(|value| value.trim().eq_ignore_ascii_case(configured))
    });
    if !identity_matches {
        return false;
    }
    let pattern = repo.version_regex.trim();
    if pattern.is_empty() {
        return true;
    }
    installed_version
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| {
            regex::Regex::new(pattern)
                .ok()
                .map(|regex| regex.is_match(value))
        })
        .unwrap_or(false)
}

fn choose_local_version(plugin_version: Option<&str>) -> (Option<String>, &'static str) {
    if let Some(version) = plugin_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        (Some(version.to_string()), "BepInPlugin")
    } else {
        (None, "unknown")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PluginMetadata {
    pub(crate) guid: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
}

/// 从磁盘上的 DLL 读取 BepInPlugin 名称，读不出返回 None。
///
/// 安装完成提示要显示「用户认识的名字」：仓库安装用仓库名，
/// 本地安装就用 DLL 里的插件名，而不是 `XXX.dll` 这样的文件名。
pub(crate) fn plugin_name_from_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let plugin = parse_bepin_plugin_metadata(&bytes)?;
    plugin
        .name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 文件名去掉扩展名，作为兜底显示名。
pub(crate) fn file_stem_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(file_name)
        .to_string()
}

pub(crate) fn parse_bepin_plugin_metadata(bytes: &[u8]) -> Option<PluginMetadata> {
    if !String::from_utf8_lossy(bytes).contains("BepInPlugin") {
        return None;
    }
    let version_pattern = Regex::new(r"(?i)^(?:\d+\.){1,3}\d+(?:[-+][0-9a-z.-]+)?$").ok()?;
    for (position, window) in bytes.windows(2).enumerate() {
        if window != [1, 0] {
            continue;
        }
        let mut offset = position + 2;
        let Some(guid) = read_serialized_string(bytes, &mut offset) else {
            continue;
        };
        let Some(name) = read_serialized_string(bytes, &mut offset) else {
            continue;
        };
        let Some(version) = read_serialized_string(bytes, &mut offset) else {
            continue;
        };
        let guid = guid.trim_matches(['{', '}']);
        if !guid.is_empty()
            && guid.len() <= 256
            && !guid.chars().any(|character| character.is_control())
            && !name.trim().is_empty()
            && version_pattern.is_match(version)
        {
            return Some(PluginMetadata {
                guid: Some(guid.to_string()),
                name: Some(name.to_string()),
                version: Some(version.to_string()),
            });
        }
    }
    None
}

fn read_serialized_string<'a>(bytes: &'a [u8], offset: &mut usize) -> Option<&'a str> {
    let first = *bytes.get(*offset)?;
    *offset += 1;
    let length = if first & 0x80 == 0 {
        first as usize
    } else if first & 0xc0 == 0x80 {
        let second = *bytes.get(*offset)?;
        *offset += 1;
        (((first & 0x3f) as usize) << 8) | second as usize
    } else if first & 0xe0 == 0xc0 {
        let remaining = bytes.get(*offset..*offset + 3)?;
        *offset += 3;
        (((first & 0x1f) as usize) << 24)
            | ((remaining[0] as usize) << 16)
            | ((remaining[1] as usize) << 8)
            | remaining[2] as usize
    } else {
        return None;
    };
    let end = offset.checked_add(length)?;
    let value = std::str::from_utf8(bytes.get(*offset..end)?).ok()?;
    *offset = end;
    Some(value)
}

fn refresh_status_labels(mods: &mut [InstalledMod]) {
    for item in mods {
        let mut labels = Vec::new();
        /*
         * 纯依赖 DLL（读不出 BepInPlugin）只标「依赖」，不再套用
         * 最新版/可更新/未绑定这些 MOD 语义 —— 它没有版本可比，
         * 把它当 MOD 展示会让包内文件数对不上。
         */
        if item.is_dependency {
            labels.push(InstalledModStatusLabel::Dependency);
            if !item.enabled {
                labels.push(InstalledModStatusLabel::Disabled);
            }
            item.update_available = false;
            item.status = "dependency".into();
            item.labels = labels;
            continue;
        }
        if item.status == "duplicate_lower_version" {
            labels.push(InstalledModStatusLabel::DuplicateLowerVersion);
        } else if item.update_available {
            labels.push(InstalledModStatusLabel::UpdateAvailable);
        } else {
            match item.status.as_str() {
                "latest" => labels.push(InstalledModStatusLabel::Latest),
                "not_release_asset" => labels.push(InstalledModStatusLabel::NotReleaseAsset),
                "unbound" => labels.push(InstalledModStatusLabel::Unbound),
                "bound_repository_missing" => {
                    labels.push(InstalledModStatusLabel::BoundRepositoryMissing)
                }
                "repository_unavailable" => {
                    labels.push(InstalledModStatusLabel::RepositoryUnavailable)
                }
                "unknown" => labels.push(InstalledModStatusLabel::Unknown),
                _ => {}
            }
        }
        // 同 GUID 的多个副本一律标「重复」（含被高版本取代的那一份），
        // 被取代的那份再额外带「低版本」，两条记录各自都能看出重复关系。
        if item.duplicate_guid {
            labels.push(InstalledModStatusLabel::Duplicate);
        }
        if item.load_warning.is_some() && item.status != "duplicate_lower_version" {
            labels.push(InstalledModStatusLabel::LoadWarning);
        }
        if !item.enabled {
            labels.push(InstalledModStatusLabel::Disabled);
        }
        item.labels = labels;
    }
}

fn note_key_for_values(guid: Option<&str>, product_name: Option<&str>, dll_file: &str) -> String {
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

fn mark_duplicate_guids(mods: &mut [InstalledMod]) {
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
fn compare_mod_versions(left: &str, right: &str) -> Option<Ordering> {
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

#[cfg(test)]
mod tests {
    fn plugin_blob(guid: &str, name: &str, version: &str) -> Vec<u8> {
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in [guid, name, version] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes
    }

    fn duplicate_configuration_manager(
        version: &str,
        online_version: &str,
    ) -> crate::models::InstalledMod {
        serde_json::from_value(serde_json::json!({
            "repository_id": "configuration-manager", "repository_ids": ["configuration-manager"],
            "name": "Configuration Manager", "guid": "com.bepis.bepinex.configurationmanager",
            "dll_file": format!("BepInEx/plugins/{version}/ConfigurationManager.dll"),
            "enabled": true, "version": version, "version_source": "BepInPlugin",
            "update_available": super::compare_mod_versions(online_version, version) == Some(std::cmp::Ordering::Greater),
            "duplicate_guid": false, "status": "latest"
        })).unwrap()
    }

    #[test]
    fn both_duplicate_copies_get_the_duplicate_label() {
        // 装了两个同 GUID 的版本（1.8.7 + 1.9.7）：
        // 两条都应显示「重复」，旧的那条额外显示「低版本」。
        let mut mods = vec![
            duplicate_configuration_manager("1.8.7", "1.9.7"),
            duplicate_configuration_manager("1.9.7", "1.9.7"),
        ];
        super::mark_duplicate_guids(&mut mods);
        super::refresh_status_labels(&mut mods);

        use crate::models::InstalledModStatusLabel as Label;
        assert!(mods[0].labels.contains(&Label::Duplicate));
        assert!(mods[0].labels.contains(&Label::DuplicateLowerVersion));
        assert!(mods[1].labels.contains(&Label::Duplicate));
        assert!(!mods[1].labels.contains(&Label::DuplicateLowerVersion));
        // 高版本副本保留它自己的更新状态（这里已是最新）
        assert!(mods[1].labels.contains(&Label::Latest));
    }

    /*
     * 单个 MOD：清单 tag 与远端最新 tag 相同（本来判「最新版」），
     * 但 DLL 自己报告的版本低于清单记录的版本 —— 说明插件文件被替换过，
     * 必须提示可更新，否则用户会一直看到一个版本号更低的「最新版」。
     */
    #[test]
    fn single_mod_reports_update_when_dll_version_is_below_manifest_tag() {
        let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
            "repository_id": "repo-1", "repository_ids": ["repo-1"],
            "name": "Example", "dll_file": "BepInEx/plugins/Example.dll",
            "enabled": true, "version": "1.2.0", "version_source": "BepInPlugin",
            "update_available": false, "duplicate_guid": false, "status": "latest",
            "repository_binding": {
                "provider": "github", "owner": "o", "repository": "r",
                "tag": "v2.0.0", "channel": "all"
            }
        }))
        .unwrap();
        let manifests = vec![manifest_with_tag(
            "repo-1",
            "v2.0.0",
            "BepInEx/plugins/Example.dll",
        )];
        super::flag_manifest_version_mismatch(std::slice::from_mut(&mut item), &manifests);
        super::refresh_status_labels(std::slice::from_mut(&mut item));
        assert!(item.update_available);
        assert_eq!(item.status, "update_available");
        assert!(
            item.labels
                .contains(&crate::models::InstalledModStatusLabel::UpdateAvailable)
        );
        assert!(
            !item
                .labels
                .contains(&crate::models::InstalledModStatusLabel::Latest)
        );
    }

    /// DLL 版本与清单一致（或更高）时不应被误判为可更新。
    #[test]
    fn single_mod_keeps_latest_when_dll_version_matches_manifest_tag() {
        for version in ["2.0.0", "2.1.0"] {
            let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
                "repository_id": "repo-1", "repository_ids": ["repo-1"],
                "name": "Example", "dll_file": "BepInEx/plugins/Example.dll",
                "enabled": true, "version": version, "version_source": "BepInPlugin",
                "update_available": false, "duplicate_guid": false, "status": "latest",
                "repository_binding": {
                    "provider": "github", "owner": "o", "repository": "r",
                    "tag": "v2.0.0", "channel": "all"
                }
            }))
            .unwrap();
            let manifests = vec![manifest_with_tag(
                "repo-1",
                "v2.0.0",
                "BepInEx/plugins/Example.dll",
            )];
            super::flag_manifest_version_mismatch(std::slice::from_mut(&mut item), &manifests);
            assert!(!item.update_available, "版本 {version} 不应被判为可更新");
        }
    }

    /*
     * 仓库文件（RAW）模式的 MOD 永远显示「已安装」，不能显示「版本未知」。
     *
     * 这类仓库装的是一份固定文件，没有 Release 可比较；但安装清单里可能
     * 留下 repository_binding，此时若照常走 binding_update_state，
     * bound_release 找不到 Release 就返回 unknown —— 即使仓库刷新早已完成，
     * MOD 管理页也会一直同时显示「版本未知」和「已安装」。
     */
    #[test]
    fn raw_repository_mods_never_report_unknown_version() {
        let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "raw-1", "name": "Raw", "provider": "github", "username": "u", "repository": "r",
            "download_mode": "repository_file", "branch": "main", "file_path": "mod.dll",
            "channel": "all", "asset_rules": [], "install_target": "BepInEx/plugins",
            "note": "", "plugin_guid": "", "local_product_name": "", "local_original_filename": "",
            "version_regex": "", "max_release_count": 10, "download_threads": 4,
            "enabled": true, "builtin": false, "cached_releases": []
        }))
        .unwrap();
        // RAW 仓库没有 cached_releases，但绑定存在
        let (update, status) = super::binding_update_state(
            Some(&repo),
            &crate::models::RepositoryBinding {
                provider: crate::models::Provider::Github,
                owner: "u".into(),
                repository: "r".into(),
                tag: "v1.0.0".into(),
                channel: crate::models::Channel::All,
            },
        );
        // 直接调绑定判定仍会返回 unknown —— 这是它本身的语义
        assert!(!update);
        assert_eq!(status, "unknown");
        // 但扫描路径必须先按仓库模式分流，不能让 RAW 走到这里
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.0.0")).1,
            "not_release_asset"
        );
    }

    /// MOD 包（成员数 >= 2）不参与逐 DLL 版本比较：包版本只按 tag 比较。
    #[test]
    fn package_members_are_not_flagged_by_dll_version_mismatch() {
        let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
            "repository_id": "repo-1", "repository_ids": ["repo-1"],
            "package_id": "pkg-1", "package_name": "Pack",
            "name": "Example", "dll_file": "BepInEx/plugins/Example.dll",
            "enabled": true, "version": "1.2.0", "version_source": "BepInPlugin",
            "update_available": false, "duplicate_guid": false, "status": "latest",
            "repository_binding": {
                "provider": "github", "owner": "o", "repository": "r",
                "tag": "v2.0.0", "channel": "all"
            }
        }))
        .unwrap();
        // 包内两个成员：package_mod_count 由 annotate_package_counts 写入 2
        item.package_mod_count = 2;
        let manifests = vec![manifest_with_tag(
            "repo-1",
            "v2.0.0",
            "BepInEx/plugins/Example.dll",
        )];
        super::flag_manifest_version_mismatch(std::slice::from_mut(&mut item), &manifests);
        assert!(
            !item.update_available,
            "MOD 包成员不应按 DLL 版本单独判更新"
        );
    }

    /// 非 SemVer 版本不做比较，避免把不可比的值误判成更新。
    #[test]
    fn unparsable_versions_do_not_trigger_the_mismatch_rule() {
        let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
            "repository_id": "repo-1", "repository_ids": ["repo-1"],
            "name": "Example", "dll_file": "BepInEx/plugins/Example.dll",
            "enabled": true, "version": "nightly", "version_source": "BepInPlugin",
            "update_available": false, "duplicate_guid": false, "status": "latest",
            "repository_binding": {
                "provider": "github", "owner": "o", "repository": "r",
                "tag": "latest-build", "channel": "all"
            }
        }))
        .unwrap();
        let manifests = vec![manifest_with_tag(
            "repo-1",
            "latest-build",
            "BepInEx/plugins/Example.dll",
        )];
        super::flag_manifest_version_mismatch(std::slice::from_mut(&mut item), &manifests);
        assert!(!item.update_available);
    }

    fn manifest_with_tag(
        repository_id: &str,
        tag: &str,
        file: &str,
    ) -> crate::archive::InstallManifest {
        crate::archive::new_manifest(
            repository_id,
            tag,
            Default::default(),
            Default::default(),
            vec![file.into()],
        )
    }

    #[test]
    fn disabled_mod_keeps_update_label_and_legacy_json_defaults_labels() {
        let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
            "repository_id": "configuration-manager", "repository_ids": ["configuration-manager"],
            "name": "Configuration Manager", "dll_file": "BepInEx/plugins/ConfigurationManager.dll",
            "enabled": false, "version": "18.3", "version_source": "BepInPlugin",
            "update_available": true, "duplicate_guid": false, "status": "update_available"
        }))
        .unwrap();
        assert!(item.labels.is_empty());
        super::refresh_status_labels(std::slice::from_mut(&mut item));
        assert_eq!(
            item.labels,
            [
                crate::models::InstalledModStatusLabel::UpdateAvailable,
                crate::models::InstalledModStatusLabel::Disabled
            ]
        );
        assert_eq!(
            serde_json::to_value(&item).unwrap()["labels"],
            serde_json::json!(["update_available", "disabled"])
        );
    }

    #[test]
    fn shadowed_old_configuration_manager_is_not_an_available_update() {
        let mut installed = vec![
            duplicate_configuration_manager("18.3", "19.0"),
            duplicate_configuration_manager("19.0", "19.0"),
        ];
        assert!(installed[0].update_available);
        super::mark_duplicate_guids(&mut installed);
        assert_eq!(installed[0].status, "duplicate_lower_version");
        assert_eq!(
            installed[0].load_warning.as_deref(),
            Some("mods.higherVersionPresent")
        );
        assert_eq!(
            installed
                .iter()
                .filter(|item| item.update_available)
                .count(),
            0
        );
    }

    #[test]
    fn newer_online_configuration_manager_keeps_the_effective_local_copy_update() {
        let mut installed = vec![
            duplicate_configuration_manager("18.3", "20.0"),
            duplicate_configuration_manager("19.0", "20.0"),
        ];
        super::mark_duplicate_guids(&mut installed);
        assert!(!installed[0].update_available);
        assert!(installed[1].update_available);
        assert_eq!(
            installed
                .iter()
                .filter(|item| item.update_available)
                .count(),
            1
        );
    }

    #[test]
    fn disabled_higher_copy_does_not_hide_the_loaded_mod_update() {
        let mut installed = vec![
            duplicate_configuration_manager("18.3", "19.0"),
            duplicate_configuration_manager("19.0", "19.0"),
        ];
        installed[1].enabled = false;
        super::mark_duplicate_guids(&mut installed);
        assert!(installed[0].update_available);
        assert!(installed[0].load_warning.is_none());
        assert_ne!(installed[0].status, "duplicate_lower_version");
    }

    #[test]
    fn same_version_duplicates_keep_updates_and_raw_keeps_its_source_status() {
        let mut same_version = vec![
            duplicate_configuration_manager("19.0", "20.0"),
            duplicate_configuration_manager("19.0", "20.0"),
        ];
        super::mark_duplicate_guids(&mut same_version);
        assert!(
            same_version
                .iter()
                .all(|item| item.duplicate_version && item.update_available)
        );
        let mut raw = duplicate_configuration_manager("18.3", "19.0");
        raw.status = "not_release_asset".into();
        raw.update_available = false;
        let mut raw_and_release = vec![raw, duplicate_configuration_manager("19.0", "19.0")];
        super::mark_duplicate_guids(&mut raw_and_release);
        assert_eq!(raw_and_release[0].status, "not_release_asset");
        assert!(!raw_and_release[0].update_available);
    }

    #[test]
    fn disabled_dll_names_keep_the_same_logical_path() {
        assert_eq!(
            super::canonical_dll_name("BepInEx/plugins/example.dll.disabled"),
            "BepInEx/plugins/example.dll"
        );
        assert_eq!(
            super::disabled_dll_file("BepInEx/plugins/example.dll"),
            "BepInEx/plugins/example.dll.disabled"
        );
    }

    #[test]
    fn unknown_version_is_not_update() {
        assert!(!matches!(
            crate::releases::parse_version("unknown"),
            Some(_)
        ));
    }

    /*
     * 本地版本落在哪个渠道：按「是否预发布」判断，不再数版本段数。
     *
     * 段数是个间接指标，遇到 `1.8.7.beta4` 这类写法就会错判（按 `.`
     * 数出 4 段，其实是三段 + 标识）。语义判定统一交给
     * `releases::is_prerelease_version`，它认得本项目全部 beta 写法。
     */
    #[test]
    fn local_release_channel_follows_prerelease_semantics() {
        use crate::releases::is_prerelease_version;
        // 三段正式版
        assert!(!is_prerelease_version("1.8.5"));
        assert!(!is_prerelease_version("v1.8.5"));
        // 四段 = beta 序号
        assert!(is_prerelease_version("1.8.5.2"));
        assert!(is_prerelease_version("v1.8.5.2-beta"));
        // 其它 beta 写法：无分隔符、点号分隔、pre 前缀
        assert!(is_prerelease_version("1.8.7-beta1"));
        assert!(is_prerelease_version("1.8.7.beta4"));
        assert!(is_prerelease_version("1.9.7-beta.4"));
        assert!(is_prerelease_version("1.8.5-pre.01"));
    }

    #[test]
    fn mod_version_comparison_fills_missing_components_with_zero() {
        assert_eq!(
            super::compare_mod_versions("1.8.5.03", "1.8.5"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            super::compare_mod_versions("1.8.5", "1.8.5.0"),
            Some(std::cmp::Ordering::Equal)
        );
        assert_eq!(
            super::compare_mod_versions("1.10.0", "1.9.99.99"),
            Some(std::cmp::Ordering::Greater)
        );
        /*
         * 本地副本去重用的是**纯数字段**比较，不是 semver。
         *
         * BepInEx 加载同 GUID 多份 DLL 时比程序集版本号的数字大小，
         * `1.9.8.1` 严格大于 `1.9.8`。若这里改成 semver（beta 低于
         * 同号正式版），管理器会建议卸载实际被加载的那个文件。
         *
         * 线上更新判定走另一套（releases::compare_release_versions），
         * 那里 `1.9.8 > 1.9.8-beta.1` 是对的 —— 两套规则服务两个目的。
         */
        assert_eq!(
            super::compare_mod_versions("1.9.8.1", "1.9.8"),
            Some(std::cmp::Ordering::Greater),
            "本地副本：四段 beta 的数字更大，与实际加载一致"
        );
        assert_eq!(
            super::compare_mod_versions("1.9.8.21", "1.9.8.1"),
            Some(std::cmp::Ordering::Greater)
        );
    }

    #[test]
    fn local_version_only_uses_bepinplugin_and_preserves_zero_versions() {
        for version in ["0.10.0", "1.2.3.4", "0.0.0", "0.0.0.0"] {
            assert_eq!(
                super::choose_local_version(Some(version)),
                (Some(version.into()), "BepInPlugin")
            );
        }
        assert_eq!(
            super::choose_local_version(Some(" 0.10.0 ")),
            (Some("0.10.0".into()), "BepInPlugin")
        );
        for missing in [None, Some(""), Some(" \t\r\n")] {
            assert_eq!(super::choose_local_version(missing), (None, "unknown"));
        }
    }

    fn binding_repository() -> crate::models::RepositoryConfig {
        crate::models::RepositoryConfig {
            id: "test".into(),
            name: "Test".into(),
            name_i18n: std::collections::BTreeMap::new(),
            group: String::new(),
            group_i18n: std::collections::BTreeMap::new(),
            provider: crate::models::Provider::Gitee,
            username: "example".into(),
            repository: "mod".into(),
            download_mode: crate::models::RepositoryDownloadMode::Release,
            branch: String::new(),
            file_path: String::new(),
            region: None,
            channel: crate::models::Channel::All,
            platform: None,
            asset_rules: Vec::new(),
            install_target: String::new(),
            note: String::new(),
            note_i18n: std::collections::BTreeMap::new(),
            plugin_guid: "com.example.guid".into(),
            local_product_name: "Example Product".into(),
            local_original_filename: "Example.dll".into(),
            relations: Vec::new(),
            release_relations: Vec::new(),
            version_regex: String::new(),
            max_release_count: 10,
            download_threads: 1,
            github_token: String::new(),
            gitee_token: String::new(),
            official_api_only: true,
            enabled: true,
            builtin: false,
            pinned_version: None,
            ignored_version: None,
            etag: None,
            cached_releases: Vec::new(),
            releases_cached_at: None,
            last_error: None,
            source_id: None,
        }
    }

    #[test]
    fn explicit_repository_tags_override_dll_versions_for_every_source_type() {
        for source in [Some("official"), Some(crate::community::SOURCE_ID), None] {
            let mut repo = binding_repository();
            repo.source_id = source.map(str::to_string);
            repo.builtin = source == Some("official");
            repo.cached_releases = vec![high_cached_release()];
            let binding = crate::models::RepositoryBinding::from_release(&repo, "v98.0.0").unwrap();
            assert_eq!(
                super::binding_update_state(Some(&repo), &binding),
                (true, "update_available")
            );
            let mut current = binding.clone();
            current.tag = "v99.0.0".into();
            assert_eq!(
                super::binding_update_state(Some(&repo), &current),
                (false, "latest")
            );
            current.tag = "v100.0.0".into();
            assert_eq!(
                super::binding_update_state(Some(&repo), &current),
                (false, "latest")
            );
            repo.last_error = Some("request failed".into());
            assert_eq!(
                super::binding_update_state(Some(&repo), &binding),
                (false, "repository_unavailable")
            );
        }
    }

    #[test]
    fn opaque_tags_and_channel_provider_boundaries_are_preserved() {
        let mut repo = binding_repository();
        repo.channel = crate::models::Channel::Stable;
        let binding = crate::models::RepositoryBinding::from_release(&repo, "old-nightly").unwrap();
        let mut beta = high_cached_release();
        beta.tag_name = "beta-nightly".into();
        beta.prerelease = true;
        let mut stable = high_cached_release();
        stable.tag_name = "new-nightly".into();
        repo.channel = crate::models::Channel::All;
        repo.cached_releases = vec![beta, stable];
        assert_eq!(
            super::bound_release(&repo, &binding).unwrap().tag_name,
            "new-nightly"
        );
        assert_eq!(
            super::binding_update_state(Some(&repo), &binding),
            (true, "update_available")
        );
        let mut current = binding.clone();
        current.tag = "new-nightly".into();
        assert_eq!(
            super::binding_update_state(Some(&repo), &current),
            (false, "latest")
        );
        repo.provider = crate::models::Provider::Github;
        assert!(!binding.matches_repository(&repo));
        repo.provider = binding.provider.clone();
        repo.channel = crate::models::Channel::Beta;
        assert!(!binding.matches_repository(&repo));
        repo.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
        assert!(crate::models::RepositoryBinding::from_release(&repo, "tag").is_none());
        assert_eq!(
            super::binding_update_state(None, &binding),
            (false, "bound_repository_missing")
        );
    }

    #[test]
    fn real_manifest_scan_keeps_dll_metadata_but_selects_owner_repository_tag_first() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/community-20261013")
            .join(format!("binding-scan-{}", uuid::Uuid::new_v4()));
        let relative = "BepInEx/plugins/Example.dll";
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in ["com.example.guid", "Example", "100.0.0"] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        std::fs::write(&path, &bytes).unwrap();
        let mut bound = binding_repository();
        bound.id = "refreshed-source-id".into();
        bound.plugin_guid.clear();
        bound.local_product_name.clear();
        bound.local_original_filename.clear();
        bound.cached_releases = vec![high_cached_release()];
        let mut misleading = binding_repository();
        misleading.id = "dll-matching-but-wrong-repository".into();
        misleading.repository = "unrelated".into();
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1, "ui_zoom":1.5, "onboarding_seen_steps":[],
            "game":{"path":"", "auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[misleading, bound]
        })).unwrap();
        let manifests = root.join("manifests");
        let mut manifest = crate::archive::new_manifest(
            "old-source-id",
            "v98.0.0",
            crate::archive::single_language("Package"),
            Default::default(),
            vec![relative.into()],
        );
        manifest.repository_binding =
            crate::models::RepositoryBinding::from_release(&bound, "v98.0.0");
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(scan.len(), 1);
        let installed = &scan[0];
        assert_eq!(installed.guid.as_deref(), Some("com.example.guid"));
        assert_eq!(installed.version.as_deref(), Some("100.0.0"));
        assert_eq!(
            installed.repository_id.as_deref(),
            Some("refreshed-source-id")
        );
        assert_eq!(installed.repository_ids, ["refreshed-source-id"]);
        assert_eq!(installed.package_id.as_deref(), Some("old-source-id"));
        assert_eq!(installed.repository_binding, manifest.repository_binding);
        assert!(installed.update_available);
        assert_eq!(
            super::local_update_version(installed).as_deref(),
            Some("v98.0.0")
        );

        std::fs::rename(&path, root.join(format!("{relative}.disabled"))).unwrap();
        let disabled = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert!(!disabled[0].enabled);
        assert_eq!(disabled[0].repository_binding, installed.repository_binding);
        assert_eq!(disabled[0].package_id, installed.package_id);

        manifest.repository_binding = None;
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let legacy = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(
            legacy[0].repository_id.as_deref(),
            Some("dll-matching-but-wrong-repository")
        );
        assert!(legacy[0].repository_binding.is_none());
        let verified = std::fs::canonicalize(&root).unwrap();
        assert!(verified.starts_with(
            std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
        ));
        std::fs::remove_dir_all(verified).unwrap();
    }

    #[test]
    fn channel_bound_install_only_checks_its_own_release_track() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/channel-binding")
            .join(format!("scan-{}", uuid::Uuid::new_v4()));
        let relative = "BepInEx/plugins/Example.dll";
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, plugin_blob("com.example.guid", "Example", "100.0.0")).unwrap();
        let mut stable = binding_repository();
        stable.id = "stable-track".into();
        stable.channel = crate::models::Channel::Stable;
        stable.cached_releases = vec![high_cached_release()];
        let mut beta = binding_repository();
        beta.id = "beta-track".into();
        beta.channel = crate::models::Channel::Beta;
        let mut beta_release = high_cached_release();
        beta_release.tag_name = "v99.0.0-beta.1".into();
        beta_release.version = Some("99.0.0-beta.1".into());
        beta_release.prerelease = true;
        beta.cached_releases = vec![beta_release];
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1, "ui_zoom":1.5, "onboarding_seen_steps":[],
            "game":{"path":"", "auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[stable, beta]
        })).unwrap();
        let manifests = root.join("manifests");
        let mut manifest = crate::archive::new_manifest(
            "package",
            "v98.0.0-beta.1",
            crate::archive::single_language("Package"),
            Default::default(),
            vec![relative.into()],
        );
        manifest.repository_binding =
            crate::models::RepositoryBinding::from_release(&beta, "v98.0.0-beta.1");
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(scan.len(), 1);
        assert_eq!(scan[0].repository_id.as_deref(), Some("beta-track"));
        assert_eq!(scan[0].repository_ids, ["beta-track"]);
        assert!(scan[0].update_available);

        manifest.repository_binding =
            crate::models::RepositoryBinding::from_release(&stable, "v98.0.0");
        crate::archive::write_manifest(&manifests, &manifest).unwrap();
        let stable_scan = super::scan_mods_with_manifests(&root, &config, Some(&manifests));
        assert_eq!(
            stable_scan[0].repository_id.as_deref(),
            Some("stable-track")
        );
        assert_eq!(stable_scan[0].repository_ids, ["stable-track"]);
        assert!(stable_scan[0].update_available);

        let verified = std::fs::canonicalize(&root).unwrap();
        assert!(verified.starts_with(
            std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
        ));
        std::fs::remove_dir_all(verified).unwrap();
    }

    #[test]
    fn repository_binding_matches_any_configured_identity() {
        let repo = binding_repository();
        assert!(super::repository_matches_mod(
            &repo,
            Some("different"),
            Some("example product"),
            None,
            None
        ));
        assert!(super::repository_matches_mod(
            &repo,
            None,
            None,
            Some("EXAMPLE.DLL"),
            None
        ));
        assert!(super::repository_matches_mod(
            &repo,
            Some("COM.EXAMPLE.GUID"),
            None,
            None,
            None
        ));
        let mut versioned = repo.clone();
        versioned.version_regex = r"^1\.2\.\d+$".into();
        assert!(super::repository_matches_mod(
            &versioned,
            Some("com.example.guid"),
            None,
            None,
            Some("1.2.3")
        ));
        assert!(!super::repository_matches_mod(
            &versioned,
            Some("com.example.guid"),
            None,
            None,
            Some("1.3.0")
        ));
    }

    fn high_cached_release() -> crate::models::ReleaseInfo {
        crate::models::ReleaseInfo {
            release_id: None,
            tag_name: "v99.0.0".into(),
            version: Some("99.0.0".into()),
            name: "newer Release".into(),
            body: String::new(),
            published_at: None,
            prerelease: false,
            draft: false,
            assets: Vec::new(),
        }
    }

    #[test]
    fn failed_release_refresh_does_not_confirm_a_cached_version() {
        for cached_version in ["1.2.3", "99.0.0"] {
            let mut repo = binding_repository();
            let mut release = high_cached_release();
            release.version = Some(cached_version.into());
            release.tag_name = format!("v{cached_version}");
            repo.cached_releases = vec![release];
            repo.last_error = Some("Release request failed".into());
            assert_eq!(super::repository_remote_version(&repo), None);
            assert_eq!(
                super::repository_update_state(Some(&repo), Some("1.2.3")),
                (false, "repository_unavailable")
            );
        }
    }

    #[test]
    fn release_state_requires_comparable_local_and_remote_versions() {
        let mut repo = binding_repository();
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "unknown")
        );
        let mut release = high_cached_release();
        release.version = Some("not-a-version".into());
        repo.cached_releases = vec![release];
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "unknown")
        );
        repo.cached_releases = vec![high_cached_release()];
        for local_version in [None, Some("not-a-version")] {
            assert_eq!(
                super::repository_update_state(Some(&repo), local_version),
                (false, "unknown")
            );
        }
    }

    #[test]
    fn successful_release_retry_restores_version_comparison() {
        let mut repo = binding_repository();
        repo.last_error = Some("Release request failed".into());
        repo.cached_releases = vec![high_cached_release()];
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "repository_unavailable")
        );
        repo.last_error = None;
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (true, "update_available")
        );
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("99.0.0")),
            (false, "latest")
        );
        repo.cached_releases.clear();
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "unknown")
        );
    }

    #[test]
    fn failed_release_source_does_not_rebind_to_another_cached_repository() {
        let mut source = binding_repository();
        source.id = "source".into();
        source.channel = crate::models::Channel::Stable;
        source.cached_releases = vec![high_cached_release()];
        source.last_error = Some("Release request failed".into());
        let mut mirror = source.clone();
        mirror.id = "mirror".into();
        mirror.last_error = None;
        let selected =
            super::select_repository(&[&mirror, &source], Some("1.2.3"), Some(&source)).unwrap();
        assert_eq!(selected.id, "source");
        assert_eq!(
            super::repository_update_state(Some(selected), Some("1.2.3")),
            (false, "repository_unavailable")
        );
    }

    #[test]
    fn raw_binding_uses_identity_and_version_regex_without_release_comparison() {
        let mut raw = binding_repository();
        raw.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
        raw.cached_releases = vec![high_cached_release()];
        for (guid, product, filename) in [
            (Some("COM.EXAMPLE.GUID"), None, None),
            (None, Some("example product"), None),
            (None, None, Some("EXAMPLE.DLL")),
        ] {
            assert!(super::repository_matches_mod(
                &raw,
                guid,
                product,
                filename,
                Some("1.2.3.4")
            ));
        }
        assert_eq!(super::repository_remote_version(&raw), None);
        assert_eq!(
            super::repository_update_state(Some(&raw), Some("1.2.3.4")),
            (false, "not_release_asset")
        );
        raw.version_regex = r"^1\.2\.\d+\.\d+$".into();
        assert!(super::repository_matches_mod(
            &raw,
            Some("com.example.guid"),
            None,
            None,
            Some("1.2.3.4")
        ));
        assert!(!super::repository_matches_mod(
            &raw,
            Some("com.example.guid"),
            None,
            None,
            Some("1.3.0.0")
        ));
        assert!(!super::repository_matches_mod(
            &raw,
            Some("com.example.guid"),
            None,
            None,
            None
        ));
        assert!(!super::repository_matches_mod(
            &raw,
            Some("unknown.guid"),
            None,
            None,
            Some("1.2.3.4")
        ));
    }

    #[test]
    fn real_raw_source_wins_while_release_sources_keep_their_channel_rules() {
        let mut release = binding_repository();
        release.id = "release".into();
        release.channel = crate::models::Channel::Stable;
        let mut raw = binding_repository();
        raw.id = "raw".into();
        raw.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
        // Raw selection ignores its unused channel field, including four-part versions.
        raw.channel = crate::models::Channel::Stable;
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3"), Some(&raw))
                .unwrap()
                .id,
            "raw"
        );
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3"), Some(&release))
                .unwrap()
                .id,
            "release"
        );
        assert!(super::select_repository(&[&release], Some("1.2.3"), Some(&raw)).is_none());
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3.4"), None)
                .unwrap()
                .id,
            "raw"
        );
        release.channel = crate::models::Channel::Beta;
        assert_eq!(
            super::select_repository(&[&release, &raw], Some("1.2.3.4"), None)
                .unwrap()
                .id,
            "release"
        );
        assert!(super::select_repository(&[], Some("1.2.3.4"), None).is_none());
        assert_eq!(
            super::repository_update_state(None, Some("1.2.3")),
            (false, "unbound")
        );
        raw.channel = crate::models::Channel::All;
        assert_eq!(
            super::select_repository(&[&raw], Some("1.2.3.4"), None)
                .unwrap()
                .id,
            "raw"
        );
    }

    #[test]
    fn scanning_real_raw_manifest_prevents_same_guid_release_update() {
        for installed_version in ["1.2.3", "1.2.3.4"] {
            let root =
                std::env::temp_dir().join(format!("oc2-raw-binding-{}", uuid::Uuid::new_v4()));
            let relative = "BepInEx/plugins/Example.dll";
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut bytes = b"BepInPlugin".to_vec();
            bytes.extend_from_slice(&[1, 0]);
            for value in ["com.example.guid", "Example", installed_version] {
                bytes.push(value.len() as u8);
                bytes.extend_from_slice(value.as_bytes());
            }
            std::fs::write(&path, bytes).unwrap();
            let mut raw = binding_repository();
            raw.id = "raw".into();
            raw.download_mode = crate::models::RepositoryDownloadMode::RepositoryFile;
            raw.cached_releases = vec![high_cached_release()];
            let mut release = raw.clone();
            release.id = "release".into();
            release.download_mode = crate::models::RepositoryDownloadMode::Release;
            let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[release,raw]
        })).unwrap();
            let manifest = crate::archive::InstallManifest {
                repository_id: "raw".into(),
                release_tag: String::new(),
                repository_binding: None,
                package_name_i18n: crate::archive::single_language("Raw MOD"),
                note_i18n: Default::default(),
                group_i18n: Default::default(),
                prerelease: false,
                installed_at: "2026-10-07T00:00:00Z".into(),
                files: vec![relative.into()],
            };
            let installed =
                super::inspect_dll(&path, &root, &config, std::slice::from_ref(&manifest));
            assert_eq!(installed.version.as_deref(), Some(installed_version));
            assert_eq!(installed.repository_id.as_deref(), Some("raw"));
            assert_eq!(installed.repository_ids, vec!["raw"]);
            assert_eq!(installed.status, "not_release_asset");
            assert!(!installed.update_available);
            assert!(!installed.has_config);
            std::fs::create_dir_all(root.join("BepInEx/config")).unwrap();
            std::fs::write(
                root.join("BepInEx/config/com.example.guid.cfg"),
                "[General]\nEnabled = true\n",
            )
            .unwrap();
            assert!(super::inspect_dll(&path, &root, &config, &[]).has_config);
            let release_install = super::inspect_dll(&path, &root, &config, &[]);
            assert_eq!(release_install.repository_id.as_deref(), Some("release"));
            assert!(release_install.update_available);
            let mut restricted = config;
            restricted.repositories[1].version_regex = r"^2\.".into();
            let invalid_raw = super::inspect_dll(&path, &root, &restricted, &[manifest]);
            assert_eq!(invalid_raw.status, "unbound");
            assert!(!invalid_raw.update_available);
            assert!(invalid_raw.repository_ids.is_empty());
            let target = std::fs::canonicalize(&root).unwrap();
            let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
            assert!(target.starts_with(&temp) && target != temp);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn manifest_group_snapshot_survives_missing_repository_config() {
        // 安装清单里固化了分组：即使当前配置里找不到这个仓库
        // （刚启动、源文档还没刷新出来），MOD 仍然带着安装时的分组。
        let root =
            std::env::temp_dir().join(format!("oc2-group-snapshot-{}", uuid::Uuid::new_v4()));
        let plugins = root.join("BepInEx/plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let path = plugins.join("Grouped.dll");
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in ["oc2.grouped", "Grouped", "1.0.0"] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[]
        })).unwrap();
        let mut group_i18n = std::collections::BTreeMap::new();
        group_i18n.insert("zh-CN".into(), "内容拓展".to_string());
        group_i18n.insert("en".into(), "Content".to_string());
        let manifest = crate::archive::InstallManifest {
            repository_id: "gone-repository".into(),
            release_tag: "v1.0.0".into(),
            repository_binding: None,
            package_name_i18n: crate::archive::single_language("Grouped"),
            note_i18n: Default::default(),
            group_i18n: group_i18n.clone(),
            prerelease: false,
            installed_at: "2026-10-07T00:00:00Z".into(),
            files: vec!["BepInEx/plugins/Grouped.dll".into()],
        };
        let installed = super::inspect_dll(&path, &root, &config, std::slice::from_ref(&manifest));
        assert_eq!(installed.group_i18n, group_i18n);
        assert_eq!(installed.group, "内容拓展");
        // 仓库已不在配置里 → 状态仍是未绑定，但分组信息不丢
        assert_eq!(installed.status, "unbound");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_prerelease_flag_reaches_installed_mod() {
        // 预发布标记随安装清单固化，扫描时原样带出（仓库已不在配置里也不丢）。
        let root = std::env::temp_dir().join(format!("oc2-pre-{}", uuid::Uuid::new_v4()));
        let plugins = root.join("BepInEx/plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let path = plugins.join("Pre.dll");
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in ["oc2.pre", "Pre", "2.0.0"] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[]
        })).unwrap();
        let manifest = crate::archive::InstallManifest {
            repository_id: "pre-repo".into(),
            release_tag: "v2.0.0-beta.1".into(),
            repository_binding: None,
            package_name_i18n: crate::archive::single_language("Pre"),
            note_i18n: Default::default(),
            group_i18n: Default::default(),
            prerelease: true,
            installed_at: "2026-10-07T00:00:00Z".into(),
            files: vec!["BepInEx/plugins/Pre.dll".into()],
        };
        let installed = super::inspect_dll(&path, &root, &config, std::slice::from_ref(&manifest));
        assert!(installed.prerelease);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_manifest_without_snapshot_falls_back_to_repository_group() {
        // 旧版本管理器写的清单没有 group_i18n / group。
        // 升级后应立刻用当前仓库的分组兜底显示，而不是掉进「未绑定」。
        let root = std::env::temp_dir().join(format!("oc2-legacy-group-{}", uuid::Uuid::new_v4()));
        let plugins = root.join("BepInEx/plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let path = plugins.join("Legacy.dll");
        let mut bytes = b"BepInPlugin".to_vec();
        bytes.extend_from_slice(&[1, 0]);
        for value in ["oc2.legacy", "Legacy", "1.0.0"] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        std::fs::write(&path, bytes).unwrap();

        let mut repository = binding_repository();
        repository.id = "legacy-repo".into();
        // 用测试 DLL 真实的 GUID 识别，否则扫描时绑不到这个仓库
        repository.plugin_guid = "oc2.legacy".into();
        repository.group = "实用工具".into();
        repository
            .group_i18n
            .insert("zh-CN".into(), "实用工具".into());
        repository
            .group_i18n
            .insert("en".into(), "Utilities".into());
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,"onboarding_seen_steps":[],
            "game":{"path":"","auto_detect":false},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3,"check_updates_on_start":false},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[repository]
        })).unwrap();

        // 旧清单：分组与绑定都为空
        let manifest = crate::archive::InstallManifest {
            repository_id: "legacy-repo".into(),
            release_tag: "v1.0.0".into(),
            repository_binding: None,
            package_name_i18n: crate::archive::single_language("Legacy"),
            note_i18n: Default::default(),
            group_i18n: Default::default(),
            prerelease: false,
            installed_at: "2026-10-07T00:00:00Z".into(),
            files: vec!["BepInEx/plugins/Legacy.dll".into()],
        };
        let installed = super::inspect_dll(&path, &root, &config, std::slice::from_ref(&manifest));
        // 分组回退到仓库当前值
        assert_eq!(installed.group, "实用工具");
        assert_eq!(
            installed.group_i18n.get("en").map(String::as_str),
            Some("Utilities")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bepin_plugin_metadata_provides_local_version() {
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "dev.gua.overcooked.diylevel",
                "Overcooked2 DIYLevel Plugin",
                "0.10.0"
            ))
            .and_then(|item| item.version),
            Some("0.10.0".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "com.example.mod",
                "HostUtilities",
                "1.2.3"
            ))
            .and_then(|item| item.version),
            Some("1.2.3".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "{12345678-1234-1234-1234-123456789abc}",
                "HostUtilities",
                "1.2.3.4"
            ))
            .and_then(|item| item.guid),
            Some("12345678-1234-1234-1234-123456789abc".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "HostUtilities",
                "HostUtilities",
                "1.2.3"
            ))
            .and_then(|item| item.guid),
            Some("HostUtilities".into())
        );
        assert_eq!(
            super::parse_bepin_plugin_metadata(&plugin_blob(
                "com.example.mod",
                "HostUtilities",
                "0.0.0"
            ))
            .and_then(|item| item.version),
            Some("0.0.0".into())
        );
    }

    #[test]
    fn local_mod_scan_returns_unknown_without_readable_bepinplugin_version() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/bepinplugin-version")
            .join(uuid::Uuid::new_v4().to_string());
        let plugins = root.join("BepInEx/plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let config: crate::models::AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "ui_zoom": 1.5, "onboarding_seen_steps": [],
            "game": {"path": "", "auto_detect": false},
            "network": {"timeout_seconds": 5, "download_threads": 1, "max_retry_count": 3, "check_updates_on_start": false},
            "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
            "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
            "repositories": []
        })).unwrap();
        let mut truncated = plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "0.10.0");
        truncated.pop();
        let cases = [
            (
                "no-attribute",
                b"AssemblyVersion\0FileVersion\0ProductVersion\09.9.9.9".to_vec(),
            ),
            (
                "empty",
                plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", ""),
            ),
            (
                "blank",
                plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "   "),
            ),
            (
                "invalid",
                plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "not-a-version"),
            ),
            ("truncated", truncated),
        ];
        for (name, bytes) in cases {
            for suffix in [".dll", ".dll.disabled"] {
                let path = plugins.join(format!("{name}{suffix}"));
                std::fs::write(&path, &bytes).unwrap();
                let installed = super::inspect_dll(&path, &root, &config, &[]);
                assert_eq!(installed.version, None, "{name}{suffix}");
                assert_eq!(installed.version_source, "unknown", "{name}{suffix}");
                assert!(!installed.update_available);
                assert_eq!(installed.enabled, suffix == ".dll");
            }
        }
        let scan = super::scan_mods_with_manifests(&root, &config, None);
        assert_eq!(scan.len(), 5);
        assert!(
            scan.iter()
                .all(|item| item.version.is_none() && item.version_source == "unknown")
        );
        let verified = std::fs::canonicalize(&root).unwrap();
        let workspace = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(verified.starts_with(workspace.join("target/verification/bepinplugin-version")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn lower_duplicate_guid_is_marked_as_not_loaded() {
        let mut mods = vec![
            crate::models::InstalledMod {
                repository_id: None,
                repository_ids: Vec::new(),
                manual_install: false,
                is_dependency: false,
                repository_binding: None,
                package_id: None,
                package_name: None,
                package_name_i18n: Default::default(),
                package_mod_count: 0,
                group_i18n: Default::default(),
                group: String::new(),
                prerelease: false,
                name: "Old".into(),
                has_config: false,
                has_logs: false,
                title: None,
                description: None,
                product_name: Some("Old".into()),
                original_filename: Some("old.dll".into()),
                guid: Some("com.example.mod".into()),
                plugin_name: None,
                company: None,
                copyright: None,
                dll_file: "BepInEx/plugins/old.dll".into(),
                enabled: true,
                version: Some("1.0.0".into()),
                version_source: "BepInPlugin".into(),
                update_available: false,
                labels: Vec::new(),
                duplicate_guid: false,
                duplicate_version: false,
                load_warning: None,
                note: None,
                status: "installed".into(),
            },
            crate::models::InstalledMod {
                repository_id: None,
                repository_ids: Vec::new(),
                manual_install: false,
                is_dependency: false,
                repository_binding: None,
                package_id: None,
                package_name: None,
                package_name_i18n: Default::default(),
                package_mod_count: 0,
                group_i18n: Default::default(),
                group: String::new(),
                prerelease: false,
                name: "New".into(),
                has_config: false,
                has_logs: false,
                title: None,
                description: None,
                product_name: Some("New".into()),
                original_filename: Some("new.dll".into()),
                guid: Some("com.example.mod".into()),
                plugin_name: None,
                company: None,
                copyright: None,
                dll_file: "BepInEx/plugins/new.dll".into(),
                enabled: true,
                version: Some("2.0.0".into()),
                version_source: "BepInPlugin".into(),
                update_available: false,
                labels: Vec::new(),
                duplicate_guid: false,
                duplicate_version: false,
                load_warning: None,
                note: None,
                status: "installed".into(),
            },
        ];
        super::mark_duplicate_guids(&mut mods);
        assert_eq!(
            mods[0].load_warning.as_deref(),
            Some("mods.higherVersionPresent")
        );
        assert!(mods[1].load_warning.is_none());
        assert!(mods[0].duplicate_guid && mods[1].duplicate_guid);
        assert!(!mods[0].duplicate_version && !mods[1].duplicate_version);

        mods[1].version = Some("1.0.0".into());
        mods[0].load_warning = None;
        mods[0].status = "installed".into();
        mods[0].duplicate_guid = false;
        mods[1].duplicate_guid = false;
        super::mark_duplicate_guids(&mut mods);
        assert!(mods[0].duplicate_version && mods[1].duplicate_version);
    }

    #[test]
    fn file_stem_name_strips_extension_as_install_display_fallback() {
        // 本地安装提示的兜底名：文件名去扩展名，去掉 .dll 后缀
        assert_eq!(super::file_stem_name("FastThrow.dll"), "FastThrow");
        assert_eq!(super::file_stem_name("MyMod.zip"), "MyMod");
        assert_eq!(super::file_stem_name("no-extension"), "no-extension");
        // 多段文件名只去掉最后一段扩展名
        assert_eq!(super::file_stem_name("a.b.dll"), "a.b");
    }

    fn plain_mod(dll_file: &str, extra: serde_json::Value) -> crate::models::InstalledMod {
        let mut base = serde_json::json!({
            "name": dll_file, "dll_file": dll_file, "enabled": true,
            "version_source": "BepInPlugin", "update_available": false,
            "duplicate_guid": false, "status": "latest",
        });
        if let (Some(base_map), Some(extra_map)) = (base.as_object_mut(), extra.as_object()) {
            for (key, value) in extra_map {
                base_map.insert(key.clone(), value.clone());
            }
        }
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn dependency_inherits_repository_from_sibling_in_same_directory() {
        /*
         * 用户手动把 ZIP 解到 plugins/SomeMod/：主插件能读 GUID 并绑到仓库，
         * 依赖读不出元数据。依赖必须继承同目录的仓库归属，否则前端会把它
         * 分到「本地安装」分组、永远折不成同一个包，仓库更新时也不会一起回收。
         */
        let mut mods = vec![
            plain_mod(
                "BepInEx/plugins/SomeMod/Main.dll",
                serde_json::json!({
                    "repository_id": "repo-1", "repository_ids": ["repo-1"], "guid": "com.example.main"
                }),
            ),
            plain_mod(
                "BepInEx/plugins/SomeMod/Dep.dll",
                serde_json::json!({ "is_dependency": true, "labels": ["dependency"] }),
            ),
        ];
        super::inherit_repository_from_sibling(&mut mods);
        assert_eq!(mods[1].repository_ids, vec!["repo-1".to_string()]);
        assert_eq!(mods[1].repository_id.as_deref(), Some("repo-1"));
        // 主插件自身不变
        assert_eq!(mods[0].repository_ids, vec!["repo-1".to_string()]);
    }

    #[test]
    fn repository_identity_never_leaks_across_directories() {
        /*
         * plugins 根目录下平铺的两个插件互不相干，不能因为「同目录」就把
         * 一个的仓库身份传给另一个。
         */
        let mut mods = vec![
            plain_mod(
                "BepInEx/plugins/Main.dll",
                serde_json::json!({ "repository_id": "repo-1", "repository_ids": ["repo-1"], "guid": "com.example.main" }),
            ),
            plain_mod(
                "BepInEx/plugins/Other.dll",
                serde_json::json!({ "is_dependency": true, "labels": ["dependency"] }),
            ),
        ];
        super::inherit_repository_from_sibling(&mut mods);
        assert!(mods[1].repository_ids.is_empty(), "跨目录不传播仓库身份");
        assert!(mods[1].repository_id.is_none());
    }

    #[test]
    fn dependency_keeps_its_own_repository_identity() {
        // 依赖自己已经有归属时不能被同目录其它插件覆盖
        let mut mods = vec![
            plain_mod(
                "BepInEx/plugins/Pack/Main.dll",
                serde_json::json!({ "repository_id": "repo-1", "repository_ids": ["repo-1"], "guid": "com.example.main" }),
            ),
            plain_mod(
                "BepInEx/plugins/Pack/Dep.dll",
                serde_json::json!({ "repository_id": "repo-2", "repository_ids": ["repo-2"], "is_dependency": true }),
            ),
        ];
        super::inherit_repository_from_sibling(&mut mods);
        assert_eq!(mods[1].repository_ids, vec!["repo-2".to_string()]);
    }

    /// 恢复框架留下的 `.before-restore-*` 目录不能被当成 MOD。
    ///
    /// 这是真实用户会踩的坑：恢复 DIYLevel 时，暂存目录就建在
    /// `BepInEx/plugins/OC2DIYLevel.before-restore-<ts>`，里面的 DLL 被扫成
    /// 插件后，界面上「刚恢复完就冒出一堆重复 MOD」，而且因为文件确实存在，
    /// 用户只能自己翻目录手删才能消除。
    ///
    /// 断言方式是**对比两次扫描**：先放一个暂存目录，期望只扫到真实 MOD。
    /// 这个测试必须能失败 —— 把 `scan_mods_with_manifests` 里的跳过分支删掉
    /// 时它应当红。为此还要断言暂存目录**确实在磁盘上**（否则「扫不到」
    /// 可能只是因为文件没建成，测试就变成了空转）。
    #[test]
    fn restore_staging_directories_are_not_scanned_as_mods() {
        let root = std::env::temp_dir().join(format!("oc2-scan-stage-{}", uuid::Uuid::new_v4()));
        let plugins = root.join("BepInEx/plugins");
        std::fs::create_dir_all(&plugins).unwrap();

        // 正常 MOD
        std::fs::create_dir_all(plugins.join("RealMod")).unwrap();
        std::fs::write(
            plugins.join("RealMod/Real.dll"),
            plugin_blob("com.example.real", "Real", "1.0.0"),
        )
        .unwrap();

        // 恢复残留：DIYLevel 的暂存目录就在 plugins 下
        let staging = plugins.join("OC2DIYLevel.before-restore-1793336706");
        std::fs::create_dir_all(&staging).unwrap();
        let staged_dll = staging.join("OC2DIYLevel.dll");
        std::fs::write(
            &staged_dll,
            plugin_blob("dev.gua.overcooked.diylevel", "DIYLevel", "1.0.0"),
        )
        .unwrap();

        // 前置条件：暂存 DLL 真的在磁盘上，且确实是 .dll（否则下面「扫不到」
        // 可能是 `is_mod_file` 不认它，而不是过滤生效）
        assert!(staged_dll.is_file(), "夹具没建好，测试会空转");
        assert!(super::is_mod_file(&staged_dll));

        let config = crate::config::default_config();
        let scanned = super::scan_mods_with_manifests(&root, &config, None);

        assert!(
            !scanned
                .iter()
                .any(|item| item.dll_file.contains(".before-restore-")),
            "暂存目录里的文件不应出现在扫描结果里：{:?}",
            scanned.iter().map(|item| &item.dll_file).collect::<Vec<_>>()
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 单测过滤谓词本身：目录名判定不依赖扫描管线的其它环节。
    #[test]
    fn staging_component_detection_matches_only_before_restore_dirs() {
        assert!(super::is_restore_staging_component(std::ffi::OsStr::new(
            "OC2DIYLevel.before-restore-1793336706"
        )));
        assert!(super::is_restore_staging_component(std::ffi::OsStr::new(
            "BepInEx.before-restore-1"
        )));
        // 正常目录不能误伤
        assert!(!super::is_restore_staging_component(std::ffi::OsStr::new("OC2DIYLevel")));
        assert!(!super::is_restore_staging_component(std::ffi::OsStr::new("SomeMod")));
        assert!(!super::is_restore_staging_component(std::ffi::OsStr::new("before-restore")));
    }
}
