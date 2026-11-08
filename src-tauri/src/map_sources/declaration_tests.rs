use super::context::MapSourceContext;
use super::document::build_declaration_map;
use super::document::parse_document;
use super::document::{MapSourceDocument, MapSourceMode};
use super::identity::{map_repository_id, map_tag};
use super::release_tags::note_block_map;
use super::state::SourceLabel;
use super::tree::BUILTIN_MAP_INDEX;
use super::tree::load_map_tree;
use crate::config::ConfigError;
use crate::repository_sources::SourceLocation;

fn origin() -> SourceLocation {
    SourceLocation::parse(BUILTIN_MAP_INDEX).expect("内置索引应该可解析")
}

fn parse(text: &str) -> Result<MapSourceDocument, ConfigError> {
    parse_document(text, &origin())
}

fn error_tag(result: Result<MapSourceDocument, ConfigError>) -> String {
    match result {
        Ok(_) => panic!("应当报错"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_repository_source_builds_one_inline_map() {
    let doc = parse(
        r#"
[[map_sources]]
id = "solo"
mode = "repository"
provider = "gitee"
username = "u"
repository = "r"
[map_sources.map]
name_i18n.zh-CN = "单仓地图"
level_set_name = "Solo"
author = "A"
asset_rules = [{ type = "glob", value = "solo_v*.zip" }]
[[map_sources.map.relations]]
kind = "requires"
guid = "dev.gua.overcooked.diylevel"
version = ">=0.10.0"
"#,
    )
    .unwrap();
    let source = &doc.map_sources[0];
    assert_eq!(source.mode, MapSourceMode::Repository);
    assert_eq!(
        source.map.as_ref().unwrap().name,
        "单仓地图",
        "单语言镜像应从 zh-CN 折出来"
    );
    let label = SourceLabel {
        id: source.id.clone(),
        ..SourceLabel::default()
    };
    let map = build_declaration_map(source, &label, true, &origin()).unwrap();
    // 声明级坐标落到条目上，否则取 Release 时 404
    assert_eq!(map.provider, crate::models::Provider::Gitee);
    assert_eq!(map.username, "u");
    assert_eq!(map.repository, "r");
    assert_eq!(map.id, map_repository_id("solo", &map));
    assert_eq!(map.relations.len(), 1);
    assert!(map.builtin);
}

#[test]
fn a_document_source_must_carry_a_url() {
    let tag = error_tag(parse("[[map_sources]]\nid = \"x\"\n"));
    assert!(tag.contains("mapSource.declarationMissingUrl"), "{tag}");
}

#[test]
fn a_repository_source_must_carry_its_map_entry() {
    let tag = error_tag(parse(
        "[[map_sources]]\nid = \"x\"\nmode = \"repository\"\nusername = \"u\"\nrepository = \"r\"\n",
    ));
    assert!(tag.contains("mapSource.declarationMissingMap"), "{tag}");
}

#[test]
fn a_release_tags_source_must_name_its_repository() {
    let tag = error_tag(parse(
        "[[map_sources]]\nid = \"x\"\nmode = \"release-tags\"\n",
    ));
    assert!(tag.contains("mapSource.missingCoordinates"), "{tag}");
}

#[test]
fn a_release_note_block_becomes_a_tag_mode_entry() {
    let body = r#"共 2 张关卡。

<!-- oc2-map
level_set_name = "GAODA"
level_set_name_zh = "胡闹达学"
author = "GAODA"
name_i18n.zh-CN = "胡闹达学"
name_i18n.en = "GAODA"
note_i18n.zh-CN = "两张关卡，需要 DIYLevel。"
asset = "daxue_v${version}.zip"
latest_mapset_version = "0.3.2"
latest_info_version = "0.3.1"
requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }
-->
"#;
    let doc = parse(
            "[[map_sources]]\nid = \"community\"\nmode = \"release-tags\"\nprovider = \"github\"\nusername = \"CH3NGYZ\"\nrepository = \"oc2\"\n",
        )
        .unwrap();
    let declaration = &doc.map_sources[0];
    let note = crate::map_note::parse_release_note(body).unwrap().unwrap();
    let rule =
        crate::map_note::versioned_asset_rule(note.metadata.asset.as_deref(), "daxue").unwrap();
    let label = SourceLabel {
        id: declaration.id.clone(),
        name: "社区地图仓库".to_string(),
        ..SourceLabel::default()
    };
    let map = note_block_map(declaration, &label, &note, "daxue", rule, true, &origin()).unwrap();

    // 一个 tag 一张地图：tag 来自 Release，身份来自 note 里的块
    assert_eq!(map_tag(&map), Some("daxue"));
    assert_eq!(map.level_set_name, "GAODA");
    assert_eq!(map.level_set_name_zh, "胡闹达学");
    assert_eq!(map.author, "GAODA");
    // 卡片短语只来自块里的 note_i18n，单语言镜像跟着折出来
    assert_eq!(
        map.note_i18n.get("zh-CN").map(String::as_str),
        Some("两张关卡，需要 DIYLevel。")
    );
    assert_eq!(map.note, "两张关卡，需要 DIYLevel。");
    // 块里没写 group：用作者名回填（分组语义就是作者）
    assert_eq!(map.group, "GAODA");
    assert_eq!(
        map.group_i18n.get("zh-CN").map(String::as_str),
        Some("GAODA")
    );
    assert_eq!(map.name, "胡闹达学", "显示名单语言镜像必须折出来");
    assert!(map.id.starts_with("community::"), "{}", map.id);
    assert_eq!(map.source_id.as_deref(), Some("community"));
    assert_eq!(map.relations.len(), 1);
    assert_eq!(map.latest_mapset_version.as_deref(), Some("0.3.2"));
    // 包版本与 info 版本是两个字段：这里刻意写成不同值，验证两者各自透传
    assert_eq!(map.latest_info_version.as_deref(), Some("0.3.1"));
    // 版本号从资产名里按模板抽，规则必须整串锚定
    assert_eq!(map.asset_rules.len(), 1);
    assert!(map.asset_rules[0].value.starts_with('^'));
    assert!(map.asset_rules[0].value.ends_with('$'));
}

#[test]
fn a_block_without_declared_names_falls_back_to_info_and_the_release_body() {
    // 只写身份和资产、其余全靠 info 与正文，是最常见的发布方式：显示名要
    // 逐语言回填（zh-CN ← level_set_name_zh，en ← level_set_name），分组
    // 回填作者，卡片说明用块外正文 —— 少兜任何一条，卡片就退化成仓库坐标
    // 或「暂无说明」
    let source = r#"共 2 张关卡，需要 DIYLevel。

<!-- oc2-map
level_set_name = "GAODA"
level_set_name_zh = "胡闹达学"
author = "GAODA"
asset = "daxue_v${version}.zip"
-->
"#;
    let note = crate::map_note::parse_release_note(source)
        .unwrap()
        .unwrap();
    assert_eq!(note.body, "共 2 张关卡，需要 DIYLevel。");
    let rule =
        crate::map_note::versioned_asset_rule(note.metadata.asset.as_deref(), "daxue").unwrap();
    let doc = parse(
            "[[map_sources]]\nid = \"community\"\nmode = \"release-tags\"\nprovider = \"github\"\nusername = \"CH3NGYZ\"\nrepository = \"oc2\"\n",
        )
        .unwrap();
    let declaration = &doc.map_sources[0];
    let label = SourceLabel {
        id: declaration.id.clone(),
        name: "社区地图仓库".to_string(),
        ..SourceLabel::default()
    };
    let map = note_block_map(declaration, &label, &note, "daxue", rule, true, &origin()).unwrap();
    // 显示名逐语言回填
    assert_eq!(
        map.name_i18n.get("zh-CN").map(String::as_str),
        Some("胡闹达学")
    );
    assert_eq!(map.name_i18n.get("en").map(String::as_str), Some("GAODA"));
    assert_eq!(map.name, "胡闹达学");
    // 分组回填作者
    assert_eq!(map.group, "GAODA");
    assert_eq!(
        map.group_i18n.get("zh-CN").map(String::as_str),
        Some("GAODA")
    );
    // 卡片说明回填块外正文
    assert_eq!(map.note, "共 2 张关卡，需要 DIYLevel。");
    assert_eq!(
        map.note_i18n.get("zh-CN").map(String::as_str),
        Some("共 2 张关卡，需要 DIYLevel。")
    );
}

/// 真机联网回归：内置索引 → 单仓库地图 + release-tags 源 → 真实条目。
///
/// 默认 `#[ignore]`：单测不联网。改完 tag 源或 `map_note` 的字段后手动跑一次：
///
/// ```text
/// cargo test --manifest-path src-tauri/Cargo.toml --lib \
///   real_release_tag_source_builds_maps -- --ignored --nocapture
/// ```
///
/// 它验证的是整条真实链路：`maps.toml` → `load_map_tree` → 分页读完全部
/// Release → 解析每份 note 的 oc2-map 块 → 生成带版本规则的地图条目。
#[ignore]
#[tokio::test]
async fn real_release_tag_source_builds_maps() {
    let config = crate::config::default_config();
    let cache = crate::release_cache::RepoReleaseCache::default();
    let client =
        crate::releases::build_client(config.network.timeout_seconds).expect("HTTP 客户端应可创建");
    let ctx = MapSourceContext {
        cache: &cache,
        client: &client,
        config: &config,
        github_token: None,
        gitee_token: None,
        gitcode_token: None,
        pass_id: Some("real-release-tags"),
        force: false,
        previous: &[],
    };
    let maps = load_map_tree(crate::config::current_user_region(), true, &ctx)
        .await
        .expect("内置索引应能加载");

    // 单仓库地图（RW）离线就有；三张 tag 模式地图来自真实 Release note
    assert!(
        maps.iter().any(|map| map.level_set_name == "RW_LevelSet"),
        "单仓库地图应来自内联条目：{}",
        maps.len()
    );
    let mut tags: Vec<&str> = maps
        .iter()
        .filter(|map| map.source_id.as_deref() == Some("community-maps"))
        .filter_map(|map| map.tag.as_deref())
        .collect();
    tags.sort_unstable();
    assert_eq!(
        tags,
        ["blni", "chan", "daxue"],
        "三份 Release note 应各生成一张图"
    );

    let gaoda = maps
        .iter()
        .find(|map| map.level_set_name == "GAODA")
        .expect("胡闹达学应在列表里");
    assert_eq!(gaoda.author, "GAODA");
    assert_eq!(gaoda.level_set_name_zh, "胡闹达学");
    assert_eq!(gaoda.provider, crate::models::Provider::Github);
    assert_eq!(gaoda.username, "CH3NGYZ");
    assert_eq!(gaoda.repository, "oc2");
    assert!(
        !gaoda.note_i18n.get("zh-CN").is_none_or(String::is_empty),
        "卡片说明应来自 note_i18n：{:?}",
        gaoda.note_i18n.keys().collect::<Vec<_>>()
    );
    assert!(gaoda.name_i18n.contains_key("en"));
    assert!(
        gaoda
            .asset_rules
            .iter()
            .any(|rule| rule.value.starts_with('^') && rule.value.ends_with('$')),
        "版本规则必须整串锚定"
    );
    assert_eq!(gaoda.latest_mapset_version.as_deref(), Some("0.3.2"));
    assert!(
        gaoda
            .relations
            .iter()
            .any(|relation| relation.guid == "dev.gua.overcooked.diylevel")
    );

    // 没有元数据块的 Release（Dmm）不该混进来
    assert!(
        maps.iter().all(|map| map.tag.as_deref() != Some("Dmm")),
        "没有 oc2-map 块的 Release 必须被跳过"
    );
}
