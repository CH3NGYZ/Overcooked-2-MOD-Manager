//! The only prefer parser and lossless editor.
mod document;
mod edit;
pub(crate) use document::{Document, LineKind, decode};
pub(crate) use edit::plan;

#[cfg(test)]
mod tests;
