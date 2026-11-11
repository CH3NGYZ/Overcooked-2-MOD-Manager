use super::*;

fn state() -> AppState {
    AppState::with_config(ConfigService::for_test(
        std::env::temp_dir().join(format!("oc2-state-{}", uuid::Uuid::new_v4())),
    ))
}

#[test]
fn clone_keeps_every_lock_observer_and_cache_shared() {
    let state = state();
    let cloned = state.clone();
    assert_eq!(state.config.config_path(), cloned.config.config_path());
    assert!(Arc::ptr_eq(
        &state.local_mod_snapshot,
        &cloned.local_mod_snapshot
    ));
    assert!(Arc::ptr_eq(
        &state.diychef_sessions,
        &cloned.diychef_sessions
    ));
    assert!(Arc::ptr_eq(&state.operation, &cloned.operation));
    assert!(Arc::ptr_eq(
        &state.repository_cache,
        &cloned.repository_cache
    ));
    assert!(Arc::ptr_eq(&state.community, &cloned.community));
    assert!(Arc::ptr_eq(&state.readme, &cloned.readme));
    assert!(Arc::ptr_eq(&state.game_state, &cloned.game_state));
    assert!(Arc::ptr_eq(&state.level_watch, &cloned.level_watch));
    assert!(Arc::ptr_eq(&state.release_cache, &cloned.release_cache));
    assert!(Arc::ptr_eq(
        &state.app_update_check,
        &cloned.app_update_check
    ));

    let _repository = state.repository_cache.try_lock().unwrap();
    assert!(cloned.repository_cache.try_lock().is_err());
    let _operation = state.operation.lock().unwrap();
    assert!(cloned.operation.try_lock().is_err());
    assert!(!state.config.config_path().exists());
}

#[test]
fn independently_constructed_states_do_not_share_runtime_state() {
    let first = state();
    let second = state();
    assert_ne!(first.config.config_path(), second.config.config_path());
    assert!(!Arc::ptr_eq(&first.operation, &second.operation));
    assert!(!Arc::ptr_eq(
        &first.repository_cache,
        &second.repository_cache
    ));
    assert!(!Arc::ptr_eq(&first.community, &second.community));
    assert!(!Arc::ptr_eq(&first.readme, &second.readme));
    assert!(!Arc::ptr_eq(&first.game_state, &second.game_state));
    assert!(!Arc::ptr_eq(&first.level_watch, &second.level_watch));
    assert!(!Arc::ptr_eq(&first.release_cache, &second.release_cache));
    assert!(!Arc::ptr_eq(
        &first.app_update_check,
        &second.app_update_check
    ));
    assert!(first.game_state.lock().unwrap().is_none());
    assert!(first.app_update_check.lock().unwrap().is_none());
}
