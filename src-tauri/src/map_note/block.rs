//! Release note 里的地图元数据块。
//!
//! ## 为什么元数据放在 note 里
//!
//! 「一个仓库放多张地图」需要的不是又一份 TOML 源文档，而是**让地图信息跟着
//! Release 自己走**：作者发一个新 tag 就等于发布一张新地图，改一次 note 就是
//! 改地图的说明与依赖，不需要再维护、再同步一份集中式索引。
//!
//! 于是约定在 Release note 里写一段隐藏注释：
//!
//! ```text
//! <!-- oc2-map
//! level_set_name        = "GAODA"
//! level_set_name_zh     = "胡闹达学"
//! author                = "GAODA"
//! name_i18n.zh-CN       = "胡闹达学"
//! name_i18n.en          = "GAODA"
//! asset                 = "daxue_v${version}.zip"
//! latest_mapset_version = "0.3.2"
//! latest_info_version   = "0.3.2"
//! requires              = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }
//! -->
//! ```
//!
//! 注释在 GitHub 上不渲染，普通用户看到的仍然是正常的更新说明；块外面的文字
//! 原样成为地图卡片上的「说明」。格式由伴生仓的 `tools/oc2-info` 生成，两边
//! 必须保持同一套字段名与区间语法。
//!
//! ## 与 `[[maps]]` 条目字段的对应
//!
//! 块里的字段就是同名字段的一个子集，`requires` / `compatible` / `conflicts`
//! 三张表按 `RelationKind` 展开成 `RepositoryRelation`。剩下的字段（provider、
//! username、repository、tag、asset_rules 的形态）由解析方按「源声明 + Release
//! 实际资产」补齐 —— 那些是仓库坐标，写在每份 note 里只会重复。
use super::metadata::MapNoteMetadata;

/// 元数据块的起始标记。
pub const NOTE_BLOCK_OPEN: &str = "<!-- oc2-map";

/// 元数据块的结束标记。
pub const NOTE_BLOCK_CLOSE: &str = "-->";

/// 一份 note 解析出来的东西。
///
/// 元数据块是给管理器的机器可读部分，其中 `note_i18n` 是卡片上的简短说明；
/// `body` 是块外的正文（已剥掉声明块）：它主要是「这个版本的更新说明」，
/// 由 `synthesize_releases` 交给详情面板显示，作者没写 `note_i18n` 时
/// 地图卡片也拿它当说明。
#[derive(Debug, Clone)]
pub struct NoteBlock {
    pub metadata: MapNoteMetadata,
    /// 去掉元数据块之后的正文（首尾空白已裁掉）。
    pub body: String,
}

/// 从 Release note 正文里解析元数据块。
///
/// - `Ok(None)`：没有块 —— 这个 Release 不是「一张地图」，调用方跳过即可；
/// - `Err(reason)`：有块但读不懂（合法 TOML 但不缺必填字段等），调用方记
///   warn 后跳过这一条；
/// - `Ok(Some(_))`：解析成功。
pub fn parse_release_note(body: &str) -> Result<Option<NoteBlock>, String> {
    let Some((start, end)) = block_span(body) else {
        return Ok(None);
    };
    let toml_text = &body[start + NOTE_BLOCK_OPEN.len()..end - NOTE_BLOCK_CLOSE.len()];
    let metadata: MapNoteMetadata =
        toml::from_str(toml_text).map_err(|error| format!("oc2-map 块不是合法 TOML：{error}"))?;
    if metadata.level_set_name.trim().is_empty() {
        return Err("oc2-map 块缺少 level_set_name".to_string());
    }
    Ok(Some(NoteBlock {
        metadata,
        body: strip_note_block(body),
    }))
}

/// 去掉元数据块、只留作者写的正文；没有块时原样返回（去首尾空白）。
///
/// 调用点是合成版本列表：`ReleaseInfo.body` 会原样进详情面板的「提交日志」，
/// 那串 HTML 注释只是给管理器的机器可读块，不该跟着作者的更新说明一起露出。
pub fn strip_note_block(body: &str) -> String {
    match block_span(body) {
        // 只删标记连同它包住的内容，块外的正文按原顺序接起来
        Some((start, end)) => format!("{}{}", &body[..start], &body[end..])
            .trim()
            .to_string(),
        None => body.trim().to_string(),
    }
}

/// 块的字符区间：`[起始标记的第一个字符, 结束标记之后的第一个字符)`。
///
/// 没有成对的起止标记时返回 `None` —— 那说明作者只是写了句 HTML 注释，或者
/// 块写了一半；两种都不该被当成元数据，也不该被从正文里删掉。
pub(super) fn block_span(body: &str) -> Option<(usize, usize)> {
    let open = body.find(NOTE_BLOCK_OPEN)?;
    let after_open = open + NOTE_BLOCK_OPEN.len();
    let close = body[after_open..].find(NOTE_BLOCK_CLOSE)?;
    Some((open, after_open + close + NOTE_BLOCK_CLOSE.len()))
}

#[cfg(test)]
mod tests;
