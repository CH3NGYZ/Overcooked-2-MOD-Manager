use super::context::MapSourceContext;
use super::document::MapSourceDeclaration;
use super::document::validate_map_entry;
use super::identity::map_identity;
use super::identity::map_repository_id;
use super::state::SourceLabel;
use crate::config::ConfigError;
use crate::models::MapRepositoryConfig;
use crate::models::Provider;
use crate::repository_sources::SourceLocation;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// 「单仓多 Release」源要读哪个仓库。
///
/// 只为复用 `release_cache` 与 `fetch_releases` 的限流/代理/重试链路：
/// 那条链路吃的是 `RepositoryConfig`，为地图再造一条抓取路径等于把规则
/// 复制一遍，迟早两边行为不一致。
pub(super) fn declaration_probe(
    declaration: &MapSourceDeclaration,
) -> crate::models::RepositoryConfig {
    let export: crate::models::RepositoryExport = serde_json::from_value(serde_json::json!({
        "name": declaration.nickname,
        "name_i18n": declaration.nickname_i18n,
        "group": declaration.nickname,
        "group_i18n": declaration.nickname_i18n,
        "provider": declaration.provider.clone().unwrap_or(Provider::Github),
        "username": declaration.username.trim(),
        "repository": declaration.repository.trim(),
        "enabled": true,
        "download_mode": "release",
        "install_target": "",
    }))
    .expect("地图源投影的字段形状固定，构造失败说明模型改了默认值");
    crate::config::repository_from_export(
        &export,
        declaration.id.trim().to_string(),
        declaration.builtin.unwrap_or(false),
    )
}

/// 读一个「单仓多 Release」源，把每个 Release note 里的元数据块变成一张地图。
///
/// 一个 tag 只描述一张地图：note 里的块缺 `level_set_name` 或读不出来就跳过
/// 那一条（记 warn），绝不猜。整份结果共用同一个 `label`，前端因此把它们
/// 归在同一个源下面。
pub(super) async fn load_release_tag_maps(
    declaration: &MapSourceDeclaration,
    label: &SourceLabel,
    builtin: bool,
    origin: &SourceLocation,
    ctx: &MapSourceContext<'_>,
) -> Result<Vec<MapRepositoryConfig>, ConfigError> {
    let provider = declaration.provider.clone().unwrap_or(Provider::Github);
    let probe = declaration_probe(declaration);
    /*
     * 保留历史 `::all` 缓存键，避免结构迁移改变现有缓存生命周期。
     * 目前两个入口均读取全部分页；历史上「首页 10 条」与「全部分页」不是
     * 同一份数据，共用键要么让地图源读到不完整列表，要么让 MOD 侧拿到全量。
     */
    let key = format!(
        "{}::all",
        crate::releases::repository_cache_key(
            provider.clone(),
            &declaration.username,
            &declaration.repository,
        )
    );
    let token = ctx.token(&provider).map(str::to_string);
    let official_api_only = ctx.config.auth.official_api_only;
    let api_proxies = ctx.api_proxies(&provider).to_vec();
    let releases = match ctx
        .cache
        .list(key, ctx.pass_id, || async move {
            crate::releases::fetch_releases(
                ctx.client,
                &probe,
                token.as_deref(),
                official_api_only,
                &api_proxies,
            )
            .await
            .map_err(|error| error.to_string())
        })
        .await
    {
        Ok(releases) => releases,
        Err(error) => {
            /*
             * 取不到就沿用上一次成功时生成的条目：第三方源不该因为一次限流
             * 或断网从地图页整块消失（用户会以为作者下架了地图）。首次运行
             * 没有旧条目时就是空，下一次刷新补上。
             */
            let previous = ctx.previous_for(&label.id);
            log::warn!(
                "按 Release note 生成地图失败，沿用上次结果：源={}，原因={}，沿用={}",
                label.id,
                error,
                previous.len()
            );
            return Ok(previous);
        }
    };

    let mut maps = Vec::new();
    let mut seen = HashSet::new();
    for release in releases.iter() {
        let tag = release.tag_name.trim();
        if tag.is_empty() {
            continue;
        }
        let note = match crate::map_note::parse_release_note(&release.body) {
            Ok(Some(note)) => note,
            Ok(None) => {
                // 没有块就是这个 Release 不是地图（例如只是普通更新说明）
                log::debug!(
                    "Release note 没有 oc2-map 块，跳过：源={}，tag={}",
                    label.id,
                    tag
                );
                continue;
            }
            Err(reason) => {
                log::warn!(
                    "Release note 的 oc2-map 块无效，跳过：源={}，tag={}，原因={}",
                    label.id,
                    tag,
                    reason
                );
                continue;
            }
        };
        let Some(rule) = crate::map_note::versioned_asset_rule(note.metadata.asset.as_deref(), tag)
        else {
            log::warn!(
                "Release 里没有能提取版本号的地图包，跳过：源={}，tag={}",
                label.id,
                tag
            );
            continue;
        };
        match note_block_map(declaration, label, &note, tag, rule, builtin, origin) {
            Ok(map) => {
                if !seen.insert(map_identity(&map)) {
                    log::warn!(
                        "同一个源里有两张身份相同的地图，跳过后者：源={}，tag={}",
                        label.id,
                        tag
                    );
                    continue;
                }
                maps.push(map);
            }
            Err(error) => log::warn!(
                "Release note 生成的条目无效，跳过：源={}，tag={}，原因={}",
                label.id,
                tag,
                error
            ),
        }
    }
    log::info!(
        "按 Release note 生成地图完成：源={}，Release={}，地图={}",
        label.id,
        releases.len(),
        maps.len()
    );
    Ok(maps)
}

/// 把一个 Release 的 note 元数据拼成地图条目。
///
/// 走 JSON 而不是手写结构体：`MapRepositoryConfig` 字段很多、多数有默认值，
/// 只写这次真正来自 note 的字段，其余交给 serde 默认值 —— 与
/// `release_probe` 同一条理由。
#[allow(clippy::too_many_arguments)]
pub(super) fn note_block_map(
    declaration: &MapSourceDeclaration,
    label: &SourceLabel,
    note: &crate::map_note::NoteBlock,
    tag: &str,
    rule: crate::models::AssetRule,
    builtin: bool,
    origin: &SourceLocation,
) -> Result<MapRepositoryConfig, ConfigError> {
    let metadata = &note.metadata;
    /*
     * 显示名逐语言兜底：缺 `zh-CN` 用 `level_set_name_zh`，缺 `en` 用
     * `level_set_name` —— 这两个名字 info 里本来就有，作者只是没另写一份
     * 多语言表。不兜的话卡片会退化成 `username/repository`，同一个仓库里
     * 所有地图都长一个样。
     */
    let mut name_i18n = metadata.name_i18n.clone();
    if name_i18n
        .get("zh-CN")
        .is_none_or(|value| value.trim().is_empty())
    {
        let zh = metadata.level_set_name_zh.trim();
        if !zh.is_empty() {
            name_i18n.insert("zh-CN".to_string(), zh.to_string());
        }
    }
    if name_i18n
        .get("en")
        .is_none_or(|value| value.trim().is_empty())
    {
        let en = metadata.level_set_name.trim();
        if !en.is_empty() {
            name_i18n.insert("en".to_string(), en.to_string());
        }
    }
    /*
     * 分类默认回填**作者名**：地图的分组语义就是作者，同一个仓库里同一作者的
     * 几张图应该并排；作者没写（官方导出常为空）才退回显示名，至少不让卡片
     * 掉进「无分类」。
     */
    let group_i18n = if !metadata.group_i18n.is_empty() {
        metadata.group_i18n.clone()
    } else {
        let author = metadata.author.trim();
        if author.is_empty() {
            name_i18n.clone()
        } else {
            BTreeMap::from([("zh-CN".to_string(), author.to_string())])
        }
    };
    /*
     * 单语言镜像必须在这里折出来：`get_maps` 只透传 `name`/`group`，前端拿
     * 到空串时会退回 `username/repository`，一个仓库多张图就全长一个样。
     */
    // 单语镜像与多语言表同一优先级：没有中文名就用英文名，两者都是 info 里的名字
    let name = name_i18n
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            name_i18n
                .get("en")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default();
    let group = group_i18n
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| name.clone());
    /*
     * 卡片短语优先级：块里的 `note_i18n`（五语）→ 块里的单语 `note` → 块外正文。
     * 很多作者只发「正文写介绍 + 一个 oc2-map 声明块」，正文本来就是这张图的
     * 说明，丢掉它卡片上只剩「暂无说明」。正文同时是版本详情的提交日志，两处
     * 展示同一次 API 的数据，没有第三处要喂；块标记已由 `parse_release_note` 剥掉。
     */
    let mut note_i18n = metadata.note_i18n.clone();
    if note_i18n.is_empty() {
        let fallback = if metadata.note.trim().is_empty() {
            note.body.trim()
        } else {
            metadata.note.trim()
        };
        if !fallback.is_empty() {
            note_i18n.insert("zh-CN".to_string(), fallback.to_string());
        }
    }
    let note_text = note_i18n
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let value = serde_json::json!({
        "provider": declaration.provider.clone().unwrap_or(Provider::Github),
        "username": declaration.username.trim(),
        "repository": declaration.repository.trim(),
        // tag 是这张地图在仓库里的唯一标识：一个 Release = 一张地图，
        // 版本列表从这个 tag 的资产里按 asset 规则拆
        "tag": tag,
        "level_set_name": metadata.level_set_name.trim(),
        "level_set_name_zh": metadata.level_set_name_zh.trim(),
        "author": metadata.author.trim(),
        "name": name,
        "group": group,
        "name_i18n": name_i18n,
        "group_i18n": group_i18n,
        "note": note_text,
        "note_i18n": note_i18n,
        "asset_rules": [rule],
        // 表非空时它才是最新版：块里的标量是旧块的历史残留
        "latest_mapset_version": metadata.derived_mapset_version(),
        "latest_info_version": metadata.normalized_info_version(),
        "asset_versions": metadata.asset_versions,
        "relations": metadata.relations(),
        "release_relations": metadata.release_relation_overrides(),
        "enabled": true,
    });
    let mut map: MapRepositoryConfig =
        serde_json::from_value(value).expect("note 块的字段形状与 MapRepositoryConfig 一致");
    label.attach(&mut map);
    map.builtin = builtin;
    map.id = map_repository_id(&label.id, &map);
    validate_map_entry(&map, origin)?;
    Ok(map)
}
