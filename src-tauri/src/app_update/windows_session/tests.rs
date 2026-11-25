use super::{
    ERROR_ACCESS_DENIED, ERROR_ACCESS_DISABLED_BY_POLICY, ERROR_ELEVATION_REQUIRED,
    ERROR_PRIVILEGE_NOT_HELD, PermissionRoute, elevated_route, io, ordinary_launch_failure,
    token::Identity,
};

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
fn ordinary_desktop_offers_relaunch() {
    let admin = identity("S-1-5-21-1", 7, 0x3000, true, true);
    let desktop = identity("S-1-5-21-1", 7, 0x2000, false, false);
    assert_eq!(
        elevated_route(&admin, &desktop, 0).unwrap(),
        PermissionRoute::RestartAsUser
    );
}

#[test]
fn failed_native_launch_conclusively_offers_admin_path() {
    let admin = identity("S-1-5-21-1", 7, 0x3000, true, true);
    let desktop = identity("S-1-5-21-1", 7, 0x2000, false, false);
    assert_eq!(
        elevated_route(&admin, &desktop, ERROR_ELEVATION_REQUIRED as i32).unwrap(),
        PermissionRoute::Administrator
    );
}

#[test]
fn only_native_elevation_required_can_change_an_ordinary_desktop_to_admin_route() {
    let admin = identity("S-1-5-21-1", 7, 0x3000, true, true);
    let desktop = identity("S-1-5-21-1", 7, 0x2000, false, false);
    let requires_elevation = io::Error::from_raw_os_error(ERROR_ELEVATION_REQUIRED as i32);
    assert_eq!(ordinary_launch_failure(&requires_elevation, false), None);
    assert_eq!(
        ordinary_launch_failure(&requires_elevation, true),
        requires_elevation.raw_os_error()
    );
    for code in [
        ERROR_ACCESS_DENIED,
        ERROR_ACCESS_DISABLED_BY_POLICY,
        ERROR_PRIVILEGE_NOT_HELD,
    ] {
        let error = io::Error::from_raw_os_error(code as i32);
        assert_eq!(ordinary_launch_failure(&error, true), Some(code as i32));
        assert_eq!(
            elevated_route(&admin, &desktop, code as i32)
                .unwrap_err()
                .raw_os_error(),
            Some(code as i32)
        );
    }
    assert_eq!(
        ordinary_launch_failure(&io::ErrorKind::TimedOut.into(), true),
        None
    );
}

#[test]
fn cross_account_or_unknown_session_fails_closed() {
    let admin = identity("S-1-5-21-1", 7, 0x3000, true, true);
    let other = identity("S-1-5-21-2", 7, 0x2000, false, false);
    assert!(elevated_route(&admin, &other, 0).is_err());
    assert!(elevated_route(&admin, &other, ERROR_ELEVATION_REQUIRED as i32).is_err());
    let no_session = identity("S-1-5-21-1", 0, 0x2000, false, false);
    assert!(elevated_route(&admin, &no_session, 0).is_err());
}

#[test]
fn administrator_desktop_never_restarts_as_user() {
    let admin = identity("S-1-5-21-1", 7, 0x3000, true, true);
    let desktop = identity("S-1-5-21-1", 7, 0x3000, true, true);
    assert_eq!(
        elevated_route(&admin, &desktop, 0).unwrap(),
        PermissionRoute::Administrator
    );
}
