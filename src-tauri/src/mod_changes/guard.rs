//! Recovery gate for operations that can mutate files or invalidate recovery material.
use super::transaction::{self, TransactionRoots};
use crate::{app_state::AppState, game};
use std::path::Path;

const REQUIRED: &str = "modChanges.recoveryRequired";

/// Legacy write IPCs remain callable for older clients, but authorization now
/// comes exclusively from a prepared, confirmed chain plan.
pub(crate) fn require_shared_plan<T>() -> Result<T, String> {
    Err("modChanges.installationBlocked".into())
}

/// Call only while holding AppState.operation. This function never acquires it.
pub(crate) fn ensure_recovered(state: &AppState) -> Result<(), String> {
    let data_root = state
        .config
        .config_path()
        .parent()
        .ok_or(REQUIRED)?
        .to_path_buf();
    let transaction_root = data_root.join("mod-change-transactions");
    if !transaction::has_pending(&transaction_root).map_err(|_| REQUIRED.to_string())? {
        return Ok(());
    }
    let result = (|| {
        let config = state.config.load_or_default();
        let detected = game::detect_game(&config).path.ok_or(REQUIRED)?;
        let game_root = game::resolve_existing_ancestor(Path::new(&detected))?;
        game::ensure_game_stopped(&game_root)?;
        let roots = TransactionRoots {
            game_root,
            data_root,
        };
        let report = transaction::recover_all(&transaction_root, &roots)?;
        if !report.blocked.is_empty() {
            return Err(REQUIRED.into());
        }
        if !report.recovered.is_empty() || !report.cleaned.is_empty() {
            state.invalidate_local_mod_snapshot()?;
        }
        Ok(())
    })();
    result.map_err(|error: String| {
        log::error!("未完成链式事务阻止当前操作，恢复材料已保留：{error}");
        REQUIRED.into()
    })
}
