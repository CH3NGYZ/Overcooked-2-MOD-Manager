//! Logical member keys authorize an owner; live renames retain filesystem spelling.
use super::*;

impl Builder<'_> {
    pub(super) fn physical(&self, owner: &str, member_key: &str) -> Result<String, String> {
        let files = self
            .session
            .trusted
            .files
            .get(owner)
            .ok_or("modChanges.ambiguousOwnership")?;
        let key = inventory::key(&mods::canonical_dll_name(member_key));
        let originals = files
            .summary
            .members
            .iter()
            .map(|member| member.dll_file.as_str())
            .chain(files.summary.dll_files.iter().map(String::as_str))
            .map(mods::canonical_dll_name)
            .filter(|path| inventory::key(path) == key)
            .collect::<BTreeSet<_>>();
        let root = crate::game::resolve_existing_ancestor(&self.roots.game_root)?;
        let mut physical = BTreeSet::new();
        for canonical in originals {
            let disabled = mods::disabled_dll_file(&canonical);
            // Resolve through transaction validation before canonicalization, so
            // a matching key cannot hide a symlink or junction inside the game.
            let active = game(&canonical).resolve(&self.roots)?;
            let inactive = game(&disabled).resolve(&self.roots)?;
            let relative = match (active.is_file(), inactive.is_file()) {
                (true, false) => canonical,
                (false, true) => disabled,
                _ => return Err("modChanges.ambiguousOwnership".into()),
            };
            let actual = crate::game::safe_game_path(&self.roots.game_root, &relative)?;
            physical.insert(inventory::relative(&root, &actual)?);
        }
        if physical.len() != 1 {
            return Err("modChanges.ambiguousOwnership".into());
        }
        physical
            .pop_first()
            .ok_or("modChanges.ambiguousOwnership".into())
    }
}
