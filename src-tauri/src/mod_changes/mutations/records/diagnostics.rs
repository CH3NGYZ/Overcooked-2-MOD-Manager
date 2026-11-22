use super::*;
use crate::file_check::log_field;

impl Builder<'_> {
    pub(in crate::mod_changes::mutations) fn diagnose_record_claim<'a>(
        &self,
        requester: &str,
        target: &str,
        claimed_file: Option<&str>,
        reason: &str,
        records: impl Iterator<Item = &'a InstallRecord>,
    ) {
        if !log::log_enabled!(log::Level::Warn) {
            return;
        }
        const LIMIT: usize = 6;
        let canonical = mods::canonical_dll_name(target);
        let field = log_field;
        let mut entries = Vec::new();
        let mut omitted = 0;
        let file_key = claimed_file.map(mods::manifest_dll_key);
        for record in records {
            if entries.len() == LIMIT {
                omitted += 1;
                continue;
            }
            let manifest = &record.manifest;
            let binding = manifest.repository_binding.as_ref().map(|binding| {
                (
                    &binding.provider,
                    field(&binding.owner),
                    field(&binding.repository),
                    binding.channel,
                    field(&binding.tag),
                )
            });
            let (files, skipped_files) = if let Some(key) = &file_key {
                let mut matching = manifest
                    .files
                    .iter()
                    .filter(|file| mods::manifest_dll_key(file) == *key);
                let files: Vec<_> = matching
                    .by_ref()
                    .take(LIMIT)
                    .map(|file| field(file))
                    .collect();
                (files, matching.count())
            } else {
                (Vec::new(), 0)
            };
            let dll_version = file_key
                .as_ref()
                .filter(|key| key.ends_with(".dll"))
                .map(
                    |key| match manifest.installed_dll_versions.get(key).map(String::as_str) {
                        None => ("not_recorded", None),
                        Some("") => ("unreadable_at_install", None),
                        Some(version) => ("recorded", Some(field(version))),
                    },
                );
            entries.push(format!(
                "占用者={:?}，清单={:?}，仓库ID={:?}，Release Tag={:?}，绑定={binding:?}，命中文件={files:?}，旧DLL版本快照={dll_version:?}，其余命中={skipped_files}，文件总数={}",
                field(&record.owner),
                field(&record.relative),
                field(&manifest.repository_id),
                field(&manifest.release_tag),
                manifest.files.len(),
            ));
        }
        let endpoint = |path: &TransactionPath| match path.resolve(&self.roots) {
            Ok(path) => match fs::symlink_metadata(path) {
                Ok(metadata) if metadata.is_dir() => "directory".into(),
                Ok(metadata) if metadata.is_file() => format!("file:{}B", metadata.len()),
                Ok(_) => "other".into(),
                Err(error) if error.kind() == ErrorKind::NotFound => "missing".into(),
                Err(error) => format!("unknown:{:?}:os={:?}", error.kind(), error.raw_os_error()),
            },
            Err(error) => format!("unknown:{error}"),
        };
        let scene = if reason == "install_record_path_occupied" {
            format!("清单现场={}", endpoint(&data(target)))
        } else if canonical.to_ascii_lowercase().ends_with(".dll") {
            format!(
                "active={}，disabled={}",
                endpoint(&game(&canonical)),
                endpoint(&game(&mods::disabled_dll_file(&canonical)))
            )
        } else {
            format!("目标现场={}", endpoint(&game(target)))
        };
        log::warn!(
            "安装清单拒绝接管 op={}：申请者={:?}，目标={:?}，依据={reason}，现场={scene}，证据=[{}]，其余清单={omitted}",
            field(self.session.operation_id()),
            field(requester),
            field(target),
            entries.join("; "),
        );
    }
}
