use crate::app_state::AppState;
use crate::game;
use crate::http_diagnostics::HttpRequestExt;
use crate::models::OperationResult;
use std::path::Path;
use tauri::State;

/// 街机MOD 里「多彩名字」功能的本地监听端口范围（含两端）。
const COLORFUL_NAME_PORTS: std::ops::RangeInclusive<u16> = 28780..=28790;

/// 街机MOD 的显示名上限：UTF-16LE 238 字节，即 119 个 UTF-16 代码单元。
const COLORFUL_NAME_MAX_UNITS: usize = 119;

/// 官方改名网页的来源，仅用于**旧版**街机MOD 的兜底通道。
///
/// 街机MOD v1.9.9 起 `IsValidToken` 只认 `X-OC2-Token`，Origin 已被明确
/// 移除（网页同样改发请求头）。这里的常量只服务还没写出握手文件的旧版，
/// 不要把它当常规通道，也不要据此放宽校验。
const COLORFUL_NAME_ORIGIN: &str = "https://oc2-change-colorful-name.ch3ng.top";

/// 街机MOD 为本地管理器写出的握手文件（相对游戏目录）。
///
/// MOD 已经把自己的业务数据放在 `BepInEx/config/OC2HostUtilities/` 下，
/// 握手文件沿用同一目录。管理器读到 token 就走 token 鉴权，读不到（旧版
/// MOD 还没写这个文件）才退回官方 Origin。
const COLORFUL_NAME_HANDSHAKE: &str = "BepInEx/config/OC2HostUtilities/web-bridge.json";

/// 握手文件的内容。字段全部可缺省，缺失即视为「没有可用握手」。
#[derive(serde::Deserialize)]
struct ColorfulNameHandshake {
    port: u16,
    token: String,
    /// MOD 自己声明的协议版本；不认识的版本按旧行为处理。
    #[serde(default)]
    version: u32,
}

/// 本次推送使用的凭据。
struct ColorfulNameCredential {
    port: u16,
    token: String,
}

/// 读取街机MOD 写出的握手文件。
///
/// 只接受落在端口段内、32 位十六进制的 token，以及已知的协议版本 —— 这个
/// 文件在游戏目录里，任何能写游戏目录的程序都能伪造它，所以校验的作用是
/// 不把畸形/过期内容当成有效凭据，而不是把它当安全边界。
fn read_colorful_name_handshake(game_root: &Path) -> Option<ColorfulNameCredential> {
    let path = crate::game::safe_game_path(game_root, COLORFUL_NAME_HANDSHAKE).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let handshake: ColorfulNameHandshake = serde_json::from_str(&text).ok()?;
    if handshake.version != 1 {
        log::warn!(
            "街机MOD 握手文件版本未知，按旧方式推送：版本={}",
            handshake.version
        );
        return None;
    }
    if !COLORFUL_NAME_PORTS.contains(&handshake.port) {
        log::warn!("街机MOD 握手端口越界，忽略：端口={}", handshake.port);
        return None;
    }
    let token = handshake.token.trim();
    if token.len() != 32 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        log::warn!("街机MOD 握手 token 格式非法，忽略");
        return None;
    }
    Some(ColorfulNameCredential {
        port: handshake.port,
        token: token.to_ascii_lowercase(),
    })
}

/// 单个端口的推送结果。
///
/// 「连上了但被拒绝」必须和「端口没人监听」区分开：前者说明街机MOD 正在
/// 运行，报错方向是鉴权或版本，后者才是「请打开游戏」。
enum ColorfulNameOutcome {
    Delivered(u16),
    Rejected(u16, String),
    Unreachable(u16),
}

/// 向街机MOD 的 `/set-name` 发一次改名请求。
///
/// `token` 为 `None` 时带官方 Origin（只有旧版街机MOD 还认它）；有 token 时
/// 改用 `X-OC2-Token`，这是当前唯一的正规通道。
async fn send_colorful_name(
    client: &reqwest::Client,
    url: &str,
    name: &str,
    token: Option<&str>,
) -> Result<reqwest::Response, reqwest::Error> {
    // 凭据是常量或握手文件里的值，不含控制字符，不需要额外转义。
    let request = client
        .post(url)
        .header("Content-Type", "application/json;charset=utf-8");
    let request = match token {
        Some(token) => request.header("X-OC2-Token", token),
        None => request.header("Origin", COLORFUL_NAME_ORIGIN),
    };
    request
        .body(serde_json::json!({ "name": name }).to_string())
        .send_logged()
        .await
}

/// 把一次请求的结果归类。
///
/// 与发请求分开，测试才能直接对着任意本地地址跑完整的 2xx / 非 2xx /
/// 连不上三条分支，而不必去占用真实的 28780-28790。
async fn classify_colorful_name(
    port: u16,
    response: Result<reqwest::Response, reqwest::Error>,
) -> ColorfulNameOutcome {
    match response {
        Ok(response) if response.status().is_success() => ColorfulNameOutcome::Delivered(port),
        Ok(response) => {
            let status = response.status();
            // 共享诊断已读取并脱敏正文；复用它，避免 MOD 回显握手令牌时
            // 原有的短错误日志再次输出凭据，也不额外等待慢正文。
            let body = crate::http_diagnostics::body_preview(&response).unwrap_or_default();
            let body = body.chars().take(80).collect::<String>();
            let detail = format!("{status} {}", body.trim()).trim().to_string();
            ColorfulNameOutcome::Rejected(port, detail)
        }
        Err(_) => ColorfulNameOutcome::Unreachable(port),
    }
}

async fn post_colorful_name(
    client: &reqwest::Client,
    port: u16,
    name: &str,
    token: Option<&str>,
) -> ColorfulNameOutcome {
    let url = colorful_name_url(port);
    let response = send_colorful_name(client, &url, name, token).await;
    classify_colorful_name(port, response).await
}

fn colorful_name_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/set-name")
}

/// 把生成好的名字推送给街机MOD 的「多彩名字」功能。
///
/// 前端 CSP 只允许 `self` 与 IPC，无法直接访问 `http://127.0.0.1:287xx`，
/// 所以由后端尝试 POST；任一端口返回 2xx 即视为成功。
#[tauri::command]
pub async fn push_colorful_name(
    state: State<'_, AppState>,
    name: String,
) -> Result<OperationResult, String> {
    if name.trim().is_empty() {
        return Err("rename.nameEmpty".into());
    }
    if name.chars().any(|character| character.is_control()) {
        return Err("rename.nameHasControlChars".into());
    }
    let units = name.encode_utf16().count();
    if units > COLORFUL_NAME_MAX_UNITS {
        // tag + 内联参数：前端按 tag 翻译并插值，后端不持有任何界面文案
        return Err(format!(
            "error.rename.nameTooLong?bytes={}&limit={}",
            units * 2,
            COLORFUL_NAME_MAX_UNITS * 2
        ));
    }
    log::info!("推送多彩名字到街机MOD：UTF-16 单元={}", units);

    /*
     * 握手文件必须按**探测到的**游戏目录去找，不能读 `config.game.path`。
     *
     * 真实事故：配置里 `game.path` 为空（目录是每次运行时探测出来的，
     * 用户从没手动填过），于是这里判定「没配目录」直接走 Origin 兜底；
     * 而新街机MOD 已改成只认 `X-OC2-Token`，Origin 必然 403，用户看到
     * 的是「MOD 拒绝了请求」。与其它命令保持一致，用 `detect_game` 的
     * 结果。
     */
    let credential = {
        let config = state.config.load_or_default();
        game::detect_game(&config)
            .path
            .as_deref()
            .and_then(|game_root| read_colorful_name_handshake(Path::new(game_root)))
    };
    let token = credential
        .as_ref()
        .map(|credential| credential.token.as_str());
    let ports: Vec<u16> = match &credential {
        Some(credential) => vec![credential.port],
        None => COLORFUL_NAME_PORTS.collect(),
    };
    if token.is_some() {
        log::info!("街机MOD 握手可用：端口={ports:?}，使用 token 鉴权");
    } else {
        log::warn!(
            "未找到街机MOD 握手文件（{COLORFUL_NAME_HANDSHAKE}），改用 Origin 兜底；新版本街机MOD 只认 token，这一步很可能被拒绝"
        );
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
        .map_err(|e| {
            crate::locale_tag::with_params(
                "network.clientCreateFailed",
                &[("reason", &e.to_string())],
            )
        })?;
    let attempts = ports.into_iter().map(|port| {
        let client = client.clone();
        let name = &name;
        async move { post_colorful_name(&client, port, name, token).await }
    });
    let outcomes = futures_util::future::join_all(attempts).await;
    let mut rejected = Vec::new();
    let mut unreachable = Vec::new();
    for outcome in &outcomes {
        match outcome {
            ColorfulNameOutcome::Delivered(port) => {
                log::info!("街机MOD 已接收多彩名字：端口={port}");
                return Ok(OperationResult {
                    ok: true,
                    message: "rename.pushed".into(),
                });
            }
            ColorfulNameOutcome::Rejected(port, detail) => {
                log::warn!("多彩名字端口 {port} 拒绝了请求：{detail}");
                rejected.push(*port);
            }
            ColorfulNameOutcome::Unreachable(port) => unreachable.push(*port),
        }
    }
    if !rejected.is_empty() {
        log::warn!(
            "多彩名字推送失败：街机MOD 在端口 {rejected:?} 上拒绝请求，无响应的端口={unreachable:?}，检查街机MOD 是否已写出握手文件"
        );
        // 走到这里说明端口有人听、但没通过鉴权：对用户只有一种可执行的建议。
        return Err("diyLevel.rejected".into());
    }
    log::warn!("多彩名字推送失败：{unreachable:?} 端口均无响应");
    Err("diyLevel.notRunning".into())
}

#[cfg(test)]
mod colorful_name_push_tests;
