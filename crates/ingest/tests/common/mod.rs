//! Helpers shared by the fixture tests.
//!
//! Nothing here knows a fixture. Each test file states its own fixture's truth,
//! so that a change to one fixture cannot quietly move another's expectations.

use extract::Grid;

/// Every cell of a grid, joined, so a test can ask what a band swallowed.
pub fn text_of(grid: &Grid) -> String {
    (0..grid.row_count())
        .flat_map(|row| (0..grid.columns().len()).map(move |column| (row, column)))
        .map(|(row, column)| grid.cell(row, column))
        .collect::<Vec<&str>>()
        .join("|")
}
