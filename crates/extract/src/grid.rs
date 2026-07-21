//! The positioned cell grid the clustering emits.

/// A detected column, as a page-space x-range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    /// Left edge of the column, the smallest glyph `x0` it holds.
    pub x0: f32,
    /// Right edge of the column, the largest glyph `x1` it holds.
    pub x1: f32,
}

/// A table of positioned cells, built from geometry alone.
///
/// Rows run top to bottom, columns left to right. A cell holds the text of the
/// glyphs that fell in that row and column, ordered left to right. The grid
/// carries no F1 knowledge and names no column; it is the primitive later
/// document parsers consume.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    columns: Vec<Column>,
    // Row-major: `cells[row][col]`. Every row has `columns.len()` entries.
    cells: Vec<Vec<String>>,
}

impl Grid {
    /// Build a grid from its columns and row-major cell text.
    ///
    /// # Panics
    ///
    /// Panics if any row's length differs from the column count. The clustering
    /// upholds this, so a mismatch is a bug in the caller.
    #[must_use]
    pub fn new(columns: Vec<Column>, cells: Vec<Vec<String>>) -> Self {
        assert!(
            cells.iter().all(|row| row.len() == columns.len()),
            "every grid row must have one cell per column",
        );
        Self { columns, cells }
    }

    /// Number of rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.cells.len()
    }

    /// The columns, left to right.
    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// The text in the cell at `row` and `col`, or `""` if either is out of
    /// range.
    #[must_use]
    pub fn cell(&self, row: usize, col: usize) -> &str {
        self.cells
            .get(row)
            .and_then(|r| r.get(col))
            .map_or("", String::as_str)
    }
}
