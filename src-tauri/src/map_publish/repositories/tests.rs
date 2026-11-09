use super::*;

#[test]
fn gitcode_null_topics_are_ignored_when_listing_publish_repositories() {
    let repositories: Vec<ApiRepository> = serde_json::from_value(serde_json::json!([
        {"name": "maps", "full_name": "author/maps", "private": false,
         "default_branch": "main", "topics": null,
         "permission": {"push": true, "pull": true, "admin": true}}
    ]))
    .unwrap();
    let result = writable_repositories(ApiInstallationRepositories { repositories });
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].owner, "author");
    assert!(result[0].push);
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
