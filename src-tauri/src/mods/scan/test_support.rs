use super::*;

pub(in crate::mods) fn inspect_dll_at(
    path: &Path,
    game: &Path,
    config: &AppConfig,
    manifests: &[InstallManifest],
) -> InstalledMod {
    let requests = [crate::metadata_read::MetadataRequest {
        path: path.to_path_buf(),
        display_name: canonical_dll_file(path, game),
        kind: crate::metadata_read::MetadataKind::ModDll,
    }];
    let outcomes = crate::metadata_read::read_metadata_batch(&requests, 1, &|_, _| {});
    inspect_dll(path, &outcomes[0], game, config, manifests)
}
