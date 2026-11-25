use crate::app_update::UpdateTarget;
use crate::app_update::channels::{
    INSTALLER_GITCODE_OWNER, INSTALLER_GITCODE_REPO, INSTALLER_GITEE_OWNER, INSTALLER_GITEE_REPO,
    INSTALLER_GITHUB_OWNER, INSTALLER_GITHUB_REPO,
};
use crate::models::Provider;
use tauri_plugin_updater::RemoteRelease;

pub(super) struct Package {
    pub(super) url: String,
    pub(super) signature: String,
    pub(super) name: String,
    pub(super) provider: Provider,
}

pub(super) fn select(manifest: &RemoteRelease, target: UpdateTarget) -> Result<Package, String> {
    let key = target.manifest_key();
    let url = manifest
        .download_url(&key)
        .map_err(|_| "update.noMatchingPackage")?;
    let signature = manifest
        .signature(&key)
        .map_err(|_| "update.manifestOrSignatureUnavailable")?
        .trim();
    let mismatch = "update.manifestPlatformMismatch";
    let reference = crate::providers::release_asset_reference(url).ok_or(mismatch)?;
    let (host, owner, repositories): (_, _, &[&str]) = match reference.provider {
        Provider::Github => (
            "github.com",
            INSTALLER_GITHUB_OWNER,
            &[INSTALLER_GITHUB_REPO],
        ),
        Provider::Gitee => ("gitee.com", INSTALLER_GITEE_OWNER, &[INSTALLER_GITEE_REPO]),
        Provider::Gitcode => (
            "gitcode.com",
            INSTALLER_GITCODE_OWNER,
            &[INSTALLER_GITCODE_REPO, "Overcooked-2-MOD-Manager"],
        ),
    };
    if url.scheme() != "https"
        || url.host_str() != Some(host)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || reference.username != owner
        || !repositories.contains(&reference.repository.as_str())
        || !url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .is_some_and(|segment| segment == reference.asset)
        || reference.tag != format!("v{}", manifest.version)
        || url.path()
            != format!(
                "/{owner}/{}/releases/download/{}/{}",
                reference.repository, reference.tag, reference.asset
            )
        || !target.matches_name(reference.asset.as_str(), &manifest.version)
        || signature.is_empty()
    {
        return Err(mismatch.into());
    }
    Ok(Package {
        url: url.to_string(),
        signature: signature.to_owned(),
        name: reference.asset,
        provider: reference.provider,
    })
}
