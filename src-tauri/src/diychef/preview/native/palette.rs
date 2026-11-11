//! The first player colour is read from MainAvatarDirectory, never guessed from RGB.
use super::{
    avatar,
    reader::{Ref, Store},
};

#[derive(Clone)]
pub(in crate::diychef::preview) struct PlayerColour {
    pub material: Ref,
    pub mask: [f32; 4],
}

pub(super) fn read(store: &mut Store, source: Ref) -> Result<PlayerColour, String> {
    let raw = store.raw(source)?;
    let (pos, _) = avatar::cursor(&raw).ok_or("diychef.previewUnavailable")?;
    let material = store.resolve(
        source,
        &avatar::pointer(&raw, pos).ok_or("diychef.previewUnavailable")?,
    )?;
    let mut mask = [0.; 4];
    for (i, value) in mask.iter_mut().enumerate() {
        let start = pos + 12 + i * 4;
        *value = f32::from_le_bytes(
            raw.get(start..start + 4)
                .ok_or("diychef.previewUnavailable")?
                .try_into()
                .map_err(|_| "diychef.previewUnavailable")?,
        );
        if !value.is_finite() {
            return Err("diychef.previewUnavailable".into());
        }
    }
    Ok(PlayerColour { material, mask })
}
