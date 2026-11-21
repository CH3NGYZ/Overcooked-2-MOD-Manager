//! Inspect both live paths and report the obstacle only when the solve uses it.
use super::*;

#[derive(Debug)]
pub(super) struct EndpointError {
    obstacle: InstallBlock,
    relative: String,
}

impl EndpointError {
    fn new(obstacle: InstallBlock, relative: &str) -> Self {
        Self {
            obstacle,
            relative: relative.into(),
        }
    }

    fn from_transaction(error: &str, relative: &str) -> Self {
        let obstacle = match error {
            "recycle.unsafePath" => InstallBlock::Unsafe,
            "recycle.fileOperationFailed" => InstallBlock::Unreadable,
            _ => InstallBlock::Unverified,
        };
        Self::new(obstacle, relative)
    }

    fn from_io(error: std::io::Error, relative: &str) -> Self {
        log::debug!("读取安装位置失败：路径={relative}，系统错误={error}");
        Self::new(InstallBlock::Unreadable, relative)
    }

    pub(super) fn report(self, owner: &str) -> String {
        self.obstacle.report(owner, &self.relative)
    }
}

impl Builder<'_> {
    pub(super) fn live_endpoint(
        &self,
        canonical: &str,
        is_dll: bool,
    ) -> Result<Option<(String, Fingerprint)>, EndpointError> {
        let active = self.live_file(canonical)?;
        let inactive = if is_dll {
            self.live_file(&mods::disabled_dll_file(canonical))?
        } else {
            None
        };
        if active.is_some() && inactive.is_some() {
            return Err(EndpointError::new(InstallBlock::BothStates, canonical));
        }
        Ok(active.or(inactive))
    }

    fn live_file(&self, relative: &str) -> Result<Option<(String, Fingerprint)>, EndpointError> {
        let path = game(relative)
            .resolve(&self.roots)
            .map_err(|error| EndpointError::from_transaction(&error, relative))?;
        let fingerprint = match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() => Fingerprint::capture(&path)
                .map_err(|error| EndpointError::from_transaction(&error, relative))?,
            Ok(meta) if meta.is_dir() => {
                return Err(EndpointError::new(InstallBlock::Directory, relative));
            }
            Ok(_) => return Err(EndpointError::new(InstallBlock::Unsafe, relative)),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(EndpointError::from_io(error, relative)),
        };
        let real =
            fs::canonicalize(&path).map_err(|error| EndpointError::from_io(error, relative))?;
        let spelled = inventory::relative(&self.roots.game_root, &real)
            .map_err(|_| EndpointError::new(InstallBlock::Unsafe, relative))?;
        if !matches!(fingerprint, Fingerprint::File { .. }) {
            return Err(EndpointError::new(
                InstallBlock::VerificationChanged,
                relative,
            ));
        }
        if path_key(Path::new(&spelled)) != path_key(Path::new(relative)) {
            return Err(EndpointError::new(InstallBlock::Unsafe, relative));
        }
        Ok(Some((spelled, fingerprint)))
    }
}
