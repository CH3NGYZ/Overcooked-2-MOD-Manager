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
            let source = crate::unity_assets::Source::Bundle(path.into(), node.clone());
            let file = crate::unity_assets::metadata(&source, &mut self.blocks)?;
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
