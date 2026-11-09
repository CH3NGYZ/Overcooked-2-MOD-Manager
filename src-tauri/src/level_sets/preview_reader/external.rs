//! External bundles retain only their object directory/type metadata. Referenced
//! objects and texture streams are read through the shared bounded block reader.
use super::*;
impl Context {
    pub(super) fn load_external(&mut self, path: &Path) -> Result<(), String> {
        let names = crate::asset_bundle::bundle_file_names(path).map_err(|_| failure())?;
        let mut documents = vec![];
        let mut streams = vec![];
        for node in names {
            let name = node.rsplit('/').next().unwrap_or(&node).to_string();
            if name.to_ascii_lowercase().ends_with(".ress")
                || name.to_ascii_lowercase().ends_with(".resource")
            {
                streams.push((name, (path.to_path_buf(), node)));
                continue;
            }
            if self.documents.iter().any(|d| d.name == name) {
                continue;
            }
            let header = crate::asset_bundle::read_bundle_file_range(
                path,
                &node,
                0,
                Some(48),
                Some(&mut self.blocks),
            )
            .map_err(|_| failure())?;
            let version = u32::from_be_bytes(header[8..12].try_into().unwrap());
            if !(9..=30).contains(&version) {
                return Err(failure());
            }
            let offset = if version >= 22 {
                usize::try_from(u64::from_be_bytes(header[32..40].try_into().unwrap()))
                    .map_err(|_| failure())?
            } else {
                u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize
            };
            if !(48..=16 * 1024 * 1024).contains(&offset) {
                return Err(failure());
            }
            let bytes = crate::asset_bundle::read_bundle_file_range(
                path,
                &node,
                0,
                Some(offset),
                Some(&mut self.blocks),
            )
            .map_err(|_| failure())?;
            let mut reader = Reader::new(bytes, unityfs::UnityVersion::default());
            let file = try_read_serialized(&mut reader).ok_or_else(failure)?;
            documents.push(Document {
                backing: Some((path.into(), node)),
                name,
                source: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                file,
            });
        }
        let retained: usize = self
            .documents
            .iter()
            .chain(&documents)
            .map(|d| d.file.data.len())
            .sum::<usize>()
            + self.raw.values().map(Vec::len).sum::<usize>();
        if retained > 512 * 1024 * 1024 {
            return Err(failure());
        }
        self.documents.extend(documents);
        self.streams.extend(streams);
        Ok(())
    }
}
