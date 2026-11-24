use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::Notify;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RaceAction {
    Lock(usize),
    Retry,
    Unlock,
}

#[derive(Default)]
struct Mailbox {
    allowed: Vec<usize>,
    locked_failed: bool,
    locked: bool,
    pending: Option<RaceAction>,
}

#[derive(Default)]
struct Shared {
    mailbox: Mutex<Mailbox>,
    notification: Notify,
}

static CONTROLS: LazyLock<Mutex<HashMap<Uuid, Arc<Shared>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) struct RaceControl {
    key: Uuid,
    id: String,
    shared: Arc<Shared>,
}

impl RaceControl {
    pub(super) fn register() -> Self {
        let key = Uuid::new_v4();
        let shared = Arc::new(Shared::default());
        CONTROLS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(key, shared.clone());
        Self {
            key,
            id: key.to_string(),
            shared,
        }
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn update(&self, allowed: Vec<usize>, locked_failed: bool, locked: bool) {
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        mailbox.allowed = allowed;
        mailbox.locked_failed = locked_failed;
        mailbox.locked = locked;
    }

    pub(super) fn take(&self) -> Option<RaceAction> {
        let mut mailbox = self
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let action = mailbox.pending.take()?;
        *mailbox = Mailbox::default();
        Some(action)
    }

    pub(super) async fn changed(&self) {
        self.shared.notification.notified().await;
    }
}

impl Drop for RaceControl {
    fn drop(&mut self) {
        CONTROLS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.key);
    }
}

pub(crate) fn request(id: &str, action: RaceAction) -> bool {
    let Ok(key) = Uuid::parse_str(id) else {
        return false;
    };
    let controls = CONTROLS.lock().unwrap_or_else(|error| error.into_inner());
    let Some(shared) = controls.get(&key) else {
        return false;
    };
    let mut mailbox = shared
        .mailbox
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let allowed = match action {
        RaceAction::Lock(source) => !mailbox.locked && mailbox.allowed.contains(&source),
        RaceAction::Retry => mailbox.locked_failed,
        RaceAction::Unlock => mailbox.locked,
    };
    if !allowed || mailbox.pending.is_some() {
        return false;
    }
    mailbox.pending = Some(action);
    shared.notification.notify_one();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn commands_only_reach_the_current_race_and_current_state() {
        let first = RaceControl::register();
        let second = RaceControl::register();
        first.update(vec![1, 2], false, false);
        second.update(vec![3], false, false);
        assert!(!request(first.id(), RaceAction::Lock(3)));
        assert!(!request(first.id(), RaceAction::Retry));
        assert!(request(first.id(), RaceAction::Lock(1)));
        assert!(!request(first.id(), RaceAction::Lock(2)));
        first.changed().await;
        assert_eq!(first.take(), Some(RaceAction::Lock(1)));
        assert_eq!(second.take(), None);
        first.update(Vec::new(), true, true);
        assert!(request(first.id(), RaceAction::Retry));
        assert_eq!(first.take(), Some(RaceAction::Retry));
        first.update(Vec::new(), false, true);
        assert!(request(first.id(), RaceAction::Unlock));
        let obsolete = first.id().to_owned();
        drop(first);
        assert!(!request(&obsolete, RaceAction::Unlock));
    }
}
