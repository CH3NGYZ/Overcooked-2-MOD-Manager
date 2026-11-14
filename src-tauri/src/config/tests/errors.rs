use crate::config::ConfigError;

#[test]
fn config_errors_encode_complete_reasons_using_the_shared_tag_protocol() {
    let reason = r"另一个程序正在使用此文件 (os error 32): C:\配置\config.toml?x=a=b&rate=100%+";
    let cases = [
        (
            ConfigError::Directory(reason.into()),
            "config.directoryUnavailableReason",
        ),
        (ConfigError::Read(reason.into()), "config.readFailedReason"),
        (
            ConfigError::Parse(reason.into()),
            "config.parseFailedReason",
        ),
        (
            ConfigError::Write(reason.into()),
            "config.writeFailedReason",
        ),
        (
            ConfigError::Invalid(reason.into()),
            "config.invalidFormatReason",
        ),
    ];
    for (error, expected_tag) in cases {
        let wire = error.to_string();
        let (tag, query) = wire.split_once('?').unwrap();
        assert_eq!(tag, expected_tag);
        let decoded = url::form_urlencoded::parse(query.as_bytes()).collect::<Vec<_>>();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].0, "reason");
        assert_eq!(decoded[0].1, reason);
    }
}

#[test]
fn config_errors_preserve_nested_tags_as_a_single_reason_parameter() {
    let nested = crate::locale_tag::with_params("source.addressInvalid", &[("reason", "a?b=c&d")]);
    let wire = ConfigError::Invalid(nested.clone()).to_string();
    let (_, query) = wire.split_once('?').unwrap();
    let decoded = url::form_urlencoded::parse(query.as_bytes()).collect::<Vec<_>>();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].1, nested);
}
