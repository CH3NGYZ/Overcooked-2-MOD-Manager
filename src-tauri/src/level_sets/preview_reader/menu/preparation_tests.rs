use super::*;

fn pointer(path_id: i64) -> V {
    V::PPtr {
        file_id: 0,
        path_id,
    }
}

fn composition(ids: &[i64]) -> V {
    V::Array(ids.iter().copied().map(pointer).collect())
}

fn custom_recipe(kind: i32, ingredients: &[i64]) -> Map {
    Map::from([
        ("type".into(), V::Int32(kind)),
        ("compositionSOs".into(), composition(ingredients)),
        ("mixingIcon".into(), pointer(0)),
        ("mixingIconSO".into(), pointer(0)),
        ("cookingStepIcon".into(), pointer(0)),
        ("cookingStepIconSO".into(), pointer(0)),
    ])
}

fn cooked_menu(context: &mut Context, ingredient: i64) -> Vec<MenuTile> {
    let recipe_ref = Ref { file: 0, id: 1 };
    let recipe = custom_recipe(2, &[ingredient]);
    context.menu_budget = 512;
    context.objects.insert((0, recipe_ref.id), recipe.clone());
    context.menu_tiles(recipe_ref, &recipe, "finished dish", None)
}

#[test]
fn custom_mixing_and_cooking_keep_empty_step_slots() {
    let mut context = super::super::tests::empty_context();
    context.objects.insert((0, 2), custom_recipe(3, &[3, 3, 3]));
    context.objects.insert(
        (0, 3),
        Map::from([("prefabName".into(), V::String("flour".into()))]),
    );

    let tiles = cooked_menu(&mut context, 2);
    let preparation = &tiles[tiles[0].children[0]];
    assert_eq!(preparation.pictures.len(), 3);
    assert!(preparation.pictures.iter().all(|icon| icon.name == "flour"));
    assert_eq!(preparation.modifiers.len(), 2);
    assert!(
        preparation
            .modifiers
            .iter()
            .all(|icon| icon.data_url.is_none())
    );
}

#[test]
fn native_mixed_preparation_expands_only_the_required_ingredient_slots() {
    let mut context = super::super::tests::empty_context();
    context.objects.insert(
        (0, 2),
        Map::from([
            ("m_Script".into(), pointer(3)),
            ("m_composition".into(), composition(&[4, 0, 4])),
            ("m_optional".into(), composition(&[5])),
            ("m_platingStep".into(), pointer(0)),
            ("m_platingPrefab".into(), pointer(0)),
        ]),
    );
    context.objects.insert(
        (0, 3),
        Map::from([(
            "m_ClassName".into(),
            V::String("MixedCompositeOrderNode".into()),
        )]),
    );
    context
        .objects
        .insert((0, 4), Map::from([("m_iconSprite".into(), pointer(0))]));

    let tiles = cooked_menu(&mut context, 2);
    let preparation = &tiles[tiles[0].children[0]];
    assert_eq!(preparation.pictures.len(), 3);
    assert_eq!(preparation.modifiers.len(), 2);
    assert!(
        preparation
            .modifiers
            .iter()
            .all(|icon| icon.data_url.is_none())
    );

    context.objects.get_mut(&(0, 3)).unwrap().insert(
        "m_ClassName".into(),
        V::String("CookedCompositeOrderNode".into()),
    );
    let tiles = cooked_menu(&mut context, 2);
    let preparation = &tiles[tiles[0].children[0]];
    assert_eq!(preparation.pictures.len(), 1);
    assert_eq!(preparation.modifiers.len(), 1);
}
