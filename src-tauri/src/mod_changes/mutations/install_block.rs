//! Reasons established by the checks that stop an installation or replacement.
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
            Self::PathCollision => "modChanges.installTargetPathCollision",
            Self::ResidueDiffers => "modChanges.installTargetResidueDiffers",
            Self::Unverified => "modChanges.fileOwnershipConflict",
        };
        log::warn!("整链安装目标检查未通过：节点={owner}，路径={relative}，原因={tag}");
        crate::locale_tag::with_params(tag, &[("path", relative)])
    }
}
