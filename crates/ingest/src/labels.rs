//! Mapping a snapshot table's columns to component codes.
//!
//! The header of a snapshot table wraps. A long code splits down the page:
//! `MGU` prints above `-K`, and `PU-` above `CE` and `ANC`. Only the short
//! codes fit the middle line, so that line reads `ICE TC EXH ES` and its fourth
//! label sits over the column that really holds `MGU-K`. A parser reading the
//! labels along the line therefore shifts two components by one column and
//! produces a table that looks right and is not.
//!
//! Reading down each column instead of along each line fixes it, because the
//! grid already placed every header glyph in the column its x range covers.
//! Joining a column's header cells top to bottom spells that column's code:
//! `MGU` then `-K`. The geometry is the clustering's; this module only declines
//! to throw it away.
//!
//! The legend supplies the codes. Nothing here names a component, so a document
//! whose separator is not an ordinary hyphen still matches: its header and its
//! legend carry the same bytes.

use domain::ComponentCode;
use extract::Grid;

use crate::{error::LabelError, legend::Legend};

/// Which component each labelled column of a snapshot table holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnLabels {
    /// The labelled columns, ascending. Identity columns are absent.
    labelled: Vec<(usize, ComponentCode)>,
    header_rows: usize,
}

impl ColumnLabels {
    /// The code the column at `column` holds, or `None` where the header
    /// labels no component, as over the car number and driver columns.
    #[must_use]
    pub fn code_at(&self, column: usize) -> Option<&ComponentCode> {
        self.labelled
            .iter()
            .find(|(labelled, _)| *labelled == column)
            .map(|(_, code)| code)
    }

    /// The column holding `code`.
    #[must_use]
    pub fn column_of(&self, code: &ComponentCode) -> Option<usize> {
        self.labelled
            .iter()
            .find(|(_, labelled)| labelled == code)
            .map(|(column, _)| *column)
    }

    /// Every labelled column with its code, ascending by column.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &ComponentCode)> {
        self.labelled.iter().map(|(column, code)| (*column, code))
    }

    /// How many rows at the top of the grid the header occupies. The first data
    /// row follows it.
    #[must_use]
    pub fn header_rows(&self) -> usize {
        self.header_rows
    }
}

/// Map each component column of a snapshot table to its code.
///
/// `table` must cover the table band alone, header rows first, and `legend`
/// must come from the same document, so the codes compared are the same bytes.
///
/// The header depth is not given; it is found. Each candidate depth spells a
/// code per column, and the shallowest depth that gives every legend code
/// exactly one column wins. A depth short of the true header leaves a split
/// code half-spelled, `MGU` rather than `MGU-K`, and fails. A depth past it
/// pulls a count into the label, `ICE2` rather than `ICE`, and fails too. The
/// legend is what makes the search decidable: it says which codes must appear
/// and how many.
///
/// # Errors
///
/// Returns [`LabelError::HeaderDoesNotMatchLegend`] when no depth labels one
/// column per legend code.
pub fn label_columns(table: &Grid, legend: &Legend) -> Result<ColumnLabels, LabelError> {
    for header_rows in 1..=table.row_count() {
        if let Some(labelled) = labels_at_depth(table, legend, header_rows) {
            return Ok(ColumnLabels {
                labelled,
                header_rows,
            });
        }
    }

    Err(LabelError::HeaderDoesNotMatchLegend {
        unmatched: unmatched_codes(table, legend),
    })
}

/// Label the columns treating the top `header_rows` rows as the header.
///
/// Returns `None` unless every legend code lands on exactly one column, which
/// is the signal that the depth is wrong.
fn labels_at_depth(
    table: &Grid,
    legend: &Legend,
    header_rows: usize,
) -> Option<Vec<(usize, ComponentCode)>> {
    let mut column_of: Vec<Option<usize>> = vec![None; legend.entries().len()];
    let mut labelled = Vec::new();

    for column in 0..table.columns().len() {
        let Some(index) = legend.position(&header_text(table, column, header_rows)) else {
            continue;
        };
        if column_of[index].is_some() {
            // Two columns spell one code. The depth is wrong, or the table is.
            return None;
        }
        column_of[index] = Some(column);
        labelled.push((column, legend.entries()[index].code().clone()));
    }

    column_of.iter().all(Option::is_some).then_some(labelled)
}

/// Join a column's header cells top to bottom, which spells its code.
fn header_text(table: &Grid, column: usize, header_rows: usize) -> String {
    (0..header_rows)
        .map(|row| table.cell(row, column).trim())
        .collect()
}

/// The legend codes no column spells at any depth, for the error message.
fn unmatched_codes(table: &Grid, legend: &Legend) -> Vec<String> {
    legend
        .entries()
        .iter()
        .filter(|entry| {
            !(1..=table.row_count()).any(|header_rows| {
                (0..table.columns().len())
                    .any(|column| header_text(table, column, header_rows) == entry.code().as_str())
            })
        })
        .map(|entry| entry.code().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use extract::Column;

    use super::*;
    use crate::legend::read_legend;

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

    /// The 2026 legend, with `sep` between the halves of the two-part codes.
    fn legend_with(sep: char) -> Legend {
        let grid = grid_of(&[
            row(&["ICE  Internal Combustion Engine", "TC  Turbo Charger"]),
            row(&["EXH  EXhaust set", "MGU-K  Motor Generator Unit Kinetic"]),
            row(&[
                "ES  Energy Store unit",
                &format!("PU{sep}CE  Power Unit Control Electronics unit"),
            ]),
            row(&[&format!("PU{sep}ANC  Power Unit ANCillary component"), ""]),
        ]);
        read_legend(&grid).expect("the legend must read")
    }

    /// A 2026 table: three identity columns, seven component columns, and a
    /// header wrapped over three baselines exactly as the documents print it.
    fn wrapped_header_table(sep: char) -> Grid {
        let pu = format!("PU{sep}");
        grid_of(&[
            row(&["", "", "", "", "", "", "MGU", "", &pu, &pu]),
            row(&["N", "Car", "Driver", "ICE", "TC", "EXH", "", "ES", "", ""]),
            row(&["", "", "", "", "", "", "-K", "", "CE", "ANC"]),
            row(&[
                "7",
                "Falcon Racing",
                "Ana Ferreira",
                "2",
                "2",
                "2",
                "1",
                "2",
                "2",
                "3",
            ]),
        ])
    }

    #[test]
    fn labels_every_component_column_through_a_wrapped_header() {
        let legend = legend_with('-');
        let table = wrapped_header_table('-');

        let labels = label_columns(&table, &legend).expect("the columns must label");

        let codes: Vec<Option<&str>> = (0..10)
            .map(|column| labels.code_at(column).map(ComponentCode::as_str))
            .collect();
        assert_eq!(
            codes,
            [
                None,
                None,
                None,
                Some("ICE"),
                Some("TC"),
                Some("EXH"),
                Some("MGU-K"),
                Some("ES"),
                Some("PU-CE"),
                Some("PU-ANC"),
            ]
        );
    }

    #[test]
    fn puts_mgu_k_on_the_column_a_wrapped_header_would_give_to_es() {
        let legend = legend_with('-');
        let table = wrapped_header_table('-');

        let labels = label_columns(&table, &legend).expect("the columns must label");

        // Reading the middle line left to right, `ES` is the fourth code and
        // lands here. Reading down the column spells `MGU` then `-K`.
        assert_eq!(labels.code_at(6).map(ComponentCode::as_str), Some("MGU-K"));
    }

    #[test]
    fn labels_a_code_separated_by_u0002() {
        // The separator the 2026 documents really carry, which a hardcoded
        // "PU-CE" never matches.
        let legend = legend_with('\u{2}');
        let table = wrapped_header_table('\u{2}');

        let labels = label_columns(&table, &legend).expect("the columns must label");

        assert_eq!(
            labels.code_at(8).map(ComponentCode::as_str),
            Some("PU\u{2}CE")
        );
    }

    #[test]
    fn finds_the_column_of_a_code_separated_by_u0002() {
        let legend = legend_with('\u{2}');
        let table = wrapped_header_table('\u{2}');

        let labels = label_columns(&table, &legend).expect("the columns must label");

        assert_eq!(labels.column_of(&ComponentCode::new("PU\u{2}ANC")), Some(9));
    }

    #[test]
    fn reports_the_header_depth_it_found() {
        let legend = legend_with('-');
        let table = wrapped_header_table('-');

        let labels = label_columns(&table, &legend).expect("the columns must label");

        assert_eq!(labels.header_rows(), 3);
    }

    #[test]
    fn labels_a_header_that_does_not_wrap() {
        let grid = grid_of(&[row(&["A  first", "B  second"])]);
        let legend = read_legend(&grid).expect("the legend must read");
        let table = grid_of(&[row(&["Car", "A", "B"]), row(&["7", "1", "2"])]);

        let labels = label_columns(&table, &legend).expect("the columns must label");

        assert_eq!(labels.header_rows(), 1);
    }

    #[test]
    fn refuses_a_table_missing_a_legend_code() {
        let legend = legend_with('-');
        // The `PU-ANC` column is gone.
        let table = grid_of(&[
            row(&["", "", "", "", "", "", "MGU", "", "PU-"]),
            row(&["N", "Car", "Driver", "ICE", "TC", "EXH", "", "ES", ""]),
            row(&["", "", "", "", "", "", "-K", "", "CE"]),
        ]);

        assert_eq!(
            label_columns(&table, &legend),
            Err(LabelError::HeaderDoesNotMatchLegend {
                unmatched: vec!["PU-ANC".to_owned()]
            })
        );
    }

    #[test]
    fn refuses_a_table_that_spells_one_code_in_two_columns() {
        let grid = grid_of(&[row(&["A  first", "B  second"])]);
        let legend = read_legend(&grid).expect("the legend must read");
        let table = grid_of(&[row(&["A", "A", "B"]), row(&["1", "2", "3"])]);

        assert!(label_columns(&table, &legend).is_err());
    }

    #[test]
    fn refuses_an_empty_table() {
        let legend = legend_with('-');

        assert!(label_columns(&grid_of(&[]), &legend).is_err());
    }
}
