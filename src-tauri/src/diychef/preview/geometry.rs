use crate::models::ChefPreviewBounds;

#[derive(Clone, Debug, Default)]
pub(crate) struct Geometry {
    pub vertices: Vec<[f32; 8]>,
    pub indices: Vec<u32>,
}

impl Geometry {
    pub fn bounds(&self) -> ChefPreviewBounds {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in &self.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex[axis]);
                max[axis] = max[axis].max(vertex[axis]);
            }
        }
        if self.vertices.is_empty() {
            min = [-0.5; 3];
            max = [0.5; 3];
        }
        ChefPreviewBounds { min, max }
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.vertices.len() * 32 + self.indices.len() * 4);
        for vertex in &self.vertices {
            for value in vertex {
                bytes.extend(value.to_le_bytes());
            }
        }
        for index in &self.indices {
            bytes.extend(index.to_le_bytes());
        }
        bytes
    }
    pub fn reflect_x(&mut self) {
        for v in &mut self.vertices {
            v[0] = -v[0];
            v[3] = -v[3];
        }
        for tri in self.indices.chunks_exact_mut(3) {
            tri.swap(1, 2);
        }
    }
    pub fn apply(&mut self, matrix: &[f32; 16]) {
        for v in &mut self.vertices {
            let position = transform(matrix, [v[0], v[1], v[2]], 1.0);
            let normal = transform(matrix, [v[3], v[4], v[5]], 0.0);
            v[..3].copy_from_slice(&position);
            let length = normal
                .iter()
                .map(|v| v * v)
                .sum::<f32>()
                .sqrt()
                .max(0.00001);
            for axis in 0..3 {
                v[axis + 3] = normal[axis] / length;
            }
        }
    }
}
pub(crate) fn transform(m: &[f32; 16], v: [f32; 3], w: f32) -> [f32; 3] {
    std::array::from_fn(|r| {
        m[r * 4] * v[0] + m[r * 4 + 1] * v[1] + m[r * 4 + 2] * v[2] + m[r * 4 + 3] * w
    })
}
