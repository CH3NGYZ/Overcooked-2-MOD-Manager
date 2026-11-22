use crate::app_state::AppState;
use crate::archive;
use crate::display_path::display_path;
use crate::http_diagnostics::redact_text;
use crate::level_sets::staging::cache_file_to_discard;
use crate::level_sets::staging::validate_map_release_sets;
use crate::level_sets::staging::zip_level_sets;
use crate::mod_changes::{downloaded_source::Source, transaction::Fingerprint};
use crate::models::{
    AppConfig, LevelSetImportResult, MapInstallOrigin, MapRepositoryConfig, StagedMapSource,
};
use crate::releases;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::State;

/// 准备一个地图来源：本地 ZIP 只解析，直链先下载到缓存目录。
///
/// 返回的 `path` 用于后续整链安装；`staged=true` 时调用方
/// 结束（导入完成或用户取消）后必须用 `discard_map_source` 清理，否则缓存
/// 目录里会留下下载下来的 ZIP。
#[tauri::command]
pub async fn stage_map_source(
    app: AppHandle,
    state: State<'_, AppState>,
    source: crate::mod_preview::ModSource,
) -> Result<StagedMapSource, String> {
    let stage = uuid::Uuid::new_v4();
    log::info!("开始暂存地图来源 stage={stage} kind={:?}", source.kind);
    let result = async {
        let config = state.config.load_or_default();
        let (path, downloaded) = match &source.kind {
            crate::mod_preview::ModSourceKind::Local => {
                let path = PathBuf::from(source.value.trim());
                if !path.is_file() {
                    return Err("mods.fileMissing".into());
                }
                if !path
                    .extension()
                    .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
                {
                    return Err("modPreview.unsupportedType".into());
                }
                (path, false)
            }
            crate::mod_preview::ModSourceKind::Url => {
                let (temp, _) = crate::direct_mod::download_remote_to_temp(
                    &app,
                    &state,
                    &source.value,
                    "levelSet.installMap",
                    crate::direct_mod::DIRECT_DOWNLOAD_THREADS,
                    crate::direct_mod::RemoteValidation::Zip,
                )
                .await?;
                (temp, true)
            }
        };
        inspect_staged_zip(&app, state.inner(), config, path, downloaded, None, stage).await
    }
    .await;
    if let Err(error) = &result {
        log::warn!(
            "地图来源暂存失败 stage={stage} kind={:?} reason={}",
            source.kind,
            redact_text(error),
        );
    }
    result
}

async fn inspect_staged_zip(
    app: &AppHandle,
    state: &AppState,
    config: AppConfig,
    path: PathBuf,
    downloaded: bool,
    release: Option<(MapRepositoryConfig, String)>,
    stage: uuid::Uuid,
) -> Result<StagedMapSource, String> {
    let zip = redact_text(&format!("{:?}", path.file_name()));
    log::info!("开始识别地图 ZIP stage={stage} zip={zip} downloaded={downloaded}");
    let parse_app = app.clone();
    let parse_path = path.clone();
    let downloaded_sources = state.downloaded_sources.clone();
    let parsed = tauri::async_runtime::spawn_blocking(move || {
        let downloaded_fingerprint = downloaded
            .then(|| Fingerprint::capture(&parse_path))
            .transpose()?;
        let sets = zip_level_sets(&parse_app, &config, &parse_path)?;
        let origin = if let Some((map, tag)) = release {
            let primary = validate_map_release_sets(&sets, &map)?;
            Some(MapInstallOrigin {
                repository_id: Some(map.id),
                level_set_name: primary.name.clone(),
                level_set_name_zh: primary.name_zh.clone().unwrap_or_default(),
                author: primary.author.clone(),
                mapset_version: tag,
            })
        } else {
            None
        };
        if let Some(fingerprint) = downloaded_fingerprint {
            if Fingerprint::capture(&parse_path)? != fingerprint {
                return Err("modChanges.payloadChanged".into());
            }
            downloaded_sources.record(
                stage,
                &parse_path,
                fingerprint,
                Source::Map(origin.clone()),
            )?;
        }
        Ok::<_, String>((sets, origin))
    })
    .await
    .map_err(|error| {
        log::warn!(
            "地图 ZIP 识别进程退出 stage={stage} reason={}",
            redact_text(&error.to_string())
        );
        "metadataRead.taskFailed".to_owned()
    })
    .and_then(|result| result);
    let (sets, origin) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            log::warn!(
                "地图 ZIP 识别未通过 stage={stage} zip={zip} reason={}",
                redact_text(&error)
            );
            if downloaded {
                if let Err(cleanup) = fs::remove_file(&path) {
                    log::warn!(
                        "地图暂存文件清理失败 stage={stage} zip={zip} reason={}",
                        redact_text(&cleanup.to_string())
                    );
                }
            }
            return Err(error);
        }
    };
    log::info!(
        "地图来源已就绪 stage={stage} zip={zip} maps={} plugins={}",
        sets.len(),
        sets.iter().map(|set| set.plugins.len()).sum::<usize>(),
    );
    Ok(StagedMapSource {
        path: display_path(&path),
        download_id: downloaded.then_some(stage),
        staged: downloaded,
        sets,
        origin,
    })
}

/// 丢弃一个已暂存的地图来源（只允许缓存目录内的文件）。
///
/// 暂存文件由 `stage_map_source` 落在缓存目录里；路径来自前端，不能当成
/// 可信路径 —— 判定规则见 [`cache_file_to_discard`]。
#[tauri::command]
pub fn discard_map_source(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let Some(target) = cache_file_to_discard(&state.config.cache_dir(), &path)? else {
        return Ok(());
    };
    state.downloaded_sources.forget_path(&target);
    fs::remove_file(&target).map_err(|error| {
        crate::locale_tag::with_params("levelSet.deleteFailed", &[("reason", &error.to_string())])
    })?;
    log::info!("已清理暂存的地图来源：路径={}", display_path(&target));
    Ok(())
}

/// 从本地 ZIP 导入选定的关卡集。
///
/// 同身份旧装项由前端安装前检查先卸载，导入阶段拒绝覆盖。
///
/// 两段都在阻塞线程池里跑：识别一个几十集的合集要几十秒，解压落盘同样按秒
/// 计，同步命令会把整个窗口冻住（见 `get_level_sets` 的注释）。识别只做一次
/// —— 以前每装一个集就整包重新 inspect 一遍，N 张合集就是 N 倍识别时间。
#[tauri::command]
pub async fn import_level_sets(
    app: AppHandle,
    state: State<'_, AppState>,
    zip_path: String,
    selected: Vec<String>,
    origin: Option<MapInstallOrigin>,
) -> Result<LevelSetImportResult, String> {
    let _ = (app, state, zip_path, selected, origin);
    crate::mod_changes::guard::require_shared_plan()
}

/// 把一个地图 Release 暂存成「可确认的导入来源」。
///
/// 选资产、下载、校验、识别后交给同一整链计划，确认后才写入游戏目录。
///
/// 下载**完全复用** MOD 的候选规划与竞速协调器：GitHub 按自动直连 / 自动代理
/// 生成多个完整文件源并竞速（最多三个并行，慢的让位），Gitee 只直连且单线程。
/// 地图以前是单源直下，中国大陆走 Gitee 时慢得没有退路，走 GitHub 时又用不上
/// 用户配好的代理。
///
/// 暂存文件落在缓存目录（`staged = true`）：导入完成或用户取消后必须用
/// `discard_map_source` 清掉。
#[tauri::command]
pub async fn stage_map_release(
    app: AppHandle,
    state: State<'_, AppState>,
    map_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
    locale: Option<String>,
) -> Result<StagedMapSource, String> {
    let stage = uuid::Uuid::new_v4();
    let id = redact_text(&format!("{map_id:?}"));
    log::info!(
        "开始暂存地图来源 stage={stage} kind=Release map_id={id} requested_tag={} requested_asset={}",
        redact_text(&format!("{tag:?}")),
        redact_text(&format!("{asset_name:?}")),
    );
    let snapshot = state.config.load_or_default();
    let policy = snapshot
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(crate::network_policy::NetworkPolicy::active)?;
    let result = crate::network_policy::NetworkPolicy::in_snapshot(policy, async {
        // 取消标记只在命令入口重置，不能在等待元数据或补充候选之后重置
        archive::start_download();
        // Keep region, credentials and network policy from the same entry snapshot.
        let config = snapshot;
        let map = crate::level_sets::map_repository_by_id(&config.map_repositories, &map_id)
            .ok_or("mapSource.mapMissing")?;
        /*
         * 暂存不写游戏目录，不检查运行状态。整链准备与提交会按实际文件判断
         * 是否涉及 MOD；若涉及，才要求退出游戏。
         */
        let release = releases::select_cached_release(&map.cached_releases, tag.as_deref(), None)
            .ok_or(if tag.is_some() {
            "mapSource.releaseMissing"
        } else {
            "source.noInstallableRelease"
        })?;
        /*
         * 指定资产名时精确匹配；未指定时普通地图仍按 `asset_rules` 自动选，
         * tag 模式直接用合成版本自带的唯一资产（见 `select_map_stage_asset`）。
         */
        let asset = crate::map_sources::select_map_stage_asset(map, release, asset_name.as_deref())
            .ok_or("source.noMatchingAsset")?;

        log::info!(
            "地图安装资产已确定 stage={stage} map={} tag={} asset={}",
            redact_text(&format!("{:?}", map.name)),
            redact_text(&format!("{:?}", release.tag_name)),
            redact_text(&format!("{:?}", asset.name)),
        );

        let assets = archive::release_download::asset_set(asset, &release.assets)?;
        let display_name = crate::config::localized_map_name(map, locale.as_deref());
        let client = releases::build_download_client(config.network.first_byte_timeout())
            .map_err(|error| error.to_string())?;
        let cache_dir = state.config.cache_dir();
        let cache_scope =
            archive::release_download::ReleaseCacheScope::for_map(map, &release.tag_name, asset);
        let temp = archive::release_download::download(
            &client,
            assets,
            &cache_dir,
            cache_scope.as_ref(),
            &display_name,
            Some(&app),
            &config.network,
            |url| {
                crate::download::planning::plan_repository_download(
                    &map.provider,
                    crate::models::RepositoryKind::Map,
                    &map.network_plan,
                    &config.region,
                    url,
                    None,
                    None,
                    None,
                )
            },
        )
        .await?
        .into_path();

        let source = Some((map.clone(), release.tag_name.clone()));
        inspect_staged_zip(&app, state.inner(), config, temp, true, source, stage).await
    })
    .await;
    if let Err(error) = &result {
        log::warn!(
            "地图来源暂存失败 stage={stage} kind=Release map_id={id} reason={}",
            redact_text(error),
        );
    }
    result
}
