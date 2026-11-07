use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub(crate) struct ModZipInstallPlan {
    pub(super) target_dir: String,
    pub(super) strip_root: Option<String>,
}

pub(super) fn repository_mod_zip_plan(
    zip_path: &Path,
    install_target: &str,
    archive_name: &str,
) -> Result<ModZipInstallPlan, String> {
    let layout = inspect_mod_zip(zip_path)?;
    let strip_root = match layout {
        ModZipLayout::TopLevelBepInEx | ModZipLayout::TopLevelPlugins(_) => {
            online_zip_strip_root(zip_path, install_target)?
        }
        _ => None,
    };
    let plan = ModZipInstallPlan {
        target_dir: if layout == ModZipLayout::Wrapped {
            wrapped_zip_target(install_target, archive_name)?
        } else {
            install_target.to_string()
        },
        strip_root,
    };
    log::debug!(
        "生成在线 MOD ZIP 安装计划：目标={}，剥离根目录={:?}",
        plan.target_dir,
        plan.strip_root
    );
    Ok(plan)
}

pub(super) fn local_mod_zip_plan(
    zip_path: &Path,
    archive_name: &str,
) -> Result<ModZipInstallPlan, String> {
    let layout = inspect_mod_zip(zip_path)?;
    log::debug!(
        "识别本地 MOD ZIP 布局：文件={}，布局={:?}",
        zip_path.display(),
        layout
    );
    match layout {
        ModZipLayout::TopLevelBepInEx => Ok(ModZipInstallPlan {
            target_dir: "BepInEx".into(),
            strip_root: Some("BepInEx".into()),
        }),
        ModZipLayout::TopLevelPlugins(root) => Ok(ModZipInstallPlan {
            target_dir: "BepInEx/plugins".into(),
            strip_root: Some(root),
        }),
        ModZipLayout::TopLevelDlls | ModZipLayout::SingleDirectory => Ok(ModZipInstallPlan {
            target_dir: "BepInEx/plugins".into(),
            strip_root: None,
        }),
        ModZipLayout::Wrapped => Ok(ModZipInstallPlan {
            target_dir: wrapped_zip_target("BepInEx/plugins", archive_name)?,
            strip_root: None,
        }),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ModZipLayout {
    TopLevelBepInEx,
    TopLevelPlugins(String),
    TopLevelDlls,
    SingleDirectory,
    Wrapped,
}

fn safe_zip_stem(archive_name: &str) -> Result<&str, String> {
    if archive_name
        .chars()
        .any(|character| matches!(character, '/' | '\\' | ':'))
    {
        return Err("archive.invalidZipName".into());
    }
    let stem = Path::new(archive_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("archive.invalidZipName")?;
    if stem.is_empty() || stem == "." || stem == ".." {
        return Err("archive.invalidZipName".into());
    }
    Ok(stem)
}

fn wrapped_zip_target(install_target: &str, archive_name: &str) -> Result<String, String> {
    let stem = safe_zip_stem(archive_name)?;
    let target = Path::new(install_target);
    let is_package_directory = !stem.eq_ignore_ascii_case("BepInEx")
        && !stem.eq_ignore_ascii_case("plugins")
        && target
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(stem));
    Ok(if is_package_directory {
        target.to_string_lossy().into_owned()
    } else {
        target.join(stem).to_string_lossy().into_owned()
    })
}

fn inspect_mod_zip(zip_path: &Path) -> Result<ModZipLayout, String> {
    log::trace!("检查 MOD ZIP 顶层布局：文件={}", zip_path.display());
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &e.to_string())])
    })?;
    let mut top_level = HashSet::new();
    let mut directory_root = None;
    let mut only_root_dlls = !archive.is_empty();

    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| {
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &e.to_string())],
            )
        })?;
        let path = entry.enclosed_name().ok_or_else(|| {
            crate::locale_tag::with_params("archive.unsafePath", &[("entry", entry.name())])
        })?;
        let components = path.components().collect::<Vec<_>>();
        let Some(root) = components.first() else {
            only_root_dlls = false;
            continue;
        };
        let root_name = root.as_os_str().to_string_lossy().into_owned();
        top_level.insert(root_name.to_ascii_lowercase());
        if components.len() > 1 || entry.is_dir() {
            only_root_dlls = false;
            directory_root = Some(root_name);
        } else if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            only_root_dlls = false;
        }
    }

    if top_level.len() == 1 {
        if let Some(root) = directory_root {
            return Ok(if root.eq_ignore_ascii_case("BepInEx") {
                ModZipLayout::TopLevelBepInEx
            } else if root.eq_ignore_ascii_case("plugins") {
                ModZipLayout::TopLevelPlugins(root)
            } else {
                ModZipLayout::SingleDirectory
            });
        }
    }
    if only_root_dlls {
        return Ok(ModZipLayout::TopLevelDlls);
    }
    Ok(ModZipLayout::Wrapped)
}

fn online_zip_strip_root(zip_path: &Path, install_target: &str) -> Result<Option<String>, String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        crate::locale_tag::with_params("archive.zipReadFailed", &[("reason", &e.to_string())])
    })?;
    let mut file_paths = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| {
            crate::locale_tag::with_params(
                "archive.zipEntryReadFailed",
                &[("reason", &e.to_string())],
            )
        })?;
        let path = entry.enclosed_name().ok_or_else(|| {
            crate::locale_tag::with_params("archive.unsafePath", &[("entry", entry.name())])
        })?;
        if !entry.is_dir() {
            file_paths.push(path.to_path_buf());
        }
    }
    if file_paths.is_empty() {
        return Ok(None);
    }
    let target_components = Path::new(install_target).components().collect::<Vec<_>>();
    // Only remove directories already represented by the configured target.
    // A lone ordinary MOD folder is payload, not an archive wrapper.
    for start in 0..target_components.len() {
        let expected = &target_components[start..];
        let target_matches_all = file_paths.iter().all(|path| {
            let components = path.components().collect::<Vec<_>>();
            components.len() > expected.len()
                && components
                    .iter()
                    .zip(expected.iter())
                    .all(|(actual, expected)| {
                        actual
                            .as_os_str()
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
                    })
        });
        if target_matches_all {
            let strip_root = expected.iter().collect::<PathBuf>();
            return Ok(Some(strip_root.to_string_lossy().into_owned()));
        }
    }
    if let Some(expected_root) = target_components.first() {
        let expected_root = expected_root.as_os_str().to_string_lossy();
        if !expected_root.is_empty()
            && file_paths.iter().all(|path| {
                path.components().next().is_some_and(|actual| {
                    actual
                        .as_os_str()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&expected_root)
                })
            })
        {
            return Ok(Some(expected_root.into_owned()));
        }
    }
    Ok(None)
}
