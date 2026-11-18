use super::*;

#[test]
fn ordinary_and_external_dll_toggles_preserve_real_source_and_destination_spelling() {
    for package_owned in [true, false] {
        for enabled in [true, false] {
            let mut f = fixture();
            let canonical = "BepInEx/plugins/MixedFolder/MyPlugin.DLL";
            let source = if enabled {
                canonical.into()
            } else {
                mods::disabled_dll_file(canonical)
            };
            let destination = if enabled {
                mods::disabled_dll_file(canonical)
            } else {
                canonical.into()
            };
            write(&f.roots.game_root, &source, b"mixed case plugin");
            add_mod(
                &mut f,
                "plugin",
                vec![member(canonical, enabled, "fixture-plugin")],
                vec![canonical.into()],
            );
            let files = f.session.trusted.files.get_mut("plugin").unwrap();
            files.summary.dll_files = vec![canonical.into()];
            files.summary.members[0].dll_file = canonical.into();
            if !package_owned {
                files.manifests.clear();
                files.summary.package_id = None;
                fs::remove_file(f.roots.data_root.join("manifests/plugin.toml")).unwrap();
            }
            let node = &mut f.session.solved.as_mut().unwrap().nodes[0];
            node.enabled = !enabled;
            node.current.as_mut().unwrap().provides[0].enabled = !enabled;

            let changes = assemble(&f.session, f.roots.clone()).unwrap();
            assert_eq!(changes.len(), 1);
            let Mutation::Move(change) = &changes[0] else {
                panic!("a member toggle must use one physical rename");
            };
            assert_eq!(change.from.relative, source);
            assert_eq!(change.to.relative, destination);
            super::super::super::transaction::prepare(
                &f.roots.data_root.join("transactions"),
                &f.roots,
                changes,
            )
            .unwrap()
            .apply()
            .unwrap();
            let file_names = fs::read_dir(f.roots.game_root.join("BepInEx/plugins/MixedFolder"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                file_names,
                vec![
                    Path::new(&destination)
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                ]
            );
        }
    }
}
