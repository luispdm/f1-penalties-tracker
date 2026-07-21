//! Error type for extraction.

use std::fmt;

/// A failure while turning a PDF into glyphs.
///
/// The engine's own error is flattened into a message so this type, part of the
/// [`GlyphSource`](crate::GlyphSource) contract, names no engine type.
#[derive(Debug)]
pub enum ExtractError {
    /// The underlying PDF engine failed to load the document or read a page.
    Engine(String),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine(msg) => write!(f, "pdf engine error: {msg}"),
        }
    }
}

impl std::error::Error for ExtractError {}
