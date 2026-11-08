use super::*;

/// 测试用的排序器：直接按数值段比较，够验证区间语义（真实排序由调用方传入）。
fn numeric(left: &str, right: &str) -> Option<Ordering> {
    fn parse(value: &str) -> Option<Vec<u64>> {
        value
            .trim()
            .trim_start_matches(['v', 'V'])
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect()
    }
    let (left, right) = (parse(left)?, parse(right)?);
    let length = left.len().max(right.len());
    for index in 0..length {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        if a != b {
            return Some(a.cmp(&b));
        }
    }
    Some(Ordering::Equal)
}

#[test]
fn empty_and_star_mean_any_version() {
    for spec in ["", "  ", "*", " * "] {
        let range = VersionRange::parse(spec).expect("any");
        assert!(range.is_any(), "{spec:?} should be any");
        assert!(range.matches("", numeric));
        assert!(range.matches("9.9.9", numeric));
    }
}

#[test]
fn upper_and_lower_bounds_are_anded() {
    let range = VersionRange::parse(">=3.10.2,<3.12.0").expect("range");
    assert!(!range.matches("3.10.1", numeric));
    assert!(range.matches("3.10.2", numeric));
    assert!(range.matches("3.11.9", numeric));
    assert!(!range.matches("3.12.0", numeric));
}

#[test]
fn single_bounds_and_equality() {
    assert!(
        VersionRange::parse(">=2.0")
            .expect("ge")
            .matches("2.1", numeric)
    );
    assert!(
        !VersionRange::parse(">=2.0")
            .expect("ge")
            .matches("1.9", numeric)
    );
    assert!(
        VersionRange::parse("<2.0")
            .expect("lt")
            .matches("1.9", numeric)
    );
    assert!(
        VersionRange::parse("=1.2.3")
            .expect("eq")
            .matches("1.2.3", numeric)
    );
    assert!(
        VersionRange::parse("==1.2.3")
            .expect("eq")
            .matches("v1.2.3", numeric)
    );
    assert!(
        !VersionRange::parse("==1.2.3")
            .expect("eq")
            .matches("1.2.4", numeric)
    );
    assert!(
        VersionRange::parse("!=1.2.3")
            .expect("ne")
            .matches("1.2.4", numeric)
    );
    assert!(
        !VersionRange::parse("!=1.2.3")
            .expect("ne")
            .matches("1.2.3", numeric)
    );
}

#[test]
fn open_bounds_are_supported() {
    assert!(
        VersionRange::parse(">2.0")
            .expect("gt")
            .matches("2.0.1", numeric)
    );
    assert!(
        !VersionRange::parse(">2.0")
            .expect("gt")
            .matches("2.0", numeric)
    );
}

#[test]
fn unknown_or_empty_version_never_matches_a_bounded_spec() {
    let range = VersionRange::parse(">=1.0").expect("ge");
    assert!(!range.matches("", numeric));
    assert!(!range.matches("not-a-version", numeric));
    let eq = VersionRange::parse("==1.2.3").expect("eq");
    assert!(!eq.matches("", numeric));
}

#[test]
fn broken_specs_are_rejected() {
    for spec in [
        ">=1,,<2", ">=", "~=1.2", "abc", ">=beta", "> = 1.0", "1.0", "1.0,<2.0",
    ] {
        assert!(
            VersionRange::parse(spec).is_err(),
            "{spec:?} should be rejected"
        );
    }
}

#[test]
fn spaces_around_operator_and_version_are_tolerated() {
    let range = VersionRange::parse(">= 1.0 , < 2.0").expect("range");
    assert_eq!(range.canonical(), ">=1.0,<2.0");
}

#[test]
fn canonical_rewrites_single_equals() {
    let range = VersionRange::parse(" =1.2 , !=1.3 ").expect("clauses");
    assert_eq!(range.canonical(), "==1.2,!=1.3");
    assert_eq!(VersionRange::default().canonical(), "");
}

#[test]
fn fold_bounds_matches_the_old_two_sided_form() {
    assert_eq!(fold_bounds("", ""), "");
    assert_eq!(fold_bounds("1.0", ""), ">=1.0");
    assert_eq!(fold_bounds("", "2.0"), "<=2.0");
    assert_eq!(fold_bounds("1.0", "1.0"), "==1.0");
    assert_eq!(fold_bounds("1.0", "2.0"), ">=1.0,<=2.0");
}
