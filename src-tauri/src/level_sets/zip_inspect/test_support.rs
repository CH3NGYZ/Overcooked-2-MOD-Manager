use super::*;

pub fn inspect_zip(zip_path: &Path) -> Result<Vec<ImportedLevelSet>, String> {
    let workers = crate::concurrency::resolve_concurrency(
        crate::concurrency::AUTO_CONCURRENCY,
        crate::concurrency::cores(),
    );
    inspect_zip_with_progress(zip_path, workers, workers, &|_, _, _| {})
}
