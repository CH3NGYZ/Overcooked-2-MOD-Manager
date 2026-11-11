//! FastInit's allowed OBJ parts and exact rigid/two-bone binding rules.
use super::{
    assembly::{self, Matrix, Tree, multiply},
    geometry::{Geometry, transform},
    native,
};

/// Exact allowedPartToBone from FastInit 2.2; Body_Body is the sole two-bone part.
pub(super) const CUSTOM: &[(&str, &str)] = &[
    ("Eyes", "Eyes"),
    ("Eyebrows", "Eyebrows"),
    ("Eyes2_Blinks", "Eyes2_Blinks"),
    ("Hand_Grip_L", "LeftHand"),
    ("Hand_Grip_R", "RightHand"),
    ("Hand_Open_L", "LeftHand"),
    ("Hand_Open_R", "RightHand"),
    ("Tail", "Jnt_Tail"),
    ("Head", "Head"),
    ("Head1", "Head"),
    ("Head2", "Head"),
    ("Body_NeckTie", "NeckTie"),
    ("Body_Top", "Body_Top"),
    ("Body_Bottom", "Jnt_Body"),
    ("Body_Tail", "Jnt_Tail"),
    ("Body_Body", ""),
    ("Wheelchair", "Jnt_Wheelchair"),
    ("Knife", "HeldItem"),
];

pub(super) fn is_head(name: &str) -> bool {
    matches!(
        name,
        "Head" | "Head1" | "Head2" | "Eyes" | "Eyebrows" | "Eyes2_Blinks"
    )
}

pub(super) fn apply(geometry: &mut Geometry, tree: &Tree, name: &str) -> Result<(), String> {
    let bone = CUSTOM
        .iter()
        .find_map(|(part, bone)| (*part == name).then_some(*bone))
        .ok_or("diychef.previewUnavailable")?;
    let matrix = |bone| -> Result<Matrix, String> {
        Ok(multiply(
            &native::bone(tree, bone)?,
            &assembly::bind(bone).ok_or("diychef.previewUnavailable")?,
        ))
    };
    if name == "Body_Body" {
        apply_body(geometry, &matrix("Body_Top")?, &matrix("Jnt_Body")?);
    } else {
        geometry.apply(&matrix(bone)?);
    }
    Ok(())
}

// C# compares the f32 position against double literals. Preserve that boundary
// behavior as well as the six weights from SkinWeights.Create.
fn top_weight(y: f32) -> f32 {
    let y = f64::from(y);
    if y > 0.54 {
        1.0
    } else if y > 0.46 {
        0.75
    } else if y > 0.38 {
        0.5
    } else if y > 0.3 {
        0.3
    } else if y > 0.22 {
        0.1
    } else {
        0.0
    }
}

fn apply_body(geometry: &mut Geometry, top: &Matrix, bottom: &Matrix) {
    for vertex in &mut geometry.vertices {
        let weight = top_weight(vertex[1]);
        let position = [vertex[0], vertex[1], vertex[2]];
        let normal = [vertex[3], vertex[4], vertex[5]];
        let a = transform(top, position, 1.);
        let b = transform(bottom, position, 1.);
        let na = transform(top, normal, 0.);
        let nb = transform(bottom, normal, 0.);
        let blended: [f32; 3] = std::array::from_fn(|i| na[i] * weight + nb[i] * (1. - weight));
        let length = blended
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt()
            .max(0.00001);
        for i in 0..3 {
            vertex[i] = a[i] * weight + b[i] * (1. - weight);
            vertex[i + 3] = blended[i] / length;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_allowed_part_has_the_mods_binding_matrix() {
        assert_eq!(CUSTOM.len(), 18);
        for (part, bone) in CUSTOM {
            if *part == "Body_Body" {
                assert!(assembly::bind("Body_Top").is_some());
                assert!(assembly::bind("Jnt_Body").is_some());
            } else {
                assert!(assembly::bind(bone).is_some(), "{part}: {bone}");
            }
        }
        assert!(assembly::bind("invented").is_none());
    }
    #[test]
    fn body_skin_uses_original_height_to_blend_the_two_real_bones() {
        let heights = [0.2, 0.25, 0.35, 0.42, 0.5, 0.6];
        let mut geometry = Geometry {
            vertices: heights.map(|y| [0., y, 0., 0., 1., 0., 0.2, 0.7]).to_vec(),
            indices: vec![],
        };
        let mut top = assembly::IDENTITY;
        top[3] = 10.;
        let mut bottom = assembly::IDENTITY;
        bottom[3] = -10.;
        apply_body(&mut geometry, &top, &bottom);
        for ((vertex, height), expected) in geometry
            .vertices
            .iter()
            .zip(heights)
            .zip([-10., -8., -4., 0., 5., 10.])
        {
            assert!((vertex[0] - expected).abs() < 0.0001);
            assert_eq!(vertex[1], height);
            assert_eq!(&vertex[3..], &[0., 1., 0., 0.2, 0.7]);
        }
    }
}
