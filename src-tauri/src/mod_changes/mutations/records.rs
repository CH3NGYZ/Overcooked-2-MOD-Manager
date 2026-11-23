use super::*;
use crate::file_metadata::FileIdentity;
use sha2::{Digest, Sha256};
use std::io::ErrorKind;

mod diagnostics;

pub(super) struct InstallRecord {
    pub relative: String,
    pub manifest: archive::InstallManifest,
    pub owner: String,
    digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct RecordStamp {
    pub fingerprint: Fingerprint,
    pub identity: FileIdentity,
}

#[derive(Default)]
pub(super) struct InstallRecords {
    records: Vec<InstallRecord>,
    claims: BTreeMap<String, BTreeSet<usize>>,
}

pub(super) fn install_record_path(repository_id: &str) -> TransactionPath {
    data(&format!("manifests/{repository_id}.toml"))
}

impl InstallRecords {
    pub fn read(roots: &TransactionRoots, session: &PlanSession) -> Result<Self, String> {
        let mut result = Self::default();
        let root = &roots.data_root;
        let unreadable = |relative: &str, error: std::io::Error| {
            log::warn!(
                "整链安装清单读取失败 op={}：文件={:?}，类型={:?}，OS码={:?}",
                session.operation_id(),
                crate::file_check::log_path(&root.join(relative)),
                error.kind(),
                error.raw_os_error(),
            );
            InstallBlock::RecordUnreadable.report("install-record", relative)
        };
        let directory = data("manifests").resolve(roots)?;
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound && !directory.is_symlink() => {
                return Ok(result);
            }
            Err(error) => return Err(unreadable("manifests", error)),
        };
        for entry in entries {
            let path = entry
                .map_err(|error| unreadable("manifests", error))?
                .path();
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
            {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("archive.pathEscape")?;
            let relative = format!("manifests/{filename}");
            data(&relative).resolve(roots)?;
            let text = fs::read_to_string(&path).map_err(|error| unreadable(&relative, error))?;
            let manifest = toml::from_str::<archive::InstallManifest>(&text).map_err(|error| {
                log::warn!(
                    "整链安装清单解析失败 op={}：文件={:?}，TOML错误字节位置={:?}",
                    session.operation_id(),
                    crate::file_check::log_path(&path),
                    error.span()
                );
                InstallBlock::RecordInvalid.report("install-record", &relative)
            })?;
            let mut owners = session.trusted.files.iter().filter(|(_, node)| {
                node.manifests
                    .iter()
                    .any(|owned| owned.repository_id == manifest.repository_id)
            });
            let owner = match (owners.next(), owners.next()) {
                (Some((id, _)), None) => id.clone(),
                _ => format!("unknown-package:{}", manifest.repository_id),
            };
            for file in &manifest.files {
                result
                    .claims
                    .entry(mods::manifest_dll_key(file))
                    .or_default()
                    .insert(result.records.len());
            }
            result.records.push(InstallRecord {
                relative,
                manifest,
                owner,
                digest: format!("{:x}", Sha256::digest(text.as_bytes())),
            });
        }
        Ok(result)
    }

    pub fn members(&self) -> impl Iterator<Item = (&str, &str)> {
        self.claimed_files()
            .map(|(file, record)| (file, record.owner.as_str()))
    }

    pub fn claimed_files(&self) -> impl Iterator<Item = (&str, &InstallRecord)> {
        self.records.iter().flat_map(|record| {
            record
                .manifest
                .files
                .iter()
                .map(move |file| (file.as_str(), record))
        })
    }

    pub fn at_relative<'a>(&'a self, relative: &'a str) -> impl Iterator<Item = &'a InstallRecord> {
        let key = path_key(Path::new(relative));
        self.records
            .iter()
            .filter(move |record| path_key(Path::new(&record.relative)) == key)
    }

    pub fn portable_claims(&self, path: &str) -> impl Iterator<Item = &InstallRecord> {
        self.claims
            .get(&mods::manifest_dll_key(path))
            .into_iter()
            .flatten()
            .map(|index| &self.records[*index])
    }

    pub fn claims<'a>(&'a self, logical_key: &'a str) -> impl Iterator<Item = &'a InstallRecord> {
        self.portable_claims(logical_key).filter(move |record| {
            record
                .manifest
                .files
                .iter()
                .any(|file| inventory::key(&mods::canonical_dll_name(file)) == logical_key)
        })
    }

    pub fn records_for_id<'a>(
        &'a self,
        repository_id: &str,
    ) -> impl Iterator<Item = &'a InstallRecord> {
        let target = install_record_path(repository_id);
        let key = path_key(Path::new(&target.relative));
        let uuid = uuid::Uuid::parse_str(repository_id).ok();
        let id_key = path_key(Path::new(repository_id));
        self.records.iter().filter(move |record| {
            path_key(Path::new(&record.manifest.repository_id)) == id_key
                || uuid.is_some_and(|expected| {
                    uuid::Uuid::parse_str(&record.manifest.repository_id) == Ok(expected)
                })
                || path_key(Path::new(&record.relative)) == key
        })
    }

    pub fn unique_record(&self, repository_id: &str) -> Option<&InstallRecord> {
        let target = install_record_path(repository_id);
        let mut candidates = self.records_for_id(repository_id);
        let record = candidates.next()?;
        (candidates.next().is_none()
            && record.manifest.repository_id == repository_id
            && inventory::key(&record.relative) == inventory::key(&target.relative))
        .then_some(record)
    }

    pub fn bound_records(
        &self,
    ) -> impl Iterator<Item = (&InstallRecord, &crate::models::ModRepositoryBinding)> {
        self.records
            .iter()
            .filter_map(|record| Some((record, record.manifest.repository_binding.as_ref()?)))
    }

    pub fn exact_orphan<'a>(
        &'a self,
        repository_id: &str,
        session: &PlanSession,
    ) -> Option<&'a InstallRecord> {
        let record = self.unique_record(repository_id)?;
        (record.relative == install_record_path(repository_id).relative
            && !session.trusted.files.values().any(|files| {
                files
                    .manifests
                    .iter()
                    .any(|manifest| manifest.repository_id == repository_id)
            }))
        .then_some(record)
    }
}

impl InstallRecord {
    pub fn exact_stamp(&self, roots: &TransactionRoots, node: &str) -> Result<RecordStamp, String> {
        let path = data(&self.relative).resolve(roots)?;
        let changed = || InstallBlock::VerificationChanged.report(node, &self.relative);
        let identity = FileIdentity::capture(&path)
            .ok()
            .flatten()
            .ok_or_else(changed)?;
        let root = fs::canonicalize(&roots.data_root).map_err(|_| changed())?;
        if !fs::canonicalize(&path).is_ok_and(|actual| actual == root.join(&self.relative)) {
            return Err(InstallBlock::AliasesOrLink.report(node, &self.relative));
        }
        let fingerprint = self.verified_stamp(roots, node)?;
        if FileIdentity::capture(&path).ok().flatten().as_ref() != Some(&identity) {
            return Err(changed());
        }
        Ok(RecordStamp {
            fingerprint,
            identity,
        })
    }

    pub fn verified_stamp(
        &self,
        roots: &TransactionRoots,
        node: &str,
    ) -> Result<Fingerprint, String> {
        let path = data(&self.relative).resolve(roots)?;
        let expected = Fingerprint::capture(&path)?;
        if !matches!(&expected, Fingerprint::File { sha256, .. } if sha256 == &self.digest) {
            return Err(InstallBlock::VerificationChanged.report(node, &self.relative));
        }
        if !fs::File::open(path).is_ok_and(|file| has_single_link(&file)) {
            return Err(InstallBlock::AliasesOrLink.report(node, &self.relative));
        }
        Ok(expected)
    }
}
