//! The legend block of a snapshot document.
//!
//! Every snapshot prints a legend pairing each component code with its
//! description. It is the only complete and reliable source of a season's
//! component set. The table header cannot serve: it wraps across baselines, so
//! reading it along a line mislabels columns. Nor can a `New PU elements`
//! document, which lists only the parts somebody fitted that weekend and so
//! omits any component nobody replaced.
//!
//! Codes are taken verbatim, never normalised. The 2026 documents separate
//! `PU-CE` and `PU-ANC` with U+0002 rather than an ordinary hyphen, and readers
//! disagree on whether to report that raw or quietly rewrite it. Storing what
//! the reader gave us keeps the legend and the header in the same alphabet, so
//! they match whatever the reader did, while a hardcoded `"PU-CE"` would match
//! only one of the two outcomes.

use domain::ComponentCode;
use extract::Grid;

use crate::error::LabelError;

/// A component code and the description printed beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegendEntry {
    code: ComponentCode,
    description: String,
}

impl LegendEntry {
    /// The code, exactly as the document printed it.
    #[must_use]
    pub fn code(&self) -> &ComponentCode {
        &self.code
    }

    /// The description printed beside the code.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// The component set a snapshot's legend declares, in printed order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Legend {
    entries: Vec<LegendEntry>,
}

impl Legend {
    /// The entries, in the order the document prints them.
    #[must_use]
    pub fn entries(&self) -> &[LegendEntry] {
        &self.entries
    }

    /// The entry whose code is exactly `code`.
    #[must_use]
    pub fn entry(&self, code: &str) -> Option<&LegendEntry> {
        self.position(code).map(|index| &self.entries[index])
    }

    /// The index of the entry whose code is exactly `code`.
    pub(crate) fn position(&self, code: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.code.as_str() == code)
    }
}

/// Read a legend block, pairing each code with its description.
///
/// `grid` must cover the legend band alone. The band matters: a legend
/// description runs across the page without a gap wide enough to split, so a
/// grid holding the legend and the table together collapses the component
/// columns into a handful. The caller slices the page.
///
/// Each non-empty cell is one entry, the code up to the first space and the
/// description after it. That follows the page: a document draws an entry as
/// one run of text, its code padded out to the description's column, so the
/// padding binds the pair into a single cell even where a row carries two
/// entries side by side.
///
/// A cell holding a bare code is rejoined with the cell to its right. The
/// clustering derives its columns from inked glyphs, so an entry's padding can
/// exceed the column gap and put the code in one column and the description in
/// the next. Whether it does turns on the longest code in the band: a code that
/// reaches past where the descriptions start bridges the two, and one that falls
/// short leaves them apart. The legend reads the same either way.
///
/// The rejoin is guarded, so one entry never swallows another's code. See
/// [`reads_as_a_description`].
///
/// # Errors
///
/// Returns [`LabelError::EmptyLegend`] when no cell holds an entry,
/// [`LabelError::LegendEntryWithoutDescription`] when a bare code has no
/// description beside it, and [`LabelError::DuplicateLegendCode`] when one code
/// appears twice.
pub fn read_legend(grid: &Grid) -> Result<Legend, LabelError> {
    let mut entries: Vec<LegendEntry> = Vec::new();

    for row in 0..grid.row_count() {
        let mut column = 0;
        while column < grid.columns().len() {
            let cell = grid.cell(row, column).trim();
            column += 1;
            if cell.is_empty() {
                continue;
            }

            let (code, description) = match cell.split_once(char::is_whitespace) {
                Some((code, description)) => (code, description.trim()),
                // A bare code: take the cell to its right as the description,
                // but only where that cell prints no code of its own.
                None => {
                    let neighbour = grid.cell(row, column).trim();
                    if !reads_as_a_description(neighbour) {
                        return Err(LabelError::LegendEntryWithoutDescription {
                            entry: cell.to_owned(),
                        });
                    }
                    column += 1;
                    (cell, neighbour)
                }
            };
            if description.is_empty() {
                return Err(LabelError::LegendEntryWithoutDescription {
                    entry: cell.to_owned(),
                });
            }
            if entries.iter().any(|entry| entry.code.as_str() == code) {
                return Err(LabelError::DuplicateLegendCode {
                    code: code.to_owned(),
                });
            }

            entries.push(LegendEntry {
                code: ComponentCode::new(code),
                description: description.to_owned(),
            });
        }
    }

    if entries.is_empty() {
        return Err(LabelError::EmptyLegend);
    }
    Ok(Legend { entries })
}

/// Whether a cell reads as a description on its own, rather than as an entry
/// carrying its own code.
///
/// The page separates the two. An entry pads its code out to the description's
/// column, so its first whitespace is a run of several. A description is prose,
/// so its first whitespace is one space. `Internal Combustion Engine` is a
/// description; `TC       Turbo Charger` is an entry.
///
/// A cell with no whitespace at all fails this, and so does an empty one. A bare
/// token could be either a one-word description or a code, and pairing a code
/// with the code beside it would invent an entry and lose one. Refusing says so.
fn reads_as_a_description(cell: &str) -> bool {
    let after_first_token = cell.trim_start_matches(|ch: char| !ch.is_whitespace());
    let mut whitespace = after_first_token
        .chars()
        .take_while(|ch| ch.is_whitespace());
    whitespace.next().is_some() && whitespace.next().is_none()
}

#[cfg(test)]
mod tests {
    use extract::Column;

    use super::*;

    /// A grid of one cell per entry, laid out as the given rows of cells.
    fn grid_of(rows: &[&[&str]]) -> Grid {
        let width = rows.first().map_or(0, |row| row.len());
        let columns = (0..width)
            .map(|i| Column {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test geometry; the mapping reads cells, not coordinates"
                )]
                x0: i as f32 * 100.0,
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test geometry; the mapping reads cells, not coordinates"
                )]
                x1: i as f32 * 100.0 + 80.0,
            })
            .collect();
        let cells = rows
            .iter()
            .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
            .collect();
        Grid::new(columns, cells)
    }

    #[test]
    fn pairs_each_code_with_its_description() {
        let grid = grid_of(&[
            &[
                "ICE      Internal Combustion Engine",
                "TC     Turbo Charger",
            ],
            &["ES       Energy Store unit", ""],
        ]);

        let legend = read_legend(&grid).expect("the legend must read");

        let codes: Vec<&str> = legend
            .entries()
            .iter()
            .map(|entry| entry.code().as_str())
            .collect();
        assert_eq!(codes, ["ICE", "TC", "ES"]);
    }

    #[test]
    fn keeps_the_description_beside_its_code() {
        let grid = grid_of(&[&["ICE      Internal Combustion Engine"]]);

        let legend = read_legend(&grid).expect("the legend must read");

        assert_eq!(
            legend.entry("ICE").map(LegendEntry::description),
            Some("Internal Combustion Engine")
        );
    }

    #[test]
    fn keeps_a_separator_that_is_not_a_hyphen_verbatim() {
        // U+0002, the separator the 2026 documents really carry.
        let grid = grid_of(&[&["PU\u{2}CE   Power Unit Control Electronics unit"]]);

        let legend = read_legend(&grid).expect("the legend must read");

        assert_eq!(
            legend.entries()[0].code().as_str(),
            "PU\u{2}CE",
            "the code must survive unrewritten, or it stops matching its own header"
        );
    }

    #[test]
    fn reads_rows_top_to_bottom_and_columns_left_to_right() {
        let grid = grid_of(&[&["A  first", "B  second"], &["C  third", "D  fourth"]]);

        let legend = read_legend(&grid).expect("the legend must read");

        let codes: Vec<&str> = legend
            .entries()
            .iter()
            .map(|entry| entry.code().as_str())
            .collect();
        assert_eq!(codes, ["A", "B", "C", "D"]);
    }

    #[test]
    fn rejects_an_empty_legend() {
        let grid = grid_of(&[&["", ""]]);

        assert_eq!(read_legend(&grid), Err(LabelError::EmptyLegend));
    }

    #[test]
    fn rejects_a_code_with_no_description() {
        let grid = grid_of(&[&["ICE"]]);

        assert_eq!(
            read_legend(&grid),
            Err(LabelError::LegendEntryWithoutDescription {
                entry: "ICE".to_owned()
            })
        );
    }

    #[test]
    fn rejoins_a_code_the_column_boundaries_split_from_its_description() {
        // Boundaries come from inked glyphs, so an entry's padding can exceed
        // the column gap and leave the code in one column and the description in
        // the next. The pair still reads as one entry.
        let grid = grid_of(&[
            &[
                "ICE",
                "Internal Combustion Engine",
                "TC       Turbo Charger",
            ],
            &["ES", "Energy Store unit", ""],
        ]);

        let legend = read_legend(&grid).expect("the legend must read");

        let entries: Vec<(&str, &str)> = legend
            .entries()
            .iter()
            .map(|entry| (entry.code().as_str(), entry.description()))
            .collect();
        assert_eq!(
            entries,
            [
                ("ICE", "Internal Combustion Engine"),
                ("TC", "Turbo Charger"),
                ("ES", "Energy Store unit"),
            ]
        );
    }

    #[test]
    fn refuses_to_rejoin_a_code_with_a_cell_that_carries_its_own() {
        // The guard. `TC       Turbo Charger` pads a code out to a description,
        // so it is an entry, not a description. Rejoining would give `ICE` the
        // whole of it and lose `TC`.
        let grid = grid_of(&[&["ICE", "TC       Turbo Charger"]]);

        assert_eq!(
            read_legend(&grid),
            Err(LabelError::LegendEntryWithoutDescription {
                entry: "ICE".to_owned()
            })
        );
    }

    #[test]
    fn refuses_to_rejoin_two_bare_codes() {
        // A lone token could be a one-word description or a code. Pairing them
        // would invent an entry and lose one.
        let grid = grid_of(&[&["ICE", "TC"]]);

        assert_eq!(
            read_legend(&grid),
            Err(LabelError::LegendEntryWithoutDescription {
                entry: "ICE".to_owned()
            })
        );
    }

    #[test]
    fn rejects_a_repeated_code() {
        let grid = grid_of(&[&["ICE  Internal Combustion Engine"], &["ICE  again"]]);

        assert_eq!(
            read_legend(&grid),
            Err(LabelError::DuplicateLegendCode {
                code: "ICE".to_owned()
            })
        );
    }
}
