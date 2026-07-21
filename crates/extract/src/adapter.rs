//! The `pdf_oxide` adapter.
//!
//! This is the only module that names `pdf_oxide` or calls `extract_chars`.
//! Everything else, the trait, the clustering, the grid, stays engine-agnostic,
//! so swapping the engine touches this file alone.
//!
//! `extract_chars` is deliberate over `extract_spans`: spans merge adjacent
//! tokens such as `"ICE TC EXH"` into one and destroy the column boundaries the
//! clustering depends on.

use pdf_oxide::PdfDocument;

use crate::{engine::GlyphSource, error::ExtractError, glyph::Glyph};

/// A [`GlyphSource`] backed by the pure-Rust `pdf_oxide` engine.
pub struct PdfOxideEngine {
    doc: PdfDocument,
}

impl PdfOxideEngine {
    /// Load a document from in-memory bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ExtractError::Engine`] if the bytes are not a PDF the engine
    /// can parse.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ExtractError> {
        let doc = PdfDocument::from_bytes(bytes).map_err(engine_error)?;
        Ok(Self { doc })
    }
}

impl GlyphSource for PdfOxideEngine {
    fn glyphs(&self, page: usize) -> Result<Vec<Glyph>, ExtractError> {
        let chars = self.doc.extract_chars(page).map_err(engine_error)?;
        Ok(chars
            .into_iter()
            .map(|c| Glyph {
                ch: c.char,
                x0: c.bbox.left(),
                x1: c.bbox.right(),
                y: c.origin_y,
            })
            .collect())
    }
}

/// Flatten an engine error into an engine-agnostic message.
fn engine_error(err: pdf_oxide::error::Error) -> ExtractError {
    ExtractError::Engine(err.to_string())
}
