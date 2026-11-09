//! 按引用读取关卡声明；图片/配方与普通扫描分离。
use crate::asset_bundle::{int_of, read_bundle_entries, string_of, try_read_serialized};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use unityfs::{Reader, SerializedFile, UnityValue as V};
mod menu;
mod recipes;
pub(crate) mod sources;
mod texture;

type Map = BTreeMap<String, V>;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Level {
    pub name: String,
    pub name_zh: String,
    pub scene_name: String,
    pub min_order_count: i32,
    pub max_order_count: i32,
    pub dependencies: Vec<String>,
    pub scene_exists: bool,
    pub recipe_count: usize,
    pub active_recipe_count: usize,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MenuIcon {
    pub name: String,
    pub data_url: Option<String>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MenuTile {
    pub pictures: Vec<MenuIcon>,
    pub modifiers: Vec<MenuIcon>,
    pub children: Vec<usize>,
    pub large: bool,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Recipe {
    pub name: String,
    pub score: Option<i32>,
    pub active: bool,
    pub resolved: bool,
    pub ingredients: Vec<String>,
    pub icon: Option<String>,
    pub source: String,
    pub error: Option<String>,
    pub tiles: Vec<MenuTile>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Picture {
    pub data_url: Option<String>,
    pub width: u32,
    pub height: u32,
    pub state: String,
}
#[derive(Clone, Copy, Debug)]
struct Ref {
    file: usize,
    id: i64,
}
struct Document {
    name: String,
    source: String,
    file: SerializedFile,
}
pub struct Context {
    documents: Vec<Document>,
    raw: HashMap<String, Vec<u8>>,
    streams: HashMap<String, (PathBuf, String)>,
    objects: HashMap<(usize, i64), Map>,
    images: HashMap<(usize, i64, u32), (String, u32, u32)>,
    textures: HashMap<(usize, i64), std::sync::Arc<image::RgbaImage>>,
    recipe_cache: HashMap<(usize, i64, bool), Recipe>,
    menu_budget: usize,
    recipe_budget: usize,
    assets: HashMap<String, (Ref, V)>,
    indexed_docs: std::collections::HashSet<usize>,
    loaded: std::collections::HashSet<PathBuf>,
    failed: std::collections::HashSet<PathBuf>,
    external_index: Option<HashMap<String, PathBuf>>,
    pub levels: Vec<Level>,
    level_refs: Vec<Ref>,
    local_root: PathBuf,
    game_root: PathBuf,
}
fn failure() -> String {
    "assetBundle.previewUnavailable".into()
}
fn array<'a>(map: &'a Map, key: &str) -> &'a [V] {
    match map.get(key) {
        Some(V::Array(a)) => a,
        _ => &[],
    }
}
fn reference(value: &V, file: usize, docs: &[Document]) -> Option<Ref> {
    let V::PPtr { file_id, path_id } = value else {
        return None;
    };
    if *path_id == 0 {
        return None;
    }
    let target = if *file_id == 0 {
        file
    } else {
        let external = docs
            .get(file)?
            .file
            .externals
            .get((*file_id as usize).checked_sub(1)?)?;
        let name = external.path_name.rsplit('/').next()?;
        docs.iter()
            .position(|d| d.name.eq_ignore_ascii_case(name))?
    };
    Some(Ref {
        file: target,
        id: *path_id,
    })
}
impl Context {
    pub fn load(bytes: &[u8], local_root: &Path, game_root: &Path) -> Result<Self, String> {
        let mut c = Self {
            documents: vec![],
            raw: HashMap::new(),
            streams: HashMap::new(),
            objects: HashMap::new(),
            images: HashMap::new(),
            textures: HashMap::new(),
            menu_budget: 0,
            recipe_cache: HashMap::new(),
            recipe_budget: 0,
            assets: HashMap::new(),
            indexed_docs: Default::default(),
            loaded: Default::default(),
            failed: Default::default(),
            external_index: None,
            levels: vec![],
            level_refs: vec![],
            local_root: local_root.into(),
            game_root: game_root.into(),
        };
        c.add_bundle(bytes, "info")?;
        let mut set = None;
        // Metadata only: never walk arbitrary scene objects or texture byte arrays here.
        for i in 0..c.documents.len() {
            let ids: Vec<_> = c.documents[i]
                .file
                .objects
                .iter()
                .filter(|o| o.class_id == 114 && o.byte_size <= 1024 * 1024)
                .map(|o| o.path_id)
                .collect();
            for id in ids {
                let r = Ref { file: i, id };
                if let Ok(m) = c.object(r) {
                    if m.contains_key("levelSetName") {
                        set = Some((i, m));
                        break;
                    }
                }
            }
            if set.is_some() {
                break;
            }
        }
        let (file, set) = set.ok_or_else(failure)?;
        if array(&set, "levelInfos").len() > 4096 {
            return Err(failure());
        }
        for v in array(&set, "levelInfos") {
            let r = reference(v, file, &c.documents).ok_or_else(failure)?;
            let m = c.object(r)?;
            let scene = string_of(&m, "sceneName");
            let count = array(&m, "recipes").len();
            if count > 4096 {
                return Err(failure());
            }
            let limit = int_of(&m, "debugRecipeCount");
            c.levels.push(Level {
                name: string_of(&m, "levelName"),
                name_zh: string_of(&m, "levelNameZH"),
                scene_exists: c.safe_file(&c.local_root, &scene).is_some(),
                scene_name: scene,
                min_order_count: int_of(&m, "minOrderCount"),
                max_order_count: int_of(&m, "maxOrderCount"),
                dependencies: array(&m, "dependencies")
                    .iter()
                    .filter_map(|v| {
                        if let V::String(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
                recipe_count: count,
                active_recipe_count: if limit == 0 {
                    count
                } else {
                    count.min(limit.max(0) as usize)
                },
            });
            c.level_refs.push(r);
        }
        Ok(c)
    }
    fn add_bundle(&mut self, bytes: &[u8], source: &str) -> Result<(), String> {
        let entries = read_bundle_entries(bytes).map_err(|e| e.to_string())?;
        self.add_entries(entries, source)
    }
    fn add_entries(&mut self, entries: Vec<(String, Vec<u8>)>, source: &str) -> Result<(), String> {
        let retained: usize = self
            .documents
            .iter()
            .map(|d| d.file.data.len())
            .sum::<usize>()
            + self.raw.values().map(Vec::len).sum::<usize>();
        if retained.saturating_add(entries.iter().map(|(_, b)| b.len()).sum()) > 512 * 1024 * 1024 {
            return Err(failure());
        }
        for (name, bytes) in entries {
            let basename = name.rsplit('/').next().unwrap_or(&name).to_string();
            if self.raw.contains_key(&basename) || self.documents.iter().any(|d| d.name == basename)
            {
                continue;
            }
            // resource streams aren't SerializedFiles; never feed arbitrary bytes to its panic-prone parser.
            if bytes.len() >= 20
                && matches!(u32::from_be_bytes(bytes[8..12].try_into().unwrap()), 9..=30)
            {
                let mut reader = Reader::new(bytes, unityfs::UnityVersion::default());
                let file = try_read_serialized(&mut reader).ok_or_else(failure)?;
                self.documents.push(Document {
                    name: basename,
                    source: source.into(),
                    file,
                });
            } else {
                self.raw.insert(basename, bytes);
            }
        }
        Ok(())
    }
    fn object(&mut self, r: Ref) -> Result<Map, String> {
        if let Some(m) = self.objects.get(&(r.file, r.id)) {
            return Ok(m.clone());
        }
        let file = &self.documents.get(r.file).ok_or_else(failure)?.file;
        let object = file
            .objects
            .iter()
            .find(|o| o.path_id == r.id)
            .ok_or_else(failure)?;
        if object.byte_size > 128 * 1024 * 1024 {
            return Err(failure());
        }
        let mut reader = Reader {
            data: file.data.clone(),
            pos: 0,
            unity_version: file.unity_version.clone(),
            endian: file.endian,
        };
        let v = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            object.read_typetree(&mut reader, &[])
        }))
        .map_err(|_| failure())?
        .map_err(|_| failure())?;
        let V::Map(m) = v else {
            return Err(failure());
        };
        // Avoid retaining decoded texture payloads twice.
        if object.class_id != 28 {
            self.objects.insert((r.file, r.id), m.clone());
        }
        Ok(m)
    }
    fn safe_file(&self, root: &Path, relative: &str) -> Option<PathBuf> {
        if relative.is_empty()
            || relative.contains(':')
            || relative.contains('\\')
            || Path::new(relative)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return None;
        }
        let base = root.canonicalize().ok()?;
        let p = base.join(relative).canonicalize().ok()?;
        (p.starts_with(base) && p.is_file()).then_some(p)
    }
    fn ensure_external(&mut self, r: Ref, v: &V) {
        let V::PPtr { file_id, .. } = v else {
            return;
        };
        if *file_id <= 0 {
            return;
        }
        let Some(e) = self.documents[r.file]
            .file
            .externals
            .get(*file_id as usize - 1)
        else {
            return;
        };
        let name = e.path_name.rsplit('/').next().unwrap_or("").to_string();
        if self
            .documents
            .iter()
            .any(|d| d.name.eq_ignore_ascii_case(&name))
        {
            return;
        }
        // Read only the directory tables first: unrelated commonW* texture bundles
        // must not be decompressed just to discover which CAB owns this pointer.
        if self.external_index.is_none() {
            let mut index = HashMap::new();
            for path in sources::common_sources(&self.game_root, &self.local_root)
                .into_iter()
                .chain(sources::game_sources(&self.game_root))
            {
                if let Ok(names) = crate::asset_bundle::bundle_file_names(&path) {
                    for name in names {
                        index
                            .entry(name.to_ascii_lowercase())
                            .or_insert_with(|| path.clone());
                    }
                }
            }
            self.external_index = Some(index);
        }
        if let Some(path) = self
            .external_index
            .as_ref()
            .and_then(|index| index.get(&name.to_ascii_lowercase()))
            .cloned()
        {
            let _ = self.load_file(&path);
        }
    }
    fn load_file(&mut self, path: &Path) -> Result<(), String> {
        if self.loaded.contains(path) {
            return Ok(());
        }
        if self.failed.contains(path) {
            return Err(failure());
        }
        let result = (|| {
            let names = crate::asset_bundle::bundle_file_names(path).map_err(|_| failure())?;
            let mut entries = Vec::new();
            let mut streams = Vec::new();
            for name in names {
                if name.to_ascii_lowercase().ends_with(".ress")
                    || name.to_ascii_lowercase().ends_with(".resource")
                {
                    streams.push(name);
                } else {
                    entries.push((
                        name.clone(),
                        crate::asset_bundle::read_bundle_file_range(path, &name, 0, None)
                            .map_err(|_| failure())?,
                    ));
                }
            }
            self.add_entries(
                entries,
                &path.file_name().unwrap_or_default().to_string_lossy(),
            )?;
            for name in streams {
                self.streams.insert(
                    name.rsplit('/').next().unwrap_or(&name).into(),
                    (path.into(), name),
                );
            }
            Ok(())
        })();
        if result.is_ok() {
            self.loaded.insert(path.into());
        } else {
            self.failed.insert(path.into());
        }
        result
    }

    pub fn refresh_files(&mut self) {
        for i in 0..self.levels.len() {
            self.levels[i].scene_exists = self
                .safe_file(&self.local_root, &self.levels[i].scene_name)
                .is_some();
        }
    }

    fn resolve(&mut self, r: Ref, v: &V) -> Result<Ref, String> {
        self.ensure_external(r, v);
        reference(v, r.file, &self.documents).ok_or_else(failure)
    }
    pub fn picture(&mut self, index: usize) -> Result<Picture, String> {
        let r = *self.level_refs.get(index).ok_or_else(failure)?;
        let m = self.object(r)?;
        let Some(v) = m.get("screenshot") else {
            return Ok(Picture::missing());
        };
        if matches!(v, V::PPtr { path_id: 0, .. }) {
            return Ok(Picture::missing());
        }
        let image = self.resolve(r, v).and_then(|r| self.image(r, 1280));
        Ok(match image {
            Ok((data_url, width, height)) => Picture {
                data_url: Some(data_url),
                width,
                height,
                state: "ready".into(),
            },
            Err(_) => Picture {
                data_url: None,
                width: 0,
                height: 0,
                state: "error".into(),
            },
        })
    }
    pub fn recipes(&mut self, index: usize) -> Result<Vec<Recipe>, String> {
        self.recipe_budget = 4096;
        self.menu_budget = 512;
        let r = *self.level_refs.get(index).ok_or_else(failure)?;
        let m = self.object(r)?;
        let active = self.levels[index].active_recipe_count;
        let mut icon_bytes = 0usize;
        Ok(array(&m, "recipes")
            .iter()
            .enumerate()
            .map(|(i, v)| {
                self.menu_budget = 512;
                let result = self.resolve(r, v).and_then(|r| self.recipe(r, 0));
                match result {
                    Ok(mut recipe) => {
                        recipe.active = i < active;
                        icon_bytes += recipe.icon.as_ref().map_or(0, String::len);
                        if icon_bytes > 16 * 1024 * 1024 {
                            recipe.icon = None;
                        }
                        for tile in &mut recipe.tiles {
                            for icon in tile.pictures.iter_mut().chain(&mut tile.modifiers) {
                                icon_bytes += icon.data_url.as_ref().map_or(0, String::len);
                                if icon_bytes > 16 * 1024 * 1024 {
                                    icon.data_url = None;
                                }
                            }
                        }
                        recipe
                    }
                    Err(_) => Recipe {
                        name: String::new(),
                        score: None,
                        active: i < active,
                        resolved: false,
                        ingredients: vec![],
                        icon: None,
                        source: String::new(),
                        error: Some(failure()),
                        tiles: vec![],
                    },
                }
            })
            .collect())
    }
}
impl Picture {
    fn missing() -> Self {
        Self {
            data_url: None,
            width: 0,
            height: 0,
            state: "missing".into(),
        }
    }
}

/// Bounds apply before allocating file bytes, including external bundles.
pub(crate) fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| failure())?;
    if file.metadata().map_err(|_| failure())?.len() > 256 * 1024 * 1024 {
        return Err(failure());
    }
    let mut bytes = Vec::new();
    file.take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure())?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(failure());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires local real map and game fixtures"]
    fn real_preview_acceptance() {
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let report: serde_json::Value = serde_json::from_slice(
            &std::fs::read("../.tmp/map-preview-inspection.json")
                .or_else(|_| std::fs::read(".tmp/map-preview-inspection.json"))
                .expect("inspection fixture required"),
        )
        .unwrap();
        let mut total = 0;
        let mut ready = 0;
        let mut recipes = 0;
        let mut resolved = 0;
        for sample in report.as_array().unwrap() {
            let relative = sample["info"].as_str().unwrap();
            let mut path = PathBuf::from(relative);
            if !path.exists() {
                path = Path::new("..").join(relative);
            }
            let bytes = read_limited(&path).expect("real info required");
            let start = std::time::Instant::now();
            let mut c = Context::load(&bytes, path.parent().unwrap(), game).unwrap();
            assert_eq!(c.levels.len(), sample["levels"].as_array().unwrap().len());
            for i in 0..c.levels.len() {
                let picture = c.picture(i).unwrap();
                assert_ne!(picture.state, "error", "{} level {}", path.display(), i);
                if picture.state == "ready" {
                    ready += 1;
                    if total == 0 {
                        use base64::Engine;
                        let png = base64::engine::general_purpose::STANDARD
                            .decode(picture.data_url.unwrap().split(',').nth(1).unwrap())
                            .unwrap();
                        let out = if Path::new(".tmp").exists() {
                            ".tmp/preview-rust-first.png"
                        } else {
                            "../.tmp/preview-rust-first.png"
                        };
                        std::fs::write(out, png).unwrap();
                    }
                }
                let entries = c.recipes(i).unwrap();
                recipes += entries.len();
                resolved += entries.iter().filter(|r| r.resolved).count();
                total += 1;
                eprintln!(
                    "{} {} picture={} recipes={}/{}",
                    path.display(),
                    i,
                    picture.state,
                    entries.iter().filter(|r| r.resolved).count(),
                    entries.len()
                );
            }
            eprintln!("sample elapsed {:?}", start.elapsed());
        }
        assert_eq!(total, 17);
        assert_eq!(ready, 13);
        eprintln!("ACCEPTANCE levels={total} pictures={ready} recipes={resolved}/{recipes}");
    }
    pub(super) fn empty_context() -> Context {
        Context {
            documents: vec![],
            raw: HashMap::new(),
            streams: HashMap::new(),
            objects: HashMap::new(),
            images: HashMap::new(),
            textures: HashMap::new(),
            menu_budget: 0,
            recipe_cache: HashMap::new(),
            recipe_budget: 0,
            assets: HashMap::new(),
            indexed_docs: Default::default(),
            loaded: Default::default(),
            failed: Default::default(),
            external_index: None,
            levels: vec![],
            level_refs: vec![],
            local_root: PathBuf::new(),
            game_root: PathBuf::new(),
        }
    }
    #[test]
    fn rejects_unsafe_paths_and_missing_sources() {
        let c = empty_context();
        for p in [
            "../escape",
            "/absolute",
            "C:/game",
            "archive:/CAB",
            "a\\b",
            "",
        ] {
            assert!(c.safe_file(Path::new("."), p).is_none());
        }
        assert!(Context::load(b"invalid", Path::new("."), Path::new(".")).is_err());
    }
}

#[cfg(test)]
mod real_regressions {
    use super::*;
    fn fixture() -> Vec<u8> {
        read_limited(Path::new("../.tmp/info_rw.bin"))
            .or_else(|_| read_limited(Path::new(".tmp/info_rw.bin")))
            .expect("real RW fixture required")
    }
    fn lz4_bundle(bytes: &[u8]) -> Vec<u8> {
        let entries = read_bundle_entries(bytes).unwrap();
        let data: Vec<u8> = entries
            .iter()
            .flat_map(|(_, b)| b.iter().copied())
            .collect();
        let compressed = lz4_flex::block::compress(&data);
        let mut table = vec![0u8; 16];
        table.extend(1u32.to_be_bytes());
        table.extend((data.len() as u32).to_be_bytes());
        table.extend((compressed.len() as u32).to_be_bytes());
        table.extend(2u16.to_be_bytes());
        table.extend((entries.len() as u32).to_be_bytes());
        let mut offset = 0i64;
        for (name, b) in entries {
            table.extend(offset.to_be_bytes());
            table.extend((b.len() as i64).to_be_bytes());
            table.extend(0u32.to_be_bytes());
            table.extend(name.as_bytes());
            table.push(0);
            offset += b.len() as i64;
        }
        let mut output = b"UnityFS\0".to_vec();
        output.extend(6u32.to_be_bytes());
        output.extend(b"5.x.x\0");
        output.extend(b"5.6.7f1\0");
        let size_offset = output.len();
        output.extend(0u64.to_be_bytes());
        output.extend((table.len() as u32).to_be_bytes());
        output.extend((table.len() as u32).to_be_bytes());
        output.extend(0x40u32.to_be_bytes());
        output.extend(table);
        output.extend(compressed);
        let len = output.len() as u64;
        output[size_offset..size_offset + 8].copy_from_slice(&len.to_be_bytes());
        output
    }
    #[test]
    #[ignore = "real info fixture required"]
    fn lz4_renamed_directory_and_broken_reference_regression() {
        let bytes = fixture();
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let root = std::env::temp_dir().join(format!(
            "oc2-preview-real-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let before = root.join("before");
        let after = root.join("renamed");
        std::fs::create_dir_all(&before).unwrap();
        std::fs::write(before.join("info"), &bytes).unwrap();
        std::fs::write(before.join("s_rw_1"), b"scene-presence-fixture").unwrap();
        let mut c = Context::load(&bytes, &before, game).unwrap();
        assert!(c.levels[0].scene_exists);
        let first = c.picture(0).unwrap();
        assert_eq!(first.state, "ready");
        assert_eq!((first.width, first.height), (524, 308));
        let lz4 = lz4_bundle(&bytes);
        // Seeked file ranges must equal the existing in-memory parser for both codecs.
        for (name, source) in [("lzma", &bytes), ("lz4", &lz4)] {
            let path = before.join(name);
            std::fs::write(&path, source).unwrap();
            for (node, data) in read_bundle_entries(source).unwrap() {
                let n = data.len().min(127);
                let offset = data.len().saturating_sub(n) / 2;
                assert_eq!(
                    crate::asset_bundle::read_bundle_file_range(&path, &node, offset, Some(n))
                        .unwrap(),
                    data[offset..offset + n]
                );
                assert!(
                    crate::asset_bundle::read_bundle_file_range(&path, &node, data.len(), Some(1))
                        .is_err()
                );
            }
        }
        let mut converted = Context::load(&lz4, &before, game).unwrap();
        assert_eq!(converted.picture(0).unwrap().data_url, first.data_url);
        assert_eq!(
            crate::asset_bundle::parse_level_set(&lz4).unwrap(),
            crate::asset_bundle::parse_level_set(&bytes).unwrap()
        );
        std::fs::rename(&before, &after).unwrap();
        let mut renamed = Context::load(&bytes, &after, game).unwrap();
        assert!(renamed.levels[0].scene_exists);
        assert_eq!(renamed.picture(0).unwrap().data_url, first.data_url);
        let r = renamed.level_refs[0];
        let mut map = renamed.object(r).unwrap();
        map.insert(
            "screenshot".into(),
            V::PPtr {
                file_id: 0,
                path_id: i64::MAX,
            },
        );
        map.insert(
            "recipes".into(),
            V::Array(vec![V::PPtr {
                file_id: 0,
                path_id: i64::MAX,
            }]),
        );
        renamed.objects.insert((r.file, r.id), map);
        assert_eq!(renamed.picture(0).unwrap().state, "error");
        let recipes = renamed.recipes(0).unwrap();
        assert_eq!(recipes.len(), 1);
        assert!(!recipes[0].resolved);
        assert!(renamed.picture(usize::MAX).is_err());
        std::fs::remove_file(after.join("s_rw_1")).unwrap();
        renamed.refresh_files();
        assert!(!renamed.levels[0].scene_exists);
        std::fs::remove_dir_all(&root).unwrap();
        eprintln!("LZMA/LZ4, rename, missing scene, corrupt pointer and invalid index: passed");
    }
}
