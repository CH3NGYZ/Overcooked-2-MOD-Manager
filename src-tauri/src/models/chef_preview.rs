//! Static preview transport. Buffers use little-endian floats and u32 indices.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefPreviewBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefPreviewMesh {
    pub name: String,
    pub geometry_asset: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub texture_asset: Option<String>,
    pub texture_scale: [f32; 2],
    pub texture_offset: [f32; 2],
    pub material_floats: std::collections::BTreeMap<String, f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChefPreviewScene {
    pub fingerprint: String,
    pub scene_version: u32,
    /// Admission estimate for decoded frontend buffers and their GPU copies.
    #[serde(default)]
    pub estimated_bytes: usize,
    pub status: String,
    pub meshes: Vec<ChefPreviewMesh>,
    pub head_bounds: ChefPreviewBounds,
    pub diagnostics: Vec<String>,
}
