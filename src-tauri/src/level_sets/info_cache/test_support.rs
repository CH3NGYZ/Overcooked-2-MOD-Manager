use super::*;

pub(in crate::level_sets) fn info_cache_parses(info_path: &Path) -> Option<u32> {
    let cache = info_cache().lock().ok()?;
    cache
        .get(&info_cache_key(info_path))
        .map(|entry| entry.parses)
}
