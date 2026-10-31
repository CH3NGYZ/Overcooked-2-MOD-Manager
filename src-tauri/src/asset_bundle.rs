//! 从 DIYLevel 的关卡集 `info` 文件里读出元数据。
//!
//! ## 为什么需要解析 AssetBundle
//!
//! `levels/<关卡集>/info` 是个 Unity AssetBundle（UnityFS 容器）。关卡集名、
//! 作者、版本号、关卡列表全部只在里面，文件系统层面只有一个二进制文件。
//! 没有这些字段，地图管理就只能显示一个目录名，做不了版本比较、也做不了
//! 「同一个地图集的升级/降级」。
//!
//! ## 为什么用 `unityfs` 而不是自己写
//!
//! 本项目自己实现过一版，做到「类型表解析正确、字段名全对」但仍然栽在
//! 字段对齐上：Unity 按字段大小对齐（`bool` 之后跳 3 字节），漏一步就会
//! 把填充字节当成长度前缀，读出一个荒谬值后越界。这类细节零散、没有规格
//! 文档，只能靠反复试错，而错一次的表现往往是「解析成功但字段为空」。
//!
//! `unityfs` 是 UnityPy 的 Rust 移植，把 Bundle / SerializedFile / 类型树
//! 三层都封装好了，`read_typetree` 直接返回 `UnityValue` 树。用它比维护
//! 一份自带的对齐逻辑可靠得多。
//!
//! ## 只取需要的字段
//!
//! 完整的 AssetBundle 里有纹理、动画、材质、着色器。这里只读两种
//! ScriptableObject：
//!
//! - `LevelSetInfoSO`：关卡集名（中英）、作者、版本、uid、关卡列表
//! - `LevelInfoSO`：单关卡名（中英）、场景名、订单数区间
//!
//! 缩略图（Texture2D / Sprite）**预留但暂不实现**：需要解码 DXT/ETC 纹理，
//! 属于第二阶段。`LevelMetadata::thumbnail_index` 现在恒为 `None`。
use unityfs::{Reader, SerializedFile, UnityValue};

/// 解析出来的关卡集元数据。
///
/// 可序列化是为了**磁盘缓存**：解析一份 info 在 debug 构建下要几秒，而内容
/// 只在安装/替换关卡集时变。缓存键是文件内容的 SHA-256，与路径和 mtime 无关，
/// 所以重命名、复制、重装到别的目录都能复用同一份结果。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LevelSetMetadata {
    pub name: String,
    pub name_zh: Option<String>,
    pub author: String,
    pub version: String,
    /// 关卡集 uid（由 `baseUID + version` 的 MD5 得到）。
    ///
    /// 注意它**随版本变化**，不能当「同一个地图集」的稳定标识：做升级检测
    /// 要用 `name + author`，或在有 `base_uid` 时优先用它。
    pub uid: String,
    /// 作者的稳定标识（不随版本变化）。老导出可能没有。
    pub base_uid: Option<String>,
    pub levels: Vec<LevelMetadata>,
}

/// 单个关卡的元数据。
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LevelMetadata {
    pub name: String,
    pub name_zh: Option<String>,
    pub scene_name: String,
    pub min_order_count: i32,
    pub max_order_count: i32,
}

/// 解析失败的原因。
///
/// 区分「不是 AssetBundle」「是但结构不认识」「找不到关卡集对象」，因为
/// 三者的处理方式不同：第一种说明文件损坏，第二种说明遇到未支持的 Unity
/// 版本，第三种说明资源导出不完整。扫描时都不应中断整个列表。
#[derive(Debug)]
pub enum ParseError {
    /// 魔数不对，不是 UnityFS 文件。
    NotABundle,
    /// 容器或 SerializedFile 解析失败（截断、压缩损坏、版本不支持）。
    Corrupt(String),
    /// 没找到关卡集信息对象（`LevelSetInfoSO`）。
    MissingLevelSetInfo,
    /// IO 错误。
    Io(std::io::Error),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 返回 tag，由前端翻译；冒号后是技术细节（原样展示，便于排查）
        match self {
            Self::NotABundle => write!(f, "assetBundle.notABundle"),
            Self::Corrupt(reason) => write!(f, "assetBundle.corrupt:{reason}"),
            Self::MissingLevelSetInfo => write!(f, "assetBundle.missingLevelSetInfo"),
            Self::Io(error) => write!(f, "assetBundle.ioFailed:{error}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// 从 info 文件的完整字节里解析关卡集元数据。
pub fn parse_level_set(bytes: &[u8]) -> Result<LevelSetMetadata, ParseError> {
    if !unityfs::is_unity_bundle(bytes) {
        return Err(ParseError::NotABundle);
    }
    /*
     * 自己解 UnityFS 容器，再把每个内部文件交给 `unityfs` 的 SerializedFile。
     *
     * 为什么不直接用 `unityfs::Bundle`：它的数据块解压只处理 LZ4（`2 | 3`），
     * 遇到 LZMA（`1`）会把**压缩数据当原始数据**直接拼进去，于是内部文件的
     * offset/size 校验全部失败、`files` 返回空。而 DIYLevel 的 info 恰恰是用
     * LZMA 压缩数据块（块 flags = 0x41）。
     *
     * 容器格式本身简单（头部 + 块表 + 两段解压），自己解更可靠；
     * 真正复杂的是 SerializedFile 与类型树，那部分交给 crate。
     */
    let data = read_bundle_data(bytes)?;

    /*
     * 数据区里可能有多个 SerializedFile（info 一个、resS 一个）。
     * 逐个尝试：关卡集信息在其中一个里面。
     */
    let mut offset = 0usize;
    while offset + 20 <= data.len() {
        let mut reader = Reader::new(data[offset..].to_vec(), unityfs::UnityVersion::default());
        let Some(file) = try_read_serialized(&mut reader) else {
            break;
        };
        if let Some(metadata) = collect_level_set(&file) {
            return Ok(metadata);
        }
        // 跳到下一个 SerializedFile：按它自己声明的 file_size 前进
        let file_size = u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap());
        if file_size == 0 {
            break;
        }
        offset += file_size as usize;
    }
    Err(ParseError::MissingLevelSetInfo)
}

/// 尝试把一段字节当成 SerializedFile 读；失败返回 `None`。
///
/// `unityfs` 遇到不认识的布局时可能 panic（内部有 `unwrap`），而扫描一个坏
/// 文件不该让整个地图列表崩掉，所以这里捕获 panic。
fn try_read_serialized(reader: &mut Reader) -> Option<SerializedFile> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        SerializedFile::read(reader)
    }))
    .ok()
}

/// 单个对象值得解析的大小上限。
///
/// 关卡集与关卡元数据对象是几百字节到几十 KB；超过这个量级的对象只可能是
/// info 里附带的截图纹理或资源块，解析它们既不产出元数据，又会把逐字节展开
/// 成 `UnityValue`，是地图列表加载慢的主因。见 `collect_level_set` 的说明。
const MAX_METADATA_OBJECT_BYTES: usize = 1024 * 1024;

/// 在一个 SerializedFile 里找出关卡集对象与它引用的关卡对象。
///
/// ## 怎么认出是哪种 ScriptableObject
///
/// MonoBehaviour 的 `m_Script` 指向 MonoScript，而 `read_typetree` 会把它的
/// 类名带出来（`m_ClassName`）。所以不需要硬编码 GUID，按类名匹配即可 ——
/// 这对作者换了导出工具、GUID 变了的情况更稳。
fn collect_level_set(file: &SerializedFile) -> Option<LevelSetMetadata> {
    let mut set_object = None;
    let mut set_path_id = 0i64;
    let mut level_objects: Vec<(i64, LevelMetadata)> = Vec::new();

    /*
     * 对象数据是**小端**，而 `Reader::new` 默认大端。
     *
     * 不显式设置的话，`m_Name` 的 4 字节长度会被读成天文数字，随后整段
     * 读空 —— 表现为「字段名全都对、值全是空串」，非常难查。
     *
     * Reader 在循环外建一次：`read_typetree` 内部用 `absolute_reader` 定位到
     * 对象自己的区间，不会消费外层游标；而 `Reader::new(file.data.to_vec())`
     * 会把整个 SerializedFile 复制一遍，放进循环等于按对象个数复制。
     */
    let mut object_reader = Reader::new(file.data.to_vec(), file.unity_version.clone());
    object_reader.endian = file.endian;

    for object in &file.objects {
        /*
         * 只解析**元数据量级**的对象。
         *
         * info 这个 bundle 里除了关卡集/关卡两个 ScriptableObject，还塞了
         * 一张截图纹理（实测 RW 地图的 info 有 9.4 MB，其中绝大部分是它）。
         * `read_typetree` 会把纹理的原始字节当成 `UnityValue` 数组逐字节展开，
         * 一千万级别的元素，单张图就要几十秒 —— 地图列表的加载时间几乎全耗在
         * 这一个没人读的对象上。
         *
         * 关卡集元数据是几百字节到几十 KB；真的超过 1 MiB 的对象不可能是它。
         * 阈值而不是按类名过滤：`m_ClassName` 依赖导出工具是否写了类型树，
         * 而大小是文件结构本身的事，遇到不认识的导出方式也不会误伤。
         */
        if object.byte_size > MAX_METADATA_OBJECT_BYTES {
            continue;
        }
        let Ok(value) = object.read_typetree(&mut object_reader, &[]) else {
            continue;
        };
        let UnityValue::Map(map) = &value else {
            continue;
        };
        if map.contains_key("levelSetName") {
            set_object = Some(map.clone());
            set_path_id = object.path_id;
        } else if map.contains_key("levelName") {
            level_objects.push((
                object.path_id,
                LevelMetadata {
                    name: string_of(map, "levelName"),
                    name_zh: non_empty(string_of(map, "levelNameZH")),
                    scene_name: string_of(map, "sceneName"),
                    min_order_count: int_of(map, "minOrderCount"),
                    max_order_count: int_of(map, "maxOrderCount"),
                },
            ));
        }
    }
    let _ = set_path_id;

    let set = set_object?;
    /*
     * 按 `levelInfos` 的声明**过滤并排序**。
     *
     * 两个都不能省：
     *
     * - **过滤**：实测 RW 地图的 info 里有 11 个 `LevelInfoSO`，但只声明了
     *   10 个 —— 多出来的 `rw_test` 是作者的开发残留，连场景文件都没打包。
     *   不过滤就会报「11 个关卡」，而实际只有 10 个可玩。
     * - **排序**：对象表顺序不保证与声明一致（oc1_story 从 6-2 开始）。
     *
     * 声明为空（老导出）时保持原样，不做过滤 —— 那时对象表是唯一依据。
     */
    let declared: Vec<i64> = match set.get("levelInfos") {
        Some(UnityValue::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                // PPtr 在 UnityValue 里是**独立变体**，不是 Map ——
                // 按 Map 取 m_PathID 会全部落空，过滤和排序一起静默失效。
                UnityValue::PPtr { path_id, .. } => Some(*path_id),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    if !declared.is_empty() {
        level_objects.retain(|(path_id, _)| declared.contains(path_id));
        level_objects.sort_by_key(|(path_id, _)| {
            declared
                .iter()
                .position(|want| want == path_id)
                .unwrap_or(usize::MAX)
        });
    }
    let levels: Vec<LevelMetadata> = level_objects.into_iter().map(|(_, level)| level).collect();

    Some(LevelSetMetadata {
        name: string_of(&set, "levelSetName"),
        name_zh: non_empty(string_of(&set, "levelSetNameZH")),
        author: string_of(&set, "author"),
        version: string_of(&set, "version"),
        uid: string_of(&set, "uid"),
        base_uid: non_empty(string_of(&set, "baseUID")),
        levels,
    })
}

/// 从 UnityValue 映射里取字符串字段，缺失时返回空串。
fn string_of(map: &std::collections::BTreeMap<String, UnityValue>, key: &str) -> String {
    match map.get(key) {
        Some(UnityValue::String(value)) => value.clone(),
        _ => String::new(),
    }
}

/// 从 UnityValue 映射里取整数字段，缺失时返回 0。
fn int_of(map: &std::collections::BTreeMap<String, UnityValue>, key: &str) -> i32 {
    match map.get(key) {
        Some(UnityValue::Int32(value)) => *value,
        Some(UnityValue::Int64(value)) => *value as i32,
        Some(UnityValue::UInt32(value)) => *value as i32,
        _ => 0,
    }
}

/// 把空字符串转成 `None`，避免上层到处判空。
fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用真实 info 文件验证解析结果。
    ///
    /// 期望值来自 UnityPy（社区标准的 Unity 资源解析库）独立解析的结果，
    /// 写死在测试里，运行时不依赖 Python。
    ///
    /// 路径来自开发者本机的游戏安装；找不到文件时跳过（CI 上没有游戏），
    /// 本地跑就能立刻发现解析回归。
    fn game_info(name: &str) -> Option<Vec<u8>> {
        let path = format!(
            "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevel/levels/{name}/info_{name}"
        );
        std::fs::read(&path).ok()
    }

    #[test]
    fn parses_test_level_metadata() {
        let Some(bytes) = game_info("test_level") else {
            eprintln!("跳过：本机没有 test_level（CI 环境）");
            return;
        };
        let parsed = parse_level_set(&bytes).expect("应能解析 test_level");
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.name_zh.as_deref(), Some("测试"));
        assert_eq!(parsed.author, "GUA");
        assert_eq!(parsed.version, "0.5.1");
        assert_eq!(parsed.uid, "24f57368-9ec4-a216-ec44-824ffdc33d66");
        assert_eq!(parsed.levels.len(), 5);
        assert_eq!(parsed.levels[0].name, "Test 1");
        assert_eq!(parsed.levels[0].name_zh.as_deref(), Some("测试 1"));
        assert_eq!(parsed.levels[0].scene_name, "s_test_level");
        assert_eq!(parsed.levels[0].min_order_count, 2);
        assert_eq!(parsed.levels[0].max_order_count, 5);
    }

    #[test]
    fn parses_oc1_story_metadata() {
        let Some(bytes) = game_info("oc1_story") else {
            eprintln!("跳过：本机没有 oc1_story（CI 环境）");
            return;
        };
        let parsed = parse_level_set(&bytes).expect("应能解析 oc1_story");
        assert_eq!(parsed.name, "Overcooked 1 - Story");
        assert_eq!(parsed.name_zh.as_deref(), Some("胡闹厨房 1 - 主线"));
        // 作者留空：官方导出的关卡集没有填 author
        assert_eq!(parsed.author, "");
        assert_eq!(parsed.version, "1.0.1");
        assert_eq!(parsed.levels.len(), 28);
        assert_eq!(parsed.levels[0].scene_name, "s_oc1_story_1_1");
    }

    #[test]
    fn rejects_non_bundle_data() {
        assert!(matches!(
            parse_level_set(b"not an asset bundle at all"),
            Err(ParseError::NotABundle)
        ));
    }
}

// ===== UnityFS 容器解析 =====
//
// 这部分自己实现而不依赖 `unityfs::Bundle`：后者的数据块解压缺 LZMA 分支
// （详见 `parse_level_set` 的说明）。容器格式本身不长，验证成本可控。

/// 压缩方式（AssetBundle 的块 flags 低 6 位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compression {
    None,
    Lzma,
    Lz4,
    Lz4HighCompression,
}

impl Compression {
    fn from_flags(flags: u32) -> Result<Self, ParseError> {
        match flags & 0x3f {
            0 => Ok(Self::None),
            1 => Ok(Self::Lzma),
            2 => Ok(Self::Lz4),
            3 => Ok(Self::Lz4HighCompression),
            other => Err(ParseError::Corrupt(format!("compression={other}"))),
        }
    }
}

/// 按压缩方式解开一段数据。
fn decompress(
    data: &[u8],
    uncompressed_size: usize,
    compression: Compression,
) -> Result<Vec<u8>, ParseError> {
    match compression {
        Compression::None => {
            if data.len() < uncompressed_size {
                return Err(ParseError::Corrupt("block truncated".into()));
            }
            Ok(data[..uncompressed_size].to_vec())
        }
        Compression::Lz4 | Compression::Lz4HighCompression => {
            // LZ4HC 与 LZ4 的**解压**完全相同，只是压缩率/速度不同，
            // 所以两种 flags 走同一条路径。
            let mut out = vec![0u8; uncompressed_size];
            let written = lz4_flex::block::decompress_into(data, &mut out)
                .map_err(|error| ParseError::Corrupt(format!("lz4: {error}")))?;
            out.truncate(written);
            Ok(out)
        }
        Compression::Lzma => decompress_lzma_block(data, uncompressed_size),
    }
}

/// 解一个 Unity 的 LZMA 块。
///
/// Unity 的写法是 `[5 字节 LZMA1 properties][压缩数据]` —— 比标准 `.lzma`
/// 容器少了那 8 字节的解压长度。`lzma-rs` 只认标准容器，所以这里**补回**
/// 长度字段：`[5 字节 props][8 字节小端长度][压缩数据]`。
fn decompress_lzma_block(data: &[u8], uncompressed_size: usize) -> Result<Vec<u8>, ParseError> {
    if data.len() < 5 {
        return Err(ParseError::Corrupt("lzma properties truncated".into()));
    }
    let mut standard = Vec::with_capacity(data.len() + 8);
    standard.extend_from_slice(&data[..5]);
    standard.extend_from_slice(&(uncompressed_size as u64).to_le_bytes());
    standard.extend_from_slice(&data[5..]);

    let mut input = standard.as_slice();
    let mut out = Vec::with_capacity(uncompressed_size);
    lzma_rs::lzma_decompress(&mut input, &mut out)
        .map_err(|error| ParseError::Corrupt(format!("lzma: {error}")))?;
    if out.len() != uncompressed_size {
        return Err(ParseError::Corrupt(format!(
            "lzma size {} != {uncompressed_size}",
            out.len()
        )));
    }
    Ok(out)
}

/// 大端读取辅助（AssetBundle 头与 blocksInfo 固定大端）。
struct BundleReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> BundleReader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], ParseError> {
        if self.pos + count > self.bytes.len() {
            return Err(ParseError::Corrupt("bundle truncated".into()));
        }
        let slice = &self.bytes[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, ParseError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, ParseError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64, ParseError> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn cstr(&mut self) -> Result<String, ParseError> {
        let end = self.bytes[self.pos..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|offset| self.pos + offset)
            .ok_or_else(|| ParseError::Corrupt("cstring unterminated".into()))?;
        let text = String::from_utf8_lossy(&self.bytes[self.pos..end]).into_owned();
        self.pos = end + 1;
        Ok(text)
    }
}

/// 读完整个 AssetBundle，返回按块顺序拼接的解压数据区。
fn read_bundle_data(bytes: &[u8]) -> Result<Vec<u8>, ParseError> {
    let mut reader = BundleReader { bytes, pos: 8 }; // 'UnityFS\0' 固定 8 字节
    let _version = reader.u32()?;
    let _unity_version = reader.cstr()?;
    let _unity_revision = reader.cstr()?;
    let _total_size = reader.i64()?;
    let compressed_info_size = reader.u32()?;
    let uncompressed_info_size = reader.u32()?;
    let flags = reader.u32()?;

    let info_compression = Compression::from_flags(flags)?;
    let info_raw = reader.take(compressed_info_size as usize)?;
    let info = decompress(info_raw, uncompressed_info_size as usize, info_compression)?;

    let mut info_reader = BundleReader {
        bytes: &info,
        pos: 16,
    }; // 前 16 字节是哈希
    let block_count = info_reader.u32()?;
    let mut blocks = Vec::with_capacity(block_count as usize);
    for _ in 0..block_count {
        let uncompressed_size = info_reader.u32()?;
        let compressed_size = info_reader.u32()?;
        let block_flags = info_reader.u16()? as u32;
        blocks.push((uncompressed_size, compressed_size, block_flags));
    }
    let _node_count = info_reader.u32()?; // 内部文件表：这里按块顺序拼接即可，不单独用

    /*
     * 数据区紧跟 blocksInfo 之后，每块是独立的压缩单元，必须分别解压再拼接。
     *
     * 注意块 flags 与头部 flags 的压缩方式**可以不同**（这份文件头是 LZ4、
     * 数据块是 LZMA），只看头部解数据块会得到 `corrupt input`。
     */
    let mut data = Vec::new();
    for (uncompressed_size, compressed_size, block_flags) in blocks {
        let compression = Compression::from_flags(block_flags)?;
        let raw = reader.take(compressed_size as usize)?;
        data.extend(decompress(raw, uncompressed_size as usize, compression)?);
    }
    Ok(data)
}

#[cfg(test)]
mod rw_fixture_tests {
    use super::*;

    /// 用真实发布的地图 ZIP 里的 info 验证解析（RW 地图 v0.73）。
    ///
    /// 这条比内置的两份更适合当回归基线：它是**线上真实分发**的资源，
    /// 作者用的是另一套导出环境。文件放在 `.tmp/info_rw`（下载后提取），
    /// 不存在时跳过。
    #[test]
    fn parses_published_rw_level_set() {
        let Ok(bytes) = std::fs::read(".tmp/info_rw") else {
            eprintln!("跳过：没有 .tmp/info_rw");
            return;
        };
        let parsed = parse_level_set(&bytes).expect("应能解析 RW 地图");
        assert_eq!(parsed.name, "RW_LevelSet");
        assert_eq!(parsed.name_zh.as_deref(), Some("RW_关卡合集"));
        assert_eq!(parsed.author, "RW");
        assert_eq!(parsed.version, "0.73");
        assert_eq!(
            parsed.base_uid.as_deref(),
            Some("2f39ebdb-529c-4894-b732-dc76f953ce13")
        );
        assert_eq!(parsed.levels.len(), 10);
    }
}
