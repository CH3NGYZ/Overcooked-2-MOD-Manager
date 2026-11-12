//! Bounded prefer/binding reads, shared revisions and transactional replacement.
mod bindings;
mod transaction;
use super::{prefer::Document, target::child};
use crate::models::{ChefSource, HatReference};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
const LIMIT: u64 = 2 * 1024 * 1024;
#[derive(Clone)]
pub(crate) struct Stored {
    pub document: Document,
    pub source: ChefSource,
    pub path: PathBuf,
    pub bindings_path: PathBuf,
    pub hat_bindings: BTreeMap<String, HatReference>,
    bindings_bytes: Option<Vec<u8>>,
    prefer_bytes: Option<Vec<u8>>,
}
fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = match std::fs::File::open(path) {
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
pub(crate) fn read(
    game: &Path,
    root: &Path,
    target_id: &str,
    directory: &Path,
) -> Result<Stored, String> {
    let paths = [
        child(game, root, "prefer.txt")?,
        child(game, root, "prefer.txt.txt")?,
    ];
    let bindings_path = bindings::path(directory, game, root)?;
    let bindings_bytes = read_bytes(&bindings_path)?;
    let hat_bindings = bindings::decode(bindings_bytes.as_deref())?;
    let first = read_bytes(&paths[0])?;
    let second = read_bytes(&paths[1])?;
    let mut hash = Sha256::new();
    hash.update(target_id.as_bytes());
    for value in [&first, &second, &bindings_bytes] {
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
        bindings_path,
        hat_bindings,
        prefer_bytes: bytes.map(<[u8]>::to_vec),
        bindings_bytes,
    })
}
pub(crate) fn write(
    game: &Path,
    root: &Path,
    id: &str,
    directory: &Path,
    expected: &str,
    bytes: &[u8],
    hat_bindings: &BTreeMap<String, HatReference>,
    create: bool,
) -> Result<Stored, String> {
    write_with_hook(
        game,
        root,
        id,
        directory,
        expected,
        bytes,
        hat_bindings,
        create,
        || Ok(()),
    )
}
#[allow(clippy::too_many_arguments)]
fn write_with_hook(
    game: &Path,
    root: &Path,
    id: &str,
    directory: &Path,
    expected: &str,
    bytes: &[u8],
    hat_bindings: &BTreeMap<String, HatReference>,
    create: bool,
    after_prefer: impl FnOnce() -> Result<(), String>,
) -> Result<Stored, String> {
    let encoded = bindings::encode(hat_bindings)?;
    if bytes.len() as u64 > LIMIT || encoded.len() as u64 > LIMIT {
        return Err("diychef.tooLarge".into());
    }
    let current = read(game, root, id, directory)?;
    if current.source.revision != expected || (create && current.source.state != "missing") {
        return Err("diychef.conflict".into());
    }
    transaction::replace_pair(
        (&current.path, bytes, current.prefer_bytes.as_deref()),
        (
            &current.bindings_path,
            &encoded,
            current.bindings_bytes.as_deref(),
        ),
        create,
        || {
            if read(game, root, id, directory)?.source.revision != expected {
                return Err("diychef.conflict".into());
            }
            Ok(())
        },
        || {
            after_prefer()?;
            if bindings::path(directory, game, root)? != current.bindings_path {
                return Err("diychef.conflict".into());
            }
            Ok(())
        },
        || {
            let saved = read(game, root, id, directory).map_err(|_| "diychef.writtenReadFailed")?;
            if saved.prefer_bytes.as_deref() != Some(bytes)
                || saved.bindings_bytes.as_deref() != Some(encoded.as_slice())
            {
                return Err("diychef.writtenReadFailed".into());
            }
            Ok(saved)
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn alternate_name_and_revision_protect_external_changes() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        fs::write(p.join("prefer.txt.txt"), b"Chef_Black_cat\r\n").unwrap();
        let s = read(&p, &p, "id", &p.join("diychef")).unwrap();
        assert_eq!(s.source.filename, "prefer.txt.txt");
        fs::write(p.join("prefer.txt"), b"Chef_Beard").unwrap();
        assert_eq!(
            write(
                &p,
                &p,
                "id",
                &p.join("diychef"),
                &s.source.revision,
                b"X",
                &BTreeMap::new(),
                false
            )
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
        if p.join("diychef").exists() {
            fs::remove_dir_all(p.join("diychef")).unwrap();
        }
        fs::remove_dir(p).unwrap();
    }
    #[test]
    fn initialize_does_not_clobber() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        let s = read(&p, &p, "id", &p.join("diychef")).unwrap();
        let n = write(
            &p,
            &p,
            "id",
            &p.join("diychef"),
            &s.source.revision,
            b"Chef_Black_cat\n",
            &BTreeMap::new(),
            true,
        )
        .unwrap();
        assert_eq!(n.source.state, "present");
        assert!(
            write(
                &p,
                &p,
                "id",
                &p.join("diychef"),
                &s.source.revision,
                b"X",
                &BTreeMap::new(),
                true
            )
            .is_err()
        );
        fs::remove_file(p.join("prefer.txt")).unwrap();
        if p.join("diychef").exists() {
            fs::remove_dir_all(p.join("diychef")).unwrap();
        }
        fs::remove_dir(p).unwrap();
    }
    #[test]
    fn replace_reads_back_exact_bytes_and_external_edit_preserves_original() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        let original = b"Chef_Black_cat\r\n";
        fs::write(p.join("prefer.txt"), original).unwrap();
        let s = read(&p, &p, "id", &p.join("diychef")).unwrap();
        let replacement = b"\xef\xbb\xbfChef_Beard";
        let saved = write(
            &p,
            &p,
            "id",
            &p.join("diychef"),
            &s.source.revision,
            replacement,
            &BTreeMap::new(),
            false,
        )
        .unwrap();
        assert_eq!(saved.source.encoding, "utf8bom");
        assert_eq!(fs::read(p.join("prefer.txt")).unwrap(), replacement);
        fs::write(p.join("prefer.txt"), b"external edit").unwrap();
        assert_eq!(
            write(
                &p,
                &p,
                "id",
                &p.join("diychef"),
                &saved.source.revision,
                original,
                &BTreeMap::new(),
                false
            )
            .err()
            .unwrap(),
            "diychef.conflict"
        );
        assert_eq!(fs::read(p.join("prefer.txt")).unwrap(), b"external edit");
        assert_eq!(
            fs::read_dir(&p).unwrap().count(),
            2,
            "temporary files must be cleaned; only prefer and the manager directory remain"
        );
        fs::remove_file(p.join("prefer.txt")).unwrap();
        if p.join("diychef").exists() {
            fs::remove_dir_all(p.join("diychef")).unwrap();
        }
        fs::remove_dir(p).unwrap();
    }
}

#[cfg(test)]
mod binding_tests;
