use super::*;

pub(super) fn validate_diy_level_backup_in(
    backup_path: &Path,
    backup_root: &Path,
) -> Result<PathBuf, String> {
    validate_framework_backup_in(backup_path, backup_root, &DIYLEVEL_BACKUP)
}
