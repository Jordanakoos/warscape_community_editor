//! The Terrain tool's document: a battle map open in the editor (docs/plans/terrain_editor.md, Phase 2).

pub mod authoring;
pub mod cells;
pub mod document;
pub mod lf;
mod save_validation;
pub mod terry;

pub use document::{MapDocument, MapSource};

#[cfg(test)]
mod publication_tests;

#[cfg(test)]
mod baseline_tests;
