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
//! 按需关卡预览、对象引用与纹理由 level_sets::preview_reader 拥有；扫描不解码图片。
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
        /*
         * 返回 tag，由前端翻译。
         *
         * 技术细节必须走 `?reason=` 查询串，不能像以前那样拼成
         * `assetBundle.corrupt:<细节>`：前端只认查询串形态，冒号后面那串会被
         * 当成 tag 的一部分去查表，查不到就把整串原样显示给用户
         * （实测界面里出现过 `assetBundle.corrupt:lzma: lzma data error`）。
         */
        match self {
            Self::NotABundle => write!(f, "assetBundle.notABundle"),
            Self::Corrupt(reason) => write!(
                f,
                "{}",
                crate::locale_tag::with_params("assetBundle.corrupt", &[("reason", reason)])
            ),
            Self::MissingLevelSetInfo => write!(f, "assetBundle.missingLevelSetInfo"),
            Self::Io(error) => write!(
                f,
                "{}",
                crate::locale_tag::with_params(
                    "assetBundle.ioFailed",
                    &[("reason", &error.to_string())]
                )
            ),
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
    for (_, data) in read_bundle_entries(bytes)? {
        if data.len() < 20 || !matches!(u32::from_be_bytes(data[8..12].try_into().unwrap()), 9..=30)
        {
            continue;
        }
        let mut reader = Reader::new(data, unityfs::UnityVersion::default());
        if let Some(file) = try_read_serialized(&mut reader) {
            if let Some(metadata) = collect_level_set(&file) {
                return Ok(metadata);
            }
        }
    }
    Err(ParseError::MissingLevelSetInfo)
}

/// 尝试把一段字节当成 SerializedFile 读；失败返回 `None`。
///
/// `unityfs` 遇到不认识的布局时可能 panic（内部有 `unwrap`），而扫描一个坏
/// 文件不该让整个地图列表崩掉，所以这里捕获 panic。
pub(crate) fn try_read_serialized(reader: &mut Reader) -> Option<SerializedFile> {
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
pub(crate) fn string_of(map: &std::collections::BTreeMap<String, UnityValue>, key: &str) -> String {
    match map.get(key) {
        Some(UnityValue::String(value)) => value.clone(),
        _ => String::new(),
    }
}

/// 从 UnityValue 映射里取整数字段，缺失时返回 0。
pub(crate) fn int_of(map: &std::collections::BTreeMap<String, UnityValue>, key: &str) -> i32 {
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
mod tests;
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
    if uncompressed_size > 512 * 1024 * 1024 {
        return Err(ParseError::Corrupt("block too large".into()));
    }
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
/// 容器少了那 8 字节的解压长度，说明流里带 EOS marker。所以这里**补回**一个
/// 13 字节的 alone 头，长度字段填「未知」：`[5 props][8 字节 u64::MAX][数据]`。
///
/// **为什么用 liblzma（`xz2`）而不是纯 Rust 的 `lzma-rs`**：实测同一批真实
/// 地图（合计 131 MB）decompress+解对象表，`lzma-rs` 13584 ms、liblzma
/// 4557 ms（debug 构建，见 `bench_real_level_sets`）；tutu 那一份 9202 ms →
/// 2587 ms。C 实现编译期就带优化，因此 debug 构建同样快 —— 开发时进地图页
/// 也从「等十几秒」变成「等几秒」。
fn decompress_lzma_block(data: &[u8], uncompressed_size: usize) -> Result<Vec<u8>, ParseError> {
    if data.len() < 5 {
        return Err(ParseError::Corrupt("lzma properties truncated".into()));
    }
    let mut standard = Vec::with_capacity(data.len() + 8);
    standard.extend_from_slice(&data[..5]);
    /*
     * 长度字段写 `u64::MAX`（= alone 头里的「未知长度」）而不是真实解压大小。
     *
     * Unity 既然省掉了这 8 字节，流里就带 EOS marker。liblzma 的 alone 解码器
     * 看到「已知长度」就会按那个长度停，与 EOS 位置对不上时直接报 `data error`
     * （实测 test_level / oc1_story 都栽在这里）；声明未知长度后它按 EOS 结束，
     * 解出来的长度再由下面那句校验兜底。
     */
    standard.extend_from_slice(&u64::MAX.to_le_bytes());
    standard.extend_from_slice(&data[5..]);

    let mut stream = xz2::stream::Stream::new_lzma_decoder(u64::MAX)
        .map_err(|error| ParseError::Corrupt(format!("lzma: {error}")))?;
    let mut out = Vec::with_capacity(uncompressed_size);
    stream
        .process_vec(&standard, &mut out, xz2::stream::Action::Finish)
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
        if self
            .pos
            .checked_add(count)
            .is_none_or(|end| end > self.bytes.len())
        {
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
        let end = self
            .bytes
            .get(self.pos..)
            .ok_or_else(|| ParseError::Corrupt("cstring truncated".into()))?
            .iter()
            .position(|byte| *byte == 0)
            .map(|offset| self.pos + offset)
            .ok_or_else(|| ParseError::Corrupt("cstring unterminated".into()))?;
        let text = String::from_utf8_lossy(&self.bytes[self.pos..end]).into_owned();
        self.pos = end + 1;
        Ok(text)
    }
}

/// 保留内部文件边界，预览中的外部对象和流式纹理按目录定位。
pub(crate) struct BundleLayout {
    pub directory: Vec<(String, usize, usize)>,
    data_start: usize,
    total: usize,
    blocks: Vec<(usize, usize, u16)>,
}
fn bundle_layout_with(
    length: usize,
    mut read: impl FnMut(usize, usize) -> Result<Vec<u8>, ParseError>,
) -> Result<BundleLayout, ParseError> {
    let prefix = read(0, length.min(4096))?;
    if !prefix.starts_with(b"UnityFS\0") {
        return Err(ParseError::NotABundle);
    }
    let mut reader = BundleReader {
        bytes: &prefix,
        pos: 8,
    };
    let version = reader.u32()?;
    let _ = reader.cstr()?;
    let _ = reader.cstr()?;
    let total_size = reader.i64()?;
    if total_size < 0 || total_size as usize != length {
        return Err(ParseError::Corrupt("bundle size".into()));
    }
    let compressed_info_size = reader.u32()? as usize;
    let uncompressed_info_size = reader.u32()? as usize;
    if compressed_info_size > 16 * 1024 * 1024 || uncompressed_info_size > 64 * 1024 * 1024 {
        return Err(ParseError::Corrupt("info too large".into()));
    }
    let flags = reader.u32()?;
    if version >= 7 {
        reader.pos = (reader.pos + 15) & !15;
    }
    let header_end = reader.pos;
    let info_start = if flags & 0x80 != 0 {
        length
            .checked_sub(compressed_info_size)
            .ok_or_else(|| ParseError::Corrupt("info offset".into()))?
    } else {
        header_end
    };
    let info = decompress(
        &read(info_start, compressed_info_size)?,
        uncompressed_info_size,
        Compression::from_flags(flags)?,
    )?;
    reader.pos = if flags & 0x80 != 0 {
        header_end
    } else {
        info_start + compressed_info_size
    };
    if flags & 0x200 != 0 {
        reader.pos = (reader.pos + 15) & !15;
    }
    let mut ir = BundleReader {
        bytes: &info,
        pos: 16,
    };
    let count = ir.u32()? as usize;
    if count > info.len() / 10 {
        return Err(ParseError::Corrupt("block count".into()));
    }
    let mut blocks = Vec::with_capacity(count);
    let mut total = 0usize;
    for _ in 0..count {
        let size = ir.u32()? as usize;
        let compressed = ir.u32()? as usize;
        let flags = ir.u16()?;
        total = total
            .checked_add(size)
            .filter(|n| *n <= 512 * 1024 * 1024)
            .ok_or_else(|| ParseError::Corrupt("bundle too large".into()))?;
        blocks.push((size, compressed, flags));
    }
    let nodes = ir.u32()? as usize;
    if nodes > info.len() / 21 {
        return Err(ParseError::Corrupt("node count".into()));
    }
    let mut directory = Vec::new();
    for _ in 0..nodes {
        let offset = ir.i64()?;
        let size = ir.i64()?;
        let _flags = ir.u32()?;
        let name = ir.cstr()?;
        if offset < 0 || size < 0 {
            return Err(ParseError::Corrupt("node range".into()));
        }
        directory.push((name, offset as usize, size as usize));
    }
    if directory
        .iter()
        .try_fold(0usize, |total, (_, _, size)| total.checked_add(*size))
        .is_none_or(|n| n > 512 * 1024 * 1024)
    {
        return Err(ParseError::Corrupt("directory too large".into()));
    }
    Ok(BundleLayout {
        directory,
        data_start: reader.pos,
        total,
        blocks,
    })
}
fn bundle_layout(bytes: &[u8]) -> Result<BundleLayout, ParseError> {
    bundle_layout_with(bytes.len(), |start, size| {
        let end = start
            .checked_add(size)
            .ok_or_else(|| ParseError::Corrupt("range overflow".into()))?;
        bytes
            .get(start..end)
            .map(|b| b.to_vec())
            .ok_or_else(|| ParseError::Corrupt("bundle truncated".into()))
    })
}
/// Seek only to header and directory: indexing installed game bundles never reads
/// their scene/texture payloads. The metadata-only CLI does not use this entry.
#[allow(dead_code)]
pub(crate) fn bundle_file_names(path: &std::path::Path) -> Result<Vec<String>, ParseError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let length = usize::try_from(file.metadata()?.len())
        .map_err(|_| ParseError::Corrupt("file too large".into()))?;
    let layout = bundle_layout_with(length, |start, size| {
        if start.checked_add(size).is_none_or(|end| end > length) {
            return Err(ParseError::Corrupt("file range".into()));
        }
        file.seek(SeekFrom::Start(start as u64))?;
        let mut bytes = vec![0; size];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    })?;
    Ok(layout
        .directory
        .into_iter()
        .map(|(name, _, _)| name)
        .collect())
}
/// Context-scoped decoded blocks, bounded to 192MiB. Sequential references share
/// decompression work without eagerly loading every external CAB/texture stream.
/// This accommodates commonW3's single 119MiB LZMA block plus small LZ4 blocks;
/// a smaller cap would re-decompress that same block for every referenced object.
#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct BundleBlockCache {
    layouts: std::collections::HashMap<std::path::PathBuf, std::sync::Arc<BundleLayout>>,
    entries: std::collections::VecDeque<(
        (std::path::PathBuf, usize, usize, usize, u16),
        std::sync::Arc<[u8]>,
    )>,
    bytes: usize,
}
/// Read only blocks intersecting a named node/range. External texture streams stay
/// on disk instead of crowding useful metadata out of the preview cache.
#[allow(dead_code)]
pub(crate) fn read_bundle_file_range(
    path: &std::path::Path,
    name: &str,
    offset: usize,
    size: Option<usize>,
    mut cache: Option<&mut BundleBlockCache>,
) -> Result<Vec<u8>, ParseError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let length = usize::try_from(file.metadata()?.len())
        .map_err(|_| ParseError::Corrupt("file too large".into()))?;
    if length > 256 * 1024 * 1024 {
        return Err(ParseError::Corrupt("file too large".into()));
    }
    let mut read = |start: usize, size: usize| -> Result<Vec<u8>, ParseError> {
        if start.checked_add(size).is_none_or(|end| end > length) {
            return Err(ParseError::Corrupt("file range".into()));
        }
        file.seek(SeekFrom::Start(start as u64))?;
        let mut bytes = vec![0; size];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    };
    // A CAB may reference hundreds of objects. Its block directory is immutable
    // within the owning preview context, so parse it once rather than per object.
    let layout = if let Some(layout) = cache.as_ref().and_then(|c| c.layouts.get(path)) {
        layout.clone()
    } else {
        let layout = std::sync::Arc::new(bundle_layout_with(length, &mut read)?);
        if let Some(cache) = cache.as_mut() {
            cache.layouts.insert(path.to_path_buf(), layout.clone());
        }
        layout
    };
    let (_, node_start, node_size) = layout
        .directory
        .iter()
        .find(|(n, _, _)| n == name)
        .ok_or_else(|| ParseError::Corrupt("missing node".into()))?;
    let size = size.unwrap_or(*node_size);
    if size > 128 * 1024 * 1024 || offset.checked_add(size).is_none_or(|end| end > *node_size) {
        return Err(ParseError::Corrupt("node range".into()));
    }
    let start = node_start
        .checked_add(offset)
        .ok_or_else(|| ParseError::Corrupt("node range".into()))?;
    let end = start
        .checked_add(size)
        .filter(|end| *end <= layout.total)
        .ok_or_else(|| ParseError::Corrupt("node range".into()))?;
    let mut out = Vec::with_capacity(size);
    let mut packed = layout.data_start;
    let mut unpacked = 0usize;
    for &(block_size, compressed, flags) in &layout.blocks {
        let block_end = unpacked
            .checked_add(block_size)
            .ok_or_else(|| ParseError::Corrupt("block range".into()))?;
        if unpacked < end && block_end > start {
            let key = (path.to_path_buf(), packed, compressed, block_size, flags);
            let cached = cache.as_ref().and_then(|c| {
                c.entries
                    .iter()
                    .find(|(k, _)| k == &key)
                    .map(|(_, v)| v.clone())
            });
            let bytes = if let Some(bytes) = cached {
                bytes
            } else {
                let bytes = decompress(
                    &read(packed, compressed)?,
                    block_size,
                    Compression::from_flags(flags as u32)?,
                )?;
                if bytes.len() != block_size {
                    return Err(ParseError::Corrupt("block size".into()));
                }
                let bytes: std::sync::Arc<[u8]> = bytes.into();
                if let Some(cache) = cache.as_mut().filter(|_| bytes.len() <= 192 * 1024 * 1024) {
                    while cache.bytes + bytes.len() > 192 * 1024 * 1024 {
                        if let Some((_, old)) = cache.entries.pop_front() {
                            cache.bytes -= old.len();
                        } else {
                            break;
                        }
                    }
                    cache.bytes += bytes.len();
                    cache.entries.push_back((key, bytes.clone()));
                }
                bytes
            };
            out.extend_from_slice(
                &bytes[start.saturating_sub(unpacked)..(end - unpacked).min(block_size)],
            );
        }
        packed = packed
            .checked_add(compressed)
            .filter(|end| *end <= length)
            .ok_or_else(|| ParseError::Corrupt("block range".into()))?;
        unpacked = block_end;
        if unpacked >= end {
            break;
        }
    }
    if out.len() != size {
        return Err(ParseError::Corrupt("node truncated".into()));
    }
    Ok(out)
}
/// Directory-only inspection shares header parsing without decoding any content blocks.
pub(crate) fn read_bundle_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ParseError> {
    let BundleLayout {
        directory,
        data_start,
        total,
        blocks,
    } = bundle_layout(bytes)?;
    let mut reader = BundleReader {
        bytes,
        pos: data_start,
    };
    let mut data = Vec::with_capacity(total);
    for (size, compressed, flags) in blocks {
        let block = decompress(
            reader.take(compressed)?,
            size,
            Compression::from_flags(flags as u32)?,
        )?;
        if block.len() != size {
            return Err(ParseError::Corrupt("block size".into()));
        }
        data.extend(block);
    }
    directory
        .into_iter()
        .map(|(name, offset, size)| {
            let end = offset
                .checked_add(size)
                .ok_or_else(|| ParseError::Corrupt("node overflow".into()))?;
            let bytes = data
                .get(offset..end)
                .ok_or_else(|| ParseError::Corrupt("node bounds".into()))?;
            Ok((name, bytes.to_vec()))
        })
        .collect()
}

#[cfg(test)]
#[path = "asset_bundle/rw_fixture_tests.rs"]
mod rw_fixture_tests;
