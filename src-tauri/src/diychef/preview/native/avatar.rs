//! Checked 2017 ChefAvatarData/MainAvatarDirectory layouts, when game typetrees omit scripts.
use super::reader::Avatar;
use unityfs::UnityValue as V;

pub(super) fn cursor(raw: &[u8]) -> Option<(usize, String)> {
    if raw.len() < 32 {
        return None;
    }
    let n = i32::from_le_bytes(raw[28..32].try_into().ok()?);
    if !(0..=256).contains(&n) || 32 + n as usize > raw.len() {
        return None;
    }
    let name = std::str::from_utf8(&raw[32..32 + n as usize])
        .ok()?
        .to_owned();
    Some(((32 + n as usize + 3) & !3, name))
}
pub(super) fn pointer(raw: &[u8], pos: usize) -> Option<V> {
    let data = raw.get(pos..pos + 12)?;
    Some(V::PPtr {
        file_id: i32::from_le_bytes(data[..4].try_into().ok()?),
        path_id: i64::from_le_bytes(data[4..].try_into().ok()?),
    })
}
pub(super) fn read(raw: &[u8], file: usize) -> Option<Avatar> {
    let (pos, _) = cursor(raw)?;
    let count = i32::from_le_bytes(raw.get(pos + 36..pos + 40)?.try_into().ok()?);
    if !(5..=128).contains(&count) {
        return None;
    }
    let name = std::str::from_utf8(raw.get(pos + 40..pos + 40 + count as usize)?)
        .ok()?
        .to_owned();
    if !name.starts_with("Chef_") {
        return None;
    }
    let prefabs = [
        pointer(raw, pos)?,
        pointer(raw, pos + 12)?,
        pointer(raw, pos + 24)?,
    ];
    // MainAvatarDirectory also contains placeholder/DLC records with null prefabs.
    // They must not overwrite the populated ChefAvatarData loaded from a DLC bundle.
    if !matches!(prefabs[1],V::PPtr{file_id,path_id}if file_id>=0&&path_id!=0) {
        return None;
    }
    let mode_pos = (pos + 40 + count as usize + 3) & !3;
    let colourisation_mode = i32::from_le_bytes(raw.get(mode_pos..mode_pos + 4)?.try_into().ok()?);
    if !matches!(colourisation_mode, 0 | 1) {
        return None;
    }
    Some(Avatar {
        name,
        prefabs,
        file,
        colourisation_mode,
    })
}
fn directory_entries(raw: &[u8]) -> Option<(usize, usize)> {
    let (pos, name) = cursor(raw)?;
    if !name.eq_ignore_ascii_case("MainAvatarDirectory") {
        return None;
    }
    let count = i32::from_le_bytes(raw.get(pos + 4..pos + 8)?.try_into().ok()?);
    if !(16..=256).contains(&count) {
        return None;
    }
    raw.get(pos + 8..pos + 8 + count as usize * 12)?;
    Some((pos, count as usize))
}
pub(super) fn default_reference(raw: &[u8]) -> Option<V> {
    let (pos, _) = directory_entries(raw)?;
    pointer(raw, pos + 8 + 15 * 12)
}
pub(super) fn first_colour(raw: &[u8]) -> Option<V> {
    let (pos, count) = directory_entries(raw)?;
    let colours = pos + 8 + count * 12;
    let count = i32::from_le_bytes(raw.get(colours..colours + 4)?.try_into().ok()?);
    if !(1..=16).contains(&count) {
        return None;
    }
    pointer(raw, colours + 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 28];
        bytes.extend((name.len() as i32).to_le_bytes());
        bytes.extend(name);
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        bytes
    }

    fn avatar() -> Vec<u8> {
        let name = b"Chef_Pig";
        let mut bytes = named(name);
        for id in [11_i64, 22, 33] {
            bytes.extend(0_i32.to_le_bytes());
            bytes.extend(id.to_le_bytes());
        }
        bytes.extend((name.len() as i32).to_le_bytes());
        bytes.extend(name);
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        bytes.extend(1_i32.to_le_bytes());
        bytes
    }

    #[test]
    fn ignores_null_frontend_dlc_placeholder_and_keeps_real_avatar() {
        let mut bytes = avatar();
        let pointers = cursor(&bytes).unwrap().0;
        bytes[pointers + 16..pointers + 24].copy_from_slice(&0_i64.to_le_bytes());
        assert!(read(&bytes, 0).is_none());
        bytes[pointers + 16..pointers + 24].copy_from_slice(&22_i64.to_le_bytes());
        let avatar = read(&bytes, 0).unwrap();
        assert_eq!(avatar.name, "Chef_Pig");
        assert_eq!(avatar.colourisation_mode, 1);
        assert!(matches!(avatar.prefabs[1], V::PPtr { path_id: 22, .. }));
    }

    #[test]
    fn rejects_missing_or_unknown_body_colour_mode() {
        let mut bytes = avatar();
        let mode = bytes.len() - 4;
        assert!(read(&bytes[..mode], 0).is_none());
        bytes[mode..].copy_from_slice(&2_i32.to_le_bytes());
        assert!(read(&bytes, 0).is_none());
        bytes[mode..].copy_from_slice(&0_i32.to_le_bytes());
        assert_eq!(read(&bytes, 0).unwrap().colourisation_mode, 0);
    }

    #[test]
    fn directory_reads_default_avatar_and_first_colour_from_separate_arrays() {
        let mut bytes = named(b"MainAvatarDirectory");
        let pointers = bytes.len();
        bytes.extend(0_i32.to_le_bytes());
        bytes.extend(16_i32.to_le_bytes());
        for id in 1_i64..=16 {
            bytes.extend(0_i32.to_le_bytes());
            bytes.extend(id.to_le_bytes());
        }
        bytes.extend(1_i32.to_le_bytes());
        bytes.extend(2_i32.to_le_bytes());
        bytes.extend(700_i64.to_le_bytes());
        assert!(matches!(
            default_reference(&bytes),
            Some(V::PPtr { path_id: 16, .. })
        ));
        assert!(matches!(
            first_colour(&bytes),
            Some(V::PPtr {
                file_id: 2,
                path_id: 700
            })
        ));
        assert!(first_colour(&bytes[..bytes.len() - 1]).is_none());
        bytes[pointers + 8 + 16 * 12..pointers + 12 + 16 * 12]
            .copy_from_slice(&0_i32.to_le_bytes());
        assert!(first_colour(&bytes).is_none());
    }
}
