use crate::level_sets::identity::level_set_identity_matches;

#[test]
fn identity_matches_ignore_case_and_surrounding_whitespace() {
    assert!(level_set_identity_matches(
        "RW_LevelSet",
        None,
        "RW",
        " rw_levelset ",
        None,
        " rw "
    ));
}

#[test]
fn identity_requires_a_non_empty_level_set_name() {
    // 空名 = info 没解析出来，不能把所有坏目录都算成同一张地图
    assert!(!level_set_identity_matches("", None, "", "", None, ""));
    assert!(!level_set_identity_matches(
        "  ",
        None,
        "RW",
        "RW_LevelSet",
        None,
        "RW"
    ));
    assert!(!level_set_identity_matches(
        "RW_LevelSet",
        None,
        "RW",
        "",
        None,
        "RW"
    ));
}

#[test]
fn identity_compares_the_author_too() {
    assert!(!level_set_identity_matches(
        "RW_LevelSet",
        None,
        "RW",
        "RW_LevelSet",
        None,
        "Other"
    ));
    // 官方导出的 info 常不写作者：两边都空算相同
    assert!(level_set_identity_matches(
        "RW_LevelSet",
        None,
        "",
        "RW_LevelSet",
        None,
        ""
    ));
}

#[test]
fn identity_separates_maps_that_only_differ_in_the_chinese_name() {
    // 实测「胡闹糕中」与「胡闹达学」：英文集名与作者完全相同，只有中文名不同
    assert!(!level_set_identity_matches(
        "GAODA",
        Some("胡闹糕中"),
        "GAODA",
        "GAODA",
        Some("胡闹达学"),
        "GAODA"
    ));
    // 同一张图升级：三段都不变，仍应判为同一张
    assert!(level_set_identity_matches(
        "GAODA",
        Some("胡闹糕中"),
        "GAODA",
        "GAODA",
        Some("胡闹糕中"),
        "GAODA"
    ));
    // 地图源没写中文名（老源）时退回前两段，不能因为缺字段就绑不上
    assert!(level_set_identity_matches(
        "GAODA",
        Some("胡闹糕中"),
        "GAODA",
        "GAODA",
        None,
        "GAODA"
    ));
}
