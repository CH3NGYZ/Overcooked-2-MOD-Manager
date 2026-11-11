//! Reference-driven native assets. Bundle names are hints; object identities are discovered.
use crate::{
    asset_bundle::BundleBlockCache,
    unity_assets::{self, Source},
};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};
use unityfs::{SerializedFile, UnityValue as V};

pub(in crate::diychef::preview) type Map = BTreeMap<String, V>;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(in crate::diychef::preview) struct Ref {
    pub file: usize,
    pub id: i64,
}
pub(in crate::diychef::preview) struct Document {
    pub source: Source,
    pub name: String,
    pub file: SerializedFile,
}
#[derive(Clone)]
pub(in crate::diychef::preview) struct Avatar {
    pub name: String,
    pub prefabs: [V; 3],
    pub file: usize,
    pub colourisation_mode: i32,
}
pub(in crate::diychef::preview) struct Store {
    pub docs: Vec<Document>,
    pub blocks: BundleBlockCache,
    pub avatars: HashMap<String, Avatar>,
    pub default_template: String,
    pub player_colour: Option<super::palette::PlayerColour>,
    pub objects: HashMap<Ref, Arc<Map>>,
    pub sources: Vec<PathBuf>,
    pub trees: HashMap<Ref, Arc<crate::diychef::preview::assembly::Tree>>,
    pub parts: HashMap<(Ref, Ref, Option<Ref>), crate::diychef::preview::cache::Part>,
    pub part_bytes: usize,
    pub custom_models: HashMap<String, crate::diychef::preview::cache::Prepared>,
    pub custom_bytes: usize,
    index: Option<HashMap<String, (PathBuf, String)>>,
}

pub(in crate::diychef::preview) fn sources(game: &Path) -> Vec<PathBuf> {
    let mut result = crate::unity_assets::game_sources(game);
    for relative in [
        "Overcooked2_Data/resources.assets",
        "Overcooked2.app/Contents/Resources/Data/resources.assets",
        "Contents/Resources/Data/resources.assets",
    ] {
        let Ok(path) = crate::game::safe_game_path(game, relative) else {
            continue;
        };
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        if path.is_file() {
            result.push(path.clone());
            let stream = path.with_file_name("resources.assets.resS");
            if stream.is_file() {
                result.push(stream);
            }
        }
    }
    result.sort();
    result.dedup();
    result
}

impl Store {
    pub fn new(sources: Vec<PathBuf>) -> Result<Self, String> {
        let mut store = Self {
            docs: vec![],
            blocks: Default::default(),
            avatars: HashMap::new(),
            default_template: String::new(),
            player_colour: None,
            objects: HashMap::new(),
            sources,
            index: None,
            trees: HashMap::new(),
            parts: HashMap::new(),
            part_bytes: 0,
            custom_models: HashMap::new(),
            custom_bytes: 0,
        };
        // These ship on both PC layouts. Every hint is parsed and validated; no pathID is built in.
        const AVATARS: &[&str] = &[
            "bundle18",
            "bundle158",
            "bundle161",
            "bundle208",
            "bundle224",
            "bundle247",
            "bundle289",
            "bundle293",
            "bundle354",
            "bundle404",
            "bundle419",
            "bundle427",
        ];
        for path in store.sources.clone() {
            if path
                .file_name()
                .is_some_and(|n| AVATARS.iter().any(|x| n.to_string_lossy() == *x))
            {
                let _ = store.load_bundle(&path);
            }
        }
        let mut defaults = None;
        let mut first_colour = None;
        for file in 0..store.docs.len() {
            let ids: Vec<_> = store.docs[file]
                .file
                .objects
                .iter()
                .filter(|o| o.class_id == 114 && o.byte_size < 1024 * 1024)
                .map(|o| o.path_id)
                .collect();
            for id in ids {
                let r = Ref { file, id };
                let Ok(raw) = store.raw(r) else { continue };
                if let Some(a) = super::avatar::read(&raw, file) {
                    store.avatars.insert(a.name.clone(), a);
                }
                if let Some(v) = super::avatar::default_reference(&raw) {
                    defaults = Some((r, v));
                }
                if let Some(v) = super::avatar::first_colour(&raw) {
                    first_colour = Some((r, v));
                }
            }
        }
        if let Some((r, pointer)) = defaults {
            if let Ok(r) = store.resolve(r, &pointer) {
                if let Ok(raw) = store.raw(r) {
                    if let Some(a) = super::avatar::read(&raw, r.file) {
                        store.default_template = a.name;
                    }
                }
            }
        }
        if store.default_template.is_empty() || store.avatars.is_empty() {
            return Err("diychef.previewUnavailable".into());
        }
        let (source, value) = first_colour.ok_or("diychef.previewUnavailable")?;
        let source = store.resolve(source, &value)?;
        store.player_colour = Some(super::palette::read(&mut store, source)?);
        Ok(store)
    }
    pub fn object(&mut self, r: Ref) -> Result<Arc<Map>, String> {
        if let Some(v) = self.objects.get(&r) {
            return Ok(v.clone());
        }
        let d = self.docs.get(r.file).ok_or("diychef.previewUnavailable")?;
        let o = d
            .file
            .objects
            .iter()
            .find(|o| o.path_id == r.id)
            .ok_or("diychef.previewUnavailable")?;
        let V::Map(m) = unity_assets::object(&d.file, o, Some(&d.source), &mut self.blocks)? else {
            return Err("diychef.previewUnavailable".into());
        };
        let m = Arc::new(m);
        if o.class_id != 28 && o.class_id != 43 && self.objects.len() < 32768 {
            self.objects.insert(r, m.clone());
        }
        Ok(m)
    }
    pub fn raw(&mut self, r: Ref) -> Result<Vec<u8>, String> {
        let d = self.docs.get(r.file).ok_or("diychef.previewUnavailable")?;
        let o = d
            .file
            .objects
            .iter()
            .find(|o| o.path_id == r.id)
            .ok_or("diychef.previewUnavailable")?;
        if o.byte_size > 1024 * 1024 {
            return Err("diychef.previewLimit".into());
        }
        unity_assets::read_range(&d.source, o.byte_start, o.byte_size, &mut self.blocks)
    }
    fn add(&mut self, source: Source, name: String) -> Result<usize, String> {
        if let Some(i) = self
            .docs
            .iter()
            .position(|d| d.name.eq_ignore_ascii_case(&name))
        {
            return Ok(i);
        }
        let file = unity_assets::metadata(&source, &mut self.blocks)?;
        if self.docs.iter().map(|d| d.file.data.len()).sum::<usize>() + file.data.len()
            > 128 * 1024 * 1024
        {
            return Err("diychef.previewLimit".into());
        }
        self.docs.push(Document { source, name, file });
        Ok(self.docs.len() - 1)
    }
    fn load_bundle(&mut self, path: &Path) -> Result<(), String> {
        for node in crate::asset_bundle::bundle_file_names(path)
            .map_err(|_| "diychef.previewUnavailable")?
        {
            let name = node.rsplit('/').next().unwrap_or(&node).to_owned();
            if name.to_ascii_lowercase().ends_with(".ress") || name.ends_with(".resource") {
                continue;
            }
            self.add(Source::Bundle(path.into(), node), name)?;
        }
        Ok(())
    }
    pub fn resolve(&mut self, r: Ref, value: &V) -> Result<Ref, String> {
        let V::PPtr { file_id, path_id } = value else {
            return Err("diychef.previewUnavailable".into());
        };
        if *path_id == 0 || *file_id < 0 {
            return Err("diychef.previewUnavailable".into());
        }
        if *file_id == 0 {
            return Ok(Ref {
                file: r.file,
                id: *path_id,
            });
        }
        let external = self.docs[r.file]
            .file
            .externals
            .get(*file_id as usize - 1)
            .ok_or("diychef.previewUnavailable")?;
        let name = external
            .path_name
            .rsplit('/')
            .next()
            .ok_or("diychef.previewUnavailable")?
            .to_owned();
        let file = if let Some(i) = self
            .docs
            .iter()
            .position(|d| d.name.eq_ignore_ascii_case(&name))
        {
            i
        } else {
            self.ensure_index();
            let (path, node) = self
                .index
                .as_ref()
                .and_then(|i| i.get(&name.to_ascii_lowercase()))
                .cloned()
                .ok_or("diychef.previewUnavailable")?;
            if node.is_empty() {
                self.add(Source::File(path), name)?
            } else {
                self.add(Source::Bundle(path, node), name)?
            }
        };
        Ok(Ref { file, id: *path_id })
    }
    fn ensure_index(&mut self) {
        if self.index.is_some() {
            return;
        }
        let mut index = HashMap::new();
        for path in &self.sources {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.ends_with(".assets") {
                index.insert(name.to_ascii_lowercase(), (path.clone(), String::new()));
                continue;
            }
            if let Ok(nodes) = crate::asset_bundle::bundle_file_names(path) {
                for node in nodes {
                    index
                        .entry(
                            node.rsplit('/')
                                .next()
                                .unwrap_or(&node)
                                .to_ascii_lowercase(),
                        )
                        .or_insert_with(|| (path.clone(), node));
                }
            }
        }
        self.index = Some(index);
    }
    pub fn stream(
        &mut self,
        file: usize,
        name: &str,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>, String> {
        self.ensure_index();
        let basename = name
            .rsplit('/')
            .next()
            .ok_or("diychef.previewUnavailable")?;
        if let Some((path, node)) = self
            .index
            .as_ref()
            .and_then(|i| i.get(&basename.to_ascii_lowercase()))
            .cloned()
        {
            return unity_assets::read_range(
                &Source::Bundle(path, node),
                offset,
                size,
                &mut self.blocks,
            );
        }
        let Source::File(path) = &self.docs[file].source else {
            return Err("diychef.previewUnavailable".into());
        };
        if basename.contains(':') || basename.contains('\\') || basename == ".." {
            return Err("diychef.previewUnsafeResource".into());
        }
        let parent = path
            .parent()
            .ok_or("diychef.previewUnavailable")?
            .canonicalize()
            .map_err(|_| "diychef.previewUnavailable")?;
        let stream = parent
            .join(basename)
            .canonicalize()
            .map_err(|_| "diychef.previewUnavailable")?;
        if !stream.starts_with(parent) {
            return Err("diychef.previewUnsafeResource".into());
        }
        unity_assets::file_range(&stream, offset, size)
    }
}
