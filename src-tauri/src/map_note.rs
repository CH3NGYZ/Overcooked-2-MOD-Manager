//! Release note 地图声明块的共享读取、关系、模板与写回入口。

mod assets;
mod block;
mod metadata;
mod publish;
mod relations;

pub use assets::{VERSION_PLACEHOLDER, versioned_asset_rule};
// Preserve every former `crate::map_note::*` public path; some are currently test-/type-only seams.
#[allow(unused_imports)]
pub use block::{
    NOTE_BLOCK_CLOSE, NOTE_BLOCK_OPEN, NoteBlock, parse_release_note, strip_note_block,
};
pub use metadata::MapNoteMetadata;
#[allow(unused_imports)]
pub use publish::{
    publish_metadata_from_note, render_metadata_block, render_release_body,
    replace_body_keeping_block,
};
#[allow(unused_imports)]
pub use relations::{NoteReleaseRelations, RelationSpec};

#[cfg(test)]
mod test_support;
