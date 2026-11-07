use crate::archive;
use crate::recycle_bin::repository::recycle_repository_packages_before_install;
use std::fs;

use super::{A, B, Workspace, plugin_blob};

#[test]
fn installing_repository_recycles_every_package_bound_to_it() {
    // 同一仓库历史上装出过三个包：两个靠 binding 识别、一个靠 repository_id。
    // 安装前必须全部回收，否则装完仍会显示「重复」。
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-1", "name": "Repo", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "channel": "stable"
    }))
    .unwrap();

    let binding = || crate::models::RepositoryBinding {
        provider: crate::models::Provider::Github,
        owner: "Owner".into(),
        repository: "Mod".into(),
        tag: "v1.2.3".into(),
        channel: crate::models::Channel::Stable,
    };

    // 包 A：靠 binding 匹配（旧包 ID，与本次安装 ID 不同）
    workspace.write("BepInEx/plugins/Bundle/dup-a.dll", b"dup a");
    let mut a = workspace.manifest("old-package-a", &["BepInEx/plugins/Bundle/dup-a.dll"]);
    a.repository_binding = Some(binding());
    archive::write_manifest(&workspace.manifests, &a).unwrap();

    // 包 B：同样靠 binding 匹配
    workspace.write("BepInEx/plugins/Bundle/dup-b.dll", b"dup b");
    let mut b = workspace.manifest("old-package-b", &["BepInEx/plugins/Bundle/dup-b.dll"]);
    b.repository_binding = Some(binding());
    archive::write_manifest(&workspace.manifests, &b).unwrap();

    // 包 C：repository_id 就是本次 ID
    workspace.write("BepInEx/plugins/Bundle/same-id.dll", b"same id");
    workspace.manifest("repo-1", &["BepInEx/plugins/Bundle/same-id.dll"]);

    // 同 GUID 但属于别的仓库：不能动
    workspace.write("BepInEx/plugins/Other/keep.dll", b"keep");
    let mut keep = workspace.manifest("other-repo", &["BepInEx/plugins/Other/keep.dll"]);
    keep.repository_binding = Some(crate::models::RepositoryBinding {
        provider: crate::models::Provider::Github,
        owner: "SomeoneElse".into(),
        repository: "OtherMod".into(),
        tag: "v9.9.9".into(),
        channel: crate::models::Channel::Stable,
    });
    archive::write_manifest(&workspace.manifests, &keep).unwrap();

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        &["BepInEx/plugins/Bundle/new.dll".into()],
    )
    .unwrap();

    assert_eq!(entries.len(), 3, "三个同仓库包都应被回收");
    // 三个包的清单都已离开活动目录
    assert!(workspace.active("old-package-a").is_none());
    assert!(workspace.active("old-package-b").is_none());
    assert!(workspace.active("repo-1").is_none());
    // 其它仓库不受影响
    assert!(workspace.active("other-repo").is_some());
    assert_eq!(
        fs::read(workspace.game.join("BepInEx/plugins/Other/keep.dll")).unwrap(),
        b"keep"
    );
    // 被回收的文件已从游戏目录移走
    assert!(
        !workspace
            .game
            .join("BepInEx/plugins/Bundle/dup-a.dll")
            .exists()
    );
    assert!(
        !workspace
            .game
            .join("BepInEx/plugins/Bundle/dup-b.dll")
            .exists()
    );
}

#[test]
fn repository_recycle_skips_unrelated_bindings() {
    // 仓库没装过任何东西时返回空，调用方据此跳过回滚准备
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-fresh", "name": "Fresh", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "channel": "stable"
    }))
    .unwrap();

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        &["BepInEx/plugins/new.dll".into()],
    )
    .unwrap();
    assert!(entries.is_empty());
}

#[test]
fn same_repository_packages_do_not_protect_each_others_files() {
    /*
     * 同一次安装前要回收同一仓库的多个旧包。若把它们互相当成
     * 「其他包」，每个包都会因为「文件仍由其他包使用」而跳过回收，
     * 结果旧副本全部留在磁盘上 —— 这就是「更新后仍显示重复」的根因。
     */
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-1", "name": "Repo", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "channel": "beta"
    }))
    .unwrap();

    // 两个旧包登记在同一批文件上（旧版本装出的两份副本）
    workspace.write(A, b"stale a");
    workspace.write(B, b"stale b");
    let binding = crate::models::RepositoryBinding {
        provider: crate::models::Provider::Github,
        owner: "Owner".into(),
        repository: "Mod".into(),
        tag: "v1.8.7.21".into(),
        // 渠道与当前仓库配置不同：旧安装记的是 all，仓库现在配的是 beta
        channel: crate::models::Channel::All,
    };
    let mut first = workspace.manifest("old-1", &[A, B]);
    first.repository_binding = Some(binding.clone());
    archive::write_manifest(&workspace.manifests, &first).unwrap();

    let mut second = workspace.manifest("old-2", &[A, B]);
    second.repository_binding = Some(binding);
    archive::write_manifest(&workspace.manifests, &second).unwrap();

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        &["BepInEx/plugins/new.dll".into()],
    )
    .unwrap();

    assert!(!entries.is_empty(), "至少有一个包产生回收记录");
    // 关键：文件必须真的从游戏目录移走，不能因为「同批包共享」而跳过
    assert!(!workspace.game.join(A).exists(), "旧包 A 的文件应已移走");
    assert!(!workspace.game.join(B).exists(), "旧包 B 的文件应已移走");
    // 两个旧包的清单都不能留存，否则扫描会多出指向空文件的条目
    assert!(workspace.active("old-1").is_none(), "旧包 1 的清单应已移除");
    assert!(workspace.active("old-2").is_none(), "旧包 2 的清单应已移除");
}

#[test]
fn repository_recycle_matches_identity_even_when_channel_changed() {
    // 仓库渠道从 all 改成 beta（或反之）不应让旧安装失去归属：
    // 渠道只决定拉哪个 Release，不改变「同一个仓库」这个事实。
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-1", "name": "Repo", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "channel": "beta"
    }))
    .unwrap();

    workspace.write(A, b"old");
    let mut old = workspace.manifest("legacy-package", &[A]);
    old.repository_binding = Some(crate::models::RepositoryBinding {
        provider: crate::models::Provider::Github,
        owner: "Owner".into(),
        repository: "Mod".into(),
        tag: "v1.8.7.21".into(),
        channel: crate::models::Channel::Stable,
    });
    archive::write_manifest(&workspace.manifests, &old).unwrap();

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        &["BepInEx/plugins/new.dll".into()],
    )
    .unwrap();

    assert_eq!(entries.len(), 1, "渠道不同但身份相同的旧包也要回收");
    assert!(!workspace.game.join(A).exists());
    assert!(workspace.active("legacy-package").is_none());
}

#[test]
fn repository_install_recycles_legacy_manual_manifest_of_same_package() {
    /*
     * 真实场景：早期在线安装误走了本地安装路径，留下 `manual-<uuid>`
     * 清单。之后从仓库正常安装时若不回收它，会多出一条「本地安装」
     * 分组下的孤儿条目，并与主 MOD 互判重复。
     */
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-1", "name": "DIYLevel", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "install_target": "BepInEx/plugins/DIYLevel", "enabled": true, "builtin": false,
        "channel": "all"
    }))
    .unwrap();

    // 旧的手动清单：同一个包目录下的依赖 DLL
    workspace.write("BepInEx/plugins/DIYLevel/OC2DIYLevel.dll", b"dep");
    workspace.manifest(
        "manual-legacy",
        &["BepInEx/plugins/DIYLevel/OC2DIYLevel.dll"],
    );

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        // 新包解到同一目录，但文件名不同（旧版依赖名带版本号）
        &["BepInEx/plugins/DIYLevel/DIYLevel.dll".into()],
    )
    .unwrap();

    assert_eq!(entries.len(), 1, "同包目录下的历史 manual 清单应被回收");
    assert!(workspace.active("manual-legacy").is_none());
    assert!(
        !workspace
            .game
            .join("BepInEx/plugins/DIYLevel/OC2DIYLevel.dll")
            .exists()
    );
}

#[test]
fn legacy_manual_cleanup_never_touches_sibling_plugins() {
    /*
     * 反向保护：BepInEx/plugins 是系统容器，平铺其中的散装 DLL
     * 不属于同一个包，绝不能因为「同目录」被误回收。
     */
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-1", "name": "Repo", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "channel": "all"
    }))
    .unwrap();

    workspace.write("BepInEx/plugins/Unrelated.dll", b"keep me");
    workspace.manifest("manual-unrelated", &["BepInEx/plugins/Unrelated.dll"]);

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        &["BepInEx/plugins/Other.dll".into()],
    )
    .unwrap();

    assert!(entries.is_empty(), "plugins 根目录不该被当成包目录");
    assert!(workspace.active("manual-unrelated").is_some());
    assert!(
        workspace
            .game
            .join("BepInEx/plugins/Unrelated.dll")
            .exists()
    );
}

#[test]
fn manual_package_is_recycled_by_dll_identity_across_directories() {
    /*
     * 用户实际场景：本地装了 DIYLevel 的 ZIP，管理器把主 MOD 和依赖
     * 一起写进了同一份 manual 清单，但两者可能落在不同子目录。
     * 该仓库后来发新版本时，必须按**包里任一 DLL 的身份**判定整包归属，
     * 与文件散落在哪无关，否则这条 manual 包永远不会被回收。
     */
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-1", "name": "DIYLevel", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "plugin_guid": "com.ch3ngyz.plugin.DIYLevel",
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "channel": "all"
    }))
    .unwrap();

    // 主 MOD 在子目录，依赖平铺在 plugins 根；两者同属一份 manual 清单
    let main = "BepInEx/plugins/DIYLevel/DIYLevel.dll";
    let dep = "BepInEx/plugins/OC2DIYLevel-v0.10.dll";
    workspace.write(
        main,
        &plugin_blob("com.ch3ngyz.plugin.DIYLevel", "DIYLevel", "1.8.7"),
    );
    workspace.write(dep, b"dependency without plugin metadata");
    workspace.manifest("manual-legacy", &[main, dep]);

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        // 新版本解到完全不同的可见路径，目录判定帮不上忙
        &["BepInEx/plugins/DIYLevelNew/DIYLevel.dll".into()],
    )
    .unwrap();

    assert_eq!(entries.len(), 1, "整包应按主 MOD 的 GUID 命中并回收");
    assert!(workspace.active("manual-legacy").is_none());
    assert!(!workspace.game.join(main).exists(), "主 MOD 应已移走");
    assert!(!workspace.game.join(dep).exists(), "依赖应随整包一起移走");
}

#[test]
fn unrelated_manual_package_survives_repository_install() {
    // 身份不匹配的 manual 包不能因为「同时存在」就被清掉
    let workspace = Workspace::new();
    let repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "repo-1", "name": "DIYLevel", "provider": "github",
        "username": "Owner", "repository": "Mod", "download_mode": "release",
        "plugin_guid": "com.ch3ngyz.plugin.DIYLevel",
        "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
        "channel": "all"
    }))
    .unwrap();

    let other = "BepInEx/plugins/Other/Other.dll";
    workspace.write(other, &plugin_blob("com.example.other", "Other", "2.0.0"));
    workspace.manifest("manual-other", &[other]);

    let entries = recycle_repository_packages_before_install(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        &repo,
        &["BepInEx/plugins/DIYLevel/DIYLevel.dll".into()],
    )
    .unwrap();

    assert!(entries.is_empty(), "身份不匹配的包不应被回收");
    assert!(workspace.active("manual-other").is_some());
    assert!(workspace.game.join(other).exists());
}
