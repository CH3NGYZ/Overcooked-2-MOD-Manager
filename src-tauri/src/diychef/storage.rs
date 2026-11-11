//! Bounded prefer reads, dual-name revisions and same-directory replacement.
use super::{prefer::Document, target::child};
use crate::models::ChefSource;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const LIMIT: u64 = 2 * 1024 * 1024;
#[derive(Clone)]
pub(crate) struct Stored {
    pub document: Document,
    pub source: ChefSource,
    pub path: PathBuf,
}
fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("diychef.readFailed".into()),
    };
    let mut out = vec![];
    file.take(LIMIT + 1)
        .read_to_end(&mut out)
        .map_err(|_| "diychef.readFailed")?;
    if out.len() as u64 > LIMIT {
        return Err("diychef.tooLarge".into());
    }
    Ok(Some(out))
}
pub(crate) fn read(game: &Path, root: &Path, target_id: &str) -> Result<Stored, String> {
    let paths = [
        child(game, root, "prefer.txt")?,
        child(game, root, "prefer.txt.txt")?,
    ];
    let first = read_bytes(&paths[0])?;
    let second = read_bytes(&paths[1])?;
    let mut hash = Sha256::new();
    hash.update(target_id.as_bytes());
    for value in [&first, &second] {
        match value {
            Some(b) => {
                hash.update([1]);
                hash.update((b.len() as u64).to_le_bytes());
                hash.update(b)
            }
            None => hash.update([0]),
        }
    }
    let revision = format!("{:x}", hash.finalize());
    let index = if first.is_some() {
        0
    } else if second.is_some() {
        1
    } else {
        0
    };
    let bytes = if index == 0 {
        first.as_deref()
    } else {
        second.as_deref()
    };
    let document = Document::parse(bytes.unwrap_or_default())?;
    let source = ChefSource {
        state: if bytes.is_some() {
            "present"
        } else {
            "missing"
        }
        .into(),
        filename: if index == 0 {
            "prefer.txt"
        } else {
            "prefer.txt.txt"
        }
        .into(),
        revision,
        encoding: document.encoding.clone(),
        newline: document.newline.clone(),
    };
    Ok(Stored {
        document,
        source,
        path: paths[index].clone(),
    })
}
pub(crate) fn write(
    game: &Path,
    root: &Path,
    id: &str,
    expected: &str,
    bytes: &[u8],
    create: bool,
) -> Result<Stored, String> {
    if bytes.len() as u64 > LIMIT {
        return Err("diychef.tooLarge".into());
    }
    let current = read(game, root, id)?;
    if current.source.revision != expected || (create && current.source.state != "missing") {
        return Err("diychef.conflict".into());
    }
    let temp = child(
        game,
        root,
        &format!(".oc2-diychef-{}.tmp", uuid::Uuid::new_v4()),
    )?;
    let result = (|| {
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|_| "diychef.saveFailed")?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|_| "diychef.saveFailed")?;
        drop(f);
        if read(game, root, id)?.source.revision != expected {
            return Err("diychef.conflict".into());
        }
        if create {
            fs::hard_link(&temp, &current.path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    "diychef.conflict"
                } else {
                    "diychef.saveFailed"
                }
            })?;
        } else {
            fs::rename(&temp, &current.path).map_err(|_| "diychef.saveFailed")?;
        }
        read(game, root, id).map_err(|_| "diychef.writtenReadFailed".into())
    })();
    let _ = fs::remove_file(&temp);
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternate_name_and_revision_protect_external_changes() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        fs::write(p.join("prefer.txt.txt"), b"Chef_Black_cat\r\n").unwrap();
        let s = read(&p, &p, "id").unwrap();
        assert_eq!(s.source.filename, "prefer.txt.txt");
        fs::write(p.join("prefer.txt"), b"Chef_Beard").unwrap();
        assert_eq!(
            write(&p, &p, "id", &s.source.revision, b"X", false)
                .err()
                .unwrap(),
            "diychef.conflict"
        );
        assert_eq!(
            fs::read(p.join("prefer.txt.txt")).unwrap(),
            b"Chef_Black_cat\r\n"
        );
        fs::remove_file(p.join("prefer.txt")).unwrap();
        fs::remove_file(p.join("prefer.txt.txt")).unwrap();
        fs::remove_dir(p).unwrap();
    }
    #[test]
    fn initialize_does_not_clobber() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        let s = read(&p, &p, "id").unwrap();
        let n = write(&p, &p, "id", &s.source.revision, b"Chef_Black_cat\n", true).unwrap();
        assert_eq!(n.source.state, "present");
        assert!(write(&p, &p, "id", &s.source.revision, b"X", true).is_err());
        fs::remove_file(p.join("prefer.txt")).unwrap();
        fs::remove_dir(p).unwrap();
    }
    #[test]
    fn replace_reads_back_exact_bytes_and_external_edit_preserves_original() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        let original = b"Chef_Black_cat\r\n";
        fs::write(p.join("prefer.txt"), original).unwrap();
        let s = read(&p, &p, "id").unwrap();
        let replacement = b"\xef\xbb\xbfChef_Beard";
        let saved = write(&p, &p, "id", &s.source.revision, replacement, false).unwrap();
        assert_eq!(saved.source.encoding, "utf8bom");
        assert_eq!(fs::read(p.join("prefer.txt")).unwrap(), replacement);
        fs::write(p.join("prefer.txt"), b"external edit").unwrap();
        assert_eq!(
            write(&p, &p, "id", &saved.source.revision, original, false)
                .err()
                .unwrap(),
            "diychef.conflict"
        );
        assert_eq!(fs::read(p.join("prefer.txt")).unwrap(), b"external edit");
        assert_eq!(
            fs::read_dir(&p).unwrap().count(),
            1,
            "temporary files must be cleaned"
        );
        fs::remove_file(p.join("prefer.txt")).unwrap();
        fs::remove_dir(p).unwrap();
    }
}
