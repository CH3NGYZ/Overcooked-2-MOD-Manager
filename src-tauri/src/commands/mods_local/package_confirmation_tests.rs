use super::*;

fn siblings(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn single_dll_uninstall_still_asks_before_deleting_the_whole_package() {
    // 点单个 DLL、包内还有别人、没有同 GUID 副本 → 需要二次确认
    assert!(needs_package_confirmation(
        &siblings(&["A", "B"]),
        false,
        false
    ));
}

#[test]
fn package_level_uninstall_never_asks_again() {
    // 包标题的「删除」/ 冲突卸载：已经表达过整包语义，不能重复确认
    assert!(!needs_package_confirmation(
        &siblings(&["A", "B"]),
        false,
        true
    ));
}

#[test]
fn other_cases_skip_the_confirmation() {
    // 包内没有其他 MOD：删的就是单个文件
    assert!(!needs_package_confirmation(&siblings(&[]), false, false));
    // 存在同 GUID 的启用副本：走单 DLL 回收分支，不是整包删除
    assert!(!needs_package_confirmation(
        &siblings(&["A", "B"]),
        true,
        false
    ));
}
