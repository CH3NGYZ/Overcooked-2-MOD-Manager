use crate::app_state::AppState;
use crate::models::AvatarRequest;
use std::collections::HashMap;
use tauri::State;

/// 打开游戏的 `BepInEx` 目录（框架自身文件所在处，不是 plugins）。
///
/// 与 `open_game_directory`（游戏根目录）和 `open_mod_directory`
/// （`BepInEx/plugins`）区分开：框架的 `core`、`config`、`doorstop` 相关
/// 文件都在这里，排查框架问题时要看的是这一层。
///
/// 不自动创建目录：调用方是「框架已安装」状态下的按钮，目录本该存在。
/// 不存在说明状态与磁盘不一致，报错让用户重新探测，而不是凭空建一个空目录
/// 让他以为框架装好了。
/// 读取一批仓库 owner 的头像（供卡片与详情页显示）。
///
/// **批量而不是逐个**：一次刷新会同时刷出几十个仓库，逐个 IPC 往返既慢
/// 又会让前端持有几十个独立请求。这里一次收下全部 owner，去重后并发读取，
/// 返回 `owner -> data URL` 的映射。
///
/// 去重发生在两处：
///   · 本函数内按 `(provider, owner)` 去重 —— 同一个人只读一次；
///   · `avatars::load` 内部的 in-flight 表 —— 即便有其他调用方并发请求
///     同一 owner，也只有一次真实下载。
///
/// 失败的 owner **不出现在结果里**（不返回空串或占位图），前端据此回落到
/// 平台图标。头像不是关键数据，失败不该弹错误。
#[tauri::command]
pub async fn get_repository_avatars(
    state: State<'_, AppState>,
    owners: Vec<AvatarRequest>,
) -> Result<HashMap<String, String>, String> {
    let snapshot = state.config.load_or_default();
    let policy = snapshot
        .network_policy
        .clone()
        .map(Ok)
        .unwrap_or_else(|| crate::network_policy::NetworkPolicy::active())?;
    crate::network_policy::NetworkPolicy::in_snapshot(policy, async {
        if owners.is_empty() {
            return Ok(HashMap::new());
        }
        let (config, cache_dir) = {
            let _guard = state
                .operation
                .lock()
                .map_err(|_| "lock.poisoned".to_string())?;
            (state.config.load_or_default(), state.config.cache_dir())
        };
        log::debug!("开始读取头像：请求数={}", owners.len());

        let mut result: HashMap<String, String> = HashMap::new();
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut pending = Vec::new();
        for request in owners {
            let owner = request.owner.trim().to_string();
            if owner.is_empty() {
                continue;
            }
            /*
             * 键必须与前端 `avatarKey()` **完全一致**：`{provider}:{owner_lowercase}`。
             * 统一由 `avatars::response_key` 生成，避免两边各写一份再漂移 ——
             * 之前这里用原样大小写的 owner，`CH3NGY2`、`QingyuQingxue` 这类
             * 含大写的 owner 键就对不上，后端返回了头像前端却查不到。
             */
            let key = crate::avatars::response_key(&request.provider, &owner);
            let dedupe = key.clone();
            if !seen.insert((dedupe, owner.to_lowercase())) {
                continue;
            }
            pending.push((key, owner, request.provider));
        }

        // 并发读取，但每个 owner 内部已有单飞保护；并发上限由请求数自然限制
        let mut tasks = Vec::with_capacity(pending.len());
        for (key, owner, provider) in pending {
            let config = config.clone();
            let cache_dir = cache_dir.clone();
            tasks.push(async move {
                let loaded = crate::avatars::load(&config, &cache_dir, &provider, &owner).await;
                (key, loaded)
            });
        }
        for (key, loaded) in futures_util::future::join_all(tasks).await {
            match loaded {
                Ok(bytes) => {
                    let mime = crate::avatars::sniff_mime(&bytes);
                    use base64::{Engine, engine::general_purpose::STANDARD as B64};
                    result.insert(key, format!("data:{mime};base64,{}", B64.encode(&bytes)));
                }
                Err(error) => {
                    // 只记 debug：头像取不到属于正常回落，不是错误
                    log::debug!("头像不可用，前端将回落到平台图标：key={key} 原因={error}");
                }
            }
        }
        log::debug!("头像读取完成：可用={}/{}", result.len(), seen.len());
        Ok(result)
    })
    .await
}

/// 清空头像缓存。
#[tauri::command]
pub fn clear_repository_avatars(state: State<'_, AppState>) -> Result<usize, String> {
    crate::avatars::clear_cache(&state.config.cache_dir())
}
