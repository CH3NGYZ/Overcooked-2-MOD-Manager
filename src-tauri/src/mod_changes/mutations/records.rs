use super::*;
use sha2::{Digest, Sha256};
use std::io::ErrorKind;

pub(super) struct InstallRecord {
    pub relative: String,
    pub manifest: archive::InstallManifest,
    pub owner: String,
    digest: String,
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
    pub fn read(root: &Path, session: &PlanSession) -> Result<Self, String> {
        let mut result = Self::default();
        let unreadable = |relative: &str, error| {
            log::debug!("无法读取安装记录：路径={relative}，系统错误={error}");
            InstallBlock::RecordUnreadable.report("install-record", relative)
        };
        let directory = root.join("manifests");
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
            let relative = format!(
                "manifests/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            let text = fs::read_to_string(&path).map_err(|error| unreadable(&relative, error))?;
            let manifest = toml::from_str::<archive::InstallManifest>(&text)
                .map_err(|_| InstallBlock::RecordInvalid.report("install-record", &relative))?;
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
        self.records.iter().flat_map(|record| {
            record
                .manifest
                .files
                .iter()
                .map(|file| (file.as_str(), record.owner.as_str()))
        })
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

    pub fn unique_record(&self, repository_id: &str) -> Option<&InstallRecord> {
        let target = install_record_path(repository_id);
        let key = path_key(Path::new(&target.relative));
        let mut candidates = self.records.iter().filter(|record| {
            record.manifest.repository_id == repository_id
                || path_key(Path::new(&record.relative)) == key
        });
        let record = candidates.next()?;
        (candidates.next().is_none()
            && record.manifest.repository_id == repository_id
            && inventory::key(&record.relative) == inventory::key(&target.relative))
        .then_some(record)
    }
}

impl InstallRecord {
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
