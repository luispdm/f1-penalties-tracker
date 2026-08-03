//! Geometric clustering: glyphs in, a positioned [`Grid`] out.
//!
//! Two one-dimensional single-linkage passes do the work. Columns come from
//! clustering glyph midpoints along x; rows come from clustering baselines
//! along y. Both split a cluster only where the gap between adjacent sorted
//! values exceeds a threshold.
//!
//! The row pass is where the multi-baseline trap lives. One logical row can
//! print across two baselines a fraction of a point apart, the number column on
//! one and the team text on another. A `row_gap` above that split but below the
//! line spacing keeps the two baselines in one row, so a car number never merges
//! into a team name and no row is dropped.
//!
//! The column pass is where the whitespace trap lives. A PDF engine reports a
//! space as a glyph with a real box, and a document pads the run between two
//! columns with them. Cluster those midpoints and each space is a stepping stone
//! from one column to the next, so single linkage walks the padding across every
//! boundary and a ten-column table comes out as three. No `column_gap` escapes
//! it: widening the threshold only merges more.
//!
//! So the boundaries come from the glyphs that carry ink. Whitespace is excluded
//! from the pass that finds the columns, then every glyph, spaces included,
//! joins the column its midpoint falls in. Dropping the spaces outright would be
//! simpler and wrong: it welds `Falcon Racing` into `FalconRacing`, and the fact
//! layer stores each printed team string as the document prints it. Keeping them
//! and trimming each cell's edges leaves the internal space and no padding
//! artefact.
//!
//! The two passes therefore treat whitespace differently, and deliberately. The
//! column pass drops it; the row pass clusters the baselines of every glyph. The
//! asymmetry follows from what each pass produces. A column that whitespace
//! invents is silent: the padding chains real columns together and the caller
//! reads a table that looks whole. A row that whitespace invents is not, because
//! rows are indexed positionally. A space more than `row_gap` from any inked
//! baseline would open a row of empty cells and shift every row below it by one,
//! so a caller that names its first data row, as
//! `crates/ingest/tests/pu_snapshot.rs` does, would read the header as data.
//! Filtering the row pass too would remove that hazard, but no document has
//! shown it: no committed fixture puts a space glyph on a baseline of its own.
//! Until one does, the row pass keeps every glyph.

use crate::{
    glyph::Glyph,
    grid::{Column, Grid},
};

/// Thresholds for the clustering, in page-space points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterConfig {
    /// Minimum horizontal whitespace that separates two columns. Must exceed
    /// the spacing between glyphs within a column, and stay below the gap
    /// between columns.
    pub column_gap: f32,
    /// Largest vertical baseline gap still treated as one logical row. Must
    /// exceed the multi-baseline split within a row, and stay below the line
    /// spacing between rows.
    pub row_gap: f32,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            column_gap: 12.0,
            row_gap: 3.0,
        }
    }
}

/// Group `glyphs` into a positioned [`Grid`] using geometry alone.
///
/// Columns are ordered left to right, rows top to bottom. A glyph joins the
/// column its midpoint falls in and the row its baseline falls in, so a glyph
/// whose box straddles two columns is placed once, by its centre.
///
/// Only inked glyphs shape the columns; whitespace is then assigned like any
/// other glyph and trimmed off each cell's edges. Input with no ink yields an
/// empty grid, because whitespace alone marks no column.
#[must_use]
pub fn cluster(glyphs: &[Glyph], config: &ClusterConfig) -> Grid {
    // Columns: cluster inked midpoints ascending, so column 0 is leftmost.
    let ink: Vec<Glyph> = glyphs
        .iter()
        .copied()
        .filter(|g| !g.ch.is_whitespace())
        .collect();
    if ink.is_empty() {
        return Grid::new(Vec::new(), Vec::new());
    }
    let ink_col = cluster_1d(
        &ink.iter().copied().map(Glyph::mid_x).collect::<Vec<f32>>(),
        config.column_gap,
        Order::Ascending,
    );
    let col_count = cluster_count(&ink_col);

    // Every glyph, whitespace included, then joins the column it falls in.
    let boundaries = column_boundaries(&ink, &ink_col, col_count);
    let col_of: Vec<usize> = glyphs
        .iter()
        .map(|g| column_at(&boundaries, g.mid_x()))
        .collect();

    // Rows: cluster the baselines of every glyph, descending, so row 0 is
    // topmost. See the module doc on why this pass keeps the whitespace the
    // column pass drops.
    let baseline: Vec<f32> = glyphs.iter().map(|g| g.y).collect();
    let row_of = cluster_1d(&baseline, config.row_gap, Order::Descending);
    let row_count = cluster_count(&row_of);

    let columns = column_ranges(&ink, &ink_col, col_count);
    let cells = assemble_cells(glyphs, &row_of, &col_of, row_count, col_count);

    Grid::new(columns, cells)
}

/// Sort direction for [`cluster_1d`].
#[derive(Clone, Copy)]
enum Order {
    Ascending,
    Descending,
}

/// Single-linkage 1D clustering.
///
/// Returns the cluster id of each input value. Ids increase along the sort
/// order and rise by one wherever the gap between adjacent sorted values
/// exceeds `gap`.
fn cluster_1d(values: &[f32], gap: f32, order: Order) -> Vec<usize> {
    let mut sorted: Vec<usize> = (0..values.len()).collect();
    sorted.sort_by(|&a, &b| {
        let ord = values[a].total_cmp(&values[b]);
        match order {
            Order::Ascending => ord,
            Order::Descending => ord.reverse(),
        }
    });

    let mut cluster_of = vec![0usize; values.len()];
    let mut current = 0usize;
    let mut prev: Option<f32> = None;
    for &idx in &sorted {
        let value = values[idx];
        if let Some(previous) = prev
            && (value - previous).abs() > gap
        {
            current += 1;
        }
        cluster_of[idx] = current;
        prev = Some(value);
    }
    cluster_of
}

/// Number of distinct clusters, given per-item cluster ids from [`cluster_1d`].
fn cluster_count(cluster_of: &[usize]) -> usize {
    cluster_of.iter().copied().max().map_or(0, |max| max + 1)
}

/// The x-range of each column: the tightest span over its inked glyphs.
///
/// Padding is not part of a column. A run of spaces reaches from one column's
/// ink to the next, so counting it would leave every range touching its
/// neighbours and the ranges would say nothing.
fn column_ranges(ink: &[Glyph], ink_col: &[usize], col_count: usize) -> Vec<Column> {
    let mut columns = vec![
        Column {
            x0: f32::INFINITY,
            x1: f32::NEG_INFINITY,
        };
        col_count
    ];
    for (glyph, &col) in ink.iter().zip(ink_col) {
        let range = &mut columns[col];
        range.x0 = range.x0.min(glyph.x0);
        range.x1 = range.x1.max(glyph.x1);
    }
    columns
}

/// The x cut between each pair of adjacent columns, ascending.
///
/// A cut sits halfway between the rightmost inked midpoint of one column and the
/// leftmost of the next, so every inked glyph keeps the column the clustering
/// gave it and every space joins the column it is nearer.
fn column_boundaries(ink: &[Glyph], ink_col: &[usize], col_count: usize) -> Vec<f32> {
    let mut leftmost = vec![f32::INFINITY; col_count];
    let mut rightmost = vec![f32::NEG_INFINITY; col_count];
    for (glyph, &col) in ink.iter().zip(ink_col) {
        let mid = glyph.mid_x();
        leftmost[col] = leftmost[col].min(mid);
        rightmost[col] = rightmost[col].max(mid);
    }
    (1..col_count)
        .map(|col| f32::midpoint(rightmost[col - 1], leftmost[col]))
        .collect()
}

/// The column a midpoint falls in. A midpoint exactly on a cut goes right.
fn column_at(boundaries: &[f32], mid_x: f32) -> usize {
    boundaries.partition_point(|&cut| cut <= mid_x)
}

/// Concatenate each cell's glyphs, left to right, into its text.
fn assemble_cells(
    glyphs: &[Glyph],
    row_of: &[usize],
    col_of: &[usize],
    row_count: usize,
    col_count: usize,
) -> Vec<Vec<String>> {
    let mut buckets: Vec<Vec<Vec<(f32, char)>>> = vec![vec![Vec::new(); col_count]; row_count];
    for ((glyph, &row), &col) in glyphs.iter().zip(row_of).zip(col_of) {
        buckets[row][col].push((glyph.x0, glyph.ch));
    }

    buckets
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|mut cell| {
                    cell.sort_by(|a, b| a.0.total_cmp(&b.0));
                    let text: String = cell.into_iter().map(|(_, ch)| ch).collect();
                    // Trim the edges only. The padding that reached in from a
                    // neighbouring column goes; the space in `Falcon Racing`
                    // stays.
                    text.trim().to_owned()
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Unit tests for the clustering, driven directly with hand-built glyph
    //! vectors. No PDF, which is the payoff of the trait.

    mod clustering {
        use super::super::*;

        /// Build a glyph.
        fn g(ch: char, x0: f32, x1: f32, y: f32) -> Glyph {
            Glyph { ch, x0, x1, y }
        }

        /// Lay a string out left to right from `x0` on baseline `y`, one glyph
        /// per character with a fixed advance.
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
        fn padding_between_two_columns_does_not_join_them() {
            // The whitespace trap, in miniature. One printed run pads a number
            // out to a team, so ten spaces bridge the gap and every adjacent
            // midpoint sits 6 points from the next, inside the 12-point gap.
            // Cluster those midpoints and the two columns come out as one.
            let glyphs = word("44          Mercedes", 70.0, 500.0);

            let grid = cluster(&glyphs, &ClusterConfig::default());

            assert_eq!(
                grid.columns().len(),
                2,
                "the padding must not chain the columns together"
            );
            // The padding is assigned, then trimmed off both cells' edges.
            assert_eq!(grid.cell(0, 0), "44");
            assert_eq!(grid.cell(0, 1), "Mercedes");
        }

        #[test]
        fn a_column_spans_its_ink_not_its_padding() {
            let glyphs = word("44          Mercedes", 70.0, 500.0);

            let grid = cluster(&glyphs, &ClusterConfig::default());

            // The second `4` occupies 76..81. Counting the padding would push
            // the range out to meet the column beside it.
            assert_eq!(grid.columns()[0], Column { x0: 70.0, x1: 81.0 });
        }

        #[test]
        fn keeps_the_space_inside_a_cell() {
            // The other half of the rule. Dropping whitespace before clustering
            // would recover the columns and weld this into `FalconRacing`, and
            // the fact layer stores the team string as the document prints it.
            let glyphs = word("Falcon Racing", 70.0, 500.0);

            let grid = cluster(&glyphs, &ClusterConfig::default());

            assert_eq!(grid.columns().len(), 1);
            assert_eq!(grid.cell(0, 0), "Falcon Racing");
        }

        #[test]
        fn whitespace_alone_marks_no_column() {
            let grid = cluster(&word("    ", 70.0, 500.0), &ClusterConfig::default());

            assert!(grid.columns().is_empty());
            assert_eq!(grid.row_count(), 0);
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
            // The bound is real: a row_gap under the split does drop the row
            // apart, which is the merge-and-drop failure the default guards
            // against.
            let mut glyphs = word("44", 70.0, 700.0);
            glyphs.extend(word("Mercedes", 130.0, 699.76));

            let tight = ClusterConfig {
                column_gap: 12.0,
                row_gap: 0.1,
            };
            let grid = cluster(&glyphs, &tight);

            assert_eq!(grid.row_count(), 2);
            // The number and the team split onto separate rows, each leaving a
            // hole.
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
            // A wide glyph whose box overlaps both column ranges, but whose
            // midpoint (110) sits inside the left column.
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
            // Bottom row first, right column first: the clustering must still
            // order them.
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
    }

    mod properties {
        //! Property tests: random column layouts with jittered baselines,
        //! asserting the clustering recovers the columns and the row grouping.

        use proptest::prelude::*;

        use super::super::*;

        // Geometry chosen so every gap clears the thresholds below with room to
        // spare: columns 120 apart hold tokens at most 27 wide; rows sit 30
        // apart; per-cell baseline jitter stays under half the row gap.
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
                (Just(cols), Just(rows), tokens, jitters).prop_map(
                    |(cols, rows, tokens, jitters)| Layout {
                        cols,
                        rows,
                        tokens,
                        jitters: jitters.into_iter().map(|j| j as f32 / 10.0).collect(),
                    },
                )
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
    }
}
