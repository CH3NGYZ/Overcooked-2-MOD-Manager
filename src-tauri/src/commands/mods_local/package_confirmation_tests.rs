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

#[test]
fn confirmed_package_uninstall_overrides_duplicate_guid_fast_path() {
    // 冲突弹窗已确认整包后，即使存在另一个同 GUID 副本，也必须走整包回收。
    assert!(!should_remove_only_selected_dll(true, true));
    // 普通单 DLL 删除仍保留原来的共享副本保护。
    assert!(should_remove_only_selected_dll(true, false));
    assert!(!should_remove_only_selected_dll(false, false));
}
