use super::*;
use std::path::Path;

pub fn find_install_conflicts(game_root: &Path, sets: &[ImportedLevelSet]) -> Vec<InstallConflict> {
    find_install_conflicts_with_workers(
        &super::super::paths::test_roots(game_root),
        sets,
        crate::concurrency::resolve_concurrency(
            crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY,
            crate::concurrency::cores(),
        ),
    )
}
