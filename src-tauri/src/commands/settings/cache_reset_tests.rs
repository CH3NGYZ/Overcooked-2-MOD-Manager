use super::*;
use crate::config::ConfigService;

#[test]
fn cache_reset_preserves_all_compression_materials_and_resets_release_completeness() {
    let temporary = tempfile::tempdir().unwrap();
    let state = AppState::with_config(ConfigService::for_test(temporary.path().join("data")));
    let mut config = crate::config::default_config();
    let mut repository = crate::community::repository_from_coordinates(
        &crate::models::Provider::Github,
        "Fixture",
        "cache-reset",
        "",
        "",
    )
    .unwrap();
    repository.cached_releases_complete = true;
    repository.releases_cached_at = Some(123);
    repository.etag = Some("fixture-etag".into());
    repository.last_error = Some("fixture error".into());
    config.repositories = vec![repository];
    config.ui_zoom = 1.5;
    state.config.save(&config).unwrap();
    let cache = state.config.cache_dir();
    let work = cache.join("level-compression/in-flight");
    fs::create_dir_all(&work).unwrap();
    for name in ["original-bundle", "converted-bundle", "source.txt"] {
        fs::write(work.join(name), name.as_bytes()).unwrap();
    }
    fs::create_dir_all(cache.join("ordinary/nested")).unwrap();
    fs::write(cache.join("ordinary/nested/stale"), b"stale").unwrap();
    fs::write(cache.join("stale-download"), b"stale").unwrap();

    assert!(reset_ui_in(&state).unwrap().ok);

    for name in ["original-bundle", "converted-bundle", "source.txt"] {
        assert_eq!(fs::read(work.join(name)).unwrap(), name.as_bytes());
    }
    assert!(!cache.join("ordinary").exists());
    assert!(!cache.join("stale-download").exists());
    let config = state.config.load_or_default();
    assert_eq!(config.ui_zoom, crate::config::default_ui_zoom());
    let repository = &config.repositories[0];
    assert!(repository.cached_releases.is_empty());
    assert!(!repository.cached_releases_complete);
    assert!(repository.releases_cached_at.is_none());
    assert!(repository.etag.is_none());
    assert!(repository.last_error.is_none());
}

#[test]
fn cache_reset_keeps_files_and_config_when_journal_recovery_is_required() {
    let temporary = tempfile::tempdir().unwrap();
    let data = temporary.path().join("data");
    let state = AppState::with_config(ConfigService::for_test(data.clone()));
    let mut config = crate::config::default_config();
    config.game.path = temporary
        .path()
        .join("missing-game")
        .to_string_lossy()
        .into_owned();
    config.ui_zoom = 1.5;
    state.config.save(&config).unwrap();
    let config_before = fs::read(state.config.config_path()).unwrap();
    let cache = state.config.cache_dir();
    fs::create_dir_all(&cache).unwrap();
    let cached = cache.join("must-survive.download");
    fs::write(&cached, b"pending data").unwrap();
    let journal = data
        .join("mod-change-transactions")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&journal).unwrap();
    fs::write(journal.join("journal.json"), b"interrupted journal").unwrap();

    assert_eq!(
        reset_ui_in(&state).unwrap_err(),
        "modChanges.recoveryRequired"
    );

    assert_eq!(fs::read(&cached).unwrap(), b"pending data");
    assert_eq!(fs::read(state.config.config_path()).unwrap(), config_before);
    assert!(journal.join("journal.json").exists());
}

#[cfg(windows)]
#[test]
fn cache_reset_does_not_follow_a_junction_to_compression_recovery() {
    let temporary = tempfile::tempdir().unwrap();
    let cache = temporary.path().join("cache");
    let work = cache.join("level-compression").join("recovery");
    fs::create_dir_all(&work).unwrap();
    let backup = work.join("original-bundle");
    fs::write(&backup, b"only recovery copy").unwrap();
    let alias = cache.join("cache-alias");
    let result = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&alias)
        .arg(&work)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    clear_resettable_cache(&cache).unwrap();

    assert_eq!(fs::read(&backup).unwrap(), b"only recovery copy");
    fs::remove_dir(alias).unwrap();
}
