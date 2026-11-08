use crate::app_state::AppState;
use crate::archive;
use crate::display_path::display_path;
use crate::download::DownloadContext;
use crate::download::try_download_candidates as try_asset_download_candidates;
use crate::game;
use crate::level_sets::staging::cache_file_to_discard;
use crate::level_sets::staging::validate_map_release_sets;
use crate::level_sets::staging::zip_level_sets;
use crate::models::{LevelSetImportResult, LevelSetZipProgress, MapInstallOrigin, StagedMapSource};
use crate::releases;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::State;

/// 准备一个地图来源：本地 ZIP 只解析，直链先下载到缓存目录。
///
/// 返回的 `path` 就是后续 `import_level_sets` 的输入；`staged=true` 时调用方
/// 结束（导入完成或用户取消）后必须用 `discard_map_source` 清理，否则缓存
/// 目录里会留下下载下来的 ZIP。
#[tauri::command]
pub async fn stage_map_source(
    app: AppHandle,
    state: State<'_, AppState>,
    source: crate::mod_preview::ModSource,
) -> Result<StagedMapSource, String> {
    let config = state.config.load_or_default();
    let (path, staged) = match &source.kind {
        crate::mod_preview::ModSourceKind::Local => {
            let path = PathBuf::from(source.value.trim());
            if !path.is_file() {
                return Err("mods.fileMissing".into());
            }
            let is_zip = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("zip"));
            if !is_zip {
                return Err("modPreview.unsupportedType".into());
            }
            (path, false)
        }
        crate::mod_preview::ModSourceKind::Url => {
            let (temp, _file_name) = crate::direct_mod::download_remote_to_temp(
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
    // 解析失败时下载来的暂存文件没有后续用途，当场清掉，不留垃圾
    let sets = match zip_level_sets(&app, &config, &path) {
        Ok(sets) => sets,
        Err(error) => {
            if staged {
                let _ = fs::remove_file(&path);
            }
            return Err(error);
        }
    };
    log::info!(
        "地图来源已就绪：类型={:?}，暂存={}，关卡集={}",
        source.kind,
        staged,
        sets.len()
    );
    Ok(StagedMapSource {
        path: display_path(&path),
        staged,
        sets,
        origin: None,
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
    // `State` 不能跨 await：需要的值（配置快照、游戏根、操作锁）在同步段取好
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let manifests_dir = state.config.manifests_dir();
    let operation = state.operation.clone();
    tauri::async_runtime::spawn_blocking(move || {
        log::info!("开始导入关卡集：文件={}，选择={}", zip_path, selected.len());
        let _guard = operation.lock().map_err(|_| "lock.poisoned".to_string())?;
        /*
         * 安装是新增内容，游戏运行时也允许 —— 写的是本次启动还没扫描过的新目录。
         * 用策略而不是直接 `ensure_game_stopped`：开关仍然需要停止游戏，
         * 两者的差异集中在一处声明。
         */
        crate::level_sets::ensure_operation_allowed(
            &game_root,
            crate::level_sets::LevelSetOperation::Install,
        )?;
        let zip = PathBuf::from(&zip_path);
        let info_app = app.clone();
        let sets = crate::level_sets::inspect_zip_with_progress(
            &zip,
            crate::metadata_read::workers_from(config.map_info_concurrency),
            crate::metadata_read::workers_from(config.mod_metadata_concurrency),
            &move |done, total, current| {
                let _ = info_app.emit(
                    "level-set-zip-progress",
                    LevelSetZipProgress {
                        phase: "info".into(),
                        done,
                        total,
                        current: current.to_string(),
                    },
                );
            },
        )?;
        /*
         * BepInEx 表层包是整包语义：一次解压写完整份目录结构，不能按集循环
         * （循环会重复解压同一个 ZIP、重复写登记表）。历史形态仍逐个集落盘，
         * 任一失败就停下并保留已成功的（它们已经在磁盘上），错误如实上报。
         */
        if sets
            .iter()
            .any(|set| set.layout == crate::models::LevelSetLayout::Bepinex)
        {
            let _ = app.emit(
                "level-set-zip-progress",
                LevelSetZipProgress {
                    phase: "install".into(),
                    done: 0,
                    total: selected.len(),
                    current: String::new(),
                },
            );
            let installed = crate::level_sets::install_parsed_sets(
                &sets,
                &zip,
                &game_root,
                &manifests_dir,
                &selected,
                origin.as_ref(),
            )?;
            log::info!("关卡集导入完成（BepInEx 表层包）：数量={}", installed.len());
            return Ok(LevelSetImportResult { installed });
        }
        /*
         * 逐个集落盘，而不是一次性全装：任一失败就停下并保留已成功的（它们
         * 已经在磁盘上），错误如实上报 —— 悄悄跳过会让用户以为全装上了。
         * 每张开始前发一条 `install` 阶段进度，用户能看到卡在哪张图上。
         */
        let mut installed = Vec::new();
        let total = selected.len();
        for (index, name) in selected.iter().enumerate() {
            let _ = app.emit(
                "level-set-zip-progress",
                LevelSetZipProgress {
                    phase: "install".into(),
                    done: index,
                    total,
                    current: name.clone(),
                },
            );
            let single = crate::level_sets::install_parsed_sets(
                &sets,
                &zip,
                &game_root,
                &manifests_dir,
                std::slice::from_ref(name),
                origin.as_ref(),
            )?;
            installed.extend(single);
        }
        log::info!("关卡集导入完成：数量={}", installed.len());
        Ok(LevelSetImportResult { installed })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 把一个地图 Release 暂存成「可确认的导入来源」。
///
/// 两段式与本地 ZIP 导入同构：这一步只做「选资产 → 下载 → 校验 → 识别 → 报告
/// 冲突」，用户确认后才由 `import_level_sets`（`install_from_zip`）真正写入
/// `levels/`。这样地图与 MOD 一样，安装前就能看到「将卸载本地哪些旧副本」。
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
    let snapshot = state.config.load_or_default();
    let policy = snapshot
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(crate::network_policy::NetworkPolicy::active)?;
    crate::network_policy::NetworkPolicy::in_snapshot(policy, async {
        // 取消标记只在命令入口重置，不能在等待元数据或补充候选之后重置
        archive::start_download();
        // 与 `install_repository` 同口径：先把用户指定的版本与资产写进日志。
        // 资产选不出来（`source.noMatchingAsset`）时，这行是唯一能看出他点了哪个资产的地方。
        log::info!(
            "开始暂存地图来源：地图 ID={}，指定版本={}，指定资产={}",
            map_id,
            tag.as_deref().unwrap_or("自动选择"),
            asset_name.as_deref().unwrap_or("自动匹配")
        );
        let config = state.config.load_or_default();
        let Some(map) = config
            .map_repositories
            .iter()
            .find(|entry| entry.id == map_id)
            .cloned()
        else {
            return Err("mapSource.mapMissing".into());
        };
        /*
         * 暂存不写游戏目录，所以不探测游戏目录、也不拦「游戏正在运行」：
         * 真正的写入在 `import_level_sets`，那里仍会校验 `game.notFound` 与运行状态。
         */
        let release = match tag.as_deref() {
            Some(tag) => map
                .cached_releases
                .iter()
                .find(|release| release.tag_name == tag)
                .cloned()
                .ok_or("mapSource.releaseMissing")?,
            None => map
                .cached_releases
                .first()
                .cloned()
                .ok_or("source.noInstallableRelease")?,
        };
        /*
         * 指定资产名时精确匹配；未指定时普通地图仍按 `asset_rules` 自动选，
         * tag 模式直接用合成版本自带的唯一资产（见 `select_map_stage_asset`）。
         */
        let asset =
            crate::map_sources::select_map_stage_asset(&map, &release, asset_name.as_deref())
                .ok_or("source.noMatchingAsset")?
                .clone();

        log::info!(
            "地图安装资产已确定：地图={}，Release={}，资产={}",
            map.name,
            release.tag_name,
            asset.name
        );

        let planned = crate::download::planning::plan_repository_download(
            &map.provider,
            crate::models::RepositoryKind::Map,
            &map.network_plan,
            crate::config::current_user_region(),
            &asset.download_url,
            None,
            None,
            None,
        )?;
        let candidates = planned.candidates;

        // size 是 u64（0 表示未知），只有正值才参与校验与进度总分母
        let expected_size = (asset.size > 0).then_some(asset.size);
        // The shared verifier accepts the trusted asset digest for every provider.
        let digest = asset.digest.as_deref();
        let download_name = asset.name.clone();
        /*
         * 进度标签与预览标题用界面语言下的地图名：进度事件在下载过程中持续发出，
         * 名称必须在开始前算好。
         */
        let display_name = crate::config::localized_map_name(&map, locale.as_deref());

        let client = releases::build_download_client(config.network.timeout_seconds)
            .map_err(|error| error.to_string())?;
        let racing = planned.execution.race;
        let configured_threads = planned.download_threads;
        log::debug!(
            "地图下载候选准备完成：地图={}，平台={:?}，候选数={}，线程数={}，竞速={}",
            map.name,
            map.provider,
            candidates.len(),
            configured_threads,
            racing
        );

        let cache_dir = state.config.cache_dir();
        let temp = try_asset_download_candidates(
            candidates,
            archive::is_download_cancelled,
            |candidate, reporter| {
                let client = &client;
                let cache_dir = &cache_dir;
                let label = &display_name;
                let is_zip = download_name.to_ascii_lowercase().ends_with(".zip");
                async move {
                    let temp = archive::download_to_temp_with_threads_and_first_byte_timeout(
                        client,
                        &candidate.url,
                        cache_dir,
                        label,
                        &reporter,
                        configured_threads,
                        Some(archive::FIRST_BYTE_TIMEOUT),
                    )
                    .await?;
                    let validation = archive::validate_download_file(&temp, expected_size, digest)
                        .and_then(|_| {
                            if is_zip {
                                archive::validate_zip(&temp)
                            } else {
                                Ok(())
                            }
                        });
                    if let Err(error) = validation {
                        let _ = fs::remove_file(&temp);
                        return Err(error);
                    }
                    Ok(temp)
                }
            },
            Some(DownloadContext {
                execution: Some(planned.execution),
                app: Some(&app),
                label: &display_name,
                expected_size,
            }),
        )
        .await
        .map_err(|error| {
            log::error!("地图资产下载失败：地图={}，原因={}", map.name, error);
            error
        })?;

        /*
         * 识别 + 身份校验在暂存阶段做完：
         *
         * - 仓库声明与实际 ZIP 不一致时，等用户确认完再报错太晚：界面已经展示了
         *   一份要装的地图集，而安装必然失败；
         * - 冲突列表要随 `ZipLevelSet` 一起返回，确认界面靠它提示「将卸载本地
         *   哪些旧副本」。
         *
         * 解析 `info` 是可能几秒的 CPU 工作，走 `spawn_blocking`：不要占着异步
         * 线程把窗口拖死（与 `stage_map_source` 同一条规则）。
         */
        let config_for_parse = config.clone();
        let path_for_parse = temp.clone();
        let app_for_parse = app.clone();
        let parsed = match tauri::async_runtime::spawn_blocking(move || {
            zip_level_sets(&app_for_parse, &config_for_parse, &path_for_parse)
        })
        .await
        {
            Ok(result) => result,
            Err(error) => Err(error.to_string()),
        };
        let sets = match parsed {
            Ok(sets) => sets,
            Err(error) => {
                let _ = fs::remove_file(&temp);
                return Err(error);
            }
        };
        let primary = match validate_map_release_sets(&sets, &map) {
            Ok(primary) => primary,
            Err(error) => {
                let _ = fs::remove_file(&temp);
                return Err(error);
            }
        };
        log::info!(
            "地图 Release 已暂存：地图={}，版本={}，资产={}，关卡集={}，冲突={}",
            map.name,
            release.tag_name,
            download_name,
            sets.len(),
            sets.first().map(|set| set.conflicts.len()).unwrap_or(0)
        );
        let origin = MapInstallOrigin {
            level_set_name: primary.name.clone(),
            level_set_name_zh: primary.name_zh.clone().unwrap_or_default(),
            author: primary.author.clone(),
            mapset_version: release.tag_name.clone(),
        };
        Ok(StagedMapSource {
            path: display_path(&temp),
            staged: true,
            sets,
            origin: Some(origin),
        })
    })
    .await
}
