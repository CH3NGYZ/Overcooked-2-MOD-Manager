//! Manager-owned complete hat bindings, independent of emitted chef rows.
use crate::{game, models::HatReference};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct BindingFile {
    schema: u32,
    hat_bindings: BTreeMap<String, HatReference>,
}

pub(super) fn path(directory: &Path, game_root: &Path, mod_root: &Path) -> Result<PathBuf, String> {
    let parent = directory.parent().ok_or("diychef.readFailed")?;
    let mut hash = Sha256::new();
    for root in [game_root, mod_root] {
        let root = game::resolve_existing_ancestor(root)?;
        let text = root.to_string_lossy();
        hash.update((text.len() as u64).to_le_bytes());
        hash.update(text.as_bytes());
    }
    let filename = format!("{:x}.json", hash.finalize());
    let path = parent.join("diychef").join(&filename);
    // The system data root may be redirected. Fixed manager-owned children may
    // not redirect one target to another target's binding file, even internally.
    check_child(&parent.join("diychef"), true)?;
    check_child(&path, false)?;
    game::safe_game_path(parent, &format!("diychef/{filename}"))
}

pub(super) fn decode(bytes: Option<&[u8]>) -> Result<BTreeMap<String, HatReference>, String> {
    let Some(bytes) = bytes else {
        return Ok(BTreeMap::new());
    };
    let value: BindingFile = serde_json::from_slice(bytes).map_err(|_| "diychef.readFailed")?;
    if value.schema != 1 {
        return Err("diychef.readFailed".into());
    }
    Ok(value.hat_bindings)
}

pub(super) fn encode(hat_bindings: &BTreeMap<String, HatReference>) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&BindingFile {
        schema: 1,
        hat_bindings: hat_bindings.clone(),
    })
    .map_err(|_| "diychef.saveFailed".into())
}

#[cfg(windows)]
fn is_reparse_point(info: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    info.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse_point(_: &std::fs::Metadata) -> bool {
    false
}

fn check_child(path: &Path, directory: bool) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() || is_reparse_point(&info) => {
            Err("archive.pathEscape".into())
        }
        Ok(info)
            if if directory {
                info.is_dir()
            } else {
                info.is_file()
            } =>
        {
            Ok(())
        }
        Ok(_) => Err("diychef.readFailed".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("diychef.readFailed".into()),
    }
}
