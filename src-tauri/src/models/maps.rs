use super::config::default_true;
use super::relations::{ReleaseRelationOverride, RepositoryRelation, deserialize_relations};
use super::releases::ReleaseInfo;
use super::repositories::{
    AssetRule, Channel, Provider, default_download_threads, default_max_release_count,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 版本表里的一项：某一版地图的「资产版本 + info 版本」。
///
/// 用数组而不是 map：顺序本身就是信息（末项最新），拖动排序就是移动元素。
/// 附件名不存进表里 —— 它由作者的 asset 规则渲染 `mapset_version` 得到，
/// 再存一份副本只会在改名之后与规则对不上。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetVersionEntry {
    /// 作者定义的地图包版本号；允许 `3.4.1-fix` 这种带后缀的写法。
    #[serde(default)]
    pub mapset_version: String,
    /// 这一版 ZIP 里 `info` 自报的版本号，判「装的是不是最新」用它。
    #[serde(default)]
    pub info_version: String,
}

fn default_map_provider() -> Provider {
    Provider::Github
}

/// 一个地图仓库：地图的分发单元。
///
/// 与 `RepositoryConfig` 对称但刻意分开：MOD 靠插件识别字段绑定，地图靠
/// `author + level_set_name + level_set_name_zh`；两边共用关系类型和下载/刷新管线。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapRepositoryConfig {
    /// 稳定 ID。
    ///
    /// 源文档可以不写：首次导入时按「源 ID + 身份」生成并持久化，
    /// 之后一直复用（安装清单与缓存都挂在它上面）。手写 UUID 只会制造
    /// 「复制一份配置忘了改 ID」这类问题。
    #[serde(default)]
    pub id: String,
    /// 单语言镜像。源文档**只需要写 `name_i18n`**，这里是加载后从
    /// `zh-CN` 折进 来的兼容字段，不要求作者手写两遍。
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_i18n: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// 条目可以省略：源文档顶部写了 `[bundle]` 时由它统一提供坐标，
    /// 单条目自己写就覆盖。缺省按 GitHub —— 存量条目全在 GitHub 上，
    /// 而「缺 provider 直接解析失败」会让只是漏写一行的作者整份源都发不出去。
    #[serde(default = "default_map_provider")]
    pub provider: Provider,
    /// 与 `provider` 同理：可以整组留给 `[bundle]` 提供，也可以自己写覆盖。
    /// 校验（`validate_map_entry`）在继承之后才跑，所以「两条都空」仍然报错。
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub channel: Channel,
    /// 只读这一个 Release。
    ///
    /// 空 = 老行为（列表里最新的那个）。非空时**不再按时间取最新**：直接把
    /// 该 tag 的资产按 `asset_rules` 里的版本正则拆成多个「版本」，一个资产
    /// 一条 Release（见 `map_sources/assets.rs::synthesize_releases`）。
    ///
    /// 这是「一个仓库装多个地图集」的落地方式：作者每张图只往自己的固定 tag
    /// 里追加版本文件，条目靠 tag 区分，不必给每张图各开一个仓库。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    /// 强制最新**地图包版本**，空 = 按数字段比较取最大。
    ///
    /// 只有 tag 模式有意义：资产名里的版本号是作者手写的，`0.7` 在数字比较
    /// 里小于 `0.69`，作者却可能认为前者更新。写了它就等于「按这个号发最新
    /// 版」——前端判定也改成等值才算最新（见 `src/levelSetStatus.ts`）。
    ///
    /// 旧名 `latest_version` 依旧可反序列化：已发布出去的地图源文档用的就是它。
    #[serde(
        default,
        alias = "latest_version",
        skip_serializing_if = "Option::is_none"
    )]
    pub latest_mapset_version: Option<String>,
    /// 最新 Release 里 `info` 文件**内**的版本号（Release note 块的
    /// `latest_info_version`，由 `tools/oc2-info` 自动取）。
    ///
    /// `latest_mapset_version` 是作者定义的地图包版本，这一项是包内 info 自报的版本。
    /// 两者平时相同；作者升了包版本却忘了改 info 版本号时会不同，那时按包版本
    /// 比较会让用户永远看到「可更新」，前端改用这一项确认「装的就是最新版」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_info_version: Option<String>,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    /// 选哪个资产：地图只分发 ZIP。
    #[serde(default)]
    pub asset_rules: Vec<AssetRule>,
    /// 版本表（来自 Release note 的 `asset_versions`，末项最新）。
    ///
    /// 决定「谁是最新版」和版本列表的展示顺序；空表表示这份源没排过序，
    /// 退回按附件名里的数字段降序。
    #[serde(default)]
    pub asset_versions: Vec<AssetVersionEntry>,
    /// 本地关卡集的 `levelSetName`（info 里的英文名）。
    ///
    /// 与 `author` 一起构成绑定身份。不能用 `uid`：它是
    /// `MD5(baseUID + version)`，每发一版就变。
    #[serde(default)]
    pub level_set_name: String,
    /// 本地关卡集的 `levelSetNameZH`（info 里的中文名），可选。
    ///
    /// 英文集名与作者都相同的两张地图只靠前两者分不开（实测「胡闹糕中」与
    /// 「胡闹达学」是同一个作者的两张图，都写 `GAODA / GAODA`），中文名是
    /// 唯一能区分、又不随版本变化的字段。老源不写它时退回前两者比较，
    /// 行为与加入该字段之前完全一致。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level_set_name_zh: String,
    /// 本地关卡集的 `author`（info 里的作者，官方导出常为空）。
    #[serde(default)]
    pub author: String,
    /// 依赖声明：地图需要 DIYLevel 与 Web Runtime 在特定版本以上才跑得起来。
    ///
    /// 复用 `RepositoryRelation`——「某 GUID 的版本必须在区间内」这个语义
    /// 两边完全一样，没有再造一套的理由。
    #[serde(
        default,
        alias = "requires",
        deserialize_with = "deserialize_relations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub relations: Vec<RepositoryRelation>,
    /// 按地图版本覆盖上面的关系表（与仓库源 `release_relations` 同语义）。
    ///
    /// 同一个地图集的大版本可能依赖不同版本的 DIYLevel / Web Runtime，
    /// 一张固定表表达不了；命中区间的那张整体替换，见
    /// `mods::effective_map_relations`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_relations: Vec<ReleaseRelationOverride>,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub note_i18n: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// 所属源的昵称：单语言镜像 + 多语言表。
    ///
    /// 后端**不持久化地图源树**（见 `map_sources::load_map_tree` 的说明），源
    /// 只是归类方式；但界面要按「源 → 分类 → 地图」三层展示，源名必须能读到。
    /// 所以加载源树时把昵称写进条目本身 —— `get_maps` 读的是持久化配置，
    /// 只靠 `source_id` 拿不到作者写在 `[[map_sources]]` 里的昵称。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_name_i18n: BTreeMap<String, String>,
    /// Release 缓存，与 `RepositoryConfig` 同一套字段与 TTL 语义。
    #[serde(default)]
    pub cached_releases: Vec<ReleaseInfo>,
    #[serde(default)]
    pub releases_cached_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// 一张地图仓库的依赖快照。管理器的清单，不写进游戏目录。
///
/// 关卡集不使用 MOD 的安装清单；依赖与资产版本单独写入用户数据根的地图快照。
/// 地图目录位置由 `level_sets/paths.rs` 决定，表层包另有随包插件登记表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapRelationSnapshot {
    pub repository_id: String,
    pub level_set_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level_set_name_zh: String,
    pub author: String,
    /// 安装/同步那一刻线上声明的分组（分组语义就是作者）。
    ///
    /// 地图源被删掉之后这张图不再有线上条目，分组只剩这份快照记着；
    /// 没有它，管理页只能显示 info 里的裸作者名，作者声明的中英两名全丢。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_i18n: BTreeMap<String, String>,
    /// 分组的单语镜像（旧源文档只写 `group` 时用它）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RepositoryRelation>,
    /// 按版本覆盖的依赖表：快照必须带上它，否则「已经装了 1.x、线上声明的是
    /// 2.x 的依赖」时，依赖表会显示成 2.x 那一套。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_relations: Vec<ReleaseRelationOverride>,
    /// 安装/更新那一刻的资产版本（Release 的 tag_name，如 `3.4.1-fix`）。
    ///
    /// 本地 info 里的版本号可能与资产版本不一致（作者发修复版时常忘了改 info），
    /// 只靠 info 版本无法判断装的是不是最新资产，所以安装时记下来。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub installed_mapset_version: String,
}

/// `<data_root>/manifests/map-relations.json` 的整份内容。
///
/// 为什么不用一张清单一个文件：关卡集不是以仓库 id 落地在磁盘上的（身份是
/// `info` 中英文名、双方非空中文名及作者），读取时只做一次「身份→关系」查表，
/// 单文件比几十个小文件省一次目录扫描。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MapRelationSnapshots {
    #[serde(default = "default_snapshot_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub entries: BTreeMap<String, MapRelationSnapshot>,
}

pub(crate) fn default_snapshot_schema_version() -> u32 {
    1
}

/// 一个地图仓库的界面模型。
///
/// 与 `MapRepositoryConfig` 的区别：只带界面要显示的内容（含最新 tag 与缓存
/// 状态）和 `asset_rules`，不带 `download_threads` 这类纯内部参数。
///
/// `asset_rules` 必须暴露：详情面板要标出「安装时会选哪个资产」，而自动匹配
/// 由后端 `select_install_asset` 完成，前端只有拿到同一份规则才能镜像结论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_i18n: BTreeMap<String, String>,
    pub provider: Provider,
    pub username: String,
    pub repository: String,
    /// 绑定的本地关卡集名（info 里的 `levelSetName`）。
    pub level_set_name: String,
    /// 绑定的本地中文名（info 里的 `levelSetNameZH`），可选。
    ///
    /// 英文集名与作者都相同的两张地图靠前两者分不开（实测「胡闹糕中」与
    /// 「胡闹达学」都写 GAODA / GAODA），前端判「已安装 / 有新版」时必须一起看。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level_set_name_zh: String,
    /// 绑定的本地作者（info 里的 `author`）。
    pub author: String,
    /// 下载资产规则（与 `RepositoryConfig.asset_rules` 同语义）。
    ///
    /// 地图只分发 ZIP，资产却是作者随意命名的，所以必须把规则交给前端，
    /// 让详情面板用同一套 exact/glob/regex 判定标出会被下载的那个资产。
    #[serde(default)]
    pub asset_rules: Vec<AssetRule>,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub note_i18n: BTreeMap<String, String>,
    pub enabled: bool,
    pub builtin: bool,
    /// 所属地图源：ID 与昵称（多语言表优先，单语言镜像兜底）。
    ///
    /// 地图源树不落盘（源只是归类方式，见 `map_sources`），归属与源名随每次
    /// 加载写进条目，这里原样透传 —— 否则前端只能把「哪张图属于哪个源」
    /// 重新猜一遍，页面的三层结构也就无从建立。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_name_i18n: BTreeMap<String, String>,
    /// 作者指定的「这就是最新版」的**地图包版本**；没写时为 `None`。
    ///
    /// 前端判定改的是**语义**而不是比较方式：写了它就不再比大小，只比等值
    /// （资产名里的 `0.7` 与 `0.69` 用数字比较会得出反的结论）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_mapset_version: Option<String>,
    /// 最新 Release 里 `info` 文件内的版本号；源没声明时为 `None`。
    ///
    /// 前端拿它与本地扫出的 `LevelSetInfo.version` 比：相等即「装的就是这一版」，
    /// 作者忘没忘改 info 里的版本号都不影响结论。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_info_version: Option<String>,
    /// 缓存里最新的 Release tag；没刷新过时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_tag: Option<String>,
    /// 缓存里最新的那一条 Release（卡片复用 MOD 卡片时直接喂给版本/下载量）。
    ///
    /// 只给界面要显示的字段，和 `latest_tag` 冗余但语义不同：卡片需要
    /// `assets` 算下载量、需要 `prerelease` 决定版本标记样式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_release: Option<ReleaseInfo>,
    /// 缓存里的全部 Release，供详情抽屉选版本（装旧版本回滚）。
    ///
    /// 与 `latest_release` 冗余但用途不同：那个回答「最新是哪一版」，这里回答
    /// 「能装哪几版」。只给最新一条时，详情里的版本下拉没有内容可列。
    ///
    /// 不 skip：前端类型是必填数组（`cached_releases: ReleaseInfo[]`），没刷新过
    /// 时也要拿到 `[]` 而不是 `undefined`。
    #[serde(default)]
    pub cached_releases: Vec<ReleaseInfo>,
    /// 当前地区可安装的 Release 数（与 `cached_release_count` 同源）。
    #[serde(default)]
    pub cached_release_count: usize,
    /// Release 缓存时间（Unix 毫秒），用于界面提示「几分钟前」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// 仓库声明的全部关系（依赖 / 排斥 / 兼容，原始形态）。
    ///
    /// `required_frameworks` 只投影「依赖」给框架提示用；依赖表要按原样展示，
    /// 所以冲突/兼容、发布线、可选标记也必须给前端。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RepositoryRelation>,
    /// 按地图版本覆盖 `relations` 的表（前端按「要装/已装版本」选一张）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_relations: Vec<ReleaseRelationOverride>,
    /// 需要的框架与版本要求（来自 `relations`），供界面提示升级框架。
    ///
    /// 只有**声明**：`installed_version` 恒为 `None`，后端不再为此扫描本地插件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_frameworks: Vec<RequiredFramework>,
    /// 兼容字段，当前恒为空。
    ///
    /// 依赖判定已统一到前端 `modRelations.ts`（输入是 Dashboard 已扫好的本地
    /// MOD 列表），后端不再保留第二份实现。字段留着只为旧前端读取时不崩。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmet_frameworks: Vec<RequiredFramework>,
}

/// 地图依赖的一个框架组件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredFramework {
    pub guid: String,
    /// 版本区间表达式（与 `RepositoryRelation.version` 同一语法）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// 本地已装版本（未装时为空），让界面能说清「已装 0.9.9，需要 >=0.10.0」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
}
