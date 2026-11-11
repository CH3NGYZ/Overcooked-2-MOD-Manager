//! The only prefer parser and lossless editor.
mod document;
mod edit;
pub(crate) use document::{Document, LineKind, decode};
pub(crate) use edit::plan;

pub(super) const SWITCH_LINE: &str = "LOBBYSWITCHCHEF=TRUE";
pub(crate) const DEFAULT_CONTENT: &[u8] = b"Chef_Black_cat\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n";

#[cfg(test)]
mod tests;
