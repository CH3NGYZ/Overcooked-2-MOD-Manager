use serde::{Deserialize, Serialize};

/// MOD 与另一个插件之间的关系种类。
///
/// 依赖、排斥、兼容三件事本质上是同一个问题 ——「对某个 GUID 的版本区间
/// 有什么要求」，只是满足与违反时的含义不同：
///
/// - `requires`   目标**必须存在**且版本落在区间内。缺失或版本不符 → 缺依赖。
/// - `conflicts`  目标**若存在**且版本落在区间内 → 冲突，需要卸载。
/// - `compatible` 目标**若存在**，版本必须落在区间内 → 否则不兼容，需要卸载。
///
/// 三者共用 `version` 版本区间（pip 风格 `">=1.8.0,<2.0.0"`）：不写或写 `*`
/// 就是不设限制。
/// 放在一张表里而不是三张表，是为了让作者按「跟谁有关系」组织配置，
/// 而不是按「哪种关系」拆成三处；前端也只需要一条判定管线。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    #[default]
    Requires,
    Conflicts,
    Compatible,
}

impl RelationKind {
    /// 从配置里的 `kind` 文本解析。未知值返回 `None`，由调用方决定跳过。
    pub fn from_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "requires" | "require" | "dependency" => Some(Self::Requires),
            "conflicts" | "conflict" | "incompatible_with" => Some(Self::Conflicts),
            "compatible" | "compatibility" => Some(Self::Compatible),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requires => "requires",
            Self::Conflicts => "conflicts",
            Self::Compatible => "compatible",
        }
    }
}

/// 仓库（MOD）与另一个插件的一条关系声明。
///
/// 用 GUID 表达，与本地 MOD 绑定用的 `plugin_guid` 是同一套标识 ——
/// 「哪个插件」这件事只有 GUID 是可靠的，插件名和文件名都会随作者改写。
///
/// 版本边界用本项目规则比较（四段 = beta 序号，见
/// `releases::compare_release_versions`），所以 `1.8.7.2` 与 `1.8.7-beta.2` 等价。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryRelation {
    pub kind: RelationKind,
    /// 目标插件的 GUID（比较时忽略大小写）。
    pub guid: String,
    /// 版本区间；空字符串表示不限版本。
    ///
    /// 语法见 `version_range`：逗号分隔的多个条件是 AND，支持
    /// `>=` / `>` / `<=` / `<` / `==`（`=` 同义）/ `!=` / `*`。
    /// 旧写法 `min_version` / `max_version` 仍能反序列化（由
    /// `deserialize_relations` 折成这个字段），序列化只写 `version`。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// 只对**预发布版本**成立（三态）。
    ///
    /// - 不写（`None`）：正式版与预发布版都算
    /// - `true`：只匹配预发布版（本项目的四段写法 `1.9.8.1` 即 `1.9.8-beta.1`）
    /// - `false`：只匹配正式版
    ///
    /// 为什么需要它：正式版与测试版常是**同一个 GUID** 的两条发布线
    /// （同一仓库的 stable / beta 渠道各一个仓库条目）。要表达「正式版与
    /// 测试版互斥」时，版本区间无能为力 —— 预发布版按 semver 排在**同号
    /// 正式版之下**，`[1.9.8, 1.9.9)` 会命中 `1.9.8` 却漏掉 `1.9.8.1`。
    /// 用这个标志直接按「是不是预发布」筛，两边各写一条即可。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<bool>,
    /// 仅对 `requires` 有意义：缺失时不提示、不显示按钮。
    ///
    /// 其它种类写 `optional = true` 属于配置错误 —— 「可选的冲突」没有含义。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

/// 反序列化关系表：**未知的 `kind` 跳过而不是报错**。
///
/// 这是仓库源的前向兼容边界。新版本客户端可能写入旧版本不认识的关系种类
/// （例如以后的 `loads_after`），旧客户端若直接报错，整份源文档就不可读，
/// 用户会看到「所有仓库都失败」。跳过 + 记一条 warn 更合适：
/// 旧客户端只是不认识这条关系，其余配置仍然可用。
///
/// 缺少 `kind` 按 `requires` 处理：这是最可能的意图，也让历史写法
/// （`[[repositories.requires]]` 只写 guid 的表）继续可读。
pub(super) fn deserialize_relations<'de, D>(
    deserializer: D,
) -> Result<Vec<RepositoryRelation>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct RelationWire {
        #[serde(default)]
        kind: String,
        #[serde(default)]
        guid: String,
        #[serde(default)]
        version: String,
        /// 旧写法，仍接受：与 `max_version` 一起折成 `version`。
        #[serde(default)]
        min_version: String,
        #[serde(default)]
        max_version: String,
        #[serde(default)]
        prerelease: Option<bool>,
        #[serde(default)]
        optional: bool,
        /*
         * 捕获「把仓库字段写到关系表里」这类错误。
         *
         * TOML 里 `[[repositories.relations]]` 之后的任何键都属于**关系条目**，
         * 不再属于仓库。作者很容易在关系块后面接着写 `install_target`、
         * `note_i18n` —— serde 默认忽略未知字段，于是这些字段凭空消失，
         * 最终报出的却是「missing field `install_target`」，作者会以为
         * 自己根本忘了写。这里显式收集，给出能对得上症状的提示。
         */
        #[serde(flatten)]
        extra: std::collections::BTreeMap<String, toml::Value>,
    }

    let raw = Vec::<RelationWire>::deserialize(deserializer)?;
    let mut relations = Vec::with_capacity(raw.len());
    for entry in raw {
        /*
         * 关系条目里出现仓库字段 → 说明作者把仓库字段写在关系块之后了。
         * 这个错误本身的后果是「仓库字段神秘消失」，直接报错点名更好排查。
         */
        if let Some(misplaced) = entry
            .extra
            .keys()
            .find(|key| REPOSITORY_FIELD_NAMES.contains(&key.as_str()))
        {
            return Err(serde::de::Error::custom(format!(
                "关系表里出现了仓库字段 `{misplaced}`：`[[repositories.relations]]` 之后的键都属于关系条目，\
                 仓库字段必须写在这些关系之前。"
            )));
        }
        let kind = if entry.kind.trim().is_empty() {
            RelationKind::Requires
        } else {
            let Some(kind) = RelationKind::from_token(&entry.kind) else {
                log::warn!(
                    "忽略不认识的关系类型：kind={}，guid={}（可能是更新版本的客户端写入）",
                    entry.kind.trim(),
                    entry.guid.trim()
                );
                continue;
            };
            kind
        };
        let version = relation_version(
            entry.version.trim(),
            entry.min_version.trim(),
            entry.max_version.trim(),
            entry.guid.trim(),
        );
        relations.push(RepositoryRelation {
            kind,
            guid: entry.guid,
            version,
            prerelease: entry.prerelease,
            optional: entry.optional,
        });
    }
    Ok(relations)
}

/// 关系的版本区间：新字段 `version` 优先，旧 `min_version`/`max_version` 折成它。
///
/// 两者同时出现按新字段处理并记一条 warn —— 那是迁移期手写了两套写法，
/// 猜「哪个更想要」不如用明确的优先级；旧字段迟早会从仓库源里消失。
fn relation_version(version: &str, min_version: &str, max_version: &str, guid: &str) -> String {
    let legacy = crate::version_range::fold_bounds(min_version, max_version);
    if version.is_empty() {
        return legacy;
    }
    if !legacy.is_empty() {
        log::warn!(
            "关系同时写了 version 与 min_version/max_version，按 version 处理：guid={guid}，version={version}，legacy={legacy}"
        );
    }
    version.to_string()
}

/// 仓库级字段名：出现在关系条目里几乎一定是位置写错了。
const REPOSITORY_FIELD_NAMES: &[&str] = &[
    "name",
    "name_i18n",
    "group",
    "group_i18n",
    "note",
    "note_i18n",
    "provider",
    "username",
    "repository",
    "download_mode",
    "branch",
    "file_path",
    "region",
    "platform",
    "channel",
    "asset_rules",
    "install_target",
    "plugin_guid",
    "local_product_name",
    "local_original_filename",
    "version_regex",
    "max_release_count",
    "download_threads",
    "network_plan",
    "type",
    "enabled",
];

/// 按 Release 版本区间覆盖默认关系表。
///
/// 场景：同一个 MOD 的不同大版本依赖不同版本的基础库 ——
/// `A 1.0` 要 `B ≥ 2.0`，`A 1.1` 要 `B ≥ 3.0`。仓库级的 `relations`
/// 无法表达这种差异（它是固定值），所以额外提供这张表：
///
/// ```toml
/// [[repositories.release_relations]]
/// version = ">=1.8.0,<2.0.0"   # 只对这段版本生效
/// relations = [{ kind = "requires", guid = "...", version = ">=19.0" }]
/// ```
///
/// 匹配规则：所有**区间命中**的条目里，取区间下限最高的那条，
/// 其 `relations` 整体替换仓库级 `relations`（不是合并 —— 合并会让
/// 「新版降低要求」无法表达）。下限相同取先出现的一条。
///
/// `version` 留空（或不写）是错误配置：那样它会匹配任何版本，等同于直接改
/// `relations`，所以在校验阶段就拒绝，避免「以为写了区间其实没限」。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReleaseRelationOverride {
    /// 这张覆盖表生效的 Release 版本区间；空表示不限（校验阶段拒绝）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// 命中的区间内使用的关系表（整体替换，不合并）。
    #[serde(default, alias = "requires")]
    pub relations: Vec<RepositoryRelation>,
}

impl<'de> Deserialize<'de> for ReleaseRelationOverride {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            version: String,
            /// 旧写法，仍接受：与 `max_version` 一起折成 `version`。
            #[serde(default)]
            min_version: String,
            #[serde(default)]
            max_version: String,
            #[serde(
                default,
                alias = "requires",
                deserialize_with = "deserialize_relations"
            )]
            relations: Vec<RepositoryRelation>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            version: relation_version(
                wire.version.trim(),
                wire.min_version.trim(),
                wire.max_version.trim(),
                "<release_relations>",
            ),
            relations: wire.relations,
        })
    }
}

/// `sync_dependency_snapshots` 的结果：本次真正改写了多少份清单。
#[derive(Debug, Clone, Serialize)]
pub struct DependencySyncResult {
    pub mods_updated: usize,
    pub maps_updated: usize,
}
