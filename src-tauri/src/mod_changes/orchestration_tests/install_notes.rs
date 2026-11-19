use super::*;
use serde_json::json;
use std::io::Write;

fn noted(name: &str, guid: &str) -> ModRepositoryConfig {
    let mut repo = repository(name, guid, &["2.0.0", "1.0.0"], vec![]);
    repo.note = format!("Read {name}.");
    repo.note_i18n
        .insert("en".into(), format!("Notes for {name}."));
    repo.name_i18n
        .insert("en".into(), format!("Name for {name}"));
    repo
}

fn ids(plan: &ChangePlan) -> Vec<&str> {
    plan.install_notes
        .iter()
        .map(|note| note.id.as_str())
        .collect()
}

fn manual_preview(f: &Fixture, members: &[(&str, &str)]) -> PlanRequest {
    let id = uuid::Uuid::new_v4().to_string();
    let directory = crate::mod_preview::staging::preview_root(&f.state).join(&id);
    fs::create_dir_all(&directory).unwrap();
    if let [(file, guid)] = members {
        write(
            &directory.join(file),
            &mods::plugin_blob(guid, file, "2.0.0"),
        );
    } else {
        let mut zip =
            zip::ZipWriter::new(fs::File::create(directory.join("arbitrary-name.zip")).unwrap());
        for (file, guid) in members {
            zip.start_file(*file, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&mods::plugin_blob(guid, file, "2.0.0"))
                .unwrap();
        }
        zip.finish().unwrap();
    }
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: format!("preview:{id}"),
                candidate_id: None,
                enabled: true,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    }
}

#[test]
fn four_channels_with_one_guid_use_only_the_exact_selected_notice() {
    let guid = "fixture-arcade";
    let github_stable = noted("github-stable", guid);
    let mut github_beta = noted("github-beta", guid);
    github_beta.channel = Channel::Beta;
    let mut gitee_stable = noted("gitee-stable", guid);
    gitee_stable.provider = Provider::Gitee;
    let mut gitee_beta = noted("gitee-beta", guid);
    gitee_beta.provider = Provider::Gitee;
    gitee_beta.channel = Channel::Beta;
    gitee_beta.note = "\n ".into();
    gitee_beta.note_i18n.clear();
    let f = fixture(vec![github_stable, github_beta, gitee_stable, gitee_beta]);
    let mut selected = request("gitee-stable", Some("v2.0.0"), true);
    selected.roots[0].asset_name = Some("gitee-stable.dll".into());
    let plan = planning::create(&f.state, selected).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    let wire = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        wire["install_notes"],
        json!([{
            "id": "gitee-stable", "name": "gitee-stable",
            "name_i18n": {"en": "Name for gitee-stable", "zh-CN": "Fixture/gitee-stable"},
            "note": "Read gitee-stable.", "note_i18n": {"en": "Notes for gitee-stable.", "zh-CN": ""}
        }])
    );
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(ids(&session.plan), ["gitee-stable"]);
    let before = preparation::approval_signature(&session).unwrap();
    session.plan.install_notes[0]
        .note_i18n
        .insert("en".into(), "Revised note.".into());
    assert_ne!(preparation::approval_signature(&session).unwrap(), before);

    let plan = planning::create(&f.state, request("gitee-beta", Some("v2.0.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    assert_eq!(
        serde_json::to_value(&plan).unwrap()["install_notes"],
        json!([])
    );
}

#[test]
fn installed_manifest_suppresses_notice_until_its_real_dll_is_removed() {
    let guid = "fixture-managed";
    let f = fixture(vec![noted("managed", guid), noted("mirror", guid)]);
    let file = install(&f, "managed", "Fixture-Managed", "1.0.0", false);
    let first = planning::create(&f.state, request("managed", Some("v2.0.0"), true)).unwrap();
    assert_eq!(first.result.status, SolveStatus::Ready);
    assert!(
        first
            .result
            .actions
            .iter()
            .any(|action| action.kind == ActionKind::Upgrade)
    );
    assert!(first.install_notes.is_empty());
    let disabled = mods::disabled_dll_file(&file);
    fs::remove_file(f.game.join(&disabled)).unwrap();
    let target = request("mirror", Some("v2.0.0"), true);
    let plan = planning::create(&f.state, target).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    assert_eq!(
        ids(&plan),
        ["mirror"],
        "orphaned manifest does not count as an installed DLL"
    );
}

#[test]
fn approval_covers_actions_but_not_their_explanatory_details() {
    let f = fixture(vec![noted("approved", "fixture-approved")]);
    let plan = planning::create(&f.state, request("approved", Some("v2.0.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    let mut session = take(&f, &plan);
    let before = preparation::approval_signature(&session).unwrap();

    session.plan.result.actions[0].reason_details[0].requirement = Some(">=1.0".into());
    session.plan.result.actions[0].caused_by = vec!["fixture-related-item".into()];
    assert_eq!(preparation::approval_signature(&session).unwrap(), before);

    session.plan.result.actions[0].to_tag = Some("v1.0.0".into());
    assert_ne!(preparation::approval_signature(&session).unwrap(), before);
}

#[test]
fn bare_installed_dll_with_repository_match_still_needs_the_first_notice() {
    let guid = "fixture-unmanaged";
    let f = fixture(vec![noted("unmanaged", guid)]);
    write(
        &f.game.join("BepInEx/plugins/unmanaged/unmanaged.dll"),
        &mods::plugin_blob(guid, "Unmanaged", "1.0.0"),
    );
    let plan = planning::create(&f.state, request("unmanaged", Some("v2.0.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    assert_eq!(ids(&plan), ["unmanaged"]);
    let toggle = planning::create(&f.state, request("unmanaged", None, false)).unwrap();
    assert_eq!(toggle.result.status, SolveStatus::Ready);
    assert!(toggle.install_notes.is_empty());
}

#[test]
fn manual_actual_members_use_visible_unique_sources() {
    let mut hidden = noted("not-visible", "fixture-only");
    hidden.enabled = false;
    let f = fixture(vec![
        noted("only", "fixture-only"),
        hidden,
        noted("one-of-two", "fixture-ambiguous"),
        noted("two-of-two", "fixture-ambiguous"),
        noted("another", "fixture-second"),
    ]);
    let single = planning::create(
        &f.state,
        manual_preview(&f, &[("unrelated.dll", "Fixture-Only")]),
    )
    .unwrap();
    assert_eq!(single.result.status, SolveStatus::Ready);
    assert_eq!(ids(&single), ["only"]);
    assert_eq!(single.roots[0].label, "unrelated");
    let zip = planning::create(
        &f.state,
        manual_preview(
            &f,
            &[
                ("a.dll", "fixture-only"),
                ("b.dll", "fixture-second"),
                ("c.dll", "fixture-ambiguous"),
                ("d.dll", "fixture-unknown"),
            ],
        ),
    )
    .unwrap();
    assert_eq!(zip.result.status, SolveStatus::Ready, "{:?}", zip.result);
    assert_eq!(ids(&zip), ["only", "another"]);
    assert_eq!(zip.roots[0].label, "arbitrary-name");
}

#[test]
fn blocked_plans_do_not_project_install_notices() {
    let mut repo = noted("blocked", "fixture-blocked");
    repo.relations = vec![requires("fixture-missing-requirement", "==1.0.0")];
    let f = fixture(vec![repo]);
    let plan = planning::create(&f.state, request("blocked", Some("v2.0.0"), true)).unwrap();
    assert_ne!(plan.result.status, SolveStatus::Ready);
    assert_eq!(
        serde_json::to_value(&plan).unwrap()["install_notes"],
        json!([])
    );
}
