use crate::{diychef::target, recycle_bin};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

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
        source: &Path,
        parent: &Path,
        hat: bool,
    ) -> Result<(), String> {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("diychef.migrationFailed")?;
        let mut renamed = name.to_string();
        while target::child(game, parent, &renamed)?.exists() {
            renamed = format!("{name}_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        }
        let destination = target::child(game, parent, &renamed)?;
        self.directory(&destination)?;
        let relative = source
            .strip_prefix(game)
            .map_err(|_| "archive.pathEscape")?
            .to_string_lossy()
            .replace('\\', "/");
        for relative_file in recycle_bin::directory_files(game, &relative)? {
            let from = game.join(relative_file);
            let local = from
                .strip_prefix(source)
                .map_err(|_| "archive.pathEscape")?;
            let mut to = destination.join(local);
            if hat && renamed != name && local.components().count() == 1 {
                let filename = local.to_string_lossy();
                for prefix in ["t_", "m_"] {
                    if let Some(tail) = filename.strip_prefix(&format!("{prefix}{name}.")) {
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
        source: &Path,
        destination: &Path,
    ) -> Result<(), String> {
        let source = target::child(game, source, "Resources")?;
        if !source.exists() {
            return Ok(());
        }
        let destination = target::child(game, destination, "Resources")?;
        self.directory(&destination)?;
        let mut entries = fs::read_dir(&source)
            .map_err(|_| "diychef.migrationFailed")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "diychef.migrationFailed")?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if !entry
                .file_type()
                .map_err(|_| "diychef.migrationFailed")?
                .is_dir()
            {
                continue;
            }
            if entry.file_name() == "HATS" {
                let hats = target::child(game, &destination, "HATS")?;
                self.directory(&hats)?;
                let mut folders = fs::read_dir(entry.path())
                    .map_err(|_| "diychef.migrationFailed")?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "diychef.migrationFailed")?;
                folders.sort_by_key(|entry| entry.file_name());
                for folder in folders {
                    if folder
                        .file_type()
                        .map_err(|_| "diychef.migrationFailed")?
                        .is_dir()
                    {
                        self.folder(game, &folder.path(), &hats, true)?;
                    }
                }
            } else {
                self.folder(game, &entry.path(), &destination, false)?;
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
