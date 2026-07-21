//! Turn a PDF into a table of positioned cells using geometry alone.
//!
//! The crate carries zero F1 knowledge, which keeps the geometry reusable and
//! testable on its own. It has three parts:
//!
//! - [`Glyph`], the one shape per glyph the geometry works on, and
//!   [`GlyphSource`], the trait an engine sits behind.
//! - [`PdfOxideEngine`], the `pdf_oxide` adapter. It is the only place a PDF
//!   engine appears, so the choice stays reversible.
//! - [`cluster`], which groups glyphs into a [`Grid`] of rows and columns.
//!
//! Mapping columns to component labels, reading a legend, and the other steps
//! that need F1 knowledge belong to the document-parsers layer, not here. This
//! layer names no column; it emits a positioned grid only.

mod adapter;
mod cluster;
mod engine;
mod error;
mod glyph;
mod grid;

pub use adapter::PdfOxideEngine;
pub use cluster::{ClusterConfig, cluster};
pub use engine::GlyphSource;
pub use error::ExtractError;
pub use glyph::Glyph;
pub use grid::{Column, Grid};
