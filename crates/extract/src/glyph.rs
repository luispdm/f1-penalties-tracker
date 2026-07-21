//! The single per-glyph shape the geometry works on.

/// One extracted glyph in page space.
///
/// `x0`/`x1` are the left and right edges of the glyph box; `y` is the text
/// baseline. Page space places the origin at the bottom-left corner with `y`
/// increasing upward, so a larger `y` sits higher on the page.
///
/// This is the only shape the clustering sees. It names no PDF-engine type, so
/// the geometry stays testable with hand-built vectors and reusable across
/// engines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// The character this glyph renders.
    pub ch: char,
    /// Left edge of the glyph box, in page-space points.
    pub x0: f32,
    /// Right edge of the glyph box, in page-space points.
    pub x1: f32,
    /// Text baseline, in page-space points.
    pub y: f32,
}

impl Glyph {
    /// Horizontal midpoint of the glyph box.
    ///
    /// Column membership keys on this point, so a glyph whose box straddles two
    /// columns lands in the one its centre falls in.
    #[must_use]
    pub fn mid_x(self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
}
