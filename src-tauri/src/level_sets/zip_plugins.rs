//! 随包 DLL 的临时解压准备与元数据降级。

use crate::models::DllPluginInfo;
use crate::models::DllRole;
use std::fs;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

/// 读不出 DLL 元数据时的兜底：按文件名显示成一条依赖。
pub(super) fn fallback_plugin_info(file_name: &str) -> DllPluginInfo {
    DllPluginInfo {
        role: DllRole::Dependency,
        name: Some(crate::mods::file_stem_name(file_name)),
        guid: None,
        version: None,
        title: None,
        product_name: None,
        company: None,
        original_filename: None,
    }
}

/// ZIP 内一个随包插件的解压准备结果。
pub(super) struct PreparedZipPlugin {
    pub(super) path: String,
    pub(super) file_name: String,
    /// 落到 scratch 的真实文件；`None` 表示降级为文件名兜底。
    pub(super) temp: Option<PathBuf>,
    /// 在批次请求里的下标（`temp` 为 `None` 时也是 `None`）。
    pub(super) request: Option<usize>,
}

/// 把 ZIP 内一个随包插件解到 scratch，供批次解析 PE 版本资源。
///
/// 任何一步失败都只降级成「按文件名显示的依赖」：这些字段只用来展示安装
/// 详情，一个损坏的 DLL 不该让整份地图包识别失败（地图本身还是能装的）。
pub(super) fn prepare_zip_plugin<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: Option<usize>,
    entry: &str,
    scratch: Option<&Path>,
) -> PreparedZipPlugin {
    let mut prepared = PreparedZipPlugin {
        path: entry.to_string(),
        file_name: entry.rsplit('/').next().unwrap_or(entry).to_string(),
        temp: None,
        request: None,
    };
    let Some(scratch) = scratch else {
        log::warn!(
            "随包插件元数据读取失败，降级为文件名兜底：条目={}，原因=临时目录不可用",
            entry
        );
        return prepared;
    };
    let Some(index) = index else {
        log::warn!(
            "随包插件元数据读取失败，降级为文件名兜底：条目={}，原因=条目索引缺失",
            entry
        );
        return prepared;
    };
    let temp = scratch.join(format!("{index}.dll"));
    let extracted = (|| -> Result<(), String> {
        let mut zip_entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        zip_entry
            .by_ref()
            .take(crate::mods::MAX_DLL_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        fs::write(&temp, &bytes).map_err(|error| error.to_string())
    })();
    match extracted {
        Ok(()) => prepared.temp = Some(temp),
        Err(reason) => log::warn!(
            "随包插件元数据读取失败，降级为文件名兜底：条目={}，原因={}",
            entry,
            reason
        ),
    }
    prepared
}
