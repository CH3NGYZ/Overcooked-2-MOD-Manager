//! Two bounded same-directory replacements with rollback material prepared first.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

struct Replacement {
    path: PathBuf,
    staged: PathBuf,
    backup: Option<PathBuf>,
}
impl Replacement {
    fn prepare(path: &Path, bytes: &[u8], original: Option<&[u8]>) -> Result<Self, String> {
        let directory = path.parent().ok_or("diychef.saveFailed")?;
        fs::create_dir_all(directory).map_err(|_| "diychef.saveFailed")?;
        let mut value = Self {
            path: path.into(),
            staged: directory.join(format!(".oc2-diychef-{}.tmp", uuid::Uuid::new_v4())),
            backup: None,
        };
        let result = (|| {
            write_new(&value.staged, bytes)?;
            if let Some(original) = original {
                let backup =
                    directory.join(format!(".oc2-diychef-{}.rollback", uuid::Uuid::new_v4()));
                value.backup = Some(backup);
                write_new(value.backup.as_ref().unwrap(), original)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            value.cleanup();
            return Err(error);
        }
        Ok(value)
    }
    fn commit(&self, create: bool, original: Option<&[u8]>) -> Result<(), String> {
        if super::read_bytes(&self.path)?.as_deref() != original {
            return Err("diychef.conflict".into());
        }
        if create {
            fs::hard_link(&self.staged, &self.path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    "diychef.conflict"
                } else {
                    "diychef.saveFailed"
                }
            })
        } else {
            fs::rename(&self.staged, &self.path).map_err(|_| "diychef.saveFailed")
        }
        .map_err(str::to_string)
    }
    fn restore(&self, written: &[u8]) -> Result<(), String> {
        // Do not erase another writer's newer bytes while restoring our pair.
        // Recovery material is retained when ownership can no longer be proven.
        if super::read_bytes(&self.path)
            .map_err(|_| "diychef.writtenReadFailed")?
            .as_deref()
            != Some(written)
        {
            return Err("diychef.writtenReadFailed".into());
        }
        match &self.backup {
            Some(backup) => fs::rename(backup, &self.path),
            None => fs::remove_file(&self.path),
        }
        .map_err(|_| "diychef.writtenReadFailed".into())
    }
    fn cleanup(&self) {
        let _ = fs::remove_file(&self.staged);
        if let Some(backup) = &self.backup {
            let _ = fs::remove_file(backup);
        }
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| "diychef.saveFailed")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "diychef.saveFailed".into())
}

pub(super) fn replace_pair<T>(
    prefer: (&Path, &[u8], Option<&[u8]>),
    bindings: (&Path, &[u8], Option<&[u8]>),
    create: bool,
    recheck: impl FnOnce() -> Result<(), String>,
    after_prefer: impl FnOnce() -> Result<(), String>,
    readback: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let first = Replacement::prepare(prefer.0, prefer.1, prefer.2)?;
    let second = match Replacement::prepare(bindings.0, bindings.1, bindings.2) {
        Ok(value) => value,
        Err(error) => {
            first.cleanup();
            return Err(error);
        }
    };
    let mut committed_first = false;
    let mut committed_second = false;
    let result = (|| {
        recheck()?;
        first.commit(create, prefer.2)?;
        committed_first = true;
        after_prefer()?;
        second.commit(bindings.2.is_none(), bindings.2)?;
        committed_second = true;
        readback()
    })();
    if result.is_err() {
        let restored_second = !committed_second || second.restore(bindings.1).is_ok();
        let restored_first = !committed_first || first.restore(prefer.1).is_ok();
        if !restored_first || !restored_second {
            // Keep original bytes in the rollback files when recovery itself fails.
            return Err("diychef.writtenReadFailed".into());
        }
    }
    first.cleanup();
    second.cleanup();
    result
}
