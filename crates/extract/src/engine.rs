//! The engine abstraction that keeps the reader swappable.

use crate::{error::ExtractError, glyph::Glyph};

/// A source of positioned glyphs for one page of a document.
///
/// The clustering never touches this trait or a concrete engine; it works on
/// the [`Glyph`] vectors an engine yields. Keeping the engine behind a trait
/// makes the choice reversible and lets the geometry run without any PDF.
pub trait GlyphSource {
    /// Return every glyph on `page`, in no particular order.
    ///
    /// `page` is zero-based.
    ///
    /// # Errors
    ///
    /// Returns [`ExtractError::Engine`] if the page is out of range or the
    /// engine fails to read it.
    fn glyphs(&self, page: usize) -> Result<Vec<Glyph>, ExtractError>;
}
