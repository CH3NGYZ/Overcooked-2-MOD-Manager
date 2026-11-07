//! Download transport, archive installation and persisted installation metadata.

mod control;
mod install;
mod integrity;
mod manifests;
mod map_snapshots;
mod paths;
mod progress;
mod transfer;
mod zip_layout;

#[cfg(test)]
mod test_support;

pub(crate) use control::{
    app_update_in_progress, begin_app_update, cancel_download, is_download_cancelled,
    start_download,
};
pub(crate) use install::{
    copy_asset_pending, extract_framework_zip_pending, extract_local_mod_zip_pending,
    extract_prepared_mod_zip, prepare_repository_mod_zip, prepared_mod_zip_files,
};
pub(crate) use integrity::{validate_download_bytes, validate_download_file, validate_zip};
pub(crate) use manifests::{
    InstallManifest, new_manifest, package_display_name, primary_translation, read_manifest,
    read_manifests, single_language, write_manifest,
};
pub(crate) use map_snapshots::{read_map_relation_snapshots, write_map_relation_snapshots};
pub(crate) use paths::{clear_readonly, prune_empty_parent_dirs};
pub(crate) use progress::{
    DownloadPhase, DownloadProgress, DownloadReporter, DownloadRoute, emit_progress,
    emit_progress_event,
};
pub(crate) use transfer::{
    FIRST_BYTE_TIMEOUT, download_to_temp_with_threads_and_first_byte_timeout,
};
