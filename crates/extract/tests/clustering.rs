//! Layer 1: the clustering, driven directly with hand-built glyph vectors.
//! No PDF, which is the payoff of the trait.

use extract::{ClusterConfig, Glyph, cluster};

/// Build a glyph.
fn g(ch: char, x0: f32, x1: f32, y: f32) -> Glyph {
    Glyph { ch, x0, x1, y }
}

/// Lay a string out left to right from `x0` on baseline `y`, one glyph per
/// character with a fixed advance.
fn word(text: &str, x0: f32, y: f32) -> Vec<Glyph> {
    const ADVANCE: f32 = 6.0;
    const WIDTH: f32 = 5.0;
    text.chars()
        .enumerate()
        .map(|(i, ch)| {
            let left = x0 + i as f32 * ADVANCE;
            g(ch, left, left + WIDTH, y)
        })
        .collect()
}

#[test]
fn empty_input_yields_empty_grid() {
    let grid = cluster(&[], &ClusterConfig::default());
    assert_eq!(grid.row_count(), 0);
    assert!(grid.columns().is_empty());
    assert_eq!(grid.cell(0, 0), "");
}

#[test]
fn separates_columns_by_horizontal_gap() {
    let mut glyphs = word("44", 70.0, 500.0);
    glyphs.extend(word("Mercedes", 130.0, 500.0));
    glyphs.extend(word("1:24.9", 330.0, 500.0));

    let grid = cluster(&glyphs, &ClusterConfig::default());

    assert_eq!(grid.row_count(), 1);
    assert_eq!(grid.columns().len(), 3);
    assert_eq!(grid.cell(0, 0), "44");
    assert_eq!(grid.cell(0, 1), "Mercedes");
    assert_eq!(grid.cell(0, 2), "1:24.9");
}

#[test]
fn multi_baseline_row_stays_one_row() {
    // One logical row: the number sits 0.24 points above the team text.
    let mut glyphs = word("44", 70.0, 700.0);
    glyphs.extend(word("Mercedes", 130.0, 699.76));

    let grid = cluster(&glyphs, &ClusterConfig::default());

    assert_eq!(
        grid.row_count(),
        1,
        "the 0.24 split must not open a new row"
    );
    assert_eq!(grid.columns().len(), 2);
    assert_eq!(grid.cell(0, 0), "44");
    assert_eq!(grid.cell(0, 1), "Mercedes");
}

#[test]
fn baseline_split_below_row_gap_splits_the_row() {
    // The bound is real: a row_gap under the split does drop the row apart,
    // which is the merge-and-drop failure the default guards against.
    let mut glyphs = word("44", 70.0, 700.0);
    glyphs.extend(word("Mercedes", 130.0, 699.76));

    let tight = ClusterConfig {
        column_gap: 12.0,
        row_gap: 0.1,
    };
    let grid = cluster(&glyphs, &tight);

    assert_eq!(grid.row_count(), 2);
    // The number and the team split onto separate rows, each leaving a hole.
    assert_eq!(grid.cell(0, 0), "44");
    assert_eq!(grid.cell(0, 1), "");
    assert_eq!(grid.cell(1, 0), "");
    assert_eq!(grid.cell(1, 1), "Mercedes");
}

#[test]
fn glyph_straddling_two_columns_lands_by_midpoint() {
    // Two clean columns.
    let mut glyphs = vec![
        g('A', 100.0, 112.0, 500.0), // midpoint 106
        g('B', 112.0, 124.0, 500.0), // midpoint 118
        g('C', 200.0, 212.0, 500.0), // midpoint 206
        g('D', 212.0, 224.0, 500.0), // midpoint 218
    ];
    // A wide glyph whose box overlaps both column ranges, but whose midpoint
    // (110) sits inside the left column.
    glyphs.push(g('X', 15.0, 205.0, 500.0));

    let grid = cluster(&glyphs, &ClusterConfig::default());

    assert_eq!(
        grid.columns().len(),
        2,
        "the straddle must not open a column"
    );
    assert_eq!(grid.row_count(), 1);
    // Placed once, in the left column, ordered by x0.
    assert_eq!(grid.cell(0, 0), "XAB");
    assert_eq!(grid.cell(0, 1), "CD");
}

#[test]
fn orders_rows_top_to_bottom_and_columns_left_to_right() {
    let mut glyphs = Vec::new();
    // Bottom row first, right column first: the clustering must still order them.
    glyphs.extend(word("z", 300.0, 640.0));
    glyphs.extend(word("y", 70.0, 640.0));
    glyphs.extend(word("b", 300.0, 700.0));
    glyphs.extend(word("a", 70.0, 700.0));

    let grid = cluster(&glyphs, &ClusterConfig::default());

    assert_eq!(grid.row_count(), 2);
    assert_eq!(grid.columns().len(), 2);
    assert_eq!(grid.cell(0, 0), "a"); // top-left
    assert_eq!(grid.cell(0, 1), "b"); // top-right
    assert_eq!(grid.cell(1, 0), "y"); // bottom-left
    assert_eq!(grid.cell(1, 1), "z"); // bottom-right
}
