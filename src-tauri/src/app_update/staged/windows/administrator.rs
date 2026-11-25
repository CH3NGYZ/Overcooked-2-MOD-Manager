use super::{
    Attempt, HandoffEvents, InstallerSlot, ManagerProcess, Outcome, Prepared, PreparedPackage,
    SIGNATURE_FILE, SUPERVISOR_FILE, UNAVAILABLE, WAIT_MS, finish_install, installer, lock_file,
    matches_installed_version, package_path, protected, read_signature, start_error,
    validated_destination, verify_download, write_new,
};
use crate::app_update::UpdateTarget;
use crate::app_update::windows_session::{PermissionRoute, ensure_same_admin, permission_route};
use crate::archive;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use uuid::Uuid;

pub(super) const FLAG: &str = "--oc2-admin-update-supervisor";
const MANUAL: &str = "update.installFailedManual";

pub(super) fn unavailable() -> String {
    "update.installAsAdministratorUnavailable".into()
}

pub(super) fn preflight(error: impl std::fmt::Display) -> String {
    log::warn!("未开始管理员更新：{error}");
    unavailable()
}

pub(crate) fn can_install_as_admin() -> bool {
    let check = || -> Result<(), String> {
        let source = std::env::current_exe().map_err(start_error)?;
        let destination = validated_destination(&source, "exe")?;
        validate_target(&source, &destination, UpdateTarget::current()?.kind)?;
        protected::attempts_dir(false)?;
        Ok(())
    };
    check().is_ok()
}

struct Target {
    _directories: Vec<File>,
    image: File,
}

pub(super) fn validate_target(source: &Path, destination: &Path, kind: &str) -> Result<(), String> {
    target(source, destination, kind).map(|_| ())
}

pub(super) fn validate_package(bytes: &[u8], kind: &str, version: &str) -> Result<(), String> {
    let root = protected::attempts_dir(true)
        .map_err(preflight)?
        .ok_or_else(unavailable)?;
    let work = tempfile::Builder::new()
        .prefix("inspect-")
        .tempdir_in(root.path())
        .map_err(preflight)?;
    let path = package_path(work.path(), kind)?;
    write_new(&path, &mut &*bytes).map_err(preflight)?;
    let _package = lock_file(&path).map_err(preflight)?;
    if !installer::supports_admin(&path, kind, version) {
        return Err("update.deferredUnsupported".into());
    }
    Ok(())
}

fn target(source: &Path, destination: &Path, kind: &str) -> Result<Target, String> {
    if kind != "exe" || source != destination || validated_destination(source, kind)? != destination
    {
        return Err(MANUAL.into());
    }
    let directory = source.parent().ok_or(MANUAL)?;
    let directories = protected::pin_directories(directory).map_err(start_error)?;
    let image = protected::pin_install_file(source).map_err(start_error)?;
    if !matches_installed_version(source, env!("CARGO_PKG_VERSION")) {
        return Err(MANUAL.into());
    }
    for entry in fs::read_dir(directory).map_err(start_error)? {
        let entry = entry.map_err(start_error)?;
        if entry.file_name() == source.file_name().ok_or(MANUAL)? {
            continue;
        }
        if !entry.file_name().eq_ignore_ascii_case("uninstall.exe") {
            return Err(MANUAL.into());
        }
        let file = protected::pin_install_file(&entry.path()).map_err(start_error)?;
        if !protected::has_one_link(&file).map_err(start_error)? {
            return Err(MANUAL.into());
        }
    }
    if !protected::has_one_link(&image).map_err(start_error)?
        || !installer::matches_nsis_registration(source)
    {
        return Err(MANUAL.into());
    }
    Ok(Target {
        _directories: directories,
        image,
    })
}

impl Prepared {
    pub(crate) fn start_admin(&self) -> Result<(), String> {
        let PreparedPackage::Administrator(bytes) = &self.package else {
            return Err(unavailable());
        };
        if permission_route().map_err(preflight)? != PermissionRoute::Administrator
            || std::env::current_exe().map_err(preflight)? != self.source
        {
            return Err(unavailable());
        }
        let target = target(&self.source, &self.destination, self.kind).map_err(preflight)?;
        verify_download(bytes, &self.signature, &self.version)?;
        let root = protected::attempts_dir(true)
            .map_err(preflight)?
            .ok_or_else(unavailable)?;
        let attempt = Attempt::create_with(
            root.path(),
            &self.source,
            &self.destination,
            &self.version,
            self.kind,
            false,
            protected::create_work,
        )
        .map_err(preflight)?;
        match self.start_protected_supervisor(&attempt, target, bytes) {
            Ok(()) => Ok(()),
            Err(error) => {
                attempt
                    .record_outcome(Outcome::Abandoned)
                    .map_err(preflight)?;
                Err(if error == super::INSTALLER_BUSY {
                    error
                } else {
                    preflight(error)
                })
            }
        }
    }

    fn start_protected_supervisor(
        &self,
        attempt: &Attempt,
        mut target: Target,
        bytes: &[u8],
    ) -> Result<(), String> {
        let work = attempt.work_path();
        let _work = protected::pin_work(work).map_err(start_error)?;
        let path = package_path(work, self.kind)?;
        let mut package_bytes = bytes;
        write_new(&path, &mut package_bytes).map_err(start_error)?;
        let _package = lock_file(&path).map_err(start_error)?;
        if !installer::supports_admin(&path, self.kind, &self.version) {
            return Err(unavailable());
        }
        write_new(&work.join(SIGNATURE_FILE), &mut self.signature.as_bytes())
            .map_err(start_error)?;
        let helper = work.join(SUPERVISOR_FILE);
        target.image.rewind().map_err(start_error)?;
        write_new(&helper, &mut target.image).map_err(start_error)?;
        let _helper = lock_file(&helper).map_err(start_error)?;
        let owner = ManagerProcess::current().map_err(start_error)?;
        let security =
            protected::SecurityDescriptor::new(protected::EVENT_DACL).map_err(start_error)?;
        let events = HandoffEvents::protected_update(attempt.id, Some(&mut security.attributes()))
            .map_err(start_error)?;
        let mut command = installer::protected_command(&helper, work).map_err(start_error)?;
        let mut child = command
            .arg(FLAG)
            .arg(owner.pid.to_string())
            .arg(owner.created.to_string())
            .arg(attempt.id.to_string())
            .spawn()
            .map_err(start_error)?;
        if let Err(error) = events.commit_when_ready(child.as_raw_handle().cast(), WAIT_MS) {
            return Err(super::handoff_error(&mut child, error));
        }
        log::info!(
            "已交接不重启的管理员更新：尝试={}，旧进程={}，辅助进程={}",
            attempt.id,
            owner.pid,
            child.id()
        );
        Ok(())
    }
}

pub(super) fn run_supervisor(owner: &ManagerProcess, id: Uuid) -> Result<(), String> {
    let root = protected::attempts_dir(false)?.ok_or(MANUAL)?;
    let work = root.path().join(id.to_string());
    let _work = protected::pin_work(&work).map_err(start_error)?;
    let current = std::env::current_exe().map_err(start_error)?;
    let _helper = lock_file(&current).map_err(start_error)?;
    if !same_file::is_same_file(&current, work.join(SUPERVISOR_FILE)).map_err(start_error)? {
        return Err(UNAVAILABLE.into());
    }
    let attempt = Attempt::read_at(root.path(), id)?;
    if attempt.restart
        || attempt.source_version != env!("CARGO_PKG_VERSION")
        || UpdateTarget::current()?.kind != attempt.installer_type
    {
        return Err(UNAVAILABLE.into());
    }
    let owner_handle = owner.open(&attempt.source).map_err(start_error)?;
    ensure_same_admin(&owner_handle).map_err(start_error)?;
    let attempt_lock = attempt.lock_work()?;
    let _installer_slot = InstallerSlot::enter()?;
    let target_state = target(
        &attempt.source,
        &attempt.destination,
        &attempt.installer_type,
    )?;
    let source_digest = archive::sha256_reader(&target_state.image).map_err(start_error)?;
    if archive::sha256_reader(lock_file(&current).map_err(start_error)?).map_err(start_error)?
        != source_digest
    {
        return Err(UNAVAILABLE.into());
    }
    let path = package_path(&work, &attempt.installer_type)?;
    let mut package = lock_file(&path).map_err(start_error)?;
    let signature = read_signature(&work)?;
    let mut bytes = Vec::new();
    package.read_to_end(&mut bytes).map_err(start_error)?;
    verify_download(&bytes, &signature, &attempt.version)?;
    drop(bytes);
    if !installer::supports_admin(&path, &attempt.installer_type, &attempt.version) {
        return Err("update.deferredUnsupported".into());
    }
    if let Err(error) = HandoffEvents::protected_update(attempt.id, None)
        .and_then(|events| events.yield_until_owner_exits(&owner_handle, WAIT_MS))
    {
        installer::write_diagnostic(
            &attempt,
            installer::WINDOWS_RESULT,
            &format!("outcome=Failed handoff_error={error}"),
        );
        attempt.record_outcome(Outcome::Failed)?;
        return Err(start_error(error));
    }
    let checked = (|| {
        let final_target = target(
            &attempt.source,
            &attempt.destination,
            &attempt.installer_type,
        )?;
        if archive::sha256_reader(&final_target.image).map_err(start_error)? != source_digest {
            return Err(UNAVAILABLE.into());
        }
        Ok(final_target)
    })();
    let Target {
        _directories: final_directories,
        image: final_image,
    } = match checked {
        Ok(target) => target,
        Err(error) => {
            installer::write_diagnostic(
                &attempt,
                installer::WINDOWS_RESULT,
                &format!("outcome=Failed recheck_error={error}"),
            );
            attempt.record_outcome(Outcome::Failed)?;
            return Err(error);
        }
    };
    drop(target_state);
    drop(final_image);
    finish_install(&attempt, path, owner.pid, package, attempt_lock, true);
    drop(final_directories);
    Ok(())
}
