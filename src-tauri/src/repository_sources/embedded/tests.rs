use super::super::transport::load_document;
use super::*;

#[test]
fn mod_index_has_one_closed_resource_tree_and_no_legacy_runtime_entry() {
    assert_eq!(BUILTIN_INDEX, "mods.toml");
    assert!(
        crate::models::DataProvider::Github
            .document("mods.toml")
            .ends_with("/mods.toml")
    );
    assert!(
        crate::models::DataProvider::Gitee
            .document("mods.toml")
            .ends_with("/mods.toml")
    );
    assert!(
        !BUNDLED_SOURCES
            .iter()
            .any(|(path, _)| *path == "repositories.toml")
    );
    let location = SourceLocation::parse(BUILTIN_INDEX).unwrap();
    let index = location.bundled_document().unwrap();
    assert_eq!(index.repository_sources.len(), 8);
    let mut repositories = 0;
    for source in index.repository_sources {
        // Source IDs remain stable while file locations change.
        assert!(source.id.starts_with("repositories-"));
        assert!(source.url.starts_with("mods/mods-"));
        let child = SourceLocation::parse(&source.url)
            .unwrap()
            .bundled_document()
            .unwrap();
        repositories += child.repositories.len();
    }
    assert_eq!(repositories, 27);
}

#[test]
fn embedded_index_templates_are_returned_as_independent_copies() {
    let location = SourceLocation::parse(BUILTIN_INDEX).unwrap();
    let original = location.bundled_document().unwrap();
    let mut draft = location.bundled_document().unwrap();
    draft.repository_sources[0].nickname = "Edited draft".into();
    draft.repository_sources.clear();
    let reloaded = location.bundled_document().unwrap();
    assert_eq!(
        reloaded.repository_sources.len(),
        original.repository_sources.len()
    );
    assert_eq!(
        reloaded.repository_sources[0].nickname,
        original.repository_sources[0].nickname
    );
}

/*
 * 以下嵌入文档不是 MOD 仓库源，各有自己的解析器与回归测试：
 *
 * - `repositories-proxies.toml`：只有代理清单，没有 repositories；
 * - `proxies.toml`、`proxies/`、`network-plans.toml`：新网络配置闭包，
 *   由 `network_policy::tests` 使用真实解析器覆盖；
 * - `maps*.toml`：地图源（`[[maps]]` / `[[map_sources]]`），由
 *   `map_sources::bundled_tests` 覆盖。
 *
 * 按名跳过而不是放宽断言：这条测试的价值就在「每个声称是仓库源的
 * 嵌入文件都必须能被仓库解析器读懂」，放宽会让真正的坏文档混过去。
 * 新增非仓库类嵌入文档时在这里登记，并为它补一份对应的解析测试。
 */
const NOT_REPOSITORY_DOCUMENTS: &[&str] = &[
    "repositories-proxies.toml",
    "proxies.toml",
    "network-plans.toml",
    "proxies/proxies-avatar.toml",
    "proxies/proxies-gcapi.toml",
    "proxies/proxies-ghapi.toml",
    "proxies/proxies-gtapi.toml",
    "proxies/proxies-mapsource.toml",
    "proxies/proxies-raw.toml",
    "proxies/proxies-release.toml",
    "maps.toml",
    "maps/maps-oc2-map-bundle.toml",
    "maps/maps-qingyuqingxue.toml",
];

#[test]
fn all_bundled_source_documents_use_the_shared_parser() {
    for (path, _) in BUNDLED_SOURCES {
        if NOT_REPOSITORY_DOCUMENTS
            .iter()
            .any(|skipped| path.eq_ignore_ascii_case(skipped))
        {
            continue;
        }
        let location = SourceLocation::parse(path).unwrap();
        let document = location
            .bundled_document()
            .unwrap_or_else(|error| panic!("{path}: {error}"));
        for child in document.repository_sources {
            if let SourceLocation::Bundled(_) = SourceLocation::parse(&child.url).unwrap() {
                assert!(
                    BUNDLED_SOURCES
                        .iter()
                        .any(|(path, _)| path.eq_ignore_ascii_case(&child.url)),
                    "{}",
                    child.url
                );
            }
        }
    }
}

/// 登记表必须覆盖所有非仓库类文档：新增一个忘了登记，上面那条测试就会红，
/// 但红的原因是「地图文档解析失败」，容易被误读成解析器坏了。这里直接说明。
#[test]
fn every_non_repository_document_is_registered() {
    for (path, _) in BUNDLED_SOURCES {
        let location = SourceLocation::parse(path).unwrap();
        if location.bundled_document().is_ok() {
            continue;
        }
        assert!(
            NOT_REPOSITORY_DOCUMENTS
                .iter()
                .any(|skipped| path.eq_ignore_ascii_case(skipped)),
            "{path} 无法被仓库解析器读取，但没有登记为「非仓库类文档」"
        );
    }
}

#[tokio::test]
async fn builtin_index_loads_only_declared_sources_and_relative_repository_documents() {
    let location = SourceLocation::parse(BUILTIN_INDEX).unwrap();
    /*
     * 用 `parse_document` 而不是 `toml::from_str`：昵称现在只认
     * `nickname_i18n`，镜像字段 `nickname` 是由 `normalize_source_nickname`
     * 从表里派生的 —— 直接反序列化拿到的是未归一化的空镜像。
     */
    let declared_index = parse_document(
        location.bundled_text().unwrap(),
        &location,
        &Default::default(),
    )
    .unwrap();
    let index = load_document(&location, &Default::default()).await.unwrap();
    assert!(index.repositories.is_empty());
    assert!(!index.repository_sources.is_empty());
    assert_eq!(
        index
            .repository_sources
            .iter()
            .map(|source| (&source.id, &source.nickname))
            .collect::<Vec<_>>(),
        declared_index
            .repository_sources
            .iter()
            .map(|source| (&source.id, &source.nickname))
            .collect::<Vec<_>>()
    );

    for (source, declared) in index
        .repository_sources
        .iter()
        .zip(&declared_index.repository_sources)
    {
        assert!(source.region.is_none(), "{}", source.id);
        let child_location = SourceLocation::resolve(&declared.url, Some(&location)).unwrap();
        assert!(matches!(&child_location, SourceLocation::Bundled(_)));
        assert_eq!(source.url, child_location.address());
        let mut declared_child: RepositorySourceDocument =
            toml::from_str(child_location.bundled_text().unwrap()).unwrap();
        for repository in &mut declared_child.repositories {
            crate::config::normalize_repository_i18n(repository).unwrap();
        }
        let child = load_document(&child_location, &Default::default())
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", source.url));
        assert!(!child.repositories.is_empty(), "{}", source.url);
        assert_eq!(
            child
                .repositories
                .iter()
                .map(|repository| (
                    &repository.name,
                    &repository.provider,
                    &repository.username,
                    &repository.repository
                ))
                .collect::<Vec<_>>(),
            declared_child
                .repositories
                .iter()
                .map(|repository| (
                    &repository.name,
                    &repository.provider,
                    &repository.username,
                    &repository.repository
                ))
                .collect::<Vec<_>>(),
            "{}",
            source.url
        );
        assert_eq!(
            child
                .repository_sources
                .iter()
                .map(|source| (&source.id, &source.nickname))
                .collect::<Vec<_>>(),
            declared_child
                .repository_sources
                .iter()
                .map(|source| (&source.id, &source.nickname))
                .collect::<Vec<_>>(),
            "{}",
            source.url
        );
        for repository in child.repositories {
            assert!(matches!(
                repository.region.as_deref(),
                None | Some("chinese_mainland" | "international")
            ));
        }
    }
}
