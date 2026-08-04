//! The identity columns a PU document prints left of its component columns.
//!
//! Every PU document opens its table with the same three columns in the same
//! order: the car number, the team, and the driver. Only the first one's header
//! text drifts. The snapshot heads it `N°`; the `New PU elements` document
//! heads it `Number`. The plan records case drift on top of that.
//!
//! So the columns are found by position, not by header text. Position is the
//! part that has never moved, and it needs no synonym list to grow with each
//! era. Matching `N°` would already be wrong for the next document type.
//!
//! Position is also self-checking where it counts. A caller reads the car
//! number as an integer, so landing on the wrong column refuses loudly instead
//! of returning a plausible table. The team column is checked further
//! downstream: group cars by the driver-name column and teammates fall into
//! different groups, which the seat-keyed sweep raises as a conflict.
//!
//! The count is what this module verifies. Anything other than three columns
//! left of the first component means the table is not the shape every PU
//! document prints, and the roles cannot be assigned by position, so it refuses.

use crate::{error::IdentityError, labels::ColumnLabels};

/// How many identity columns a PU document prints: the car number, the team,
/// and the driver.
const IDENTITY_COLUMNS: usize = 3;

/// Where a PU document's table prints its identity columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityColumns {
    car: usize,
    team: usize,
    driver: usize,
}

impl IdentityColumns {
    /// The column holding the car number.
    #[must_use]
    pub fn car(&self) -> usize {
        self.car
    }

    /// The column holding the team, which the documents head `Car`.
    ///
    /// It is the source of a fact's `printed_team` witness, so its text is
    /// taken verbatim and never normalised.
    #[must_use]
    pub fn team(&self) -> usize {
        self.team
    }

    /// The column holding the driver's name.
    ///
    /// No fact keys on it: a fact records the car, and the sweep resolves the
    /// seat from the roster. It is named because the count of identity columns
    /// is only meaningful once every one of them has a role.
    #[must_use]
    pub fn driver(&self) -> usize {
        self.driver
    }
}

/// Locate the identity columns of a PU document's table.
///
/// `labels` must come from [`label_columns`](crate::label_columns) over the
/// same table, which is what makes the count trustworthy: it has already
/// refused any unlabelled column right of the first code, so the columns left
/// of the first component are exactly the identity ones.
///
/// # Errors
///
/// Returns [`IdentityError::UnexpectedIdentityColumns`] when the table prints
/// any number of them other than three.
pub fn identity_columns(labels: &ColumnLabels) -> Result<IdentityColumns, IdentityError> {
    // A `ColumnLabels` labels at least one column, since `read_legend` refuses
    // an empty legend and `label_columns` refuses a code that lands nowhere. So
    // the default stands for a table with no component column at all, which
    // reports zero identity columns and refuses, rather than for three.
    let found = labels.components().first().map_or(0, |(column, _)| *column);
    if found != IDENTITY_COLUMNS {
        return Err(IdentityError::UnexpectedIdentityColumns { found });
    }

    Ok(IdentityColumns {
        car: 0,
        team: 1,
        driver: 2,
    })
}

#[cfg(test)]
mod tests {
    use extract::{Column, Grid};

    use super::*;
    use crate::{label_columns, legend::read_legend};

    /// Build a grid from row-major cell text.
    fn grid_of(rows: &[Vec<String>]) -> Grid {
        let width = rows.first().map_or(0, Vec::len);
        let columns = (0..width)
            .map(|i| Column {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test geometry; the mapping reads cells, not coordinates"
                )]
                x0: i as f32 * 40.0,
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test geometry; the mapping reads cells, not coordinates"
                )]
                x1: i as f32 * 40.0 + 30.0,
            })
            .collect();
        Grid::new(columns, rows.to_vec())
    }

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|cell| (*cell).to_owned()).collect()
    }

    /// A two-component legend, enough to label a table.
    fn legend() -> crate::Legend {
        read_legend(&grid_of(&[
            row(&["ICE  Internal Combustion Engine"]),
            row(&["TC  Turbo Charger"]),
        ]))
        .expect("the legend must read")
    }

    /// A table whose identity columns carry `headers`.
    fn table_with(headers: &[&str]) -> Grid {
        let mut header: Vec<&str> = headers.to_vec();
        header.extend(["ICE", "TC"]);
        let mut data: Vec<&str> = vec!["7"; headers.len()];
        data.extend(["2", "3"]);
        grid_of(&[row(&header), row(&data)])
    }

    #[test]
    fn names_the_three_identity_columns_left_to_right() {
        let labels = label_columns(&table_with(&["N°", "Car", "Driver"]), &legend())
            .expect("the columns must label");

        let identity = identity_columns(&labels).expect("the identity columns must resolve");

        assert_eq!(
            (identity.car(), identity.team(), identity.driver()),
            (0, 1, 2)
        );
    }

    #[test]
    fn reads_the_same_columns_whatever_the_first_header_spells() {
        // The reason position won. The snapshot heads this column `N°` and the
        // `New PU elements` document heads it `Number`, so a parser keying on
        // the text needs a synonym per document type. Position needs none.
        let snapshot = label_columns(&table_with(&["N°", "Car", "Driver"]), &legend())
            .expect("the columns must label");
        let new_elements = label_columns(&table_with(&["Number", "Car", "Driver"]), &legend())
            .expect("the columns must label");

        assert_eq!(
            identity_columns(&snapshot).expect("the snapshot must resolve"),
            identity_columns(&new_elements).expect("the new elements doc must resolve")
        );
    }

    #[test]
    fn refuses_a_table_with_too_few_identity_columns() {
        // Two columns leave no way to say which is the car and which the team,
        // so the roles cannot be assigned and the table is refused.
        let labels =
            label_columns(&table_with(&["N°", "Car"]), &legend()).expect("the columns must label");

        assert_eq!(
            identity_columns(&labels),
            Err(IdentityError::UnexpectedIdentityColumns { found: 2 })
        );
    }

    #[test]
    fn refuses_a_table_with_too_many_identity_columns() {
        let labels = label_columns(&table_with(&["N°", "Car", "Driver", "Engine"]), &legend())
            .expect("the columns must label");

        assert_eq!(
            identity_columns(&labels),
            Err(IdentityError::UnexpectedIdentityColumns { found: 4 })
        );
    }
}
