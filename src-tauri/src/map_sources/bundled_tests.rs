use super::document::MapSourceDeclaration;
use super::document::parse_document;
use super::document::{MapSourceDocument, MapSourceMode};
use super::tree::{BUILTIN_MAP_INDEX, bundled_map_tree};
use crate::repository_sources::SourceLocation;

fn index_location() -> SourceLocation {
    SourceLocation::parse(BUILTIN_MAP_INDEX).expect("内置索引应该可解析")
}

fn index_document() -> MapSourceDocument {
    let location = index_location();
    parse_document(
        location.bundled_text().unwrap(),
        &location,
        &Default::default(),
    )
    .expect("内置 maps.toml 应能解析")
}

fn declaration(id: &str) -> MapSourceDeclaration {
    fn find(location: &SourceLocation, id: &str, depth: usize) -> Option<MapSourceDeclaration> {
        if depth > 3 {
            return None;
        }
        let doc =
            parse_document(location.bundled_text().ok()?, location, &Default::default()).ok()?;
        for source in doc.map_sources {
            if source.id == id {
                return Some(source);
            }
            if source.mode == MapSourceMode::Document {
                let child = SourceLocation::resolve(&source.url, Some(location)).ok()?;
                if let Some(found) = find(&child, id, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    find(&index_location(), id, 0).unwrap_or_else(|| panic!("内置树里应有源 {id}"))
}

#[test]
fn builtin_map_index_declares_only_sources() {
    let doc = index_document();
    assert!(doc.maps.is_empty(), "索引只声明源，条目在各自源里");
    assert!(!doc.map_sources.is_empty());
    assert!(
        doc.map_sources
            .iter()
            .all(|source| source.mode == MapSourceMode::ReleaseTags)
    );
    assert_eq!(
        declaration("maps-oc2-map-bundle").provider,
        Some(crate::models::Provider::Gitcode)
    );
}

/// 第三方源：一个 tag 一张图，元数据跟着各自 Release note 走。
///
/// 单测不联网，所以这里只锁「声明写得对」；真正解析 note 块的逻辑在
/// `map_note` 的单测里（纯函数、离线可跑）。
#[test]
fn builtin_community_source_declares_release_tags_mode() {
    let source = declaration("maps-oc2-map-bundle");
    assert_eq!(source.id, "maps-oc2-map-bundle");
    assert_eq!(source.mode, MapSourceMode::ReleaseTags);
    assert_eq!(source.provider, Some(crate::models::Provider::Gitcode));
    assert_eq!(source.username, "CH3NGY2");
    assert_eq!(source.repository, "OC2-Map-Bundle");
    assert_eq!(source.page_size, None, "分页由共享 Release 入口统一控制");
    assert_eq!(source.max_pages, None, "持续分页直到 API 返回不足 50 条");
    assert!(source.map.is_none(), "release-tags 不需要内联条目");
}

/// 内置树（离线）为空：唯一的源是 `release-tags`，条目要联网读 Release
/// note 才生成。
///
/// 这条断言守住「内置树不偷偷联网、不伪造 tag 条目」；离线首启的空地图页
/// 是这套配置的已知代价（要内置离线地图就再加一个 `repository` 源）。
#[test]
fn bundled_tree_is_empty_offline() {
    assert!(
        bundled_map_tree().is_empty(),
        "内置索引只有 release-tags 源：离线解析不出条目"
    );
}

/// Explicit opt-in: reads the real public API through the product source loader.
#[tokio::test]
#[ignore = "requires public GitCode network access"]
async fn real_gitcode_bundle_loads_release_declarations() {
    let config = crate::config::default_config();
    let client = crate::releases::build_client(30).unwrap();
    let source = declaration("maps-oc2-map-bundle");
    let probe = super::release_tags::declaration_request(&source);
    let releases = crate::releases::fetch_releases(&client, &probe, None, true, &[])
        .await
        .unwrap_or_else(|error| panic!("real public Release API failed: {error}"));
    println!("public API returned {} releases", releases.len());
    let cache = crate::release_cache::RepoReleaseCache::default();
    let context = super::context::MapSourceContext {
        observer: None,
        cache: &cache,
        client: &client,
        config: &config,
        github_token: None,
        gitee_token: None,
        gitcode_token: None,
        pass_id: Some("gitcode-public-acceptance"),
        force: false,
        previous: &[],
    };
    let maps = super::tree::load_map_tree(
        crate::config::REGION_CHINESE_MAINLAND,
        crate::models::DataProvider::Builtin,
        &context,
    )
    .await
    .unwrap();
    assert!(
        maps.len() >= 50,
        "expected uploaded maps, got {}",
        maps.len()
    );
    assert!(
        maps.iter()
            .all(|map| map.provider == crate::models::Provider::Gitcode
                && map.username == "CH3NGY2"
                && map.repository == "OC2-Map-Bundle")
    );
    assert!(
        maps.iter()
            .all(|map| map.source_id.as_deref() == Some("maps-oc2-map-bundle"))
    );
    if let Some(path) = std::env::var_os("OC2_MAP_ACCEPTANCE_JSON") {
        let info = maps
            .iter()
            .map(super::view::map_to_info)
            .collect::<Vec<_>>();
        std::fs::write(path, serde_json::to_vec_pretty(&info).unwrap()).unwrap();
    }
    println!(
        "public GitCode bundle: {} maps parsed by the real source loader",
        maps.len()
    );
}
