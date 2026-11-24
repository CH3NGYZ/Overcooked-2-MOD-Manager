use super::{
    check, paths,
    types::{TransactionPath, TransactionRoots},
};
use crate::file_metadata::{WriteVolumeError, write_volume};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

fn probe(directory: &Path) -> Result<u64, String> {
    let path = crate::display_path::display_path(directory);
    let unverified = "modChanges.writeVolumeUnverified";
    paths::validate_absolute(directory)
        .map_err(|_| crate::locale_tag::with_params(unverified, &[("path", &path)]))?;
    write_volume(directory).map_err(|error| match error {
        WriteVolumeError::Unsupported(filesystem) => crate::locale_tag::with_params(
            "modChanges.writeVolumeUnsupported",
            &[("path", &path), ("filesystem", &filesystem)],
        ),
        WriteVolumeError::ReadOnly => {
            crate::locale_tag::with_params("modChanges.writeVolumeReadOnly", &[("path", &path)])
        }
        WriteVolumeError::Unverified(error) => {
            paths::log_io(directory, "verify_write_volume", &error);
            crate::locale_tag::with_params(unverified, &[("path", &path)])
        }
    })
}

pub(in crate::mod_changes) fn check_write_roots(
    game: &Path,
    data: &Path,
    journal: &Path,
) -> Result<(), String> {
    for directory in [game, data, journal] {
        probe(directory)?;
    }
    Ok(())
}

pub(super) fn check_targets<'a>(
    roots: &TransactionRoots,
    endpoints: impl IntoIterator<Item = (&'a TransactionPath, Option<&'a TransactionPath>)>,
) -> Result<(), String> {
    let mut directories = HashMap::<PathBuf, u64>::new();
    let mut volume = |path: &Path| -> Result<u64, String> {
        let directory = path.parent().ok_or(paths::UNSAFE)?;
        if let Some(serial) = directories.get(directory) {
            return Ok(*serial);
        }
        let serial = probe(directory)?;
        directories.insert(directory.to_path_buf(), serial);
        Ok(serial)
    };
    for (target, move_to) in endpoints {
        let from = check::resolve(target, roots, check::TARGET)?;
        let source_volume = volume(&from)?;
        if let Some(to) = move_to {
            let to = check::resolve(to, roots, check::TARGET)?;
            if source_volume != volume(&to)? {
                return Err(crate::locale_tag::with_params(
                    "modChanges.crossVolumeMove",
                    &[
                        ("from", &crate::display_path::display_path(&from)),
                        ("path", &crate::display_path::display_path(&to)),
                    ],
                ));
            }
        }
    }
    Ok(())
}
