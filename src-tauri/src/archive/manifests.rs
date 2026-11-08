use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

/*
 * `from = "ManifestWire"`：读旧清单时的兼容桥。
 *
 * 旧清单存的是单语 `package_name` / `group` / `note`。新结构只有 `*_i18n`，
 * 直接反序列化会把这些值丢掉 —— 用户在 MOD 管理页看到的包名和分组会空掉。
 * 中间结构 `ManifestWire` 同时接受两套字段，`InstallManifest::from` 里
 * 把单语值折成 `zh-CN` 键（正是仓库源规范里 zh-CN 必填的那一条）。
 *
 * 只影响**反序列化**：写出时始终是新结构，所以旧清单在下次写入时自动
 * 收敛成新格式，不需要单独的迁移脚本。
 */
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "ManifestWire")]
pub struct InstallManifest {
    pub repository_id: String,
    pub release_tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_binding: Option<crate::models::ModRepositoryBinding>,
    /*
     * 以下三个字段只有多语言形态，没有单语版本。
     *
     * 仓库源的规范形式本来就是 `name_i18n` / `group_i18n` / `note_i18n`
     * （`config::normalize_repository_i18n` 强制要求 zh-CN 存在，社区源
     * 也是把抓到的名字同时写进 `name` 与 `name_i18n.zh-CN`）。
     * 清单再存一份单语副本属于冗余双写，两者迟早不一致，
     * 读的时候还得判断「该信哪份」。
     *
     * 旧清单（只有 `package_name` / `group` / `note`）由
     * `legacy_names` 在反序列化时接住并折进 i18n 字段，见本结构体的
     * `#[serde(from = ...)]`。
     */
    /// 安装时仓库名的多语言快照。
    ///
    /// 安装之后仓库可能已从源文档下线，届时无处可查。存下来才能在
    /// MOD 管理页和回收站里按当前语言渲染，切语言后连历史记录也一起变。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub package_name_i18n: BTreeMap<String, String>,
    /// 安装时仓库的分组快照（多语言）。
    ///
    /// MOD 管理页按「安装时绑定的仓库分类」分组，而分组数据来自仓库源文档。
    /// 刚启动、源文档还没刷新出来时仓库配置可能为空或仍是旧内容，分组就会错乱。
    /// 把分组固化进安装清单，扫描时可直接使用稳定值，
    /// 只在仓库分组真正变化时（后续刷新到不同配置）才更新。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_i18n: BTreeMap<String, String>,
    /// 安装时仓库备注的快照（多语言）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub note_i18n: BTreeMap<String, String>,
    /// 安装的 Release 是否为预发布版本。
    ///
    /// 跟着版本号一起固化：预发布标记只存在于当时的 Release 元数据里，
    /// 之后仓库刷新可能已把该版本移出列表，必须在安装时记下来，
    /// 否则 MOD 管理页没法在「最新版」旁边标出「预发布」。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prerelease: bool,
    /// 安装时仓库声明的关系快照（依赖/排斥/兼容）。
    ///
    /// 关系只存在于仓库源文档里，源下线或被删之后本地无处可查；
    /// 安装那一刻固化一份，刷新仓库后由 `sync_dependency_snapshots` 按线上声明更新。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<crate::models::RepositoryRelation>,
    pub installed_at: String,
    pub files: Vec<String>,
}

/// 清单的反序列化中间层：同时接受「只有 i18n」和「旧单语字段」两种格式。
///
/// 为什么不给 `InstallManifest` 直接挂 `#[serde(default)]` 单语字段：
/// 那样字段会一直留在结构体里，所有构造点都必须写 `package_name: None`，
/// 而且代码里随时可能有人再读那份单语值，重新引入「两份数据信哪份」的问题。
/// 用中间层把兼容逻辑收敛到 `From` 实现里，主结构保持只有一种表示。
#[derive(Deserialize)]
struct ManifestWire {
    repository_id: String,
    release_tag: String,
    #[serde(default)]
    repository_binding: Option<crate::models::ModRepositoryBinding>,
    #[serde(default)]
    package_name: Option<String>,
    #[serde(default)]
    package_name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    group: String,
    #[serde(default)]
    group_i18n: BTreeMap<String, String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    note_i18n: BTreeMap<String, String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    relations: Vec<crate::models::RepositoryRelation>,
    installed_at: String,
    files: Vec<String>,
}

/// 把单语值折进 i18n 映射的 `zh-CN` 键（已有该键时不覆盖）。
///
/// 空值与纯空白视为「没有这个信息」，不写入 —— 否则会给 `zh-CN`
/// 塞一个空串，让下游以为「有中文翻译但是空的」。
fn fold_legacy(map: &mut BTreeMap<String, String>, legacy: Option<&str>) {
    let Some(value) = legacy.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    map.entry("zh-CN".to_string())
        .or_insert_with(|| value.to_string());
}

impl From<ManifestWire> for InstallManifest {
    fn from(wire: ManifestWire) -> Self {
        let mut package_name_i18n = wire.package_name_i18n;
        fold_legacy(&mut package_name_i18n, wire.package_name.as_deref());
        let mut group_i18n = wire.group_i18n;
        fold_legacy(&mut group_i18n, Some(&wire.group));
        let mut note_i18n = wire.note_i18n;
        fold_legacy(&mut note_i18n, wire.note.as_deref());
        Self {
            repository_id: wire.repository_id,
            release_tag: wire.release_tag,
            repository_binding: wire.repository_binding,
            package_name_i18n,
            group_i18n,
            note_i18n,
            prerelease: wire.prerelease,
            relations: wire.relations,
            installed_at: wire.installed_at,
            files: wire.files,
        }
    }
}

pub fn write_manifest(manifest_dir: &Path, manifest: &InstallManifest) -> Result<(), String> {
    log::debug!(
        "写入 MOD 安装清单：仓库={}，文件数={}",
        manifest.repository_id,
        manifest.files.len()
    );
    fs::create_dir_all(manifest_dir).map_err(|e| e.to_string())?;
    let path = manifest_dir.join(format!("{}.toml", manifest.repository_id));
    let text = toml::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let temp = manifest_dir.join(format!("{}.{}.tmp", manifest.repository_id, Uuid::new_v4()));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    let result = fs::rename(temp, path).map_err(|e| e.to_string());
    match &result {
        Ok(()) => log::info!("MOD 安装清单写入完成：仓库={}", manifest.repository_id),
        Err(error) => log::error!(
            "MOD 安装清单写入失败：仓库={}，原因={}",
            manifest.repository_id,
            error
        ),
    }
    result
}

pub fn read_manifest(manifest_dir: &Path, repository_id: &str) -> Option<InstallManifest> {
    let path = manifest_dir.join(format!("{}.toml", repository_id));
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            log::trace!(
                "读取 MOD 安装清单失败或不存在：仓库={}，原因={}",
                repository_id,
                error
            );
            return None;
        }
    };
    match toml::from_str(&text) {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            log::warn!(
                "解析 MOD 安装清单失败：仓库={}，原因={}",
                repository_id,
                error
            );
            None
        }
    }
}

pub fn read_manifests(manifest_dir: &Path) -> Vec<InstallManifest> {
    let manifests = fs::read_dir(manifest_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        })
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| toml::from_str::<InstallManifest>(&text).ok())
        .collect::<Vec<_>>();
    log::debug!(
        "读取 MOD 安装清单完成：目录={}，数量={}",
        manifest_dir.display(),
        manifests.len()
    );
    manifests
}

/// 安装包显示名的统一格式：`{仓库名} - {平台}`。
///
/// 抽成函数是因为「安装时写清单」和「扫描时补快照」两处都要生成它，
/// 各写一份迟早出现 `A - GitHub` 与 `A - Github` 这种不一致。
pub fn package_display_name(repository_name: &str, provider: &crate::models::Provider) -> String {
    let platform = match provider {
        crate::models::Provider::Github => "GitHub",
        crate::models::Provider::Gitee => "Gitee",
        crate::models::Provider::Gitcode => "GitCode",
    };
    format!("{repository_name} - {platform}")
}

/// 只有一个语言版本的文案（手动安装的包名、备注等）。
///
/// 这些内容来自文件名或用户输入，没有仓库源提供的多语言信息，
/// 只能挂在 `zh-CN` 这一个键上 —— 与仓库源规范里「zh-CN 必填」一致。
/// 空值返回空映射，避免存一个空字符串让下游以为「有翻译但是空的」。
pub fn single_language(value: &str) -> BTreeMap<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return BTreeMap::new();
    }
    let mut map = BTreeMap::new();
    map.insert("zh-CN".to_string(), trimmed.to_string());
    map
}

/// 从多语言映射里取「中文主语言」值，用于后端内部需要单一字符串的地方
/// （比如 `InstalledMod.package_name` 这种给旧调用方用的兼容字段）。
///
/// 顺序：`zh-CN` → 任取第一个非空值 → 空串。
pub fn primary_translation(map: &BTreeMap<String, String>) -> String {
    if let Some(value) = map.get("zh-CN").filter(|value| !value.trim().is_empty()) {
        return value.clone();
    }
    map.values()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

pub fn new_manifest(
    repository_id: &str,
    release_tag: &str,
    package_name_i18n: BTreeMap<String, String>,
    note_i18n: BTreeMap<String, String>,
    files: Vec<String>,
    relations: Vec<crate::models::RepositoryRelation>,
) -> InstallManifest {
    InstallManifest {
        repository_id: repository_id.into(),
        release_tag: release_tag.into(),
        repository_binding: None,
        package_name_i18n,
        group_i18n: BTreeMap::new(),
        note_i18n,
        prerelease: false,
        relations,
        installed_at: Utc::now().to_rfc3339(),
        files,
    }
}

#[cfg(test)]
mod tests;
