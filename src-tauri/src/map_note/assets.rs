use crate::models::AssetRule;
use crate::models::AssetRuleType;

/// 资产名里的版本号占位符（`asset = "daxue_v${version}.zip"`）。
pub const VERSION_PLACEHOLDER: &str = "${version}";

/// 版本号的形状：数字段，点分隔。与 `oc2-info` 的区间语法共用。
///
/// 不做「必须两段」之类的限制：`0.73`、`1.0.3`、`2` 都是作者真实用过的写法。
const VERSION_PATTERN: &str = r"[0-9]+(?:\.[0-9]+)*";

/// 由 note 的 asset 模板生成整串锚定的版本规则；未写模板时使用
/// `${tag}_v${version}.zip`，不从已有附件名猜测。
///
/// 显式模板缺少 `${version}` 时返回 None：版本是挑选安装资产的标识，
/// 不能默默换成另一个模板（见 map_sources/assets.rs 的 synthesize_releases）。
pub fn versioned_asset_rule(template: Option<&str>, tag: &str) -> Option<AssetRule> {
    let default_template = format!("{tag}_v{VERSION_PLACEHOLDER}.zip");
    let template = template
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&default_template);
    {
        if template.contains(VERSION_PLACEHOLDER) {
            return Some(AssetRule {
                rule_type: AssetRuleType::Regex,
                value: template_rule(template),
            });
        }
        log::warn!(
            "oc2-map 的 asset 模板不含 {}，无法提取版本号：{}",
            VERSION_PLACEHOLDER,
            template
        );
        return None;
    }
}

/// `daxue_v${version}.zip` → `^daxue_v(?P<version>[0-9]+(?:\.[0-9]+)*)\.zip$`。
fn template_rule(template: &str) -> String {
    let mut pattern = String::from("^");
    let mut rest = template;
    while let Some(index) = rest.find(VERSION_PLACEHOLDER) {
        pattern.push_str(&regex::escape(&rest[..index]));
        pattern.push_str("(?P<version>");
        pattern.push_str(VERSION_PATTERN);
        pattern.push(')');
        rest = &rest[index + VERSION_PLACEHOLDER.len()..];
    }
    pattern.push_str(&regex::escape(rest));
    pattern.push('$');
    pattern
}

#[cfg(test)]
mod tests;
