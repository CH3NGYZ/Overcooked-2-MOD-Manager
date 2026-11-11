//! Unity TRS and static skeleton evaluation. No guessed hat offset is used.
use super::native::reader::{Map, Ref, Store};
use std::collections::{HashMap, HashSet};
use unityfs::UnityValue as V;
pub(super) type Matrix = [f32; 16];
pub(super) const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
#[derive(Clone)]
pub(super) struct Node {
    pub renderer: Option<Ref>,
    pub name: String,
    pub world: Matrix,
    pub parent: Option<usize>,
    pub active: bool,
}
pub(super) struct Tree {
    pub nodes: Vec<Node>,
    pub by_ref: HashMap<Ref, usize>,
}
pub(super) fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| (0..4).map(|k| a[i / 4 * 4 + k] * b[k * 4 + i % 4]).sum())
}
pub(super) fn number(v: &V) -> f32 {
    match v {
        V::Float(v) => *v,
        V::Double(v) => *v as f32,
        _ => v.as_i32().unwrap_or_default() as f32,
    }
}
pub(super) fn array<'a>(m: &'a Map, key: &str) -> &'a [V] {
    match m.get(key) {
        Some(V::Array(v)) => v,
        _ => &[],
    }
}
fn vector(m: &Map, key: &str, default: [f32; 4]) -> [f32; 4] {
    let Some(V::Map(v)) = m.get(key) else {
        return default;
    };
    std::array::from_fn(|i| {
        v.get(["x", "y", "z", "w"][i])
            .map(number)
            .unwrap_or(default[i])
    })
}
fn trs(m: &Map) -> Matrix {
    let p = vector(m, "m_LocalPosition", [0.; 4]);
    let [x, y, z, w] = vector(m, "m_LocalRotation", [0., 0., 0., 1.]);
    let s = vector(m, "m_LocalScale", [1.; 4]);
    [
        (1. - 2. * (y * y + z * z)) * s[0],
        2. * (x * y - z * w) * s[1],
        2. * (x * z + y * w) * s[2],
        p[0],
        2. * (x * y + z * w) * s[0],
        (1. - 2. * (x * x + z * z)) * s[1],
        2. * (y * z - x * w) * s[2],
        p[1],
        2. * (x * z - y * w) * s[0],
        2. * (y * z + x * w) * s[1],
        (1. - 2. * (x * x + y * y)) * s[2],
        p[2],
        0.,
        0.,
        0.,
        1.,
    ]
}
impl Tree {
    pub fn load(store: &mut Store, root: Ref) -> Result<Self, String> {
        let mut tree = Self {
            nodes: vec![],
            by_ref: HashMap::new(),
        };
        let go = store.object(root)?;
        let components = components(store, root, &go)?;
        let t = components
            .iter()
            .find(|r| {
                store.docs[r.file]
                    .file
                    .objects
                    .iter()
                    .any(|o| o.path_id == r.id && o.class_id == 4)
            })
            .copied()
            .ok_or("diychef.previewUnavailable")?;
        tree.walk(store, t, None, IDENTITY, &mut HashSet::new())?;
        Ok(tree)
    }
    fn walk(
        &mut self,
        store: &mut Store,
        t: Ref,
        parent: Option<usize>,
        world: Matrix,
        visited: &mut HashSet<Ref>,
    ) -> Result<(), String> {
        if !visited.insert(t) || self.nodes.len() >= 8192 {
            return Err("diychef.previewLimit".into());
        }
        let tr = store.object(t)?;
        let go_ref = store.resolve(
            t,
            tr.get("m_GameObject").ok_or("diychef.previewUnavailable")?,
        )?;
        let go = store.object(go_ref)?;
        let world = multiply(&world, &trs(&tr));
        let components = components(store, go_ref, &go)?;
        let renderer = components.into_iter().find(|r| {
            store.docs[r.file]
                .file
                .objects
                .iter()
                .any(|o| o.path_id == r.id && o.class_id == 137)
        });
        let index = self.nodes.len();
        self.by_ref.insert(t, index);
        self.nodes.push(Node {
            renderer,
            name: crate::asset_bundle::string_of(&go, "m_Name"),
            world,
            parent,
            active: !matches!(go.get("m_IsActive"), Some(V::Boolean(false))),
        });
        for child in array(&tr, "m_Children") {
            let r = store.resolve(t, child)?;
            self.walk(store, r, Some(index), world, visited)?;
        }
        Ok(())
    }
    pub fn name(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }
    pub fn below(&self, index: usize, root: usize) -> bool {
        let mut cursor = Some(index);
        while let Some(i) = cursor {
            if i == root {
                return true;
            }
            cursor = self.nodes[i].parent;
        }
        false
    }
    pub fn world(&self, r: Ref) -> Option<Matrix> {
        self.by_ref.get(&r).map(|i| self.nodes[*i].world)
    }
}
fn components(store: &mut Store, r: Ref, go: &Map) -> Result<Vec<Ref>, String> {
    array(go, "m_Component")
        .iter()
        .map(|v| {
            let V::Map(m) = v else {
                return Err("diychef.previewUnavailable".into());
            };
            store.resolve(r, m.get("component").ok_or("diychef.previewUnavailable")?)
        })
        .collect()
}
pub(super) fn bind(name: &str) -> Option<Matrix> {
    // Exact matrices from DIYChefFastInit 2.2 BindPoses.GetMatrix (constructor transposed).
    Some(match name {
        "Head" | "Head1" | "Head2" | "Body_Top" => [
            0., -1., 0., 0.58614, -1., 0., 0., 0., 0., 0., -1., 0., 0., 0., 0., 1.,
        ],
        "Eyes" | "Eyes2_Blinks" => [
            0., -1., 0., 0.91415, -1., 0., 0., 0., 0., 0., -1., 0.26908, 0., 0., 0., 1.,
        ],
        "Eyebrows" => [
            0., -1., 0., 0.98427, -1., 0., 0., 0., 0., 0., -1., 0.26070, 0., 0., 0., 1.,
        ],
        "Hair" => [
            0., -1., 0., 1.16386, -1., 0., 0., 0., 0., 0., -1., -0.10810, 0., 0., 0., 1.,
        ],
        "HatBase" => [
            0., -0.80064, 0.59915, 0., 0., 0.59915, 0.80064, 0., -1., 0., 0., 0., 0., 0., 0., 1.,
        ],
        "LeftHand" => [
            0.84462, -0.50944, -0.16458, 0.66203, 0.28076, 0.15975, 0.94639, -0.02605, -0.45583,
            -0.84555, 0.27796, 0.11712, 0., 0., 0., 1.,
        ],
        "RightHand" => [
            0.58976, -0.07015, -0.80453, -0.21272, -0.71165, -0.51609, -0.47667, 0.66973, -0.38177,
            0.85366, -0.35429, -0.1518, 0., 0., 0., 1.,
        ],
        "Jnt_Body" => [
            0., -1., 0., 0., -1., 0., 0., 0., 0., 0., -1., 0., 0., 0., 0., 1.,
        ],
        "Jnt_Tail" => [
            -0.08938, -0.96321, 0.25346, 0.4395, -0.94307, 0., -0.33258, -0.13868, 0.32035,
            -0.26875, -0.90838, -0.28563, 0., 0., 0., 1.,
        ],
        "NeckTie" => [
            0., -1., 0., 0.57805, -1., 0., 0., 0., 0., 0., -1., 0.27746, 0., 0., 0., 1.,
        ],
        "HeldItem" => [
            0.90715, 0.41948, 0.03323, -0.66467, -0.17699, 0.30871, 0.93455, -0.18004, 0.38177,
            -0.85366, 0.35429, 0.1518, 0., 0., 0., 1.,
        ],
        "Jnt_Wheelchair" => [
            1., 0., 0., -0.00383, 0., 1., 0., -0.43046, 0., 0., 1., 0.22224, 0., 0., 0., 1.,
        ],
        _ => return None,
    })
}
