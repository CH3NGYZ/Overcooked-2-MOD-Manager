use notify::{
    EventKind,
    event::{CreateKind, ModifyKind, RemoveKind},
};
use std::path::{Path, PathBuf};

/// Filter nonrecursive ancestor watches to the names that can change a root.
/// The logical name guards junction replacement; the resolved path observes
/// content, and neither may register outside the verified game directory.
pub(crate) struct RootScope {
    pub(crate) root: PathBuf,
    pub(crate) logical: PathBuf,
    boundaries: Vec<(PathBuf, bool)>,
    guards: Vec<PathBuf>,
}

impl RootScope {
    pub(crate) fn new(game: &Path, logical: PathBuf, root: PathBuf) -> Self {
        let mut boundaries = [&logical, &root]
            .into_iter()
            .flat_map(|path| path.ancestors())
            .filter(|path| path.starts_with(game))
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        boundaries.sort_unstable();
        boundaries.dedup();
        let boundaries = boundaries
            .into_iter()
            .map(|path| {
                let present = path.is_dir();
                (path, present)
            })
            .collect::<Vec<_>>();
        let guards = boundaries
            .iter()
            .filter(|(path, present)| {
                *present
                    && path != &root
                    && path != &logical
                    && crate::game::resolve_existing_ancestor(path)
                        .is_ok_and(|resolved| resolved.starts_with(game))
            })
            .map(|(path, _)| path.clone())
            .collect();
        Self {
            root,
            logical,
            boundaries,
            guards,
        }
    }

    pub(crate) fn guards(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.guards.iter().cloned()
    }

    pub(crate) fn content<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        relative_path(path, &self.root)
            .or_else(|| relative_path(path, &self.logical))
            .filter(|relative| !relative.as_os_str().is_empty())
    }

    pub(crate) fn topology_changed(&self, path: &Path, kind: EventKind) -> bool {
        self.boundaries.iter().any(|(boundary, present)| {
            relative_path(path, boundary).is_some_and(|relative| relative.as_os_str().is_empty())
                && match kind {
                    // FSEvents can replay Create(Folder) for an already watched directory.
                    EventKind::Create(CreateKind::Folder | CreateKind::Any) => {
                        (cfg!(windows) || !present) && path.is_dir()
                    }
                    EventKind::Remove(RemoveKind::File) => false,
                    EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => {
                        *present || path.is_dir()
                    }
                    _ => false,
                }
        })
    }
}

pub(crate) fn relative_path<'a>(path: &'a Path, root: &Path) -> Option<&'a Path> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative);
    }
    #[cfg(windows)]
    {
        // A Windows parent watch reports entry names with on-disk casing.
        let mut parts = path.components();
        for part in root.components() {
            if !parts
                .next()?
                .as_os_str()
                .eq_ignore_ascii_case(part.as_os_str())
            {
                return None;
            }
        }
        Some(parts.as_path())
    }
    #[cfg(not(windows))]
    None
}
