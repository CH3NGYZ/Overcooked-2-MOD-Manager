use crate::app_update::UpdateTarget;
use crate::app_update::channels::{
    INSTALLER_GITCODE_OWNER, INSTALLER_GITCODE_REPO, INSTALLER_GITEE_OWNER, INSTALLER_GITEE_REPO,
    INSTALLER_GITHUB_OWNER, INSTALLER_GITHUB_REPO,
};
use crate::models::Provider;
use semver::Version;
use serde::Deserialize;
use serde_json::Value;
use url::Url;

#[derive(Deserialize)]
struct ManifestPackage {
    url: Option<Url>,
    signature: Option<String>,
}

pub(super) struct Package {
    pub(super) url: Option<String>,
    pub(super) signature: String,
    pub(super) name: String,
    pub(super) provider: Provider,
}

pub(super) fn select(
    document: &Value,
    target: UpdateTarget,
    version: &Version,
) -> Result<Option<Package>, String> {
    let Some(platforms) = document.get("platforms") else {
        if document.get("url").is_some() || document.get("signature").is_some() {
            return Err("update.manifestInvalid".into());
        }
        return Ok(None);
    };
    let platforms = platforms.as_object().ok_or("update.manifestInvalid")?;
    let Some(entry) = platforms.get(&target.manifest_key()) else {
        return Ok(None);
    };
    let ManifestPackage { url, signature } =
        ManifestPackage::deserialize(entry).map_err(|_| "update.manifestInvalid".to_string())?;
    let Some(url) = url else {
        return Ok(None);
    };
    let mismatch = "update.manifestPlatformMismatch";
    let reference = crate::providers::release_asset_reference(&url).ok_or(mismatch)?;
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
        || reference.tag != format!("v{version}")
        || url.path()
            != format!(
                "/{owner}/{}/releases/download/{}/{}",
                reference.repository, reference.tag, reference.asset
            )
        || !target.matches_name(reference.asset.as_str(), version)
    {
        return Err(mismatch.into());
    }
    let signature = signature.as_deref().unwrap_or_default().trim();
    Ok(Some(Package {
        url: (!signature.is_empty()).then(|| url.to_string()),
        signature: signature.to_owned(),
        name: reference.asset,
        provider: reference.provider,
    }))
}
