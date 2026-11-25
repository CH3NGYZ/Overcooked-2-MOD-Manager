use super::{PermissionRoute, elevated_route, token::Identity};

fn identity(sid: &str, session: u32, integrity: u32, elevated: bool, admin: bool) -> Identity {
    Identity {
        sid: sid.into(),
        session,
        integrity,
        elevated,
        administrator_group: admin,
        ui_access: false,
    }
}

#[test]
fn same_account_ordinary_desktop_offers_relaunch() {
    let admin = identity("S-1-5-21-1", 7, 0x3000, true, true);
    let desktop = identity("S-1-5-21-1", 7, 0x2000, false, false);
    assert_eq!(
        elevated_route(&admin, &desktop),
        PermissionRoute::RestartAsUser
    );
}

#[test]
fn nonmatching_accounts_and_nonordinary_desktops_offer_only_download() {
    let admin = identity("S-1-5-21-1", 7, 0x3000, true, true);
    let mut ui_access = identity("S-1-5-21-1", 7, 0x2000, false, false);
    ui_access.ui_access = true;
    for desktop in [
        identity("S-1-5-21-2", 7, 0x2000, false, false),
        identity("S-1-5-21-1", 8, 0x2000, false, false),
        identity("S-1-5-21-1", 0, 0x2000, false, false),
        identity("S-1-5-21-1", 7, 0x1000, false, false),
        identity("S-1-5-21-1", 7, 0x2100, false, false),
        identity("S-1-5-21-1", 7, 0x3000, true, true),
        ui_access,
    ] {
        assert_eq!(
            elevated_route(&admin, &desktop),
            PermissionRoute::ManualDownload
        );
    }
}
