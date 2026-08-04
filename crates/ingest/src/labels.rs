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

    /// Every labelled column paired with its code, left to right.
    ///
    /// Identity columns are absent, so the first pair's column is also how many
    /// identity columns the table prints: [`label_columns`] refuses any
    /// unlabelled column right of the first code. That is what
    /// [`identity_columns`](crate::identity_columns) counts.
    #[must_use]
    pub fn components(&self) -> &[(usize, ComponentCode)] {
        &self.labelled
    }

    /// The column holding `code`.
    #[must_use]
    pub fn column_of(&self, code: &str) -> Option<usize> {
        self.labelled
            .iter()
            .find(|(_, labelled)| labelled.as_str() == code)
            .map(|(column, _)| *column)
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
/// code per column, and the shallowest depth that accounts for the whole table
/// wins. A depth short of the true header leaves a split code half-spelled,
/// `MGU` rather than `MGU-K`, and fails. A depth past it pulls a count into the
/// label, `ICE2` rather than `ICE`, and fails too. The legend is what makes the
/// search decidable: it says which codes must appear and how many.
///
/// A depth accounts for the whole table when three things hold:
///
/// - every legend code lands on exactly one column;
/// - every column from the leftmost labelled one rightwards carries a code, so
///   a legend shorter than the table refuses rather than leaving a component
///   column silently unlabelled;
/// - no column's header is empty, which is what keeps
///   [`ColumnLabels::header_rows`] honest. Where every code fits the top line
///   and `N`, `Car`, and `Driver` print below it, depth 1 spells every code and
///   leaves the identity columns blank; taking it would report a one-row header
///   and feed the caller its second row as data.
///
/// The third rule refuses a table with an unheaded column. That is the trade:
/// an unheaded column leaves the depth undecidable, and this module refuses
/// rather than guesses.
///
/// # Errors
///
/// Returns [`LabelError::HeaderDoesNotMatchLegend`],
/// [`LabelError::HeaderSpellsOneCodeTwice`],
/// [`LabelError::HeaderColumnNotInLegend`], or
/// [`LabelError::HeaderColumnIsBlank`], whichever the depth that came closest
/// reports.
pub fn label_columns(table: &Grid, legend: &Legend) -> Result<ColumnLabels, LabelError> {
    let mut closest: Option<DepthRejection> = None;

    for header_rows in 1..=table.row_count() {
        match labels_at_depth(table, legend, header_rows) {
            Ok(labelled) => {
                return Ok(ColumnLabels {
                    labelled,
                    header_rows,
                });
            }
            Err(rejection) => {
                if closest
                    .as_ref()
                    .is_none_or(|closest| rejection.distance() < closest.distance())
                {
                    closest = Some(rejection);
                }
            }
        }
    }

    // A table with no rows offers no depth to try, so no code found a column.
    Err(closest
        .unwrap_or_else(|| DepthRejection::MissingCodes {
            codes: legend
                .entries()
                .iter()
                .map(|entry| entry.code().to_string())
                .collect(),
        })
        .into())
}

/// Why a candidate depth was rejected.
enum DepthRejection {
    /// Legend codes that landed on no column.
    MissingCodes { codes: Vec<String> },
    /// One code spelled by two columns.
    RepeatedCode {
        code: String,
        first: usize,
        second: usize,
    },
    /// A column whose header the legend does not declare.
    UnknownColumn { column: usize, header: String },
    /// A column with no header text.
    BlankColumn { column: usize },
}

impl DepthRejection {
    /// How far the depth fell short, closest first.
    ///
    /// `label_columns` keeps the closest rejection, so the refusal names the
    /// likely fault instead of whatever the shallowest depth happened to hit.
    /// An unknown or blank column means every code landed exactly once, so the
    /// depth was otherwise right and its rejection says the most. A repeated
    /// code names the spelling at fault. Missing codes name only what never
    /// appeared, and the fewer of them, the closer the depth.
    fn distance(&self) -> (u8, usize) {
        match self {
            Self::UnknownColumn { .. } | Self::BlankColumn { .. } => (0, 0),
            Self::RepeatedCode { .. } => (1, 0),
            Self::MissingCodes { codes } => (2, codes.len()),
        }
    }
}

impl From<DepthRejection> for LabelError {
    fn from(rejection: DepthRejection) -> Self {
        match rejection {
            DepthRejection::MissingCodes { codes } => {
                Self::HeaderDoesNotMatchLegend { unmatched: codes }
            }
            DepthRejection::RepeatedCode {
                code,
                first,
                second,
            } => Self::HeaderSpellsOneCodeTwice {
                code,
                first,
                second,
            },
            DepthRejection::UnknownColumn { column, header } => {
                Self::HeaderColumnNotInLegend { column, header }
            }
            DepthRejection::BlankColumn { column } => Self::HeaderColumnIsBlank { column },
        }
    }
}

/// Label the columns treating the top `header_rows` rows as the header.
///
/// # Errors
///
/// Returns the first rule of `label_columns` the depth breaks, checked in the
/// order: every code on one column, then no blank header, then every column
/// from the first code rightwards carrying one. That runs the third rule before
/// the second, because a blank column right of the first code breaks both, and
/// [`DepthRejection::BlankColumn`] names it where
/// [`DepthRejection::UnknownColumn`] would report a column spelling nothing.
/// The blank rule also reaches the identity columns, which the coverage rule
/// never looks at.
fn labels_at_depth(
    table: &Grid,
    legend: &Legend,
    header_rows: usize,
) -> Result<Vec<(usize, ComponentCode)>, DepthRejection> {
    let columns = table.columns().len();
    let headers: Vec<String> = (0..columns)
        .map(|column| header_text(table, column, header_rows))
        .collect();

    let mut column_of: Vec<Option<usize>> = vec![None; legend.entries().len()];
    let mut labelled = Vec::new();
    for (column, header) in headers.iter().enumerate() {
        let Some(index) = legend.position(header) else {
            continue;
        };
        if let Some(first) = column_of[index] {
            return Err(DepthRejection::RepeatedCode {
                code: header.clone(),
                first,
                second: column,
            });
        }
        column_of[index] = Some(column);
        labelled.push((column, legend.entries()[index].code().clone()));
    }

    let missing: Vec<String> = legend
        .entries()
        .iter()
        .zip(&column_of)
        .filter(|(_, column)| column.is_none())
        .map(|(entry, _)| entry.code().to_string())
        .collect();
    if !missing.is_empty() {
        return Err(DepthRejection::MissingCodes { codes: missing });
    }

    // Every code landed. The depth holds only if it also accounts for every
    // column: an identity name left of the first code, a code from there on.
    if let Some(column) = headers.iter().position(String::is_empty) {
        return Err(DepthRejection::BlankColumn { column });
    }
    let mut carries_a_code = vec![false; columns];
    for column in column_of.iter().flatten() {
        carries_a_code[*column] = true;
    }
    let uncovered = carries_a_code
        .iter()
        .copied()
        .enumerate()
        .skip_while(|(_, carries)| !carries)
        .find(|(_, carries)| !carries)
        .map(|(column, _)| column);
    if let Some(column) = uncovered {
        return Err(DepthRejection::UnknownColumn {
            column,
            header: headers[column].clone(),
        });
    }

    Ok(labelled)
}

/// Join a column's header cells top to bottom, which spells its code.
fn header_text(table: &Grid, column: usize, header_rows: usize) -> String {
    (0..header_rows)
        .map(|row| table.cell(row, column).trim())
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

    /// The 2026 legend as printed, with `sep` between the halves of the
    /// two-part codes.
    fn legend_rows(sep: char) -> Vec<Vec<String>> {
        vec![
            row(&["ICE  Internal Combustion Engine", "TC  Turbo Charger"]),
            row(&["EXH  EXhaust set", "MGU-K  Motor Generator Unit Kinetic"]),
            row(&[
                "ES  Energy Store unit",
                &format!("PU{sep}CE  Power Unit Control Electronics unit"),
            ]),
            row(&[&format!("PU{sep}ANC  Power Unit ANCillary component"), ""]),
        ]
    }

    fn legend_with(sep: char) -> Legend {
        read_legend(&grid_of(&legend_rows(sep))).expect("the legend must read")
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

        assert_eq!(labels.column_of("PU\u{2}ANC"), Some(9));
    }

    #[test]
    fn reports_the_header_depth_it_found() {
        let legend = legend_with('-');
        let table = wrapped_header_table('-');

        let labels = label_columns(&table, &legend).expect("the columns must label");

        assert_eq!(labels.header_rows(), 3);
    }

    #[test]
    fn counts_the_header_row_that_labels_only_the_identity_columns() {
        let legend = legend_with('-');
        // Every code fits the top line, so `N`, `Car`, and `Driver` print below
        // it. Depth 1 spells all seven codes and would hand the caller the
        // identity line as the first data row.
        let table = grid_of(&[
            row(&[
                "", "", "", "ICE", "TC", "EXH", "MGU-K", "ES", "PU-CE", "PU-ANC",
            ]),
            row(&["N", "Car", "Driver", "", "", "", "", "", "", ""]),
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
        ]);

        let labels = label_columns(&table, &legend).expect("the columns must label");

        assert_eq!(labels.header_rows(), 2);
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

        assert_eq!(
            label_columns(&table, &legend),
            Err(LabelError::HeaderSpellsOneCodeTwice {
                code: "A".to_owned(),
                first: 0,
                second: 1
            })
        );
    }

    #[test]
    fn refuses_a_legend_shorter_than_the_table() {
        // A legend band whose lower edge sits too high clips the last line, and
        // the table still prints the column that line names. Labelling the
        // other six would leave a component column silently unlabelled.
        let rows = legend_rows('-');
        let clipped = read_legend(&grid_of(&rows[..rows.len() - 1])).expect("the legend must read");
        let table = wrapped_header_table('-');

        assert_eq!(
            label_columns(&table, &clipped),
            Err(LabelError::HeaderColumnNotInLegend {
                column: 9,
                header: "PU-ANC".to_owned()
            })
        );
    }

    #[test]
    fn refuses_a_table_with_an_unheaded_column() {
        // The price of pinning the header depth: a column the header never
        // names is refused rather than guessed at.
        let grid = grid_of(&[row(&["A  first", "B  second"])]);
        let legend = read_legend(&grid).expect("the legend must read");
        let table = grid_of(&[row(&["", "A", "B"]), row(&["7", "1", "2"])]);

        assert_eq!(
            label_columns(&table, &legend),
            Err(LabelError::HeaderColumnIsBlank { column: 0 })
        );
    }

    #[test]
    fn names_a_blank_column_right_of_the_first_code_blank_rather_than_unknown() {
        // A blank column right of the first code breaks the blank rule and the
        // coverage rule together, so the order `labels_at_depth` checks them in
        // decides what the caller reads. Checking coverage first would report
        // `HeaderColumnNotInLegend { column: 2, header: "" }`, a column that
        // spells nothing. The column left of the first code, which the coverage
        // scan never reaches, cannot pin this.
        let grid = grid_of(&[row(&["A  first", "B  second"])]);
        let legend = read_legend(&grid).expect("the legend must read");
        let table = grid_of(&[row(&["Car", "A", "", "B"]), row(&["7", "1", "3", "2"])]);

        assert_eq!(
            label_columns(&table, &legend),
            Err(LabelError::HeaderColumnIsBlank { column: 2 })
        );
    }

    #[test]
    fn refuses_an_empty_table() {
        let legend = legend_with('-');

        // No row means no depth to try, so the refusal comes from the fallback
        // rather than from a rejected depth: every legend code is unmatched.
        assert_eq!(
            label_columns(&grid_of(&[]), &legend),
            Err(LabelError::HeaderDoesNotMatchLegend {
                unmatched: vec![
                    "ICE".to_owned(),
                    "TC".to_owned(),
                    "EXH".to_owned(),
                    "MGU-K".to_owned(),
                    "ES".to_owned(),
                    "PU-CE".to_owned(),
                    "PU-ANC".to_owned(),
                ]
            })
        );
    }
}
