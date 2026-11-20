use super::files::{create_log, ensure_directory, open_log, sorted_logs};
use super::{LogKind, unavailable};
use crate::display_path::display_path;
use crate::models::{LogFileEntry, RustLog};
use same_file::Handle;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const KEEP: usize = 5;
pub(super) const MAX_RUST_BYTES: u64 = 8 * 1024 * 1024;

struct ListedLog {
    id: String,
    path: PathBuf,
    identity: Handle,
}

struct Stream {
    path: PathBuf,
    writer: File,
    length: u64,
    listed: Vec<ListedLog>,
}

pub(super) struct History {
    directory: PathBuf,
    streams: [Stream; 3],
    rust_limit: u64,
}

impl History {
    pub(super) fn start(directory: &Path, rust_limit: u64) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        ensure_directory(directory)?;
        let streams = LogKind::ALL.map(|kind| -> io::Result<Stream> {
            let (path, writer) = create_log(directory, kind)?;
            let stream = Stream {
                path,
                writer,
                length: 0,
                listed: Vec::new(),
            };
            Self::prune(directory, kind, &stream.path)?;
            Ok(stream)
        });
        let [rust, frontend, crash] = streams;
        Ok(Self {
            directory: directory.to_path_buf(),
            streams: [rust?, frontend?, crash?],
            rust_limit,
        })
    }

    fn prune(directory: &Path, kind: LogKind, active: &Path) -> io::Result<()> {
        for path in sorted_logs(directory, kind, active)?.into_iter().skip(KEEP) {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub(super) fn list(&mut self, kind: LogKind) -> io::Result<Vec<LogFileEntry>> {
        let stream = &mut self.streams[kind as usize];
        let mut listed = Vec::new();
        for path in sorted_logs(&self.directory, kind, &stream.path)?
            .into_iter()
            .take(KEEP)
        {
            let identity = Handle::from_file(open_log(&path, false)?)?;
            let id = stream
                .listed
                .iter()
                .find(|old| old.path == path && old.identity == identity)
                .map_or_else(|| uuid::Uuid::new_v4().to_string(), |old| old.id.clone());
            listed.push(ListedLog { id, path, identity });
        }
        let entries = listed
            .iter()
            .map(|entry| LogFileEntry {
                id: entry.id.clone(),
                name: entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            })
            .collect();
        stream.listed = listed;
        Ok(entries)
    }

    fn resolve(&self, kind: LogKind, id: &str, writable: bool) -> io::Result<(&Path, File)> {
        let entry = self.streams[kind as usize]
            .listed
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(unavailable)?;
        ensure_directory(&self.directory)?;
        let file = open_log(&entry.path, writable)?;
        if Handle::from_file(file.try_clone()?)? != entry.identity {
            return Err(unavailable());
        }
        Ok((&entry.path, file))
    }

    pub(super) fn read(&self, kind: LogKind, id: &str) -> io::Result<RustLog> {
        let (path, file) = self.resolve(kind, id, false)?;
        Self::tail(path, file)
    }

    pub(super) fn clear(&mut self, kind: LogKind, id: &str) -> io::Result<RustLog> {
        let (path, file) = self.resolve(kind, id, true)?;
        file.set_len(0)?;
        let active = self.streams[kind as usize].path == path;
        let log = Self::tail(path, file)?;
        if active {
            self.streams[kind as usize].length = 0;
        }
        Ok(log)
    }

    fn tail(path: &Path, mut file: File) -> io::Result<RustLog> {
        const MAX_BYTES: u64 = 512 * 1024;
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(MAX_BYTES)))?;
        let mut bytes = Vec::with_capacity(length.min(MAX_BYTES) as usize);
        file.take(MAX_BYTES).read_to_end(&mut bytes)?;
        Ok(RustLog {
            path: display_path(path),
            content: String::from_utf8_lossy(&bytes).into_owned(),
        })
    }

    pub(super) fn selected_path(&self, kind: LogKind, id: &str) -> io::Result<PathBuf> {
        self.resolve(kind, id, false)
            .map(|(path, _)| path.to_path_buf())
    }

    pub(super) fn append(&mut self, kind: LogKind, bytes: &[u8]) -> io::Result<()> {
        let stream = &mut self.streams[kind as usize];
        if kind == LogKind::Rust
            && stream.length > 0
            && stream.length.saturating_add(bytes.len() as u64) > self.rust_limit
        {
            let (path, writer) = create_log(&self.directory, kind)?;
            stream.path = path;
            stream.writer = writer;
            stream.length = 0;
            if let Err(error) = Self::prune(&self.directory, kind, &stream.path) {
                // Still keep the ongoing diagnostics if an archived file cannot be removed.
                eprintln!("Could not prune log history: {error}");
            }
        }
        stream.writer.write_all(bytes)?;
        stream.length = stream.length.saturating_add(bytes.len() as u64);
        Ok(())
    }
}
