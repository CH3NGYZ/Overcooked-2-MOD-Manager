//! Read declared menu tiles, preserving repeated slots and grouping edges.
use super::*;
impl Context {
    // Resolve a prefab exactly through WorkableItem -> IngredientProperties/PreparationContainer.
    pub(super) fn ingredient_order(&mut self, r: Ref, depth: usize) -> Result<Ref, String> {
        if depth > 8 {
            return Err(failure());
        }
        let m = self.object(r)?;
        if m.contains_key("m_iconSprite") {
            return Ok(r);
        }
        let mut ingredient = None;
        for component in array(&m, "m_Component").iter().take(64) {
            let V::Map(component) = component else {
                continue;
            };
            let Some(v) = component.get("component") else {
                continue;
            };
            let Ok(c) = self.resolve(r, v) else { continue };
            let Ok(cm) = self.object(c) else { continue };
            if let Some(v) = cm.get("m_nextPrefab") {
                if let Ok(next) = self.resolve(c, v) {
                    return self.ingredient_order(next, depth + 1);
                }
            }
            if let Some(v) = cm.get("m_ingredientOrderNode") {
                ingredient = self.resolve(c, v).ok();
            }
        }
        ingredient.ok_or_else(failure)
    }
    fn menu_icon(&mut self, r: Ref) -> MenuIcon {
        let name = self
            .object(r)
            .map(|m| string_of(&m, "m_Name"))
            .unwrap_or_default();
        let data_url = if self.menu_budget > 0 {
            self.menu_budget -= 1;
            self.image(r, 128).ok().map(|i| i.0)
        } else {
            None
        };
        MenuIcon { name, data_url }
    }
    fn menu_icons(&mut self, r: Ref, m: &Map, key: &str) -> Vec<MenuIcon> {
        let steps = key == "m_modifierPictures";
        array(m, key)
            .iter()
            .take(64)
            .map(|v| {
                self.resolve(r, v)
                    .map(|r| {
                        let mut icon = self.menu_icon(r);
                        if steps {
                            normalize_step(&mut icon);
                        }
                        icon
                    })
                    .unwrap_or(MenuIcon {
                        name: String::new(),
                        data_url: None,
                    })
            })
            .collect()
    }
    fn declared_tiles(&mut self, r: Ref, m: &Map) -> Vec<MenuTile> {
        let entries = array(m, "m_orderGuiDescription");
        entries
            .iter()
            .take(96)
            .map(|entry| {
                let V::Map(entry) = entry else {
                    return MenuTile {
                        pictures: vec![],
                        modifiers: vec![],
                        children: vec![],
                        large: false,
                    };
                };
                let tile = match entry.get("m_tileDefinition") {
                    Some(V::Map(m)) => m,
                    _ => entry,
                };
                MenuTile {
                    pictures: self.menu_icons(r, tile, "m_mainPictures"),
                    modifiers: self.menu_icons(r, tile, "m_modifierPictures"),
                    children: array(entry, "m_children")
                        .iter()
                        .filter_map(|v| match v {
                            V::Int32(i) if *i >= 0 => Some(*i as usize),
                            _ => None,
                        })
                        .filter(|i| *i < entries.len().min(96))
                        .collect(),
                    large: int_of(tile, "m_largeIcon") != 0,
                }
            })
            .collect()
    }
    fn named_icon(&mut self, r: Ref, m: &Map, direct: &str, pseudo: &str) -> Option<MenuIcon> {
        if let Some(v) = m.get(direct) {
            if let Ok(target) = self.resolve(r, v) {
                return Some(self.menu_icon(target));
            }
        }
        let target = self.resolve(r, m.get(pseudo)?).ok()?;
        let object = self.object(target).ok()?;
        let target = self
            .game_asset(
                &string_of(&object, "bundleName"),
                &string_of(&object, "assetPath"),
            )
            .ok()?;
        Some(self.menu_icon(target))
    }
    // Same flatten/split rule as DIYLevel RecipeHelper.GetTileDefinition.
    fn ingredient_tile(&mut self, r: Ref, depth: usize) -> MenuTile {
        let mut tile = self.ingredient_tile_inner(r, depth);
        if tile.pictures.is_empty() && tile.modifiers.is_empty() {
            let name = self
                .object(r)
                .map(|m| string_of(&m, "prefabName"))
                .unwrap_or_default();
            tile.pictures.push(MenuIcon {
                name,
                data_url: None,
            });
        }
        tile
    }
    fn ingredient_tile_inner(&mut self, r: Ref, depth: usize) -> MenuTile {
        let mut empty = MenuTile {
            pictures: vec![],
            modifiers: vec![],
            children: vec![],
            large: false,
        };
        if depth > 8 || self.menu_budget == 0 {
            return empty;
        }
        self.menu_budget -= 1;
        let Ok(m) = self.object(r) else { return empty };
        let bundle = string_of(&m, "bundleName");
        let path = string_of(&m, "assetPath");
        if !bundle.is_empty() && !path.is_empty() {
            if let Ok(target) = self.game_asset(&bundle, &path) {
                return self.ingredient_tile(target, depth + 1);
            }
            return empty;
        }
        if m.contains_key("m_Component") {
            return match self.ingredient_order(r, 0) {
                Ok(target) => self.ingredient_tile(target, depth + 1),
                Err(_) => empty,
            };
        }
        let tiles = self.declared_tiles(r, &m);
        if !tiles.is_empty() {
            for tile in tiles.iter().skip(usize::from(tiles.len() > 1)) {
                empty.pictures.extend(tile.pictures.clone());
                empty.modifiers.extend(tile.modifiers.clone());
            }
            return empty;
        }
        let composition = array(&m, "compositionSOs");
        if !composition.is_empty() {
            for v in composition.iter().take(64) {
                if let Ok(child) = self.resolve(r, v) {
                    let mut tile = self.ingredient_tile(child, depth + 1);
                    empty.pictures.append(&mut tile.pictures);
                    empty.modifiers.append(&mut tile.modifiers);
                }
            }
            let kind = custom_kind(&m);
            let step = match kind {
                2 => self.named_icon(r, &m, "cookingStepIcon", "cookingStepIconSO"),
                3 => self.named_icon(r, &m, "mixingIcon", "mixingIconSO"),
                _ => None,
            };
            if matches!(kind, 2 | 3) {
                let mut step = step.unwrap_or(MenuIcon {
                    name: String::new(),
                    data_url: None,
                });
                normalize_step(&mut step);
                empty.modifiers.push(step);
            }
        } else if let Some(icon) = self
            .named_icon(r, &m, "m_iconSprite", "iconSO")
            .or_else(|| self.named_icon(r, &m, "icon", "iconSO"))
        {
            empty.pictures.push(icon)
        }
        empty
    }
    // Keep processed sub-recipes as groups. A cooking icon is never an ingredient.
    fn append_group(&mut self, r: Ref, depth: usize, out: &mut Vec<MenuTile>) -> Option<usize> {
        if depth > 8 || out.len() >= 96 || self.menu_budget == 0 {
            return None;
        }
        self.menu_budget -= 1;
        let m = self.object(r).ok()?;
        let bundle = string_of(&m, "bundleName");
        let path = string_of(&m, "assetPath");
        if !bundle.is_empty() && !path.is_empty() {
            let target = self.game_asset(&bundle, &path).ok()?;
            return self.append_group(target, depth + 1, out);
        }
        let declared = self.declared_tiles(r, &m);
        if !declared.is_empty() {
            let offset = out.len();
            if offset + declared.len() > 96 {
                return None;
            }
            for mut tile in declared {
                tile.children = tile.children.into_iter().map(|i| i + offset).collect();
                out.push(tile);
            }
            return Some(offset);
        }
        let composition = array(&m, "compositionSOs");
        if composition.is_empty() {
            let tile = self.ingredient_tile(r, depth);
            let index = out.len();
            out.push(tile);
            return Some(index);
        }
        let index = out.len();
        out.push(MenuTile {
            pictures: vec![],
            modifiers: vec![],
            children: vec![],
            large: false,
        });
        for v in composition.iter().take(64) {
            let child = self.resolve(r, v).ok();
            let child_index = child.and_then(|child| self.append_group(child, depth + 1, out));
            if let Some(child_index) = child_index {
                // Raw ingredients share a row; processed ingredients keep their own group.
                if out[child_index].children.is_empty()
                    && out[child_index].modifiers.is_empty()
                    && child_index + 1 == out.len()
                {
                    let mut raw = out.pop().unwrap();
                    out[index].pictures.append(&mut raw.pictures);
                } else {
                    out[index].children.push(child_index);
                }
            } else {
                let name = child
                    .and_then(|child| self.object(child).ok())
                    .map(|m| string_of(&m, "m_Name"))
                    .unwrap_or_default();
                out[index].pictures.push(MenuIcon {
                    name,
                    data_url: None,
                });
            }
        }
        let step = match custom_kind(&m) {
            2 => self.named_icon(r, &m, "cookingStepIcon", "cookingStepIconSO"),
            3 => self.named_icon(r, &m, "mixingIcon", "mixingIconSO"),
            _ => None,
        };
        if let Some(mut step) = step {
            normalize_step(&mut step);
            out[index].modifiers.push(step);
        }
        // A single processing chain can share the same tile while keeping steps below.
        if out[index].pictures.is_empty() && out[index].children.len() == 1 {
            let child = out[index].children[0];
            if out[child].children.is_empty() {
                out[index].pictures = std::mem::take(&mut out[child].pictures);
                let mut steps = std::mem::take(&mut out[child].modifiers);
                steps.append(&mut out[index].modifiers);
                out[index].modifiers = steps;
                out[index].children.clear();
            }
        }
        Some(index)
    }
    pub(super) fn menu_tiles(
        &mut self,
        r: Ref,
        m: &Map,
        name: &str,
        icon: Option<&str>,
    ) -> Vec<MenuTile> {
        let declared = self.declared_tiles(r, m);
        if !declared.is_empty() {
            return declared;
        }
        if !m.contains_key("compositionSOs") {
            return vec![];
        }
        let mut tiles = vec![MenuTile {
            pictures: vec![MenuIcon {
                name: name.into(),
                data_url: icon.map(str::to_owned),
            }],
            modifiers: vec![],
            children: vec![],
            large: true,
        }];
        if let Some(child) = self.append_group(r, 0, &mut tiles) {
            tiles[0].children.push(child);
        }
        tiles
    }
}
// Only monochrome cooking symbols: retain black coverage, discard white artwork.
fn normalize_step(icon: &mut MenuIcon) {
    use base64::Engine;
    let Some(url) = &icon.data_url else { return };
    let Some(data) = url.strip_prefix("data:image/png;base64,") else {
        return;
    };
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return;
    };
    let Ok(image) = image::load_from_memory(&bytes) else {
        return;
    };
    let mut image = image.to_rgba8();
    for pixel in image.pixels_mut() {
        let darkness = 255 - pixel[0].max(pixel[1]).max(pixel[2]);
        pixel[3] = ((u16::from(pixel[3]) * u16::from(darkness)) / 255) as u8;
        pixel[0] = 0;
        pixel[1] = 0;
        pixel[2] = 0;
    }
    let mut out = std::io::Cursor::new(Vec::new());
    if image::DynamicImage::ImageRgba8(image)
        .write_to(&mut out, image::ImageFormat::Png)
        .is_ok()
    {
        icon.data_url = Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(out.into_inner())
        ));
    }
}
fn custom_kind(m: &Map) -> i32 {
    let kind = int_of(m, "type");
    if kind != 0 {
        kind
    } else if matches!(m.get("cookingStepSO"),Some(V::PPtr{path_id,..}) if *path_id!=0) {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cooking_symbol_keeps_black_and_discards_white_with_alpha() {
        use base64::Engine;
        let image =
            image::RgbaImage::from_raw(3, 1, vec![0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 80])
                .unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let mut icon = MenuIcon {
            name: "OvenPot".into(),
            data_url: Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png.into_inner())
            )),
        };
        normalize_step(&mut icon);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(icon.data_url.unwrap().split(',').nth(1).unwrap())
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(0, 0)[3], 255);
        assert_eq!(image.get_pixel(1, 0)[3], 0);
        assert_eq!(image.get_pixel(2, 0)[3], 80);
    }
    #[test]
    #[ignore = "requires installed pear fixture"]
    fn real_pear_missing_icons() {
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let local = game.join("BepInEx/plugins/OC2DIYLevel/levels/pear_v11.0(菊花梨）");
        let info = crate::level_sets::scan::locate_info_file(&local).unwrap();
        let mut c = Context::load(&read_limited(&info).unwrap(), &local, game).unwrap();
        let recipes = c.recipes(0).unwrap();
        assert_eq!(recipes[14].name, "菠萝串");
        assert!(
            recipes[14].icon.is_some(),
            "cross-set info sprite must resolve"
        );
        assert!(recipes[14].tiles[0].pictures[0].data_url.is_some());
        let oven = recipes[38]
            .tiles
            .iter()
            .flat_map(|t| &t.modifiers)
            .find(|i| i.name == "OvenPot")
            .unwrap();
        assert!(oven.data_url.is_some());
    }

    #[test]
    fn declared_menu_preserves_repeated_ingredients_steps_and_group_edges() {
        let mut c = super::super::tests::empty_context();
        let reference = V::PPtr {
            file_id: 0,
            path_id: 0,
        };
        let mut tile = Map::new();
        tile.insert(
            "m_mainPictures".into(),
            V::Array(vec![reference.clone(), reference.clone()]),
        );
        tile.insert("m_modifierPictures".into(), V::Array(vec![reference]));
        let entry = V::Map(Map::from([
            ("m_tileDefinition".into(), V::Map(tile)),
            (
                "m_children".into(),
                V::Array(vec![V::Int32(1), V::Int32(-1), V::Int32(99)]),
            ),
        ]));
        let map = Map::from([(
            "m_orderGuiDescription".into(),
            V::Array(vec![entry.clone(), entry]),
        )]);
        let tiles = c.declared_tiles(Ref { file: 0, id: 0 }, &map);
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0].pictures.len(), 2);
        assert_eq!(tiles[0].modifiers.len(), 1);
        assert_eq!(tiles[0].children, vec![1]);
        assert!(tiles[0].pictures.iter().all(|i| i.data_url.is_none()));
    }
    #[test]
    fn legacy_custom_type_follows_loader_and_does_not_treat_optional_as_ingredients() {
        assert_eq!(custom_kind(&Map::new()), 1);
        assert_eq!(
            custom_kind(&Map::from([(
                "cookingStepSO".into(),
                V::PPtr {
                    file_id: 0,
                    path_id: 2
                }
            )])),
            2
        );
        for kind in [1, 2, 3] {
            assert_eq!(
                custom_kind(&Map::from([("type".into(), V::Int32(kind))])),
                kind
            );
        }
    }
    #[test]
    #[ignore = "requires installed Dmm and game bundle428"]
    fn real_dmm_corn_ingredient() {
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let local = game.join("BepInEx/plugins/OC2DIYLevel/levels/Dmm");
        let mut c = Context::load(
            &read_limited(&local.join("info_dmm")).unwrap(),
            &local,
            game,
        )
        .unwrap();
        let recipes = c.recipes(4).unwrap();
        assert_eq!(recipes.len(), 1);
        let icons = recipes[0]
            .tiles
            .iter()
            .flat_map(|t| &t.pictures)
            .map(|i| (&i.name, i.data_url.is_some()))
            .collect::<Vec<_>>();
        eprintln!("DMM corn icons={icons:?}");
        let document = c
            .documents
            .iter()
            .find(|d| d.source == "bundle428")
            .unwrap();
        assert!(document.backing.is_some());
        assert!(
            document.file.data.len() < 16 * 1024 * 1024,
            "retain only metadata, not 218MiB CAB"
        );
        eprintln!(
            "bundle428 retained metadata bytes={}",
            document.file.data.len()
        );

        assert!(
            icons
                .iter()
                .any(|(n, ready)| n.to_ascii_lowercase().contains("corn") && *ready),
            "corn ingredient must have an image"
        );
    }
    #[test]
    #[ignore = "requires installed eon map fixture"]
    fn real_nested_cooking_groups() {
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let local = game.join("BepInEx/plugins/OC2DIYLevel/levels/eon_v0.2.1(长恒)");
        let info = std::fs::read_dir(&local)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("info"))
            .unwrap();
        let mut c = Context::load(&read_limited(&info).unwrap(), &local, game).unwrap();
        let recipes = c.recipes(1).unwrap();
        let mut nested = 0;
        for recipe in &recipes {
            for tile in recipe.tiles.iter().skip(1) {
                assert!(
                    tile.pictures.iter().all(|i| !matches!(
                        i.name.as_str(),
                        "Boiled" | "Pot" | "FryingPan" | "Mixer" | "Blender"
                    )),
                    "step in ingredient row: {}",
                    recipe.name
                );
                if !tile.children.is_empty() {
                    nested += 1;
                }
            }
        }
        assert!(
            nested > 0,
            "processed ingredient groups must remain separate"
        );
        eprintln!("EON recipes={} nested_groups={nested}", recipes.len());
    }
    #[test]
    #[ignore = "requires installed onion soup fixture"]
    fn real_onion_ingredients() {
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let local = game.join("BepInEx/plugins/OC2DIYLevel/levels/mianhua_v1.5.4(棉花)");
        let info = std::fs::read_dir(&local)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("info"))
            .unwrap();
        let mut c = Context::load(&read_limited(&info).unwrap(), &local, game).unwrap();
        let recipes = c.recipes(0).unwrap();
        let soup = recipes.iter().find(|r| r.name == "Soup_Onion").unwrap();
        eprintln!(
            "ONION {:?} failed={:?}",
            soup.tiles
                .iter()
                .map(|t| t
                    .pictures
                    .iter()
                    .map(|i| (&i.name, i.data_url.is_some()))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            c.failed
        );
        assert_eq!(
            soup.tiles
                .iter()
                .skip(1)
                .flat_map(|t| &t.pictures)
                .filter(|i| i.data_url.is_some())
                .count(),
            3
        );
    }
    #[test]
    #[ignore = "requires installed map and optional plugin resources"]
    fn real_plugin_menu() {
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let local = game.join("BepInEx/plugins/OC2DIYLevel/levels/mianhua_v1.5.4(棉花)");
        let info = std::fs::read_dir(&local)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("info"))
            .unwrap();
        let mut c = Context::load(&read_limited(&info).unwrap(), &local, game).unwrap();
        // Resolve an actual reference to the optional W3 CAB; its presence in externals
        // alone does not mean the selected menus use it.
        let (file, external) = c
            .documents
            .iter()
            .enumerate()
            .find_map(|(i, d)| {
                d.file
                    .externals
                    .iter()
                    .position(|e| {
                        e.path_name
                            .to_ascii_lowercase()
                            .contains("cab-c18ca7d6e9dc6a9105401bbbf18a0f21")
                    })
                    .map(|e| (i, e))
            })
            .expect("fixture references W3");
        c.ensure_external(
            Ref { file, id: 0 },
            &V::PPtr {
                file_id: (external + 1) as i32,
                path_id: 1,
            },
        );
        assert!(c.documents.iter().any(|d| d.source == "commonW3"));
        let mut icons = 0;
        let mut steps = 0;
        let mut groups = 0;
        for index in 0..c.levels.len() {
            let recipes = c.recipes(index).unwrap();
            for recipe in recipes {
                for tile in recipe.tiles {
                    groups += usize::from(!tile.children.is_empty());
                    steps += tile.modifiers.len();
                    icons += tile
                        .pictures
                        .iter()
                        .filter(|i| i.data_url.is_some())
                        .count();
                }
            }
        }
        assert!(icons > 10 && steps > 10 && groups > 10);
        eprintln!("PLUGIN MENU icons={icons} steps={steps} groups={groups} commonW3 loaded");
    }
}
