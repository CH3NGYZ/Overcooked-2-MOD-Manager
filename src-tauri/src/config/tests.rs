const REGION_REPOSITORY: &str = r#"
[[repositories]]
name = "Regional MOD"
name_i18n.zh-CN = "Regional MOD"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "example"
repository = "mod"
channel = "stable"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.regional-mod"
enabled = true
[[repositories.asset_rules]]
type = "exact"
value = "RegionalMod.dll"
"#;

mod availability;
mod bundle_export;
mod bundle_import;
mod defaults;
mod identity;
mod metadata;
mod public;
mod sources;
mod validation;
