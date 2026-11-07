use crate::file_metadata::read_file_metadata;
use crate::models::DllRole;
use regex::Regex;
use std::fs;
use std::io::Read;
use std::path::Path;

pub(super) fn choose_local_version(plugin_version: Option<&str>) -> (Option<String>, &'static str) {
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

/// 限制 BepInPlugin 字节扫描的读取量；PE 版本资源仍由文件元数据接口读取。
///
/// MOD 预览与地图包的随包插件共用这一个上限：同一份「读多少算够」的规则
/// 不该在两个入口各写一个数字。
pub(crate) const MAX_DLL_BYTES: u64 = 32 * 1024 * 1024;

/// 一个 DLL 的元数据解析结果。
///
/// `role` 的判定是安装预览与地图随包插件共用的唯一规则：读不出 BepInPlugin
/// 的 GUID 与版本（`guid`、`version` 都空）就是纯依赖。
#[derive(Debug, Clone)]
pub(crate) struct DllDetails {
    pub(crate) role: DllRole,
    /// 显示名（BepInPlugin 名 → PE 产品名 → PE 标题 → 文件名，四级回退的最终值）。
    pub(crate) name: Option<String>,
    /// BepInPlugin 自报的插件名，**不含**任何回退值。
    ///
    /// 与 `name` 分开是因为「是不是纯依赖」必须按「有没有 BepInPlugin 元数据」
    /// 判定：拿回退过的文件名当插件名，所有依赖库都会被认成普通 MOD。
    pub(crate) plugin_name: Option<String>,
    pub(crate) guid: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) product_name: Option<String>,
    pub(crate) company: Option<String>,
    pub(crate) copyright: Option<String>,
    pub(crate) original_filename: Option<String>,
}

impl From<DllDetails> for crate::models::DllPluginInfo {
    /// `copyright` 只给 MOD 详情用，地图随包插件的展示版式里没有这一项。
    fn from(details: DllDetails) -> Self {
        Self {
            role: details.role,
            name: details.name,
            guid: details.guid,
            version: details.version,
            title: details.title,
            product_name: details.product_name,
            company: details.company,
            original_filename: details.original_filename,
        }
    }
}

/// 解析单个 DLL 的元数据（安装预览与地图随包插件唯一入口）。
///
/// 顺序（与用户确认的规则一致）：
///   1. BepInEx 插件元数据（BepInPlugin）：名称/版本/GUID 都取自这里；
///   2. 名称读不出时，回退到 PE 版本资源里的产品名/标题；
///   3. 连 GUID 与版本都读不出时，认定为「依赖」（纯依赖库，不是插件）。
///
/// PE 版本资源只能读真实文件，所以 ZIP 内的条目必须先由调用方落到临时文件。
pub(crate) fn read_dll_details(path: &Path, display_name: &str) -> DllDetails {
    let bytes = fs::File::open(path)
        .ok()
        .and_then(|file| {
            let mut buffer = Vec::new();
            file.take(MAX_DLL_BYTES).read_to_end(&mut buffer).ok()?;
            Some(buffer)
        })
        .unwrap_or_default();
    let plugin = parse_bepin_plugin_metadata(&bytes);
    let metadata = read_file_metadata(path);

    let plugin_name = plugin
        .as_ref()
        .and_then(|item| item.name.clone())
        .filter(|value| !value.trim().is_empty());
    let name = plugin_name
        .clone()
        .or_else(|| metadata.product_name.clone())
        .or_else(|| metadata.title.clone())
        .or_else(|| Some(file_stem_name(display_name)));
    let guid = plugin.as_ref().and_then(|item| item.guid.clone());
    let version = plugin.as_ref().and_then(|item| item.version.clone());
    let role = if guid.is_none() && version.is_none() {
        DllRole::Dependency
    } else {
        DllRole::Mod
    };
    DllDetails {
        role,
        name,
        plugin_name,
        guid,
        version,
        title: metadata.title,
        description: metadata.description,
        product_name: metadata.product_name,
        company: metadata.company,
        copyright: metadata.copyright,
        original_filename: metadata.original_filename,
    }
}

/// 读不出元数据时的占位：纯依赖、只有文件名。
///
/// 与「读出了文件但没有 BepInPlugin」的 `DllDetails` 同形，卡片上的差异只有
/// 名称来源 —— 宁可显示文件名，也不要让一行 MOD 从列表里消失。
pub(crate) fn placeholder_dll_details(display_name: &str) -> DllDetails {
    DllDetails {
        role: DllRole::Dependency,
        name: Some(file_stem_name(display_name)),
        plugin_name: None,
        guid: None,
        version: None,
        title: None,
        description: None,
        product_name: None,
        company: None,
        copyright: None,
        original_filename: None,
    }
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
