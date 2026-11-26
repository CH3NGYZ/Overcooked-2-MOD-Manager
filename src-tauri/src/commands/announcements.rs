use crate::announcements::{self, AnnouncementSnapshot};

#[tauri::command]
pub(crate) async fn get_announcements() -> Option<AnnouncementSnapshot> {
    announcements::load().await
}
