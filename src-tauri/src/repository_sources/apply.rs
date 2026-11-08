use crate::app_state::AppState;
use crate::archive;
use crate::models::{AppConfig, RepositoryBinding, RepositorySource, RepositorySourceImportResult};
use crate::repository_sources;
use crate::repository_sources::SourceLocation;
use crate::repository_sources::source_address_key;
use chrono::Utc;
use std::collections::HashSet;
use uuid::Uuid;

pub(crate) async fn import_repository_source_into_config(
    state: &AppState,
    source_id: String,
    source_url: String,
    builtin_fallback: bool,
) -> Result<RepositorySourceImportResult, String> {
    let snapshot = state.config.load_or_default();
    let policy = snapshot
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(|| crate::network_policy::NetworkPolicy::active())?;
    crate::network_policy::NetworkPolicy::in_snapshot(policy, async {

    if crate::community::is_source(&source_id) {
        return Err("source.communityManaged".into());
    }
    log::info!("开始刷新仓库源：源={}", source_id);
    // Clear this source and all descendants before loading. The lock is
    // deliberately released during the network request so other sources can
    // download and apply their own files concurrently. Keep the source itself
    // so the UI can show its error state, but remove stale descendants and
    // repositories instead of restoring them after a failed request.
    let (source_url, builtin_fallback_available, parent_plan) = {
        let _cache_guard = state.repository_cache.lock().await;
        let _operation_guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let original_config = state.config.load_or_default();
        let Some(source) = original_config
            .repository_sources
            .iter()
            .find(|source| source.id == source_id)
        else {
            log::warn!("刷新仓库源失败：源不存在，源={}", source_id);
            return Err("source.sourceMissing".into());
        };
        if !source.url.trim().eq_ignore_ascii_case(source_url.trim()) {
            log::warn!("刷新仓库源失败：请求地址与当前配置不一致，源={}", source_id);
            return Err("source.addressMismatch".into());
        }
        let builtin_fallback_available = source.id == repository_sources::DEFAULT_INDEX_ID
            && [
                repository_sources::DEFAULT_INDEX_GITEE_URL,
                repository_sources::DEFAULT_INDEX_GITHUB_URL,
            ]
            .contains(&source.url.as_str())
            && source.builtin
            && source.parent_id.is_none();
        if builtin_fallback && !builtin_fallback_available {
            return Err("source.noBuiltinFallback".into());
        }
        let source_url = source.url.clone();
        log::info!(
            "仓库源开始读取：源={}，地址={}，父源={}",
            source_id,
            repository_sources::safe_log_address(&source_url),
            source.parent_id.as_deref().unwrap_or("无")
        );
        let mut cleared_config = original_config.clone();
        let (removed_repository_count, removed_child_source_count) =
            repository_sources::clear_source_cache(
                &mut cleared_config,
                std::slice::from_ref(&source_id),
            );
        state.config.save(&cleared_config).map_err(|error| {
            log::error!(
                "刷新仓库源前清理旧内容失败：源={}，原因={}",
                source_id,
                error
            );
            error.to_string()
        })?;
        log::info!(
            "刷新仓库源前已清理旧内容：源={}，子源={}，仓库={}",
            source_id,
            removed_child_source_count,
            removed_repository_count
        );
        (
            source_url,
            builtin_fallback_available && !builtin_fallback,
            source.network_plan.clone(),
        )
    };
    let document_url = if builtin_fallback {
        log::warn!(
            "官方远程源不可用，开始加载内置仓库源：源={}，文件={}",
            source_id,
            repository_sources::BUILTIN_INDEX
        );
        repository_sources::BUILTIN_INDEX
    } else {
        &source_url
    };
    let location = SourceLocation::parse(document_url).map_err(|error| {
        log::error!(
            "仓库源地址解析失败：源={}，错误类别={}",
            source_id,
            repository_sources::error_category(&error)
        );
        error.to_string()
    })?;
    // 超时由 load_document 内部固定为 SOURCE_DOCUMENT_TIMEOUT（30 秒）
    let document = match repository_sources::load_document(&location, &parent_plan).await {
        Ok(document) => document,
        Err(error) => {
            log::error!(
                "仓库源读取失败：源={}，错误类别={}",
                source_id,
                repository_sources::error_category(&error)
            );
            // Read failures use access_error; invalid content stays Err. The
            // frontend decides fallback independently of this result shape.
            if matches!(error, crate::config::ConfigError::Read(_)) {
                return Ok(RepositorySourceImportResult {
                    preview: None,
                    access_error: Some(error.to_string()),
                    builtin_fallback_available,
                });
            }
            return Err(error.to_string());
        }
    };
    let repositories = document
        .repositories
        .into_iter()
        .map(|mut repository| {
            // source_id in an exported configuration describes the author's
            // local manager state, so it is deliberately ignored here.
            repository.source_id = None;
            repository
        })
        .collect::<Vec<_>>();
    let source_repository_keys = repositories
        .iter()
        .map(crate::config::export_key)
        .collect::<std::collections::HashSet<_>>();
    let child_sources = document.repository_sources;
    let child_source_count = child_sources.len();
    let bundle = crate::models::RepositoryBundle {
        format: "oc2-mod-repository-bundle".into(),
        schema_version: 1,
        exported_at: Utc::now().to_rfc3339(),
        repositories,
        repository_sources: Vec::new(),
    };
    let _cache_guard = state.repository_cache.lock().await;
    let _operation_guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    config
        .repositories
        .retain(|repo| repo.source_id.as_deref() != Some(source_id.as_str()));
    let parent_url = config
        .repository_sources
        .iter()
        .find(|source| source.id == source_id)
        .map(|source| source.url.trim().to_ascii_lowercase());
    let parent_builtin = config
        .repository_sources
        .iter()
        .find(|source| source.id == source_id)
        .is_some_and(|source| source.builtin);
    let declared_child_ids = child_sources
        .iter()
        .filter(|source| {
            parent_url
                .as_deref()
                .map(|url| !source.url.trim().eq_ignore_ascii_case(url))
                .unwrap_or(true)
        })
        .map(|source| source.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let stale_child_ids = config
        .repository_sources
        .iter()
        .filter(|source| source.parent_id.as_deref() == Some(source_id.as_str()))
        .filter(|source| !declared_child_ids.contains(source.id.as_str()))
        .map(|source| source.id.clone())
        .collect::<Vec<_>>();
    let mut removed_child_ids = stale_child_ids;
    let mut changed = true;
    while changed {
        changed = false;
        for source in &config.repository_sources {
            if let Some(parent_id) = source.parent_id.as_deref()
                && removed_child_ids.iter().any(|removed| removed == parent_id)
                && !removed_child_ids
                    .iter()
                    .any(|removed| removed == &source.id)
            {
                removed_child_ids.push(source.id.clone());
                changed = true;
            }
        }
    }
    if !removed_child_ids.is_empty() {
        config
            .repository_sources
            .retain(|source| !removed_child_ids.iter().any(|id| id == &source.id));
        config.repositories.retain(|repo| {
            repo.source_id
                .as_ref()
                .map(|id| !removed_child_ids.iter().any(|child_id| child_id == id))
                .unwrap_or(true)
        });
    }
    for child in child_sources {
        if crate::community::is_source(&child.id) {
            log::warn!("忽略源文档中的保留社区源标识：源={}", source_id);
            continue;
        }
        let child_url = child.url.trim().to_string();
        // Remote declarations cannot elevate their own privilege. An omitted
        // value inherits the parent; an explicit false cuts the chain, and
        // every descendant then remains non-builtin.
        let child_builtin = parent_builtin && child.builtin.unwrap_or(true);
        if parent_url
            .as_deref()
            .map(|url| child_url.eq_ignore_ascii_case(url))
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(existing) = config.repository_sources.iter_mut().find(|source| {
            source.id == child.id && source.parent_id.as_deref() == Some(source_id.as_str())
        }) {
            // 声明里的 nickname 已是 zh-CN 兜底值，运行时镜像与多语言表一起写
            existing.nickname = child.nickname.trim().to_string();
            existing.nickname_i18n = child.nickname_i18n;
            existing.url = child_url;
            existing.region = child.region.clone();
            existing.network_plan = child.network_plan.clone();
            existing.builtin = child_builtin;
            continue;
        }
        if config.repository_sources.iter().any(|source| {
            source_address_key(&source.url, source.region.as_deref())
                == source_address_key(&child_url, child.region.as_deref())
        }) {
            // Keep a single owner per URL and region, while allowing regional
            // variants of the same source file to coexist.
            continue;
        }
        let child_id = if config
            .repository_sources
            .iter()
            .any(|source| source.id == child.id)
        {
            Uuid::new_v4().to_string()
        } else {
            child.id.trim().to_string()
        };
        config.repository_sources.push(RepositorySource {
            network_plan: child.network_plan.clone(),
            id: child_id,
            nickname: child.nickname.trim().to_string(),
            nickname_i18n: child.nickname_i18n,
            url: child_url,
            region: child.region,
            builtin: child_builtin,
            parent_id: Some(source_id.clone()),
        });
    }
    let preview = state
        .config
        .import_bundle(&mut config, &bundle, "overwrite");
    let mut used_ids = config
        .repositories
        .iter()
        .map(|repo| repo.id.clone())
        .collect::<HashSet<_>>();
    for repo in &mut config.repositories {
        let identity = crate::config::repo_key(repo);
        if !crate::community::is_repository(repo) && source_repository_keys.contains(&identity) {
            let key = crate::config::source_repository_key(&source_id, &identity);
            used_ids.remove(&repo.id);
            if let Some(previous_id) = config.source_repository_ids.get(&key)
                && !used_ids.contains(previous_id)
            {
                repo.id = previous_id.clone();
                /*
                 * 把清理阶段暂存的 Release 缓存接回来。
                 *
                 * 不接的话，5 分钟缓存**永远命中不了**：整批刷新先删光仓库、
                 * 再从源文档重建，重建出来的是全新空仓库，于是每次刷新都把
                 * 整棵仓库树的 API 重打一遍（20+ 个仓库、几十个请求，还会撞
                 * Gitee 速率限制）。缓存本身没坏，只是被「重建」这件事丢了。
                 *
                 * ID 已由 `source_repository_ids` 保住，所以按 ID 取得到。
                 */
                if let Some(staged) = config.staged_release_cache.remove(previous_id) {
                    repo.cached_releases = staged.releases;
                    repo.releases_cached_at = staged.cached_at;
                    repo.last_error = staged.last_error;
                }
            }
            used_ids.insert(repo.id.clone());
            config.source_repository_ids.insert(key, repo.id.clone());
            repo.source_id = Some(source_id.clone());
        }
    }
    // 每次只应用一个源：仅移除上面已认领的条目，保留其他源尚未接回的缓存。
    // 当前没有批次末清表入口；未认领项也会随配置保存。
    state.config.save(&config).map_err(|error| {
        log::error!("仓库源刷新后保存配置失败：源={}，原因={}", source_id, error);
        error.to_string()
    })?;
    // 配置已经是本次源文档的最新内容：此时才同步安装清单里的分组快照。
    // 只有分组确实变化才写文件，避免每次刷新都重写全部清单。
    sync_manifest_groups(&state, &config);
    log::info!(
        "仓库源刷新应用完成：源={}，仓库={}，新增={}，重复={}，冲突={}，子源声明={}，移除旧子源={}，结果有效={}",
        source_id,
        bundle.repositories.len(),
        preview.additions.len(),
        preview.duplicates.len(),
        preview.conflicts.len(),
        child_source_count,
        removed_child_ids.len(),
        preview.valid
    );
    Ok(RepositorySourceImportResult {
        preview: Some(preview),
        access_error: None,
        builtin_fallback_available,
    })
    }).await
}

/// 把安装清单里的分组快照同步成仓库当前的分组，只在真正变化时写文件。
///
/// 仓库源是权威数据源：清单里的分组只是缓存。这里在源文档刷新成功、
/// 配置已是最新内容后调用，因此不会出现「刚启动还没刷新 → 分类被清空」。
///
/// 同时补齐旧版本升级遗留的缺失字段：
///   - `group_i18n` / `group` 为空（旧清单）→ 从当前仓库补全
///   - `repository_binding` 缺失（更旧的清单）→ 按仓库 + release_tag 重建，
///     没有 release_tag 的 RAW 安装则留空（它本来就没有版本绑定）
fn sync_manifest_groups(state: &AppState, config: &AppConfig) {
    let manifest_dir = state.config.manifests_dir();
    let manifests = archive::read_manifests(&manifest_dir);
    let mut updated = 0usize;
    for mut manifest in manifests {
        let Some(repository) = config
            .repositories
            .iter()
            .find(|repo| repo.id == manifest.repository_id)
        else {
            continue;
        };

        // 清单现在只存 i18n 快照，比较时也只看它
        let group_changed = manifest.group_i18n != repository.group_i18n;
        /*
         * 包名的多语言快照也要同步：仓库名改了之后，MOD 管理页的分组标题
         * 会更新，但包名停在上次安装时的旧翻译，两边对不上。
         * 只在仓库确实提供了 name_i18n 时才覆盖，避免把手工补的名字抹掉。
         */
        let name_changed =
            !repository.name_i18n.is_empty() && manifest.package_name_i18n != repository.name_i18n;
        // 旧清单没有版本绑定：能用 release_tag 重建就补上
        let binding_missing = manifest.repository_binding.is_none()
            && !manifest.release_tag.trim().is_empty()
            && repository.download_mode == crate::models::RepositoryDownloadMode::Release;
        // 旧清单没记预发布标记：从缓存的 Release 里按 tag 找回（找得到才改）
        let prerelease_missing = !manifest.prerelease
            && repository
                .cached_releases
                .iter()
                .find(|release| release.tag_name == manifest.release_tag)
                .is_some_and(|release| release.prerelease);
        if !group_changed && !name_changed && !binding_missing && !prerelease_missing {
            continue;
        }

        if group_changed {
            log::info!(
                "更新安装清单分组快照：仓库={}，旧分组语言数={}，新分组语言数={}",
                manifest.repository_id,
                manifest.group_i18n.len(),
                repository.group_i18n.len()
            );
            manifest.group_i18n = repository.group_i18n.clone();
        }
        if name_changed {
            manifest.package_name_i18n = repository.name_i18n.clone();
            log::info!(
                "更新安装清单仓库名快照：仓库={}，语言数={}",
                manifest.repository_id,
                repository.name_i18n.len()
            );
        }
        if binding_missing {
            manifest.repository_binding =
                RepositoryBinding::from_release(repository, &manifest.release_tag);
            log::info!(
                "补齐安装清单版本绑定：仓库={}，标签={}",
                manifest.repository_id,
                manifest.release_tag
            );
        }
        if prerelease_missing {
            manifest.prerelease = true;
            log::info!(
                "补齐安装清单预发布标记：仓库={}，标签={}",
                manifest.repository_id,
                manifest.release_tag
            );
        }
        if let Err(error) = archive::write_manifest(&manifest_dir, &manifest) {
            log::warn!(
                "更新安装清单失败：仓库={}，原因={}",
                manifest.repository_id,
                error
            );
            continue;
        }
        updated += 1;
    }
    if updated > 0 {
        log::info!("安装清单分组快照更新完成：数量={}", updated);
    }
}

#[cfg(test)]
mod repository_source_refresh_tests;
