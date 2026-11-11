//! Trusted scan results. Caller-provided InstalledMod arrays never authorize files.
use crate::models::LocalModSnapshot;
use std::path::{Path, PathBuf};
#[derive(Default)]
pub(crate) struct LocalModSnapshots {
    next: u64,
    active: u64,
    root: Option<PathBuf>,
    value: Option<LocalModSnapshot>,
}
impl LocalModSnapshots {
    pub(crate) fn begin(&mut self, root: Option<&Path>) -> u64 {
        self.next += 1;
        self.active = self.next;
        self.root = root.map(Path::to_path_buf);
        self.value = None;
        self.active
    }
    pub(crate) fn publish(
        &mut self,
        generation: u64,
        root: Option<&Path>,
        mods: Vec<crate::models::InstalledMod>,
    ) -> Result<LocalModSnapshot, String> {
        if self.active != generation || self.root.as_deref() != root {
            return Err("diychef.staleScan".into());
        }
        let value = LocalModSnapshot { generation, mods };
        self.value = Some(value.clone());
        Ok(value)
    }
    pub(crate) fn get(&self, generation: u64, root: &Path) -> Result<LocalModSnapshot, String> {
        if self.active != generation || self.root.as_deref() != Some(root) {
            return Err("diychef.staleScan".into());
        }
        self.value.clone().ok_or_else(|| "diychef.staleScan".into())
    }
    pub(crate) fn invalidate(&mut self) {
        self.next += 1;
        self.active = self.next;
        self.value = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_scan_cannot_publish() {
        let mut s = LocalModSnapshots::default();
        let p = Path::new("game");
        let a = s.begin(Some(p));
        let b = s.begin(Some(p));
        assert!(s.publish(a, Some(p), vec![]).is_err());
        assert!(s.publish(b, Some(p), vec![]).is_ok());
        s.invalidate();
        assert!(s.get(b, p).is_err());
    }
    #[test]
    fn trusted_generation_is_bound_to_root_and_completed_scan() {
        let mut s = LocalModSnapshots::default();
        let a = Path::new("game-a");
        let b = Path::new("game-b");
        let first = s.begin(Some(a));
        assert!(
            s.get(first, a).is_err(),
            "pending scan cannot authorize an editor"
        );
        assert!(s.publish(first, Some(b), vec![]).is_err());
        s.publish(first, Some(a), vec![]).unwrap();
        assert!(s.get(first, b).is_err());
        assert_eq!(s.get(first, a).unwrap().generation, first);
        let next = s.begin(Some(b));
        assert!(next > first);
        assert!(s.get(first, a).is_err());
        assert!(s.publish(first, Some(a), vec![]).is_err());
        s.publish(next, Some(b), vec![]).unwrap();
        assert_eq!(s.get(next, b).unwrap().generation, next);
    }
}
