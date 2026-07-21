//! Error type for extraction.

/// A failure while turning a PDF into glyphs.
///
/// The engine's own error is flattened into a message so this type, part of the
/// [`GlyphSource`](crate::GlyphSource) contract, names no engine type.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// The underlying PDF engine failed to load the document or read a page.
    #[error("pdf engine error: {0}")]
    Engine(String),
}
