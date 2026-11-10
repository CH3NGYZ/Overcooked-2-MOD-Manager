use super::binding::inherit_repository_from_sibling;
use super::file_state::PLUGINS_DIR;
use super::file_state::canonical_dll_file;
use super::file_state::canonical_dll_name;
use super::file_state::is_disabled_mod_file;
use super::file_state::is_mod_file;
use super::identity::mark_duplicate_guids;
use super::identity::note_key_for_values;
use super::metadata::choose_local_version;
use super::metadata::placeholder_dll_details;
use super::status::bound_release;
use super::status::refresh_status_labels;
use super::status::repository_remote_version;
use crate::archive;
use crate::archive::InstallManifest;
use crate::models::AppConfig;
use crate::models::InstalledMod;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
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
pub(super) fn is_restore_staging_component(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|text| text.contains(".before-restore-"))
}

pub fn scan_mods_with_manifests(
    game: &Path,
    config: &AppConfig,
    manifest_dir: Option<&Path>,
) -> Vec<InstalledMod> {
    let plugins = game.join(PLUGINS_DIR);
    // 关卡集目录里的 DLL 是地图的随包插件（见 `level_sets::assign_plugins`），
    // 不是普通 MOD：它们跟着整个集目录在 levels 与停用根之间移动，
    // 再出现在 MOD 列表里会让用户单独改成 `.dll.disabled`，
    // 两套开关状态互相打架。
    let level_sets_root = crate::level_sets::levels_root(game);
    let legacy_disabled_root = crate::level_sets::legacy_disabled_root(game);
    /*
     * BepInEx 表层包的随包插件登记在 `manifests/level-set-plugins.json`：它们
     * 物理上躺在 `BepInEx/plugins/` 下（作者规划好的位置，不能搬走），归属却是
     * 地图集。出现在 MOD 列表里会让用户单独禁用一张地图的插件，与地图的
     * 装卸语义打架 —— 与下面排除 `levels/` 是同一层规则。
     */
    let registered: HashSet<String> = manifest_dir
        .map(crate::level_sets::registered_plugin_paths)
        .unwrap_or_default()
        .into_iter()
        .collect();
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
                let inside_staging =
                    entry
                        .path()
                        .strip_prefix(&plugins)
                        .ok()
                        .is_some_and(|relative| {
                            relative.components().any(|component| {
                                is_restore_staging_component(component.as_os_str())
                            })
                        });
                if inside_staging {
                    continue;
                }
                let path = entry.path();
                if path.starts_with(&level_sets_root) || path.starts_with(&legacy_disabled_root) {
                    continue;
                }
                /*
                 * 登记表里的路径是「相对游戏目录、正斜杠、小写比较」。这里用
                 * 字面 `game` 剥前缀，不能拿 canonicalize 后的路径混着比
                 * （与 `plugin_dll_files` 里注释的同一类坑）。
                 */
                let registered_hit = path
                    .strip_prefix(game)
                    .ok()
                    .map(|relative| relative.to_string_lossy().replace('\\', "/").to_lowercase())
                    .is_some_and(|relative| registered.contains(&relative));
                if registered_hit {
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
    let paths = paths
        .into_iter()
        .filter(|path| seen.insert(canonical_dll_file(path, game).to_ascii_lowercase()))
        .collect::<Vec<_>>();
    /*
     * 元数据读取彼此独立，交给中间层按上限并发（默认全部核心，设置里可调）。
     * 结果按 `paths` 原顺序返回，卡片顺序与串行扫描时一致；某个文件读失败
     * 只影响它自己那一行（占位成纯依赖）。
     */
    let requests = paths
        .iter()
        .map(|path| crate::metadata_read::MetadataRequest {
            path: path.clone(),
            display_name: canonical_dll_file(path, game),
            kind: crate::metadata_read::MetadataKind::ModDll,
        })
        .collect::<Vec<_>>();
    let workers = crate::concurrency::workers_from(config.mod_metadata_concurrency);
    let details = crate::metadata_read::read_metadata_batch(&requests, workers, &|_, _| {});
    let mut installed = paths
        .into_iter()
        .zip(details)
        .map(|(path, outcome)| inspect_dll(&path, &outcome, game, config, &manifests))
        .collect::<Vec<_>>();
    annotate_package_counts(&mut installed);
    inherit_repository_from_sibling(&mut installed);
    mark_duplicate_guids(&mut installed);
    // 更新判定只看「清单 tag vs 仓库 tag」；DLL 自报版本只参与抑制误报
    // （见 effective_local_version），不再有「DLL 版本低于清单就标记可更新」的二次扫描。
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
        "本地 MOD 扫描完成：MOD数量={}，MOD包数量={}，可更新={}，重复身份={}，目录遍历错误={}，并行度={}",
        installed.len(),
        packages,
        updates,
        duplicates,
        traversal_errors,
        workers
    );
    installed
}

/// 把一条 DLL 的元数据解析结果组装成一个 MOD 行。
///
/// 元数据由 `metadata_read` 的批次统一读出（`DllDetails`），本函数只做「读
/// 磁盘状态 + 与安装清单/仓库对照」的装配 —— 不再自己解析 DLL 字节，否则
/// 同一份文件会被读两遍（批次一遍、装配一遍）。
fn inspect_dll(
    path: &Path,
    outcome: &crate::metadata_read::MetadataOutcome,
    game: &Path,
    config: &AppConfig,
    manifests: &[InstallManifest],
) -> InstalledMod {
    let details = outcome
        .mod_dll()
        .cloned()
        .unwrap_or_else(|| placeholder_dll_details(&canonical_dll_file(path, game)));
    let details = &details;
    /*
     * 版本号从中间层的统一出口取（`MetadataValue::version`）：MOD 与地图的
     * 「本地版本」判定只看这一个值，不在各处认 `details.version` 字段。
     */
    let version = outcome
        .value
        .as_ref()
        .and_then(crate::metadata_read::MetadataValue::version);
    let (installed_version, version_source) = choose_local_version(version);
    let guid = details.guid.clone();
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
    let repository_binding = manifest
        .and_then(|manifest| manifest.repository_binding.as_ref())
        .filter(|binding| binding.is_valid());
    let (repository, repository_ids) = super::binding::match_repository(
        config,
        guid.as_deref(),
        details.product_name.as_deref(),
        details.original_filename.as_deref(),
        installed_version.as_deref(),
        repository_binding,
        manifest.map(|manifest| manifest.repository_id.as_str()),
    );
    log::trace!(
        "分析 MOD：文件={}，GUID={}，版本={}，候选仓库={}，绑定={}",
        path.display(),
        guid.as_deref().unwrap_or("未知"),
        installed_version.as_deref().unwrap_or("未知"),
        repository_ids.len(),
        repository.is_some()
    );
    /*
     * 显示名的回退链（BepInPlugin 名 → PE 产品名 → PE 标题 → 文件名）只有
     * `read_dll_details` 一份实现：MOD 卡片、安装预览、地图随包插件必须显示
     * 同一个名字，各自拼一串 `or_else` 迟早会分叉。
     */
    let name = details
        .name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "mods.unknownName".to_string());
    /*
     * 仓库文件（RAW）模式没有 Release 可比较，装的是一份固定文件。
     *
     * 这类仓库也可能在清单里留下 repository_binding（安装时按 release_tag
     * 建的），但 RAW 从不抓 Release、cached_releases 永远为空。若照常走
     * binding_update_state，bound_release 找不到 Release 就返回 unknown，
     * MOD 管理页于是永远显示「版本未知」——即使刷新早已完成。
     * 所以先按仓库模式分流：RAW 直接判「已安装」，不参与版本判断。
     */
    let (update_available, status) = super::status::installed_update_state(
        repository,
        repository_binding,
        installed_version.as_deref(),
    );
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
    let product_name = details.product_name.clone();
    let original_filename = details.original_filename.clone();
    let plugin_name = details.plugin_name.clone();
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
         * 预发布判定取清单快照标记或统一版本解析器的判定（含四段 beta）。
         *
         * 只读快照不够：作者可能没在 Release 上勾 prerelease，但版本号本身
         * 叫 `0.4.0-beta`、`0.4.0.5-beta` —— 这类同样要按预发布提示。
         * 与前端 isPrereleaseVersion 保持同一规则。
         */
        prerelease: manifest.is_some_and(|item| item.prerelease)
            || installed_version
                .as_deref()
                .is_some_and(crate::releases::is_prerelease_version),
        /*
         * 依赖快照：仓库还在配置里时前端用线上声明，仓库不在了（源下线/
         * 自定义源被删）才退回这份快照 —— 否则依赖表会凭空消失。
         */
        relations: manifest
            .map(|item| item.relations.clone())
            .unwrap_or_default(),
        name,
        title: details.title.clone(),
        description: details.description.clone(),
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
        company: details.company.clone(),
        copyright: details.copyright.clone(),
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

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(super) use test_support::inspect_dll_at;
