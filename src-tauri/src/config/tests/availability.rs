use crate::config::availability::PLATFORM_MACOS;
use crate::config::availability::PLATFORM_WINDOWS;
use crate::config::availability::REGION_CHINESE_MAINLAND;
use crate::config::availability::REGION_INTERNATIONAL;
use crate::config::availability::current_platform;
use crate::config::availability::mod_repository_is_available;
use crate::config::availability::platform_matches;
use crate::config::availability::repository_matches_channel;
use crate::config::defaults::default_config;
use crate::config::identity::mod_repository_from_export;
use crate::models::*;

use super::REGION_REPOSITORY;

#[test]
fn repository_regions_are_optional_validated_and_preserved() {
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let legacy = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(legacy.repositories[0].region, None);
    for region in ["chinese_mainland", "international", ""] {
        let text =
            REGION_REPOSITORY.replace("channel =", &format!("region = \"{region}\"\nchannel ="));
        let document =
            crate::repository_sources::parse_document(&text, &location, &Default::default())
                .unwrap();
        assert_eq!(
            document.repositories[0].region.as_deref(),
            (!region.is_empty()).then_some(region)
        );
        let repository =
            mod_repository_from_export(&document.repositories[0], "regional".into(), false);
        let mut config = default_config();
        config.repositories.push(repository);
        let serialized = toml::to_string(&config).unwrap();
        let restored: AppConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(
            restored.repositories[0].region.as_deref(),
            (!region.is_empty()).then_some(region)
        );
    }
    let unknown = REGION_REPOSITORY.replace("channel =", "region = \"unknown\"\nchannel =");
    let document =
        crate::repository_sources::parse_document(&unknown, &location, &Default::default())
            .unwrap();
    assert_eq!(document.repositories[0].region, None);
    let invalid_type = REGION_REPOSITORY.replace("channel =", "region = true\nchannel =");
    assert!(
        crate::repository_sources::parse_document(&invalid_type, &location, &Default::default())
            .is_err()
    );
}

#[test]
fn repository_platform_is_optional_normalized_and_enforced() {
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();

    // 缺省：无平台字段 → 全平台可用
    let document = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(document.repositories[0].platform, None);
    assert!(platform_matches(None, PLATFORM_WINDOWS));
    assert!(platform_matches(None, PLATFORM_MACOS));

    // 显式空串：同样全平台
    let empty = REGION_REPOSITORY.replace("channel =", "platform = \"\"\nchannel =");
    let document =
        crate::repository_sources::parse_document(&empty, &location, &Default::default()).unwrap();
    assert_eq!(document.repositories[0].platform, None);

    // windows / macos：各自只对对应平台可用
    for (value, ok_on_windows, ok_on_macos) in [("windows", true, false), ("macos", false, true)] {
        let text =
            REGION_REPOSITORY.replace("channel =", &format!("platform = \"{value}\"\nchannel ="));
        let document =
            crate::repository_sources::parse_document(&text, &location, &Default::default())
                .unwrap();
        assert_eq!(document.repositories[0].platform.as_deref(), Some(value));
        assert_eq!(
            platform_matches(Some(value), PLATFORM_WINDOWS),
            ok_on_windows,
            "{value} 在 windows 上"
        );
        assert_eq!(
            platform_matches(Some(value), PLATFORM_MACOS),
            ok_on_macos,
            "{value} 在 macos 上"
        );
    }

    // 别名归一化
    for (alias, expected) in [
        ("win", PLATFORM_WINDOWS),
        ("osx", PLATFORM_MACOS),
        ("darwin", PLATFORM_MACOS),
        ("mac", PLATFORM_MACOS),
    ] {
        let text =
            REGION_REPOSITORY.replace("channel =", &format!("platform = \"{alias}\"\nchannel ="));
        let document =
            crate::repository_sources::parse_document(&text, &location, &Default::default())
                .unwrap();
        assert_eq!(
            document.repositories[0].platform.as_deref(),
            Some(expected),
            "别名 {alias}"
        );
    }

    // 无法识别的平台：验证阶段拒绝整条仓库（不是静默忽略）
    let invalid = REGION_REPOSITORY.replace("channel =", "platform = \"linux\"\nchannel =");
    let error = crate::repository_sources::parse_document(&invalid, &location, &Default::default())
        .unwrap_err();
    assert!(error.to_string().contains("invalidPlatform"), "{error}");
}

/// 平台不适用时，mod_repository_is_available 与地区不适用表现完全一致。
#[test]
fn platform_mismatch_hides_repository_like_region_mismatch() {
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let text = REGION_REPOSITORY.replace("channel =", "platform = \"macos\"\nchannel =");
    let document =
        crate::repository_sources::parse_document(&text, &location, &Default::default()).unwrap();
    let mut repository =
        mod_repository_from_export(&document.repositories[0], "mac-only".into(), false);
    repository.enabled = true;
    let mut config = default_config();
    config.repositories.push(repository);
    let stored = config.repositories[0].clone();
    // 在 macOS 上可用，在 Windows 上不可用 —— 与 region 的判定方式一致
    assert!(
        mod_repository_is_available(&config, &stored, REGION_CHINESE_MAINLAND)
            == (current_platform() == PLATFORM_MACOS)
    );
}

#[test]
fn repository_channel_requires_both_source_and_repository_regions() {
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let document = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap();
    let mut repository =
        mod_repository_from_export(&document.repositories[0], "regional".into(), false);
    let mut config = default_config();
    for (configured, china, international) in [
        (None, true, true),
        (Some(""), true, true),
        (Some("chinese_mainland"), true, false),
        (Some("international"), false, true),
        (Some(" MAINLAND-CHINA "), true, false),
        (Some("Other"), false, true),
    ] {
        repository.region = configured.map(str::to_owned);
        assert_eq!(
            repository_matches_channel(&config, &repository, REGION_CHINESE_MAINLAND),
            china
        );
        assert_eq!(
            repository_matches_channel(&config, &repository, REGION_INTERNATIONAL),
            international
        );
    }
    repository.source_id = Some("restricted-source".into());
    repository.region = None;
    assert!(!repository_matches_channel(
        &config,
        &repository,
        REGION_CHINESE_MAINLAND
    ));
    config.repository_sources.push(RepositorySource {
        network_plan: Default::default(),
        id: "restricted-source".into(),
        nickname: "Restricted source".into(),
        nickname_i18n: Default::default(),
        url: "https://example.com/source.toml".into(),
        region: Some(REGION_CHINESE_MAINLAND.into()),
        builtin: false,
        parent_id: None,
    });
    assert!(repository_matches_channel(
        &config,
        &repository,
        REGION_CHINESE_MAINLAND
    ));
    assert!(!repository_matches_channel(
        &config,
        &repository,
        REGION_INTERNATIONAL
    ));
    repository.region = Some(REGION_INTERNATIONAL.into());
    assert!(!repository_matches_channel(
        &config,
        &repository,
        REGION_CHINESE_MAINLAND
    ));
    assert!(!repository_matches_channel(
        &config,
        &repository,
        REGION_INTERNATIONAL
    ));
}

#[test]
fn gitee_community_applies_to_every_region_when_its_region_is_empty() {
    /*
     * 两个社区源默认不限地区：Gitee 社区仓库在地区为空时对所有地区可用，
     * 与 GitHub 社区一致。平台限制已移除，判定只看仓库与来源自身的 region。
     */
    let mut config = default_config();
    let mut source = crate::community::source(&Provider::Gitee);
    source.region = None;
    config.repository_sources.push(source);
    let mut repo =
        crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
            .unwrap();
    repo.region = None;
    for region in [REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL, "", "unknown"] {
        assert!(
            repository_matches_channel(&config, &repo, region),
            "region={region}"
        );
    }
    // 仓库自己声明地区时仍按该地区过滤
    repo.region = Some(REGION_INTERNATIONAL.into());
    assert!(!repository_matches_channel(
        &config,
        &repo,
        REGION_CHINESE_MAINLAND
    ));
    assert!(repository_matches_channel(
        &config,
        &repo,
        REGION_INTERNATIONAL
    ));
    assert_eq!(
        config.repositories.len(),
        default_config().repositories.len()
    );
}

#[test]
fn availability_requires_enabled_and_matching_region() {
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let document = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap();
    let mut repository =
        mod_repository_from_export(&document.repositories[0], "regional".into(), false);
    let config = default_config();

    /*
     * `mod_repository_is_available` 是「这个仓库此刻是否存在」的唯一判定：
     * enabled 与地区两个条件的交集。任一不满足都等于不存在，
     * 因此既不会显示，也不会被刷新、安装或参与更新统计。
     */
    for (enabled, region) in [
        (true, None),
        (true, Some("international")),
        (true, Some("chinese_mainland")),
        (false, None),
        (false, Some("international")),
    ] {
        repository.enabled = enabled;
        repository.region = region.map(str::to_owned);
        for current in [REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL] {
            assert_eq!(
                mod_repository_is_available(&config, &repository, current),
                enabled && repository_matches_channel(&config, &repository, current),
                "enabled={enabled}, region={region:?}, current={current}"
            );
        }
    }

    // international 仓库在中国大陆彻底不存在（这是本次修复的原始症状）
    repository.enabled = true;
    repository.region = Some(REGION_INTERNATIONAL.into());
    assert!(!mod_repository_is_available(
        &config,
        &repository,
        REGION_CHINESE_MAINLAND
    ));
    assert!(mod_repository_is_available(
        &config,
        &repository,
        REGION_INTERNATIONAL
    ));
}
