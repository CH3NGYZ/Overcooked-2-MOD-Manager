use crate::{diychef::target, recycle_bin};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub(super) struct ArchivedResource {
    pub relative: String,
    pub path: PathBuf,
}

struct ResourceFolder {
    name: String,
    files: BTreeMap<String, (PathBuf, PathBuf)>,
}

#[derive(Default)]
pub(super) struct Copies {
    files: Vec<(PathBuf, Vec<u8>)>,
    directories: Vec<PathBuf>,
}
impl Copies {
    fn directory(&mut self, path: &Path) -> Result<(), String> {
        if path.is_dir() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            self.directory(parent)?;
        }
        fs::create_dir(path).map_err(|_| "diychef.migrationFailed")?;
        self.directories.push(path.to_path_buf());
        Ok(())
    }
    fn copy(&mut self, from: &Path, to: &Path) -> Result<(), String> {
        self.directory(to.parent().ok_or("archive.pathEscape")?)?;
        let mut input = fs::File::open(from).map_err(|_| "diychef.migrationFailed")?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|_| "diychef.migrationFailed")?;
        // Track the file before copying so partial writes are rolled back too.
        self.files.push((to.to_path_buf(), vec![]));
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = input
                .read(&mut buffer)
                .map_err(|_| "diychef.migrationFailed")?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|_| "diychef.migrationFailed")?;
            hash.update(&buffer[..count]);
        }
        output.sync_all().map_err(|_| "diychef.migrationFailed")?;
        self.files.last_mut().unwrap().1 = hash.finalize().to_vec();
        Ok(())
    }
    fn folder(
        &mut self,
        game: &Path,
        folder: ResourceFolder,
        parent: &Path,
        hat: bool,
    ) -> Result<(), String> {
        let name = folder.name;
        let mut renamed = name.to_string();
        while target::child(game, parent, &renamed)?.exists() {
            renamed = format!("{name}_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        }
        let destination = target::child(game, parent, &renamed)?;
        self.directory(&destination)?;
        for (local, from) in folder.files.into_values() {
            let mut to = destination.join(&local);
            if hat && local.components().count() == 1 {
                let filename = local.to_string_lossy();
                for prefix in ["t_", "m_"] {
                    let expected = format!("{prefix}{name}.");
                    if let Some(tail) = filename.get(expected.len()..).filter(|_| {
                        filename
                            .get(..expected.len())
                            .is_some_and(|head| head.eq_ignore_ascii_case(&expected))
                    }) {
                        to = destination.join(format!("{prefix}{renamed}.{tail}"));
                    }
                }
            }
            self.copy(&from, &to)?;
        }
        Ok(())
    }
    pub(super) fn import(
        &mut self,
        game: &Path,
        live_resources: Option<&Path>,
        archived: &[ArchivedResource],
        destination: &Path,
    ) -> Result<(), String> {
        let destination = target::child(game, destination, "Resources")?;
        let mut files = Vec::<(String, PathBuf)>::new();
        if let Some(source) = live_resources {
            let source = target::child(game, source, "Resources")?;
            if source.is_dir() {
                let relative = source
                    .strip_prefix(game)
                    .map_err(|_| "archive.pathEscape")?
                    .to_string_lossy()
                    .replace('\\', "/");
                for file in recycle_bin::directory_files(game, &relative)? {
                    let from = game.join(&file);
                    let local = from
                        .strip_prefix(&source)
                        .map_err(|_| "archive.pathEscape")?
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.push((local, from));
                }
            }
        }
        for resource in archived {
            files.push((resource.relative.clone(), resource.path.clone()));
        }

        let mut chefs = BTreeMap::<String, ResourceFolder>::new();
        let mut hats = BTreeMap::<String, ResourceFolder>::new();
        // Live files precede archives (newest first). Preserve that priority for
        // folder spelling and file contents when directory casing has changed.
        for (relative, source) in files {
            let mut parts = Path::new(&relative).components();
            let Some(first) = parts.next().and_then(|part| part.as_os_str().to_str()) else {
                continue;
            };
            let rest = parts
                .map(|part| part.as_os_str().to_owned())
                .collect::<Vec<_>>();
            let (folders, name, local): (&mut BTreeMap<String, ResourceFolder>, &str, PathBuf) =
                if first.eq_ignore_ascii_case("HATS") {
                    let Some((name, tail)) = rest.split_first() else {
                        continue;
                    };
                    let Some(name) = name.to_str() else {
                        return Err("diychef.migrationFailed".into());
                    };
                    if tail.is_empty() {
                        continue;
                    }
                    (&mut hats, name, tail.iter().collect())
                } else {
                    if rest.is_empty() {
                        continue;
                    }
                    (&mut chefs, first, rest.iter().collect())
                };
            let folder = folders
                .entry(name.to_lowercase())
                .or_insert_with(|| ResourceFolder {
                    name: name.to_string(),
                    files: BTreeMap::new(),
                });
            folder
                .files
                .entry(local.to_string_lossy().replace('\\', "/").to_lowercase())
                .or_insert((local, source));
        }
        if chefs.is_empty() && hats.is_empty() {
            return Ok(());
        }
        self.directory(&destination)?;
        for folder in chefs.into_values() {
            self.folder(game, folder, &destination, false)?;
        }
        if !hats.is_empty() {
            let hats_root = target::child(game, &destination, "HATS")?;
            for folder in hats.into_values() {
                self.folder(game, folder, &hats_root, true)?;
            }
        }
        Ok(())
    }
    pub(super) fn rollback(&self) -> bool {
        let mut complete = true;
        for (file, expected) in self.files.iter().rev() {
            if !file.exists() {
                continue;
            }
            if expected.is_empty() || matches_digest(file, expected) {
                complete &= fs::remove_file(file).is_ok();
            } else {
                complete = false;
            }
        }
        for directory in self.directories.iter().rev() {
            if directory.exists() {
                complete &= fs::remove_dir(directory).is_ok();
            }
        }
        complete
    }
}

fn matches_digest(path: &Path, expected: &[u8]) -> bool {
    let check = || -> std::io::Result<bool> {
        let mut input = fs::File::open(path)?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(hash.finalize().as_slice() == expected)
    };
    check().unwrap_or(false)
}
