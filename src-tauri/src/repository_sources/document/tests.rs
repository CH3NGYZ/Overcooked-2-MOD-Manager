use super::super::embedded::default_index_url;
use super::*;

#[test]
fn official_channels_resolve_children_in_the_selected_remote_repository() {
    let text = "[[repository_sources]]\nid = \"mods\"\nnickname = \"MODs\"\nurl = \"mods/mods-gua.toml\"\n";
    for (region, expected) in [
        (
            crate::config::REGION_CHINESE_MAINLAND,
            "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/mods/mods-gua.toml",
        ),
        (
            crate::config::REGION_INTERNATIONAL,
            "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/mods/mods-gua.toml",
        ),
    ] {
        let location = SourceLocation::parse(default_index_url(region)).unwrap();
        let document = parse_document(text, &location).unwrap();
        assert_eq!(document.repository_sources[0].url, expected);
    }
}

#[test]
fn source_declarations_keep_order_and_resolve_relative_urls() {
    let text = r#"
[[repository_sources]]
id = "second"
nickname = "Second"
url = "second.toml"
builtin = "false"
[[repository_sources]]
id = "first"
nickname = "First"
url = "first.toml"
builtin = true
"#;
    let location = SourceLocation::parse("https://example.com/sources/index.toml").unwrap();
    let document = parse_document(text, &location).unwrap();
    assert_eq!(document.repository_sources[0].id, "second");
    assert_eq!(
        document.repository_sources[0].url,
        "https://example.com/sources/second.toml"
    );
    assert_eq!(document.repository_sources[0].builtin, Some(false));
    assert_eq!(document.repository_sources[1].id, "first");
    assert_eq!(document.repository_sources[1].builtin, Some(true));
    assert!(parse_document(&text.replace("\"false\"", "\"invalid\""), &location).is_err());
    assert!(parse_document("[session-id] Repository or file not found", &location).is_err());
}

#[test]
fn repositories_require_chinese_translations_and_ignore_raw_metadata() {
    let location = SourceLocation::parse("index.toml").unwrap();
    let document_for = |metadata: &str| {
        format!(
            "[[repositories]]\n{metadata}\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{ type = \"exact\", value = \"mod.dll\" }}]\n"
        )
    };
    let valid = document_for(
        "name = \"Raw name\"\nname_i18n.zh-CN = \"中文名称\"\ngroup = \"Raw group\"\ngroup_i18n.zh-CN = \"中文分组\"\nnote = \"Raw note\"\nnote_i18n.zh-CN = \"中文备注\"",
    );
    let parsed = parse_document(&valid, &location).unwrap();
    let repo = &parsed.repositories[0];
    assert_eq!(repo.name, "中文名称");
    assert_eq!(repo.group, "中文分组");
    assert_eq!(repo.note, "中文备注");

    for (metadata, expected_error) in [
        (
            "name_i18n.zh-CN = \"名称\"\nnote_i18n.zh-CN = \"\"",
            "config.missingGroupI18n",
        ),
        (
            "name_i18n.zh-CN = \"名称\"\ngroup_i18n.zh-CN = \"\"",
            "config.missingNoteI18n",
        ),
        (
            "name_i18n.zh-CN = \"\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"",
            "config.emptyNameI18n",
        ),
        (
            "name = \"Raw only\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"",
            "config.missingNameI18n",
        ),
    ] {
        let error = parse_document(&document_for(metadata), &location).unwrap_err();
        assert!(error.to_string().contains(expected_error), "{error}");
    }
}

/*
 * 昵称与 name / group / note 采用同一套多语言规则：
 *   · 只写多语言表（含 zh-CN）→ 直接用
 *   · 只写旧的单语 nickname → 折叠进 zh-CN（迁移兼容）
 *   · 两者都没有 → 报错，不产生空白昵称
 *
 * 旧实现允许「单语 + 空翻译表」，那是 `nickname_i18n.is_empty()`
 * 那条断言留下的行为；现在表里必须始终有 zh-CN。
 */
#[test]
fn source_nickname_uses_i18n_table_with_zh_cn_fallback() {
    let location = SourceLocation::parse("index.toml").unwrap();

    // 只有旧的单语字段：折叠进 zh-CN，镜像值同步
    let legacy = r#"
[[repository_sources]]
id = "mods"
nickname = "Fallback name"
url = "mods.toml"
"#;
    let document = parse_document(legacy, &location).unwrap();
    let source = &document.repository_sources[0];
    assert_eq!(source.nickname, "Fallback name");
    assert_eq!(source.nickname_i18n["zh-CN"], "Fallback name");

    // 多语言表齐全：镜像值取 zh-CN，不取单语字段
    let translated = r#"
[[repository_sources]]
id = "mods"
nickname = "Ignored fallback"
url = "mods.toml"
[repository_sources.nickname_i18n]
zh-CN = "中文昵称"
zh-TW = "繁體暱稱"
en = "Translated name"
ko = "번역 이름"
"#;
    let document = parse_document(translated, &location).unwrap();
    let source = &document.repository_sources[0];
    assert_eq!(source.nickname, "中文昵称", "镜像值应取 zh-CN");
    assert_eq!(source.nickname_i18n["en"], "Translated name");
    assert_eq!(source.nickname_i18n["ko"], "번역 이름");

    // 完全没有昵称 → 拒绝
    let missing = r#"
[[repository_sources]]
id = "mods"
url = "mods.toml"
"#;
    let error = parse_document(missing, &location).unwrap_err();
    assert!(
        error.to_string().contains("source.idOrNicknameEmpty"),
        "{error}"
    );

    // 翻译表语言值类型错误仍然报错
    assert!(
        parse_document(
            &translated.replace("en = \"Translated name\"", "en = 1"),
            &location
        )
        .is_err()
    );
}

/// 空白翻译项被丢弃，不会留下一个「看起来有、其实空」的语言键。
#[test]
fn blank_nickname_translations_are_dropped() {
    let location = SourceLocation::parse("index.toml").unwrap();
    let text = r#"
[[repository_sources]]
id = "mods"
url = "mods.toml"
[repository_sources.nickname_i18n]
zh-CN = "中文昵称"
en = "   "
"#;
    let document = parse_document(text, &location).unwrap();
    assert_eq!(document.repository_sources[0].nickname_i18n.len(), 1);
    assert_eq!(document.repository_sources[0].nickname, "中文昵称");
}

#[test]
fn same_source_url_can_have_distinct_regional_names() {
    let text = r#"
[[repository_sources]]
id = "china"
nickname = "China"
url = "mods.toml"
region = "chinese_mainland"
[repository_sources.nickname_i18n]
en = "China Mods"
[[repository_sources]]
id = "international"
nickname = "International"
url = "mods.toml"
region = "international"
[repository_sources.nickname_i18n]
en = "International Mods"
[[repository_sources]]
id = "china-duplicate"
nickname = "Duplicate"
url = "mods.toml"
region = "mainland-china"
"#;
    let location = SourceLocation::parse("https://example.com/index.toml").unwrap();
    let document = parse_document(text, &location).unwrap();
    assert_eq!(document.repository_sources.len(), 2);
    assert_eq!(document.repository_sources[0].id, "china");
    assert_eq!(document.repository_sources[1].id, "international");
    assert_eq!(
        document.repository_sources[1].nickname_i18n["en"],
        "International Mods"
    );
}
