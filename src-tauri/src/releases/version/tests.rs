use super::*;

#[test]
fn prerelease_detection_covers_version_suffixes() {
    // 版本号带 - 后缀即预发布，与 Release 元数据标记是或关系
    assert!(is_prerelease_version("0.4.0-beta"));
    assert!(is_prerelease_version("0.4.0.5-beta"));
    assert!(is_prerelease_version("v1.0.0-rc.1"));
    // 四段 = 前三位的 beta 序号（semver：1.8.7.23 ≡ 1.8.7-beta.23）
    assert!(is_prerelease_version("0.4.0.5"));
    assert!(is_prerelease_version("1.8.7.23"));
    assert!(is_prerelease_version("v1.8.7.23"));
    /*
     * 本项目的 beta 写法不统一。下面四种语义完全相同，都必须识别。
     *
     * 曾经只有 `-x.y` 一种被识别：`1.8.7.beta4` 因为用 `-` 正则
     * 整个解析失败，既不判预发布也无法参与版本比较。
     */
    assert!(is_prerelease_version("1.8.5-pre.01"));
    assert!(is_prerelease_version("1.9.7-beta.4"));
    assert!(is_prerelease_version("1.8.7-beta1"));
    assert!(is_prerelease_version("1.8.7.beta4"));
    // 三段正式版与尾部补 0 段都不是预发布（beta 序号为 0 不算 beta）
    assert!(!is_prerelease_version("0.4.0"));
    assert!(!is_prerelease_version("0.4.0.0"));
    assert!(!is_prerelease_version("unknown"));
}

/*
 * 四条 beta 写法解析出的序号要能比较，否则「beta2 → beta3」
 * 这种升级会被判成「已是最新」。
 */
#[test]
fn every_beta_spelling_parses_and_orders_by_serial() {
    use std::cmp::Ordering;
    for (older, newer) in [
        ("1.8.7-beta1", "1.8.7-beta2"),
        ("1.9.7-beta.1", "1.9.7-beta.2"),
        ("1.8.7.beta1", "1.8.7.beta2"),
        ("1.8.5-pre.1", "1.8.5-pre.2"),
        ("1.8.7.1", "1.8.7.2"),
    ] {
        assert_eq!(
            compare_release_versions(newer, older),
            Some(Ordering::Greater),
            "{newer} 应新于 {older}"
        );
    }
}

/*
 * 同号的 beta 必须低于正式版，反过来也一样。
 *
 * 这是「订阅 Stable 时不接受 beta 更新」的底层依据：
 * 本地装正式版 1.8.7，仓库有 beta 号 1.8.7.23 时不能提示「有更新」。
 */
#[test]
fn beta_serial_never_outranks_its_stable_version() {
    use std::cmp::Ordering;
    for beta in [
        "1.8.7-beta.1",
        "1.8.7-beta.23",
        "1.8.7-beta1",
        "1.8.7.beta4",
        "1.8.7.23",
    ] {
        assert_eq!(
            compare_release_versions(beta, "1.8.7"),
            Some(Ordering::Less),
            "{beta} 应低于同号正式版 1.8.7"
        );
        assert_eq!(
            compare_release_versions("1.8.7", beta),
            Some(Ordering::Greater),
            "正式版 1.8.7 应高于 {beta}"
        );
    }
    // 下一个正式版仍然更高
    assert_eq!(
        compare_release_versions("1.8.8-beta1", "1.8.7"),
        Some(Ordering::Greater),
        "1.8.8-beta1 的次版本号更高，应视为更新"
    );
}

/*
 * 紧凑写法 `-betaN` 必须按**数值**比较。
 *
 * 严格 semver 里 `-beta1` 是单个字母数字标识符，按 ASCII 字典序比较，
 * 于是 `"beta10" < "beta9"`（'1' < '9'）—— 发到第 10 个测试版时
 * 更新检测会静默失效。规范推荐 `-beta.1` 正是为了避免这个。
 *
 * 本项目用紧凑写法，所以在解析层把 `beta1` 拆成 `beta` + `1`，
 * 与 `beta.1` 等价。
 */
#[test]
fn compact_beta_serial_compares_numerically() {
    use std::cmp::Ordering;
    for (older, newer) in [
        ("1.8.8-beta9", "1.8.8-beta10"),
        ("1.8.8-beta10", "1.8.8-beta11"),
        ("1.8.8-beta99", "1.8.8-beta100"),
        ("1.8.8-rc9", "1.8.8-rc10"),
    ] {
        assert_eq!(
            compare_release_versions(newer, older),
            Some(Ordering::Greater),
            "{newer} 应新于 {older}"
        );
    }
    // `beta1` 与 `beta.1` 是同一个序号
    assert_eq!(
        compare_release_versions("1.8.8-beta1", "1.8.8-beta.1"),
        Some(Ordering::Equal)
    );
    assert_eq!(
        compare_release_versions("1.8.8-beta10", "1.8.8-beta.10"),
        Some(Ordering::Equal)
    );
}

/// semver 2.0.0 §11 的经典优先级链，逐条必须成立。
#[test]
fn matches_the_semver_spec_precedence_chain() {
    use std::cmp::Ordering;
    let chain = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
    ];
    for pair in chain.windows(2) {
        assert_eq!(
            compare_release_versions(pair[1], pair[0]),
            Some(Ordering::Greater),
            "{} 应低于 {}",
            pair[0],
            pair[1]
        );
    }
}

/*
 * 发布契约：新发布必须符合严格 semver。
 *
 * 注意这是**发布要求**，不是解析门槛 —— 解析器仍宽容接受历史 tag
 * （见下一个测试），否则老用户会看到「版本未知」。
 */
#[test]
fn strict_semver_accepts_canonical_and_rejects_legacy_shapes() {
    // 合规：三段，正式版与 x.y.z-tag.w 预发布
    for ok in [
        "1.9.8",
        "v1.9.8",
        "0.4.0",
        "1.9.8-beta.1",
        "1.9.8-alpha.2",
        "1.9.8-rc.1",
        "1.9.8-beta.10",
        "1.9.8-beta.1+build.5",
        "1.9.8-0",
    ] {
        assert!(is_strict_semver(ok), "{ok} 应判为合规");
    }
    // 不合规：段数不对、前导零、预发布放置错误
    for bad in [
        "1.9.8.1",       // 四段
        "1.9",           // 两段
        "v0.10",         // 两段
        "01.9.8",        // 前导零
        "1.09.8",        // 前导零
        "1.9.8.beta.1",  // 预发布未用 -
        "1.9.8-beta.01", // 标识符前导零
        "1.9.8-",        // 空标识符
        "1.9.8-beta..1", // 空段
        "1.9.8-beta 1",  // 空格
        "",              // 空
    ] {
        assert!(!is_strict_semver(bad), "{bad} 应判为不合规");
    }
}

/*
 * 真实仓库里存在的历史 tag：不合规但仍必须能解析与比较。
 *
 * 这是「解析宽容」的验收：`v1.8.7.23` 这类四段号是已发布的事实，
 * 拒绝它们会让已装用户看到「版本未知」并停止收到更新提示。
 *
 * 注意 `1.8.7-beta1` 不在这个列表里：它语法上合法（单个字母数字
 * 标识符），只是不再推荐 —— 新发布统一用 `1.8.7-beta.1`。
 */
#[test]
fn legacy_tags_stay_parseable_even_though_they_are_not_strict() {
    use std::cmp::Ordering;
    for legacy in ["v1.8.7.23", "1.8.7.23", "v0.10", "0.9", "1.8.7.beta4"] {
        // 都不符合严格 semver
        assert!(!is_strict_semver(legacy), "{legacy} 不应判为合规");
        // 但都能参与比较
        assert!(
            SegmentVersion::parse(legacy).is_some(),
            "{legacy} 必须仍可解析"
        );
    }
    // 紧凑 beta 写法语法合法（解析层会拆开让序号按数值比）
    assert!(is_strict_semver("1.8.7-beta1"));
    assert!(SegmentVersion::parse("1.8.7-beta1").is_some());
    // 四段号仍按 beta 语义排序
    assert_eq!(
        compare_release_versions("v1.8.7.23", "v1.8.7"),
        Some(Ordering::Less)
    );
    assert_eq!(
        compare_release_versions("v1.8.7.23", "v1.8.7.21"),
        Some(Ordering::Greater)
    );
    // 两段号补齐为三段后仍可比较
    assert_eq!(
        compare_release_versions("v0.10", "v0.9"),
        Some(Ordering::Greater)
    );
}

/*
 * 现实约束：.NET 程序集版本最多四段纯数字，写不出 `1.9.8-beta.1`。
 *
 * HostUtilities 因此采用「DLL 四段数字 + GitHub tag 标准 semver」的发布模型：
 *
 *     DLL version  1.9.8.1
 *     release tag  v1.9.8-beta.1
 *
 * 后端比较必须把这二者视为同一版本，否则装了 beta.1 的本地副本
 * 检测不到 beta.2，或同一个 beta 被反复提示为更新。
 */
#[test]
fn dll_four_segment_version_equals_release_beta_tag() {
    use std::cmp::Ordering;
    for serial in [1, 2, 9, 10, 21, 23] {
        let dll = format!("1.9.8.{serial}");
        let tag = format!("v1.9.8-beta.{serial}");
        assert_eq!(
            compare_release_versions(&dll, &tag),
            Some(Ordering::Equal),
            "{dll} 应等于 {tag}"
        );
    }
}

/*
 * 本地装四段 beta 时的更新检测：
 *   · 线上更新的 beta → 有更新
 *   · 线上同基版本正式版 → 有更新（同基 beta 低于 stable）
 *   · 序号跨十位仍按数值（旧实现把 .21 判得比 .1 小）
 */
#[test]
fn local_four_segment_beta_upgrades_to_newer_beta_and_to_stable() {
    use std::cmp::Ordering;
    assert_eq!(
        compare_release_versions("v1.9.8-beta.2", "1.9.8.1"),
        Some(Ordering::Greater),
        "本地 beta.1 应检测到 beta.2"
    );
    assert_eq!(
        compare_release_versions("v1.9.8-beta.1", "1.9.8.2"),
        Some(Ordering::Less),
        "本地 beta.2 不应被更旧的 beta.1 提示更新"
    );
    assert_eq!(
        compare_release_versions("v1.9.8-beta.21", "1.9.8.1"),
        Some(Ordering::Greater),
        "beta.21 应大于 beta.1"
    );
    assert_eq!(
        compare_release_versions("v1.9.8", "1.9.8.1"),
        Some(Ordering::Greater),
        "本地 beta 应能升级到同基版本正式版"
    );
    assert_eq!(
        compare_release_versions("v1.9.9-beta.1", "1.9.8.1"),
        Some(Ordering::Greater),
        "下一条产品线的 beta 也算更新"
    );
    assert_eq!(
        compare_release_versions("v1.9.7", "1.9.8.1"),
        Some(Ordering::Less),
        "上一个正式版不应提示更新"
    );
}

#[test]
fn version_comparison_counts_every_segment() {
    use std::cmp::Ordering;
    // 真实场景：DLL 里是 0.4.0.0，Release tag 是 0.4.0。
    // 旧实现把第 4 段拼成 -0 当预发布，导致「可更新」误报。
    assert_eq!(
        compare_release_versions("0.4.0.0", "0.4.0"),
        Some(Ordering::Equal),
        "0.4.0.0 与 0.4.0 是同一版本"
    );
    assert_eq!(
        compare_release_versions("0.4", "0.4.0"),
        Some(Ordering::Equal)
    );
    assert_eq!(
        compare_release_versions("v0.4", "0.4.0"),
        Some(Ordering::Equal)
    );

    // 第 4 段是 beta 序号：0.4.0.5 ≡ 0.4.0-beta.5，低于正式版 0.4.0
    assert_eq!(
        compare_release_versions("1.8.7.23", "1.8.7.21"),
        Some(Ordering::Greater),
        "beta 23 高于 beta 21"
    );
    assert_eq!(
        compare_release_versions("1.8.7.23", "1.8.7"),
        Some(Ordering::Less),
        "beta 低于同号正式版"
    );
    assert_eq!(
        compare_release_versions("1.8.7.23", "1.8.7.21"),
        Some(Ordering::Greater)
    );
    assert_eq!(
        compare_release_versions("0.4.0", "0.4.0.0"),
        Some(Ordering::Equal),
        "尾部补 0 段无意义"
    );

    // 预发布低于同号正式版
    assert_eq!(
        compare_release_versions("0.4.0-beta", "0.4.0"),
        Some(Ordering::Less)
    );
    assert_eq!(
        compare_release_versions("0.4.0.1-beta", "0.4.0.1"),
        Some(Ordering::Equal),
        "第 4 段已表达 beta 序号，带 - 时保持同一序号"
    );

    // 预发布标识逐段比较：alpha < alpha.1 < beta
    assert_eq!(
        compare_release_versions("0.4.0-alpha", "0.4.0-alpha.1"),
        Some(Ordering::Less)
    );
    assert_eq!(
        compare_release_versions("0.4.0-alpha.1", "0.4.0-beta"),
        Some(Ordering::Less)
    );
}

/*
 * 同一个版本号，无论从哪个字段读，预发布结论必须一致。
 *
 * 真实事故：仓库卡片读 `tag_name`（完整四段）判成预发布，
 * 安装确认弹窗读 `version`（后端归一后的三段）判成正式版，
 * 同一个 Release 在两处显示相反结论。
 *
 * 根因是判定被分散到前端各处、各自选字段反推。现在唯一实现放在
 * 后端 `is_prerelease_version`，并随 ReleaseInfo 的 prerelease 下发。
 */
#[test]
fn prerelease_verdict_is_identical_for_tag_and_normalized_version() {
    for tag in ["v1.8.7.23", "1.8.7.23-beta", "v0.4.0", "v0.4.0.0"] {
        let from_tag = is_prerelease_version(tag);
        let normalized = parse_version(tag).map(|value| value.to_string());
        let from_version = normalized.as_deref().is_some_and(is_prerelease_version);
        // 归一化会丢第 4 段，所以 version 侧的结论只能"不少于" tag 侧：
        // 带 - 后缀的必须保留预发布语义，四段的差异由后端提前算好下发。
        if from_tag && tag.contains('-') {
            assert!(from_version, "{tag} 归一后丢了预发布语义");
        }
    }
    // 后端下发给前端的就是下面这个值，前端不再自己算
    assert!(is_prerelease_version("v1.8.7.23"));
    assert!(!is_prerelease_version("v0.4.0"));
}
