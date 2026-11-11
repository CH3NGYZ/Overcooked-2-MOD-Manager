use super::*;

pub fn install_from_zip(
    zip_path: &Path,
    game_root: &Path,
    manifests_dir: &Path,
    selected: &[String],
) -> Result<Vec<String>, String> {
    let sets = super::super::zip_inspect::inspect_zip(zip_path)?;
    install_parsed_sets(
        &sets,
        zip_path,
        &super::super::paths::test_roots(game_root),
        manifests_dir,
        selected,
        None,
    )
}
