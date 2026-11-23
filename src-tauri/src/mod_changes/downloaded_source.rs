use super::transaction::Fingerprint;
use crate::models::MapInstallOrigin;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Source {
    Map(Option<MapInstallOrigin>),
    ModUrl,
}

struct Receipt {
    path: PathBuf,
    fingerprint: Fingerprint,
    source: Source,
    file_name: String,
    issued: Instant,
}

#[derive(Default)]
pub(crate) struct Downloads(Mutex<HashMap<Uuid, Receipt>>);

impl Downloads {
    pub(crate) fn record(
        &self,
        id: Uuid,
        path: &Path,
        fingerprint: Fingerprint,
        source: Source,
        file_name: &str,
    ) -> Result<(), String> {
        if !matches!(fingerprint, Fingerprint::File { .. }) {
            return Err("modChanges.payloadChanged".into());
        }
        let path = fs::canonicalize(path).map_err(|_| "modChanges.payloadChanged")?;
        let mut receipts = self.0.lock().map_err(|_| "lock.poisoned")?;
        receipts.retain(|_, receipt| !expired(receipt));
        receipts.insert(
            id,
            Receipt {
                path,
                fingerprint,
                source,
                file_name: file_name.to_owned(),
                issued: Instant::now(),
            },
        );
        Ok(())
    }

    pub(crate) fn verify(
        &self,
        id: Uuid,
        path: &Path,
        fingerprint: &Fingerprint,
        source: &Source,
        required: bool,
    ) -> Result<Option<String>, String> {
        let receipts = self.0.lock().map_err(|_| "lock.poisoned")?;
        let Some(receipt) = receipts.get(&id) else {
            return if required {
                Err("modChanges.targetChanged".into())
            } else {
                Ok(None)
            };
        };
        if expired(receipt)
            || &receipt.source != source
            || &receipt.fingerprint != fingerprint
            || fs::canonicalize(path).ok().as_ref() != Some(&receipt.path)
        {
            return Err("modChanges.payloadChanged".into());
        }
        Ok(Some(receipt.file_name.clone()))
    }

    pub(crate) fn forget_path(&self, path: &Path) {
        if let Ok(mut receipts) = self.0.lock() {
            let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            receipts.retain(|_, receipt| {
                receipt.path != path && !receipt.path.starts_with(&path) && !expired(receipt)
            });
        }
    }
}

fn expired(receipt: &Receipt) -> bool {
    receipt.issued.elapsed() >= Duration::from_secs(60 * 60)
}
