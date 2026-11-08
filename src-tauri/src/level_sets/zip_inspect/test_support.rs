use super::*;

pub fn inspect_zip(zip_path: &Path) -> Result<Vec<ImportedLevelSet>, String> {
    let workers = crate::metadata_read::resolve_concurrency(
        crate::metadata_read::AUTO_CONCURRENCY,
        crate::metadata_read::cores(),
    );
    inspect_zip_with_progress(zip_path, workers, workers, &|_, _, _| {})
}
