use crate::announcements::{self, AnnouncementSnapshot};

#[tauri::command]
pub(crate) async fn get_announcements(
    language: String,
    fresh: bool,
) -> Option<AnnouncementSnapshot> {
    announcements::load(&language, fresh).await
}
