use super::super::test_support::body;
use super::*;

#[test]
fn a_release_without_the_block_is_not_a_map() {
    assert!(parse_release_note("只是普通的更新说明").unwrap().is_none());
}

#[test]
fn the_card_note_comes_from_the_block_not_from_the_body() {
    let text = body(
        r#"level_set_name = "GAODA"
author = "GAODA"
note_i18n.zh-CN = "共 2 张关卡。"
note_i18n.en = "Two levels."
"#,
        "## 玩法\n\n需要 DIYLevel。",
    );
    let block = parse_release_note(&text).unwrap().unwrap();
    assert_eq!(block.metadata.level_set_name, "GAODA");
    // 卡片短语来自块里的 note_i18n（五语），块外正文不属于卡片
    assert_eq!(
        block.metadata.note_i18n.get("zh-CN").map(String::as_str),
        Some("共 2 张关卡。")
    );
    assert_eq!(
        block.metadata.note_i18n.get("en").map(String::as_str),
        Some("Two levels.")
    );
    // 块外正文是「这个版本的更新说明」，清洗后交给版本详情显示
    assert_eq!(strip_note_block(&text), "## 玩法\n\n需要 DIYLevel。");
    // 解析结果也带一份同样的正文，作者没写 note_i18n 时卡片拿它当说明
    assert_eq!(block.body, "## 玩法\n\n需要 DIYLevel。");
}

#[test]
fn the_metadata_block_never_reaches_the_visible_note() {
    let text = body("level_set_name = \"A\"\n", "共 2 张关卡。");
    let note = strip_note_block(&text);
    assert_eq!(note, "共 2 张关卡。");
    assert!(!note.contains("oc2-map"), "{note}");
    // 没有块时只去首尾空白，正文不动
    assert_eq!(strip_note_block("  普通说明  "), "普通说明");
}

#[test]
fn dotted_i18n_keys_are_read_as_tables() {
    let block = parse_release_note(&body(
        r#"level_set_name = "nono"
name_i18n.zh-CN = "小no"
name_i18n.en = "nono"
"#,
        "",
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        block.metadata.name_i18n.get("zh-CN").map(String::as_str),
        Some("小no")
    );
    assert_eq!(
        block.metadata.name_i18n.get("en").map(String::as_str),
        Some("nono")
    );
}

#[test]
fn a_block_without_a_level_set_name_is_rejected() {
    let error = parse_release_note(&body("author = \"A\"\n", "")).unwrap_err();
    assert!(error.contains("level_set_name"), "{error}");
}
