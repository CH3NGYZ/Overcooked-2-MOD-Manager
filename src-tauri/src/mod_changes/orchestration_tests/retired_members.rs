//! A historical package may legitimately predate a bundled optional plugin.
use super::*;

const SPLITTER: &str = "dev.gua.overcooked.diylevel.ingredientsplittermod";

fn framework_with_retired_member() -> Fixture {
    let fw = mods::plugin_target::DIYLEVEL_GUID;
    let f = fixture(vec![
        repository("framework", fw, &["0.11.1", "0.10.0"], vec![]),
        repository(
            "web",
            "fixture-web",
            &["3.10.2"],
            vec![requires(fw, ">=0.10.0,<0.11.0")],
        ),
    ]);
    install(&f, "framework", fw, "0.11.1", true);
    install(&f, "web", "fixture-web", "3.10.2", true);
    let splitter = "BepInEx/plugins/framework/modules/IngredientSplitter.dll";
    write(
        &f.game.join(splitter),
        &mods::plugin_blob(SPLITTER, "IngredientSplitter", "0.1.0"),
    );
    let manifests = f.state.config.manifests_dir();
    let mut manifest = archive::read_manifest(&manifests, "framework").unwrap();
    manifest.files.push(splitter.into());
    archive::write_manifest(&manifests, &manifest).unwrap();
    f
}

#[test]
fn repair_web_requirement_accepts_verified_older_framework_without_newer_bundled_member() {
    let f = framework_with_retired_member();
    let plan = planning::create(&f.state, request("web", None, true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let action = plan
        .result
        .actions
        .iter()
        .find(|action| action.node_id == "mod:framework")
        .unwrap();
    assert_eq!(action.kind, ActionKind::Downgrade);
    assert_eq!(action.to_tag.as_deref(), Some("v0.10.0"));
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert!(
        f.game
            .join("BepInEx/plugins/framework/modules/IngredientSplitter.dll")
            .is_file()
    );
    session.plan.prepared = true;
    super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    let actual = inventory::build(&f.state).unwrap();
    let framework = actual
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == "mod:framework")
        .unwrap();
    assert_eq!(
        framework.current.as_ref().unwrap().version.as_deref(),
        Some("0.10.0")
    );
    assert!(
        !f.game
            .join("BepInEx/plugins/framework/modules/IngredientSplitter.dll")
            .exists()
    );
    let web = super::super::evaluate(&actual.inventory)
        .into_iter()
        .find(|node| node.node_id == "mod:web")
        .unwrap();
    assert_eq!(web.runtime[0].state, EvaluationState::Satisfied);
}
