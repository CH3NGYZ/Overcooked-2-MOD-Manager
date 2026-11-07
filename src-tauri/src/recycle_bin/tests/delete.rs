use crate::recycle_bin::delete::purge_mod_dll;
use crate::recycle_bin::delete::purge_package;
use crate::recycle_bin::records::list;

use super::{A, DATA, Workspace};

/*
 * 直接删除（用户关闭「优先放入回收站」）：
 * 文件与清单都要消失，且回收站里不能留下任何记录。
 */
#[test]
fn purge_removes_files_and_leaves_no_recycle_record() {
    let workspace = Workspace::new();
    workspace.write(A, b"first DLL");
    workspace.write(DATA, b"settings");
    workspace.manifest("package", &[A, DATA]);

    purge_package(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        None,
    )
    .unwrap();

    assert!(!workspace.game.join(A).exists(), "DLL 应被删除");
    assert!(!workspace.game.join(DATA).exists(), "附属文件应被删除");
    assert!(workspace.active("package").is_none(), "安装清单应被移除");
    assert!(
        list(&workspace.recycle).unwrap().is_empty(),
        "不该留下回收记录"
    );
}

/// 直删同样要守住共享文件保护：同 GUID 的其他副本仍在使用时不能移走。
#[test]
fn purge_respects_shared_file_protection() {
    let workspace = Workspace::new();
    workspace.write(A, b"same DLL");
    workspace.manifest("package", &[A]);
    workspace.manifest("peer", &[A]);
    let installed = vec![];
    let error = purge_mod_dll(
        &workspace.recycle,
        &workspace.manifests,
        &workspace.game,
        "package",
        A,
        &installed,
    );
    // 与回收路径一致：共享文件必须有安全替代副本才允许移走
    assert!(error.is_err() || workspace.game.join(A).exists());
}
