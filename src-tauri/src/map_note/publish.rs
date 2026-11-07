use super::block::NOTE_BLOCK_CLOSE;
use super::block::NOTE_BLOCK_OPEN;
use super::block::block_span;
use super::metadata::MapNoteMetadata;
use super::relations::RelationSpec;
use crate::models::MapPublishMetadata;
use crate::models::RelationKind;

/// 解析模型 → 上传页可编辑形态。
pub fn publish_metadata_from_note(note: &MapNoteMetadata) -> MapPublishMetadata {
    let relations = |table: &std::collections::BTreeMap<String, RelationSpec>| {
        table
            .iter()
            .map(|(guid, spec)| (guid.clone(), spec.display_range()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    MapPublishMetadata {
        level_set_name: note.level_set_name.clone(),
        level_set_name_zh: note.level_set_name_zh.clone(),
        author: note.author.clone(),
        name_i18n: note.name_i18n.clone(),
        group_i18n: note.group_i18n.clone(),
        note_i18n: note.note_i18n.clone(),
        asset: note.asset.clone().unwrap_or_default(),
        // 表非空时块里的 latest_* 只是历史残留，派生的才是最新版
        latest_mapset_version: note.derived_mapset_version().unwrap_or_default(),
        latest_info_version: note.derived_info_version().unwrap_or_default(),
        asset_versions: note.asset_versions.clone(),
        requires: relations(&note.requires),
        compatible: relations(&note.compatible),
        conflicts: relations(&note.conflicts),
        release_relations: note.release_relation_overrides(),
    }
}

fn canonical_range(raw: &str) -> String {
    crate::version_range::VersionRange::parse(raw.trim())
        .map(|range| {
            if range.is_any() {
                "*".to_string()
            } else {
                range.canonical()
            }
        })
        .unwrap_or_else(|_| raw.trim().to_string())
}

/// 渲染 `<!-- oc2-map -->` 元数据块（以换行结尾）。
///
/// 这是应用内发布页的唯一写侧实现：上传页用它把表单写成 note。
/// 独立工具 `tools/oc2-info` 当前维护自己的 CLI 输出入口，不链接本模块；两者须
/// 遵守同一字段契约，并分别用各自的 Release note 往返用例验证。
///
/// `asset_versions` 一律排在所有标量键之后：它是跨行数组，夹在标量键中间会
/// 把 `key = value` 的对齐打散，读起来也像另一段。
pub fn render_metadata_block(metadata: &MapPublishMetadata) -> String {
    let mut lines: Vec<(String, String)> = Vec::new();
    let push = |key: &str, value: &str, lines: &mut Vec<(String, String)>| {
        let value = value.trim();
        if !value.is_empty() {
            lines.push((key.to_string(), toml_string(value)));
        }
    };
    push("level_set_name", &metadata.level_set_name, &mut lines);
    push("level_set_name_zh", &metadata.level_set_name_zh, &mut lines);
    push("author", &metadata.author, &mut lines);
    for (locale, value) in &metadata.name_i18n {
        if !value.trim().is_empty() {
            lines.push((
                format!("name_i18n.{}", locale.trim()),
                toml_string(value.trim()),
            ));
        }
    }
    for (locale, value) in &metadata.group_i18n {
        if !value.trim().is_empty() {
            lines.push((
                format!("group_i18n.{}", locale.trim()),
                toml_string(value.trim()),
            ));
        }
    }
    for (locale, value) in &metadata.note_i18n {
        if !value.trim().is_empty() {
            lines.push((
                format!("note_i18n.{}", locale.trim()),
                toml_string(value.trim()),
            ));
        }
    }
    push("asset", &metadata.asset, &mut lines);
    for (key, table) in [
        ("requires", &metadata.requires),
        ("compatible", &metadata.compatible),
        ("conflicts", &metadata.conflicts),
    ] {
        if let Some(rendered) = render_inline_table(table) {
            lines.push((key.to_string(), rendered));
        }
    }

    let width = lines
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    let mut block = String::from(NOTE_BLOCK_OPEN);
    block.push('\n');
    for (key, value) in &lines {
        block.push_str(key);
        block.push_str(&" ".repeat(width - key.chars().count()));
        block.push_str(" = ");
        block.push_str(value);
        block.push('\n');
    }
    /*
     * 数组排在所有标量键之后（见函数文档）。空值不写：解析端有 default，
     * round-trip 不会因为缺键丢信息；两项都空的条目整条丢掉。
     */
    let rows = metadata
        .asset_versions
        .iter()
        .filter_map(|entry| {
            let mut parts: Vec<String> = Vec::new();
            for (key, value) in [
                ("mapset_version", entry.mapset_version.trim()),
                ("info_version", entry.info_version.trim()),
            ] {
                if !value.is_empty() {
                    parts.push(format!("{key} = {}", toml_string(value)));
                }
            }
            (!parts.is_empty()).then(|| format!("{{ {} }}", parts.join(", ")))
        })
        .collect::<Vec<_>>();
    if !rows.is_empty() {
        block.push('\n');
        block.push_str("asset_versions = [\n");
        for row in &rows {
            block.push_str("  ");
            block.push_str(row);
            block.push_str(",\n");
        }
        block.push_str("]\n");
    }
    // TOML 数组表必须放在所有顶层键之后，否则后面的版本表/语言键会落入最后一个覆盖表。
    for entry in &metadata.release_relations {
        block.push_str("\n[[release_relations]]\nversion = ");
        block.push_str(&toml_string(&canonical_range(&entry.version)));
        block.push('\n');
        // 同 GUID 的 stable/beta 声明不能塞进映射（重复 TOML 键）；使用解析器已有的数组形态。
        let mut keys = std::collections::HashSet::new();
        if entry
            .relations
            .iter()
            .any(|row| !keys.insert((row.kind.as_str(), row.guid.trim())))
        {
            let rows = entry
                .relations
                .iter()
                .map(|row| {
                    let mut fields = vec![
                        format!("kind = {}", toml_string(row.kind.as_str())),
                        format!("guid = {}", toml_string(row.guid.trim())),
                        format!(
                            "version = {}",
                            toml_string(
                                &crate::version_range::VersionRange::parse(&row.version)
                                    .map(|range| range.canonical())
                                    .unwrap_or_else(|_| row.version.clone())
                            )
                        ),
                    ];
                    if let Some(prerelease) = row.prerelease {
                        fields.push(format!("prerelease = {prerelease}"));
                    }
                    if row.optional {
                        fields.push("optional = true".to_string());
                    }
                    format!("{{ {} }}", fields.join(", "))
                })
                .collect::<Vec<_>>();
            block.push_str(&format!("relations = [{}]\n", rows.join(", ")));
            continue;
        }
        for kind in [
            RelationKind::Requires,
            RelationKind::Compatible,
            RelationKind::Conflicts,
        ] {
            let rows = entry
                .relations
                .iter()
                .filter(|row| row.kind == kind)
                .map(|row| {
                    let range = canonical_range(&row.version);
                    let value = if row.prerelease.is_some() || row.optional {
                        let mut fields = vec![format!("version = {}", toml_string(&range))];
                        if let Some(prerelease) = row.prerelease {
                            fields.push(format!("prerelease = {prerelease}"));
                        }
                        if row.optional {
                            fields.push("optional = true".to_string());
                        }
                        format!("{{ {} }}", fields.join(", "))
                    } else {
                        toml_string(&range)
                    };
                    format!("{} = {value}", toml_string(row.guid.trim()))
                })
                .collect::<Vec<_>>();
            if !rows.is_empty() {
                block.push_str(&format!("{} = {{ {} }}\n", kind.as_str(), rows.join(", ")));
            }
        }
    }
    block.push_str(NOTE_BLOCK_CLOSE);
    block.push('\n');
    block
}

/// 只换块外的说明文字，远端原有的元数据块原样保留。
///
/// 「只保存 Release 信息」走这里：前端拿到的正文已经剥掉了块，直接写回会把块
/// 整块抹掉（地图从管理器的列表里消失）。远端没有块时就是新正文本身。
pub fn replace_body_keeping_block(remote: &str, body: &str) -> String {
    let body = body.trim();
    let Some((start, end)) = block_span(remote) else {
        return body.to_string();
    };
    let block = remote[start..end].trim_end();
    if body.is_empty() {
        format!("{block}\n")
    } else {
        format!("{block}\n\n{body}\n")
    }
}

/// 元数据块 + 块外的更新说明，一起构成 Release 的 `body`。
pub fn render_release_body(metadata: &MapPublishMetadata, body: &str) -> String {
    let block = render_metadata_block(metadata);
    let body = body.trim();
    if body.is_empty() {
        block
    } else {
        format!("{block}\n{body}\n")
    }
}

fn render_inline_table(table: &std::collections::BTreeMap<String, String>) -> Option<String> {
    let mut entries = Vec::new();
    for (guid, range) in table {
        let guid = guid.trim();
        let range = range.trim();
        if guid.is_empty() {
            continue;
        }
        entries.push(format!(
            "{} = {}",
            toml_string(guid),
            toml_string(&canonical_range(range))
        ));
    }
    if entries.is_empty() {
        return None;
    }
    Some(format!("{{ {} }}", entries.join(", ")))
}

/// TOML 基本字符串转义：只做必需的四种（反斜杠、引号、换行、制表符），
/// 其余控制字符走 `\u` 转义，避免把作者的换行写成裸换行让整块 TOML 失效。
fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests;
