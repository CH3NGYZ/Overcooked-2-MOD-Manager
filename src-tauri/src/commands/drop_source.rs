use std::path::PathBuf;

/// 拖放分类结果：一个本地文件到底该走地图链路还是 MOD 链路。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DropSourceKind {
    /// `map` / `mod`（包括含默认地图的 DIYLevel 包）/ `unknown`。
    pub kind: &'static str,
    /// ZIP 里含 `info` 的目录数。
    pub map_sets: usize,
    /// ZIP 里的 DLL 条目数（`.dll` 输入按 1 计）。
    pub plugins: usize,
}

/// 判断拖进来的文件是地图包、MOD 包还是都不像。
///
#[tauri::command]
pub async fn classify_drop_source(path: String) -> Result<DropSourceKind, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = PathBuf::from(path.trim());
        if !target.is_file() {
            return Err("mods.fileMissing".to_string());
        }
        let extension = target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension == "dll" {
            return Ok(DropSourceKind {
                kind: "mod",
                map_sets: 0,
                plugins: 1,
            });
        }
        if extension != "zip" {
            return Ok(DropSourceKind {
                kind: "unknown",
                map_sets: 0,
                plugins: 0,
            });
        }
        let contents = crate::level_sets::classify_zip(&target)?;
        let kind = if contents.map_sets > 0 && !contents.mod_bundled_maps {
            "map"
        } else if contents.plugins > 0 {
            "mod"
        } else {
            "unknown"
        };
        Ok(DropSourceKind {
            kind,
            map_sets: contents.map_sets,
            plugins: contents.plugins,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}
