use super::config::PublicConfig;
use super::game::GameInfo;
use super::mods::InstalledMod;
use super::releases::UpdateStatus;
use super::repositories::Provider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustLog {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
    pub download_size: Option<u64>,
    #[serde(default)]
    pub manifest_url: String,
    #[serde(default)]
    pub installer_url: Option<String>,
    #[serde(default)]
    pub installer_signature: String,
    #[serde(default)]
    pub installer_name: String,
    #[serde(default)]
    pub installer_type: String,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub source_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateChannelStatus {
    pub id: String,
    pub provider: Provider,
    pub mode: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateChannelEvent {
    pub region: String,
    pub channel: AppUpdateChannelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdatePhaseEvent {
    pub region: String,
    pub phase: String,
    pub message: Option<String>,
    pub update: Option<AppUpdateInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateCheckResult {
    pub update: Option<AppUpdateInfo>,
    pub channels: Vec<AppUpdateChannelStatus>,
    pub all_failed: bool,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub config: PublicConfig,
    pub game: GameInfo,
    pub mods: Vec<InstalledMod>,
    pub updates: Vec<UpdateStatus>,
}
