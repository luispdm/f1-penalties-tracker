//! Layer 1, property tests: random column layouts with jittered baselines,
//! asserting the clustering recovers the columns and the row grouping.

use extract::{ClusterConfig, Glyph, cluster};
use proptest::prelude::*;

// Geometry chosen so every gap clears the thresholds below with room to spare:
// columns 120 apart hold tokens at most 27 wide; rows sit 30 apart; per-cell
// baseline jitter stays under half the row gap.
const COLUMN_SPACING: f32 = 120.0;
const ROW_SPACING: f32 = 30.0;
const ADVANCE: f32 = 7.0;
const WIDTH: f32 = 6.0;
const COLUMN_X0: f32 = 100.0;
const TOP_BASELINE: f32 = 500.0;

const CONFIG: ClusterConfig = ClusterConfig {
    column_gap: 10.0,
    row_gap: 2.0,
};

/// A rectangular table of tokens plus a per-cell baseline jitter.
#[derive(Debug)]
struct Layout {
    cols: usize,
    rows: usize,
    tokens: Vec<String>, // row-major, len == rows * cols
    jitters: Vec<f32>,   // row-major, len == rows * cols, each in [-0.4, 0.4]
}

/// A non-empty token of 1..=4 uppercase-letter or digit glyphs.
fn token() -> impl Strategy<Value = String> {
    let glyph = prop_oneof![
        (b'A'..=b'Z').prop_map(char::from),
        (b'0'..=b'9').prop_map(char::from),
    ];
    proptest::collection::vec(glyph, 1..=4).prop_map(|chars| chars.into_iter().collect())
}

fn layout() -> impl Strategy<Value = Layout> {
    (2usize..=4, 1usize..=4).prop_flat_map(|(cols, rows)| {
        let count = rows * cols;
        let tokens = proptest::collection::vec(token(), count);
        let jitters = proptest::collection::vec(-4i32..=4, count);
        (Just(cols), Just(rows), tokens, jitters).prop_map(|(cols, rows, tokens, jitters)| Layout {
            cols,
            rows,
            tokens,
            jitters: jitters.into_iter().map(|j| j as f32 / 10.0).collect(),
        })
    })
}

/// Turn a layout into a glyph vector.
fn glyphs_of(layout: &Layout) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    for row in 0..layout.rows {
        for col in 0..layout.cols {
            let idx = row * layout.cols + col;
            let baseline = TOP_BASELINE - row as f32 * ROW_SPACING + layout.jitters[idx];
            let cell_x0 = COLUMN_X0 + col as f32 * COLUMN_SPACING;
            for (i, ch) in layout.tokens[idx].chars().enumerate() {
                let left = cell_x0 + i as f32 * ADVANCE;
                glyphs.push(Glyph {
                    ch,
                    x0: left,
                    x1: left + WIDTH,
                    y: baseline,
                });
            }
        }
    }
    glyphs
}

proptest! {
    #[test]
    fn recovers_columns_and_rows(layout in layout()) {
        let grid = cluster(&glyphs_of(&layout), &CONFIG);

        prop_assert_eq!(grid.row_count(), layout.rows);
        prop_assert_eq!(grid.columns().len(), layout.cols);
        for row in 0..layout.rows {
            for col in 0..layout.cols {
                let expected = layout.tokens[row * layout.cols + col].as_str();
                prop_assert_eq!(grid.cell(row, col), expected);
            }
        }
    }
}
