use super::*;

#[test]
fn classifies_installer_result_separately_from_program_launch() {
    assert_eq!(
        classify("exe", Some(0), Ok(Some("installed")), true),
        Outcome::Installed
    );
    assert_eq!(
        classify("exe", Some(1), Ok(Some("installed")), true),
        Outcome::Failed
    );
    assert_eq!(
        classify("exe", Some(0), Ok(Some("installed")), false),
        Outcome::Failed
    );
    assert_eq!(classify("exe", Some(0), Ok(None), true), Outcome::Installed);
    assert_eq!(classify("exe", Some(1), Ok(None), true), Outcome::Failed);
    assert_eq!(classify("exe", Some(0), Ok(None), false), Outcome::Failed);
    assert_eq!(
        classify("exe", Some(0), Ok(Some("wait_failed")), true),
        Outcome::Failed
    );
    let invalid = io::Error::from(io::ErrorKind::InvalidData);
    assert_eq!(
        classify("exe", Some(0), Err(&invalid), true),
        Outcome::Failed
    );
    assert_eq!(classify("msi", Some(0), Ok(None), true), Outcome::Installed);
    assert_eq!(classify("msi", Some(0), Ok(None), false), Outcome::Failed);
    assert_eq!(classify("msi", Some(1603), Ok(None), true), Outcome::Failed);
    assert_eq!(classify("msi", None, Ok(None), false), Outcome::Failed);
    assert_eq!(
        classify("msi", Some(1641), Ok(None), true),
        Outcome::RebootRequired
    );
    assert_eq!(
        classify("msi", Some(3010), Ok(None), false),
        Outcome::RebootRequired
    );
}

#[test]
fn msi_quotes_real_paths_without_enabling_autolaunch() {
    let destination = Path::new(r"C:\用户 Name\测试\OC2ModManager.exe");
    let package = Path::new(r"C:\用户 Name\update.msi");
    let log = Path::new(r"C:\用户 Name\update-msi.log");
    assert_eq!(
        msi_parameters(destination, package, log).unwrap(),
        r#"/i "C:\用户 Name\update.msi" /quiet /norestart INSTALLDIR="C:\用户 Name\测试" /L*v "C:\用户 Name\update-msi.log""#
    );
    assert_eq!(quote_path(Path::new(r"C:\")).unwrap(), r#""C:\\""#);
    assert!(quote_path(Path::new(r#"C:\invalid"path"#)).is_err());
    assert!(quote_path(Path::new("relative package.msi")).is_err());
}

#[test]
fn nsis_accepts_the_v1_receipt_only_for_the_target_attempt() {
    let work = tempfile::tempdir().unwrap();
    let target = work.path().join("测试 User");
    std::fs::create_dir(&target).unwrap();
    let report = work.path().join(NSIS_RESULT);
    assert_eq!(read_nsis_outcome(&report, "9.8.7", &target).unwrap(), None);
    let probe = |record: &str| {
        let bytes = record
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        std::fs::write(&report, bytes).unwrap();
        read_nsis_outcome(&report, "9.8.7", &target)
    };
    assert_eq!(probe("").unwrap(), None);
    let directory = target.display();

    assert_eq!(
        probe(&format!(
            "\u{feff}9.8.7|installed|{directory}|restart=0\r\n"
        ))
        .unwrap()
        .as_deref(),
        Some("installed")
    );
    assert_eq!(
        probe(&format!("9.8.7|wait_failed|{directory}|restart=0\r\n"))
            .unwrap()
            .as_deref(),
        Some("wait_failed")
    );
    assert!(probe(&format!("9.8.6|installed|{directory}|restart=0\r\n")).is_err());
    assert!(
        probe(&format!(
            "9.8.7|installed|{}|restart=0\r\n",
            work.path().display()
        ))
        .is_err()
    );
    assert!(probe(&format!("9.8.7|installed|{directory}|restart=1\r\n")).is_err());
    assert!(probe(&format!("9.8.7|installed|{directory}|restart=0|extra\r\n")).is_err());
}

#[test]
fn resolves_the_system_msi_launcher() {
    let launcher = system_msiexec().unwrap();
    assert!(launcher.is_absolute());
    assert!(launcher.is_file());
    assert_eq!(launcher.file_name().unwrap(), "msiexec.exe");
}
