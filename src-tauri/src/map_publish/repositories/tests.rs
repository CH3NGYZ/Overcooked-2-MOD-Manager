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
