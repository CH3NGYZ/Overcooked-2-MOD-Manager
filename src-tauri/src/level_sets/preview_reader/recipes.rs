//! Static recipe declarations and optional game-resource enrichment.
use super::*;
impl Context {
    pub(super) fn recipe(&mut self, r: Ref, depth: usize) -> Result<Recipe, String> {
        if depth > 8 || self.recipe_budget == 0 {
            return Err(failure());
        }
        self.recipe_budget -= 1;
        if let Some(recipe) = self.recipe_cache.get(&(r.file, r.id, depth <= 1)) {
            return Ok(recipe.clone());
        }
        let recipe = self.recipe_inner(r, depth)?;
        if recipe.resolved && self.recipe_cache.len() < 1024 {
            self.recipe_cache
                .insert((r.file, r.id, depth <= 1), recipe.clone());
        }
        Ok(recipe)
    }
    fn recipe_inner(&mut self, r: Ref, depth: usize) -> Result<Recipe, String> {
        if depth > 8 {
            return Err(failure());
        }
        let m = self.object(r)?;
        if m.contains_key("m_Component") {
            let target = self.ingredient_order(r, 0)?;
            return self.recipe(target, depth + 1);
        }
        let bundle = string_of(&m, "bundleName");
        let path = string_of(&m, "assetPath");
        let score = m.get("score").map(|_| int_of(&m, "score"));
        if !bundle.is_empty() && !path.is_empty() {
            let mut out = match self
                .game_asset(&bundle, &path)
                .and_then(|target| self.recipe(target, depth + 1))
            {
                Ok(recipe) => recipe,
                Err(_) => Recipe {
                    name: string_of(&m, "prefabName"),
                    score,
                    active: true,
                    resolved: false,
                    ingredients: vec![],
                    icon: None,
                    source: String::new(),
                    error: Some(failure()),
                    tiles: vec![],
                },
            };
            out.source = format!("{bundle} · {path}");
            if score.is_some() {
                out.score = score;
            }
            return Ok(out);
        }
        let name = ["recipeName", "m_Name"]
            .iter()
            .map(|k| string_of(&m, k))
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        let mut ingredients = Vec::new();
        for key in ["compositionSOs", "m_composition"] {
            for v in array(&m, key).iter().take(64) {
                if let Ok(target) = self.resolve(r, v) {
                    if let Ok(item) = self.recipe(target, depth + 1) {
                        ingredients.push(item.name);
                    }
                }
            }
        }
        let mut icon = None;
        // Ingredient traversal only needs names; do not decode all ingredient icons.
        if depth <= 1 {
            for key in ["icon", "sprite", "m_Icon", "m_iconSprite"] {
                if let Some(v) = m.get(key) {
                    if let Ok(target) = self.resolve(r, v) {
                        icon = self.image(target, 128).ok().map(|x| x.0);
                        if icon.is_some() {
                            break;
                        }
                    }
                }
            }
            if icon.is_none() {
                if let Some(V::Map(first)) = array(&m, "m_orderGuiDescription").first() {
                    if let Some(V::Map(tile)) = first.get("m_tileDefinition") {
                        if let Some(v) = array(tile, "m_mainPictures").first() {
                            icon = self
                                .resolve(r, v)
                                .and_then(|target| self.image(target, 128))
                                .ok()
                                .map(|x| x.0);
                        }
                    }
                }
            }
            if icon.is_none() {
                if let Some(v) = m.get("iconSO") {
                    if let Ok(target) = self.resolve(r, v) {
                        if let Ok(pseudo) = self.object(target) {
                            if let Ok(asset) = self.game_asset(
                                &string_of(&pseudo, "bundleName"),
                                &string_of(&pseudo, "assetPath"),
                            ) {
                                icon = self.image(asset, 128).ok().map(|x| x.0);
                            }
                        }
                    }
                }
            }
        }
        let tiles = if depth <= 1 {
            self.menu_tiles(r, &m, &name, icon.as_deref())
        } else {
            vec![]
        };
        Ok(Recipe {
            resolved: !name.is_empty(),
            name,
            score,
            active: true,
            ingredients,
            icon,
            source: self.documents[r.file].source.clone(),
            error: None,
            tiles,
        })
    }
    pub(super) fn game_asset(&mut self, bundle: &str, path: &str) -> Result<Ref, String> {
        // Coordinate only: reject traversal, absolute paths and stream syntax before reading.
        for base in sources::game_resource_roots(&self.game_root) {
            if let Some(p) = self.safe_file(&base, bundle) {
                self.load_file(&p)?;
                break;
            }
        }
        // Build each container index once; m_PreloadTable can contain thousands of entries.
        for i in 0..self.documents.len() {
            if !self.indexed_docs.insert(i) {
                continue;
            }
            let ids: Vec<_> = self.documents[i]
                .file
                .objects
                .iter()
                .filter(|o| o.class_id == 142)
                .map(|o| o.path_id)
                .collect();
            for id in ids {
                let r = Ref { file: i, id };
                let m = self.object(r)?;
                for pair in array(&m, "m_Container") {
                    if let V::Map(pair) = pair {
                        if let Some(V::Map(second)) = pair.get("second") {
                            if let Some(v) = second.get("asset") {
                                self.assets.insert(
                                    string_of(pair, "first")
                                        .replace('\\', "/")
                                        .to_ascii_lowercase(),
                                    (r, v.clone()),
                                );
                            }
                        }
                    }
                }
            }
        }
        if let Some((r, v)) = self
            .assets
            .get(&path.replace('\\', "/").to_ascii_lowercase())
            .cloned()
        {
            return self.resolve(r, &v);
        }

        Err(failure())
    }
}
