use super::*;

#[test]
fn changed_paths_select_only_the_affected_set() {
    let root = PathBuf::from("game/levels");
    assert_eq!(
        changed_sets(
            std::slice::from_ref(&root),
            &[root.join("pear/info"), root.join("pear/scene")]
        ),
        Some(vec!["pear".into()])
    );
    assert_eq!(
        changed_sets(std::slice::from_ref(&root), &[root.clone()]),
        None
    );
}
