use super::*;

#[test]
fn country_code_maps_only_cn_to_the_mainland_channel() {
    assert_eq!(country_region("CN"), Some(REGION_CHINESE_MAINLAND));
    assert_eq!(country_region("cn"), Some(REGION_CHINESE_MAINLAND));
    assert_eq!(country_region(" CN "), Some(REGION_CHINESE_MAINLAND));
    assert_eq!(country_region("TW"), Some(REGION_INTERNATIONAL));
    assert_eq!(country_region("US"), Some(REGION_INTERNATIONAL));
    for invalid in ["", "C", "CNN", "C1", "中国"] {
        assert_eq!(country_region(invalid), None, "{invalid}");
    }
}
