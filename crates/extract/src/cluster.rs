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
#[must_use]
pub fn cluster(glyphs: &[Glyph], config: &ClusterConfig) -> Grid {
    if glyphs.is_empty() {
        return Grid::new(Vec::new(), Vec::new());
    }

    // Columns: cluster midpoints ascending, so column 0 is leftmost.
    let mid_x: Vec<f32> = glyphs.iter().map(Glyph::mid_x).collect();
    let col_of = cluster_1d(&mid_x, config.column_gap, Order::Ascending);
    let col_count = cluster_count(&col_of);

    // Rows: cluster baselines descending, so row 0 is topmost.
    let baseline: Vec<f32> = glyphs.iter().map(|g| g.y).collect();
    let row_of = cluster_1d(&baseline, config.row_gap, Order::Descending);
    let row_count = cluster_count(&row_of);

    let columns = column_ranges(glyphs, &col_of, col_count);
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

/// The x-range of each column: the tightest span over its glyphs.
fn column_ranges(glyphs: &[Glyph], col_of: &[usize], col_count: usize) -> Vec<Column> {
    let mut columns = vec![
        Column {
            x0: f32::INFINITY,
            x1: f32::NEG_INFINITY,
        };
        col_count
    ];
    for (glyph, &col) in glyphs.iter().zip(col_of) {
        let range = &mut columns[col];
        range.x0 = range.x0.min(glyph.x0);
        range.x1 = range.x1.max(glyph.x1);
    }
    columns
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
                    cell.into_iter().map(|(_, ch)| ch).collect()
                })
                .collect()
        })
        .collect()
}
