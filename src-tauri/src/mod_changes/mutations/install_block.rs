//! Reasons established by the checks that stop an installation or replacement.
pub(crate) fn report_install_block(tag: &str, owner: &str, params: &[(&str, &str)]) -> String {
    log::warn!("整链安装检查未通过：节点={owner}，原因={tag}，详情={params:?}");
    crate::locale_tag::with_params(tag, params)
}

pub(in crate::mod_changes) fn package_identity_conflict<'a>(
    owner: &str,
    guid: &str,
    members: impl IntoIterator<Item = &'a crate::mod_changes::model::ProvidedPlugin>,
) -> String {
    let paths = members
        .into_iter()
        .filter_map(|member| member.member_key.as_deref())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n");
    report_install_block(
        "modChanges.installPackageIdentityConflict",
        owner,
        &[("guid", guid), ("paths", &paths)],
    )
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum InstallBlock {
    Directory,
    BothStates,
    Unreadable,
    Unsafe,
    InstallRecord,
    PathOwned,
    Unrecognized,
    NeedsVerifiedRelease,
    OutsidePackage,
    MapProtected,
    MapRecordsUnreadable,
    BundleNotDeclared,
    MultipleTargets,
    BundlePathChanged,
    PluginMismatch,
    PluginIdentityUnknown,
    BundleFileDiffers,
    RecordUnreadable,
    RecordInvalid,
    RecordExists,
    AliasesOrLink,
    VerificationChanged,
    Missing,
    PathCollision,
    ResidueDiffers,
    Unverified,
}

impl InstallBlock {
    pub(crate) fn report(self, owner: &str, relative: &str) -> String {
        let tag = match self {
            Self::Directory => "modChanges.installTargetIsDirectory",
            Self::BothStates => "modChanges.installTargetHasBothStates",
            Self::Unreadable => "modChanges.installTargetUnreadable",
            Self::Unsafe => "modChanges.installTargetUnsafe",
            Self::InstallRecord => "modChanges.installTargetClaimed",
            Self::PathOwned => "modChanges.installTargetOwned",
            Self::Unrecognized => "modChanges.installTargetUnrecognized",
            Self::NeedsVerifiedRelease => "modChanges.installTargetNeedsVerifiedRelease",
            Self::OutsidePackage => "modChanges.installTargetOutsidePackage",
            Self::MapProtected => "modChanges.installTargetMapProtected",
            Self::MapRecordsUnreadable => "modChanges.installTargetMapRecordsUnreadable",
            Self::BundleNotDeclared => "modChanges.installBundleNotDeclared",
            Self::MultipleTargets => "modChanges.installBundleMultipleMatches",
            Self::BundlePathChanged => "modChanges.installBundlePathChanged",
            Self::PluginMismatch => "modChanges.installBundlePluginMismatch",
            Self::PluginIdentityUnknown => "modChanges.installTargetPluginIdentityUnknown",
            Self::BundleFileDiffers => "modChanges.installBundleFileDiffers",
            Self::RecordUnreadable => "modChanges.installRecordUnreadable",
            Self::RecordInvalid => "modChanges.installRecordInvalid",
            Self::RecordExists => "modChanges.installRecordExists",
            Self::AliasesOrLink => "modChanges.installTargetAliasesOrLink",
            Self::VerificationChanged => "modChanges.installTargetVerificationChanged",
            Self::Missing => "modChanges.installTargetMissing",
            Self::PathCollision => "modChanges.installTargetPathCollision",
            Self::ResidueDiffers => "modChanges.installTargetResidueDiffers",
            Self::Unverified => "modChanges.fileOwnershipConflict",
        };
        report_install_block(tag, owner, &[("path", relative)])
    }
}
