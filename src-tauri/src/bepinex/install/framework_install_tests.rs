use super::*;
use crate::bepinex::Package;
use crate::models::Provider;
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

struct Workspace(PathBuf);

fn verification_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/framework-install")
}

impl Workspace {
    fn new() -> Self {
        let root = verification_root().join(Uuid::new_v4().to_string());
        fs::create_dir_all(root.join("game")).unwrap();
        Self(root)
    }

    fn game(&self) -> PathBuf {
        self.0.join("game")
    }

    fn backups(&self) -> PathBuf {
        self.0.join("backups")
    }

    /// 框架安装清单目录（安装时会写入 `bepinex-framework.toml`）。
    fn manifests(&self) -> PathBuf {
        self.0.join("manifests")
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.game().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn archive(&self, package: Package, script: Option<&str>) -> PathBuf {
        let path = self.0.join("framework.zip");
        let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, bytes) in [
            ("BepInEx/core/BepInEx.dll", b"new core".as_slice()),
            (
                match package {
                    Package::Windows => "winhttp.dll",
                    Package::MacOS => "libdoorstop.dylib",
                },
                b"new loader".as_slice(),
            ),
        ] {
            archive
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(bytes).unwrap();
        }
        if let Some(script) = script {
            archive
                .start_file("run_bepinex.sh", zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(script.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
        path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let root = fs::canonicalize(&self.0).unwrap();
        assert!(root.starts_with(fs::canonicalize(verification_root()).unwrap()));
        let _ = fs::remove_dir_all(&self.0);
    }
}

const SCRIPT: &str = "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\nexport DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n";

#[test]
fn fresh_install_and_reinstall_preserve_plugins_and_config_for_all_platform_packages() {
    for package in [Package::Windows, Package::MacOS] {
        let workspace = Workspace::new();
        let archive = workspace.archive(package, package.native().then_some(SCRIPT));
        install_framework_archive(
            &archive,
            &workspace.game(),
            &workspace.backups(),
            &workspace.manifests(),
            package,
            false,
            "5.4.23.5",
        )
        .unwrap();
        assert!(!workspace.backups().exists());
        workspace.write("BepInEx/plugins/mod.dll.disabled", b"keep disabled plugin");
        workspace.write("BepInEx/config/BepInEx.cfg", b"keep game config");
        workspace.write("BepInEx/core/BepInEx.dll", b"old core");
        install_framework_archive(
            &archive,
            &workspace.game(),
            &workspace.backups(),
            &workspace.manifests(),
            package,
            true,
            "5.4.23.5",
        )
        .unwrap();
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/core/BepInEx.dll")).unwrap(),
            b"new core"
        );
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/plugins/mod.dll.disabled")).unwrap(),
            b"keep disabled plugin"
        );
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/config/BepInEx.cfg")).unwrap(),
            b"keep game config"
        );
        if package.native() {
            assert!(
                fs::read_to_string(workspace.game().join("run_bepinex.sh"))
                    .unwrap()
                    .contains(&format!("executable_name=\"{}\"", package.executable()))
            );
        }
        assert_eq!(fs::read_dir(workspace.backups()).unwrap().count(), 1);
    }
}

/*
 * 危险的脚本内容必须让安装整体回滚，且原有根文件保持不动。
 *
 * 用 macOS 包：Linux 已不在发布范围内，但它原本是这条测试使用的对象
 * （`libdoorstop.so`），这里换成 macOS 的 `libdoorstop.dylib`，
 * 断言的内容完全等价。
 */
#[test]
fn malformed_native_scripts_roll_back_the_framework_and_existing_root_files() {
    for script in [None, Some("executable_name=\"\"\ninvalid enabled field\n")] {
        for reinstall in [false, true] {
            let workspace = Workspace::new();
            workspace.write("run_bepinex.sh", b"original script");
            workspace.write("libdoorstop.dylib", b"original loader");
            if reinstall {
                workspace.write("BepInEx/core/BepInEx.dll", b"original core");
                workspace.write("BepInEx/plugins/mod.dll", b"original plugin");
                workspace.write("BepInEx/config/BepInEx.cfg", b"original config");
            }
            let archive = workspace.archive(Package::MacOS, script);
            assert!(
                install_framework_archive(
                    &archive,
                    &workspace.game(),
                    &workspace.backups(),
                    &workspace.manifests(),
                    Package::MacOS,
                    reinstall,
                    "5.4.23.5",
                )
                .is_err()
            );
            assert_eq!(
                fs::read(workspace.game().join("run_bepinex.sh")).unwrap(),
                b"original script"
            );
            assert_eq!(
                fs::read(workspace.game().join("libdoorstop.dylib")).unwrap(),
                b"original loader"
            );
            if reinstall {
                for (file, bytes) in [
                    ("core/BepInEx.dll", b"original core".as_slice()),
                    ("plugins/mod.dll", b"original plugin".as_slice()),
                    ("config/BepInEx.cfg", b"original config".as_slice()),
                ] {
                    assert_eq!(
                        fs::read(workspace.game().join("BepInEx").join(file)).unwrap(),
                        bytes
                    );
                }
            } else {
                assert!(!workspace.game().join("BepInEx").exists());
            }
        }
    }
}

#[test]
fn unrecognized_macos_overlay_setup_rolls_back_fresh_install_and_reinstall() {
    for reinstall in [false, true] {
        let workspace = Workspace::new();
        workspace.write("run_bepinex.sh", b"original script");
        workspace.write("libdoorstop.dylib", b"original loader");
        if reinstall {
            workspace.write("BepInEx/core/BepInEx.dll", b"original core");
            workspace.write("BepInEx/plugins/mod.dll", b"original plugin");
        }
        let archive = workspace.archive(
            Package::MacOS,
            Some("#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\n"),
        );
        assert!(
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                &workspace.manifests(),
                Package::MacOS,
                reinstall,
                "5.4.23.5",
            )
            .is_err()
        );
        assert_eq!(
            fs::read(workspace.game().join("run_bepinex.sh")).unwrap(),
            b"original script"
        );
        assert_eq!(
            fs::read(workspace.game().join("libdoorstop.dylib")).unwrap(),
            b"original loader"
        );
        if reinstall {
            assert_eq!(
                fs::read(workspace.game().join("BepInEx/core/BepInEx.dll")).unwrap(),
                b"original core"
            );
            assert_eq!(
                fs::read(workspace.game().join("BepInEx/plugins/mod.dll")).unwrap(),
                b"original plugin"
            );
        } else {
            assert!(!workspace.game().join("BepInEx").exists());
        }
    }
}

#[test]
fn reinstall_must_be_explicit_before_backup_or_file_changes() {
    let workspace = Workspace::new();
    workspace.write("BepInEx/core/BepInEx.dll", b"original core");
    let archive = workspace.archive(Package::Windows, None);
    assert!(
        install_framework_archive(
            &archive,
            &workspace.game(),
            &workspace.backups(),
            &workspace.manifests(),
            Package::Windows,
            false,
            "5.4.23.5",
        )
        .is_err()
    );
    assert!(!workspace.backups().exists());
    assert!(!workspace.game().join("winhttp.dll").exists());
    assert_eq!(
        fs::read(workspace.game().join("BepInEx/core/BepInEx.dll")).unwrap(),
        b"original core"
    );
}

#[test]
#[ignore = "Requires all three original packages in OC2_BEPINEX_PACKAGE_DIR"]
fn installs_the_actual_platform_archives_with_digest_and_preservation_checks() {
    let packages = PathBuf::from(
        std::env::var_os("OC2_BEPINEX_PACKAGE_DIR")
            .expect("Set OC2_BEPINEX_PACKAGE_DIR to the original platform packages"),
    );
    assert!(packages.is_absolute());
    for package in [Package::Windows, Package::MacOS] {
        let name = package.url(&Provider::Github).rsplit('/').next().unwrap();
        let archive = packages.join(name);
        archive::validate_download_file(&archive, None, Some(package.digest())).unwrap();
        let workspace = Workspace::new();
        install_framework_archive(
            &archive,
            &workspace.game(),
            &workspace.backups(),
            &workspace.manifests(),
            package,
            false,
            "5.4.23.5",
        )
        .unwrap();
        let info = game::detect_bepinex(&workspace.game(), Some(&workspace.backups()));
        assert!(info.installed, "{name}");
        workspace.write("BepInEx/plugins/preserved.dll.disabled", b"user MOD");
        workspace.write("BepInEx/config/preserved.cfg", b"user settings");
        install_framework_archive(
            &archive,
            &workspace.game(),
            &workspace.backups(),
            &workspace.manifests(),
            package,
            true,
            "5.4.23.5",
        )
        .unwrap();
        assert_eq!(
            fs::read(
                workspace
                    .game()
                    .join("BepInEx/plugins/preserved.dll.disabled")
            )
            .unwrap(),
            b"user MOD"
        );
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/config/preserved.cfg")).unwrap(),
            b"user settings"
        );
        if package.native() {
            let script = fs::read_to_string(workspace.game().join("run_bepinex.sh")).unwrap();
            assert!(script.contains(&format!("executable_name=\"{}\"", package.executable())));
            assert!(script.contains("enabled=\"1\""));
            assert_eq!(
                script.contains("export DYLD_INSERT_LIBRARIES=\"$STEAM_DYLD_INSERT_LIBRARIES\""),
                package == Package::MacOS
            );
        } else {
            assert!(workspace.game().join("winhttp.dll").is_file());
        }
    }
}

/*
 * 只装了 MOD、没装框架的游戏目录：`BepInEx/plugins` 存在但没有 core。
 *
 * 这是最常见的真实场景 —— 用户先手动放了个 MOD，再来装框架。
 * 旧实现把「目录存在」当成已安装，于是首次安装被拒绝，只能点「重新安装」，
 * 而重装会把整个 BepInEx 删掉再恢复。现在必须：
 *   · 判定为「未安装」，首次安装直接放行；
 *   · 不产生备份（没有框架可备）；
 *   · 已有的 plugins 原样保留。
 */
#[test]
fn installs_over_mod_only_residue_without_backup_and_keeps_plugins() {
    let workspace = Workspace::new();
    workspace.write("BepInEx/plugins/existing.dll", b"user MOD");
    let archive = workspace.archive(Package::Windows, None);
    install_framework_archive(
        &archive,
        &workspace.game(),
        &workspace.backups(),
        &workspace.manifests(),
        Package::Windows,
        false,
        "5.4.23.5",
    )
    .unwrap();
    assert_eq!(
        fs::read(workspace.game().join("BepInEx/plugins/existing.dll")).unwrap(),
        b"user MOD"
    );
    assert!(
        workspace.game().join("BepInEx/core/BepInEx.dll").is_file(),
        "框架核心应已就位"
    );
    assert!(
        !workspace.backups().exists(),
        "没有已安装框架时不该产生备份"
    );
    let info = game::detect_bepinex(&workspace.game(), Some(&workspace.backups()));
    assert!(info.installed && !info.has_residue);
}

/// 目录在、核心不在 = 残留，不是已安装。
#[test]
fn mod_only_directory_is_residue_not_an_installed_framework() {
    let workspace = Workspace::new();
    workspace.write("BepInEx/plugins/existing.dll", b"user MOD");
    let info = game::detect_bepinex(&workspace.game(), Some(&workspace.backups()));
    assert!(!info.installed, "只有 plugins 时不应判定为已安装");
    assert!(info.has_residue, "应标记为残留目录");
}

/*
 * 旧版本装的框架没有安装清单，卸载时按「框架已知文件」现造清单。
 *
 * 这份清单绝不能包含 plugins / config：那是用户和 MOD 的内容，
 * 点「卸载框架」不该把 MOD 一起带走。
 */
#[test]
fn framework_manifest_files_exclude_plugins_and_config() {
    let workspace = Workspace::new();
    workspace.write("BepInEx/core/BepInEx.dll", b"core");
    workspace.write("BepInEx/core/extra.dll", b"core extra");
    workspace.write("BepInEx/plugins/mod.dll", b"user MOD");
    workspace.write("BepInEx/config/BepInEx.cfg", b"user config");
    workspace.write("winhttp.dll", b"loader");
    workspace.write("doorstop_config.ini", b"doorstop");

    let files = framework_manifest_files(&workspace.game());
    assert!(files.contains(&"BepInEx/core/BepInEx.dll".to_string()));
    assert!(files.contains(&"BepInEx/core/extra.dll".to_string()));
    assert!(files.contains(&"winhttp.dll".to_string()));
    assert!(files.contains(&"doorstop_config.ini".to_string()));
    // 注意：`doorstop_config.ini` 名字里也含 "config"，但它是框架自己
    // 解压在游戏根目录的加载器配置，必须回收 —— 所以只能按路径前缀判断。
    assert!(
        !files
            .iter()
            .any(|file| file.starts_with("BepInEx/plugins/")),
        "plugins 属于用户 MOD：{files:?}"
    );
    assert!(
        !files.iter().any(|file| file.starts_with("BepInEx/config/")),
        "config 属于用户配置：{files:?}"
    );
}

/*
 * 卸载必须**先整目录备份、再按清单回收**。
 *
 * 这两步职责不同，缺一不可：
 *   · 回收站只带走清单里登记的文件。用户自己往 BepInEx 里手工放的插件、
 *     改过的 config、日志都不在清单里，回收站不管它们；
 *   · 备份是整目录快照，能把上面那些「管理器不知道」的文件一起保住。
 *
 * 一旦顺序颠倒（先回收再备份），回收已经把目录搬空，备份就只剩个空壳。
 * `uninstall_bepinex` 需要 `State`，无法在这里直接调用，所以断言源码顺序 ——
 * 与 `tests/registry.test.mjs` 用源码文本守住接线契约是同一种做法。
 */
#[test]
fn uninstall_backs_up_before_recycling() {
    let source = include_str!("../../commands/bepinex.rs");
    let uninstall = source
        .split("fn uninstall_bepinex_blocking")
        .nth(1)
        .expect("uninstall_bepinex 应存在");
    // 只看这个函数体（到下一个顶层 fn 之前）
    let body = uninstall
        .split("\n#[tauri::command]")
        .next()
        .expect("函数体应可截断");

    let backup_at = body
        .find("backup_framework(")
        .expect("卸载流程必须调用 backup_framework");
    let recycle_at = body
        .find("recycle_package(")
        .expect("卸载流程必须调用 recycle_package");
    assert!(
        backup_at < recycle_at,
        "必须先备份再回收：备份偏移={backup_at}，回收偏移={recycle_at}"
    );
}
