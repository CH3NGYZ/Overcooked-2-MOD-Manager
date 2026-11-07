use super::*;

#[test]
fn effective_relations_falls_back_to_repository_declarations() {
    let mut repo = binding_repository();
    repo.relations = vec![requires("base.guid", ">=1.0.0")];
    // 没有覆盖表、版本为空、版本只有空白：都用仓库级声明
    assert_eq!(
        super::effective_relations(&repo, Some("9.9.9")),
        repo.relations
    );
    assert_eq!(super::effective_relations(&repo, None), repo.relations);
    assert_eq!(
        super::effective_relations(&repo, Some("  ")),
        repo.relations
    );
}

#[test]
fn effective_relations_replaces_the_whole_table_on_a_hit() {
    let mut repo = binding_repository();
    repo.relations = vec![
        requires("base.guid", ">=1.0.0"),
        requires("conflict.guid", ""),
    ];
    repo.release_relations = vec![override_entry(
        ">=1.5.0",
        vec![requires("new.guid", ">=3.0.0")],
    )];
    let effective = super::effective_relations(&repo, Some("1.6.0"));
    assert_eq!(effective, vec![requires("new.guid", ">=3.0.0")]);
    // 整体替换，不是合并：旧表里的冲突项不能残留
    assert!(!effective.iter().any(|item| item.guid == "conflict.guid"));
    // 区间外的版本继续用仓库级声明
    assert_eq!(
        super::effective_relations(&repo, Some("1.4.9")),
        repo.relations
    );
}

#[test]
fn effective_relations_prefers_the_highest_lower_bound() {
    let mut repo = binding_repository();
    repo.release_relations = vec![
        override_entry("*", vec![requires("any.guid", ">=1.0.0")]),
        override_entry(">=1.0.0", vec![requires("mid.guid", ">=2.0.0")]),
        override_entry(">=2.0.0", vec![requires("high.guid", ">=3.0.0")]),
    ];
    assert_eq!(
        super::effective_relations(&repo, Some("2.5.0")),
        vec![requires("high.guid", ">=3.0.0")]
    );
    assert_eq!(
        super::effective_relations(&repo, Some("1.5.0")),
        vec![requires("mid.guid", ">=2.0.0")]
    );
    // 下限为空的覆盖表匹配一切，但优先级最低：没有更高的下限命中时才用它
    assert_eq!(
        super::effective_relations(&repo, Some("0.5.0")),
        vec![requires("any.guid", ">=1.0.0")]
    );
}

#[test]
fn effective_relations_treats_both_bounds_as_inclusive() {
    let mut repo = binding_repository();
    repo.release_relations = vec![override_entry(
        ">=1.0.0,<=2.0.0",
        vec![requires("ranged.guid", ">=1.0.0")],
    )];
    assert_eq!(
        super::effective_relations(&repo, Some("1.0.0")),
        vec![requires("ranged.guid", ">=1.0.0")]
    );
    assert_eq!(
        super::effective_relations(&repo, Some("2.0.0")),
        vec![requires("ranged.guid", ">=1.0.0")]
    );
    // 越界即不命中，退回仓库级（此处为空）
    assert!(super::effective_relations(&repo, Some("2.0.1")).is_empty());
}

#[test]
fn effective_relations_honours_open_ended_intervals() {
    let mut repo = binding_repository();
    repo.relations = vec![requires("base.guid", "")];
    repo.release_relations = vec![
        override_entry(">=2.0.0", vec![requires("modern.guid", ">=4.0.0")]),
        override_entry("<2.0.0", vec![requires("legacy.guid", ">=1.0.0")]),
    ];
    assert_eq!(
        super::effective_relations(&repo, Some("2.0.0")),
        vec![requires("modern.guid", ">=4.0.0")]
    );
    assert_eq!(
        super::effective_relations(&repo, Some("1.9.9")),
        vec![requires("legacy.guid", ">=1.0.0")]
    );
    // 两边看起来都命中的版本里，下限更高的那张赢
    repo.release_relations = vec![
        override_entry(">=1.0.0", vec![requires("wide.guid", "")]),
        override_entry(">=1.5.0,<1.6.0", vec![requires("narrow.guid", "")]),
    ];
    assert_eq!(
        super::effective_relations(&repo, Some("1.5.2")),
        vec![requires("narrow.guid", "")]
    );
    // 不可解析的 tag 不命中，退回仓库级声明
    assert_eq!(
        super::effective_relations(&repo, Some("nightly")),
        repo.relations
    );
}

#[test]
fn effective_map_relations_uses_the_map_override_table() {
    let map = map_entry(
        r#"
[[maps.release_relations]]
version = ">=2.0.0"
relations = [{ kind = "requires", guid = "new.guid", version = ">=0.20.0" }]

[[maps.release_relations]]
version = ">=1.0.0,<2.0.0"
relations = [{ kind = "requires", guid = "old.guid", version = ">=0.15.0" }]
"#,
    );
    assert_eq!(
        super::effective_map_relations(&map, Some("2.1.0")),
        vec![requires("new.guid", ">=0.20.0")]
    );
    assert_eq!(
        super::effective_map_relations(&map, Some("1.4.0")),
        vec![requires("old.guid", ">=0.15.0")]
    );
    // 没有覆盖版本 / 没有命中：退回地图自己的声明
    assert_eq!(
        super::effective_map_relations(&map, Some("0.9.0")),
        vec![requires("base.guid", ">=0.10.0")]
    );
    assert_eq!(
        super::effective_map_relations(&map, None),
        vec![requires("base.guid", ">=0.10.0")]
    );
}
