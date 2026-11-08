use super::*;

#[test]
fn missing_topics_only_lists_the_ones_absent() {
    assert_eq!(
        missing_topics(&["overcooked-2".to_string(), "bepinex".to_string()]),
        vec!["map".to_string()]
    );
    assert!(missing_topics(&publish_topics()).is_empty());
    assert!(
        missing_topics(&[
            "Overcooked-2".to_string(),
            "bepinex".to_string(),
            "map".to_string(),
            "extra".to_string(),
        ])
        .is_empty()
    );
}

#[test]
fn gitcode_permissions_accept_numeric_flags_without_inventing_write_access() {
    let list: ApiInstallationRepositories =
        serde_json::from_value(serde_json::json!({"repositories":[
        {"name":"yes","full_name":"u/yes","private":1,"permission":{"push":1}},
        {"name":"no","full_name":"u/no","private":0,"permission":{"push":0}},
        {"name":"unknown","full_name":"u/unknown"}]}))
        .unwrap();
    let result = writable_repositories(list);
    assert_eq!(result.len(), 2);
    assert!(result[0].push);
    assert!(result[0].private);
    assert!(!result[1].push);
}
