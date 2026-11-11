use super::target::child;
use crate::models::{ChefHat, ChefItem};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};
#[derive(Clone)]
pub(crate) struct Catalog {
    pub chefs: Vec<ChefItem>,
    pub hats: Vec<ChefHat>,
    pub revision: String,
}
pub(crate) const NATIVE_NAMES: &str = r#"Chef_Beard
Chef_Black_cat
Chef_Gertie
Chef_Ginger_cat
Chef_Mike
Chef_Specs
Chef_Buck
Chef_Racoon
Chef_Dora
Chef_Granny_Grey
Chef_Alien_Green
Chef_Alien_Pink
Chef_Crocodile
Chef_Wizard_Boy
Chef_calico_cat
Chef_Male_Asian
Chef_mouse
Chef_middle_eastern
Chef_panda
Chef_Robot
Chef_mole
Chef_Native_American
Chef_Squirrel
Chef_eagle
Chef_miner
Chef_octopus
Chef_walrus
Chef_unicorn
Chef_monkey
Chef_Platypus
Chef_Pig
Chef_Boof
Chef_Maori
Chef_parrot
Chef_Elf
Chef_Snowman
Chef_dragon
Chef_ChinesePig
Chef_Rudolph
Chef_Box_Gift
Chef_Rat
Chef_Turtle
Chef_Bear
Chef_Beaver
Chef_Girlscout
Chef_Owl
Chef_FemalePirate
Chef_MalePirate
Chef_Mermaid
Chef_Narwhal
Chef_Shark
Chef_Box
Chef_Ghost
Chef_Vampire
Chef_Werewolf
Chef_Female_Clown
Chef_strongman
Chef_ringmaster
Chef_Stuntman
Chef_alaskan_malamute
Chef_brussel_griffon"#;
pub(crate) fn token(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | ':' | '='))
        && value != "."
        && value != ".."
        && !value.contains("LOBBYSWITCHCHEF=TRUE")
}
fn folders(game: &Path, root: &Path) -> Result<Vec<String>, String> {
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for e in fs::read_dir(root).map_err(|_| "diychef.readFailed")? {
        let e = e.map_err(|_| "diychef.readFailed")?;
        let name = e.file_name().to_string_lossy().into_owned();
        let p = child(game, root, &name)?;
        if p.is_dir() {
            out.push(name)
        }
    }
    out.sort();
    Ok(out)
}
pub(crate) fn read(game: &Path, root: &Path) -> Result<Catalog, String> {
    let mut chefs: Vec<_> = NATIVE_NAMES
        .lines()
        .map(|n| ChefItem {
            id: format!("native:{n}"),
            source_kind: "native".into(),
            display_name: n.trim_start_matches("Chef_").replace('_', " "),
            config_name: n.into(),
            head_name: n.into(),
            custom_id: None,
            resource_state: "known_name".into(),
            preview_state: "pending".into(),
        })
        .collect();
    let mut hats: Vec<_> = [
        ("", "auto"),
        ("None", "none"),
        ("Fancy", "native"),
        ("Santa", "native"),
        ("Baseballcap", "native"),
    ]
    .iter()
    .map(|(v, k)| ChefHat {
        id: format!("hat:{v}"),
        kind: (*k).into(),
        config_value: (*v).into(),
        display_name: if v.is_empty() {
            "Auto".into()
        } else {
            (*v).into()
        },
        resource_state: "known_name".into(),
        preview_state: "pending".into(),
    })
    .collect();
    let resources = child(game, root, "Resources")?;
    let mut stamp = String::new();
    for name in folders(game, &resources)? {
        if name == "HATS" {
            continue;
        }
        let dir = child(game, &resources, &name)?;
        let info = child(game, &dir, "INFO")?;
        let bytes = if info.is_file() {
            let m = fs::metadata(&info).map_err(|_| "diychef.readFailed")?;
            if m.len() > 65536 {
                return Err("diychef.tooLarge".into());
            }
            let mut bytes = Vec::new();
            fs::File::open(&info)
                .and_then(|file| file.take(65537).read_to_end(&mut bytes))
                .map_err(|_| "diychef.readFailed")?;
            if bytes.len() > 65536 {
                return Err("diychef.tooLarge".into());
            }
            bytes
        } else {
            vec![]
        };
        let text = super::prefer::decode(&bytes)
            .map(|x| x.0)
            .unwrap_or_default();
        let mut id = None;
        for line in text.lines() {
            if let Some(n) = line.strip_prefix("ID=") {
                id = n.trim().parse::<u8>().ok();
            }
        }
        let valid = token(&name)
            // The mod resolves native HeadNames before looking up DIY folders.
            && !NATIVE_NAMES.lines().any(|native| native == name)
            && id.is_some_and(|i| i != 255)
            && info.is_file()
            && child(game, &dir, "t_Head.png")?.is_file()
            && child(game, &dir, "m_Head.txt")?.is_file();
        stamp.push_str(&format!("{name}:{text}:{valid};"));
        chefs.push(ChefItem {
            id: format!("custom:{name}"),
            source_kind: "custom".into(),
            display_name: name.clone(),
            config_name: name.clone(),
            head_name: if name.starts_with("Chef_") {
                name.clone()
            } else {
                format!("Chef_{name}")
            },
            custom_id: id,
            resource_state: if valid { "valid" } else { "invalid" }.into(),
            preview_state: "pending".into(),
        });
    }
    let hatroot = child(game, &resources, "HATS")?;
    for name in folders(game, &hatroot)? {
        // Built-in values share the same config token and take precedence in
        // HatData; exposing another item would duplicate the authoritative id.
        if hats.iter().any(|h| h.config_value == name) {
            continue;
        }
        let dir = child(game, &hatroot, &name)?;
        let valid = token(&name)
            && child(game, &dir, &format!("t_{name}.png"))?.is_file()
            && child(game, &dir, &format!("m_{name}.txt"))?.is_file();
        stamp.push_str(&format!("hat:{name}:{valid};"));
        hats.push(ChefHat {
            id: format!("hat:{name}"),
            kind: "custom".into(),
            config_value: name.clone(),
            display_name: name,
            resource_state: if valid { "valid" } else { "invalid" }.into(),
            preview_state: "pending".into(),
        });
    }
    Ok(Catalog {
        chefs,
        hats,
        revision: format!("{:x}", Sha256::digest(stamp.as_bytes())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_catalog_needs_registration_resources_but_not_preview_obj() {
        let p = std::env::temp_dir().join(format!("oc2-chef-{}", uuid::Uuid::new_v4()));
        let chef = p.join("Resources/00-Fox");
        let hat = p.join("Resources/HATS/FoxHat");
        fs::create_dir_all(&chef).unwrap();
        fs::create_dir_all(&hat).unwrap();
        fs::write(chef.join("INFO"), b"ID=7\n").unwrap();
        fs::write(chef.join("t_Head.png"), b"metadata-only").unwrap();
        fs::write(chef.join("m_Head.txt"), b"metadata-only").unwrap();
        fs::write(hat.join("t_FoxHat.png"), b"metadata-only").unwrap();
        fs::write(hat.join("m_FoxHat.txt"), b"metadata-only").unwrap();
        fs::create_dir_all(p.join("Resources/HATS/Santa")).unwrap();
        let shadowed = p.join("Resources/Chef_Black_cat");
        fs::create_dir(&shadowed).unwrap();
        for (name, bytes) in [
            ("INFO", b"ID=8".as_slice()),
            ("t_Head.png", b"png".as_slice()),
            ("m_Head.txt", b"material".as_slice()),
        ] {
            fs::write(shadowed.join(name), bytes).unwrap();
        }
        let first = read(&p, &p).unwrap();
        assert_eq!(first.hats.iter().filter(|h| h.id == "hat:Santa").count(), 1);
        assert_eq!(
            first
                .chefs
                .iter()
                .find(|c| c.id == "custom:Chef_Black_cat")
                .unwrap()
                .resource_state,
            "invalid"
        );
        assert_eq!(
            first
                .chefs
                .iter()
                .filter(|c| c.source_kind == "native")
                .count(),
            61
        );
        let fox = first
            .chefs
            .iter()
            .find(|c| c.id == "custom:00-Fox")
            .unwrap();
        assert_eq!(fox.custom_id, Some(7));
        assert_eq!(fox.head_name, "Chef_00-Fox");
        assert_eq!(fox.resource_state, "valid");
        assert_eq!(
            first
                .hats
                .iter()
                .find(|h| h.id == "hat:FoxHat")
                .unwrap()
                .resource_state,
            "valid"
        );
        // Geometry and PNG byte updates are preview fingerprints, not catalog identity.
        fs::write(chef.join("t_Head.png"), b"updated texture").unwrap();
        assert_eq!(read(&p, &p).unwrap().revision, first.revision);
        fs::write(chef.join("INFO"), b"ID=255\n").unwrap();
        let changed = read(&p, &p).unwrap();
        assert_ne!(changed.revision, first.revision);
        assert_eq!(
            changed
                .chefs
                .iter()
                .find(|c| c.id == "custom:00-Fox")
                .unwrap()
                .resource_state,
            "invalid"
        );
        let verified = p.canonicalize().unwrap();
        assert!(verified.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(verified).unwrap();
    }
}
