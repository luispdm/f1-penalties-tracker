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
/// The rejoin is guarded by [`ColumnKind`], so one entry never swallows
/// another's code: the column to the right has to hold descriptions and nothing
/// else. Anything else refuses.
///
/// # Errors
///
/// Returns [`LabelError::EmptyLegend`] when no cell holds an entry,
/// [`LabelError::LegendEntryWithoutDescription`] when a bare code has no
/// description beside it, and [`LabelError::DuplicateLegendCode`] when one code
/// appears twice.
pub fn read_legend(grid: &Grid) -> Result<Legend, LabelError> {
    let kinds: Vec<ColumnKind> = (0..grid.columns().len())
        .map(|column| ColumnKind::of(grid, column))
        .collect();
    let mut entries: Vec<LegendEntry> = Vec::new();

    for row in 0..grid.row_count() {
        let mut column = 0;
        while column < grid.columns().len() {
            let cell = grid.cell(row, column).trim();
            // Where a rejoined description would come from.
            let neighbour = column + 1;
            column = neighbour;
            if cell.is_empty() {
                continue;
            }

            let (code, description) = match cell.split_once(char::is_whitespace) {
                Some((code, description)) => (code, description.trim()),
                // A bare code: the column boundaries split it from its
                // description, so take the column to its right. That column has
                // to hold descriptions alone, or this would swallow the code of
                // the entry beside it.
                None => {
                    if kinds.get(neighbour) != Some(&ColumnKind::Descriptions) {
                        return Err(LabelError::LegendEntryWithoutDescription {
                            entry: cell.to_owned(),
                        });
                    }
                    column = neighbour + 1;
                    (cell, grid.cell(row, neighbour).trim())
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

/// What a column of a legend band holds.
///
/// The column boundaries are the clustering's, and a cut is global to the grid:
/// a pair of columns splits for every entry it holds or for none. So the
/// question of what a cell is belongs to its column, and reading the column
/// answers it from every cell at once rather than from the one in hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnKind {
    /// Codes the boundaries cut away from their descriptions, or an empty
    /// column. Every cell is a lone token.
    Codes,
    /// Whole entries, each padding its own code out to a description.
    Entries,
    /// Descriptions cut from the codes in the column to the left.
    Descriptions,
}

impl ColumnKind {
    /// Read a column's kind off its non-empty cells.
    ///
    /// Padding is what tells an entry from a description. An entry pads its code
    /// out to a column, so somewhere in it sits a run of whitespace; a
    /// description is prose, whose words are one space apart. One cell holding a
    /// run makes the column [`Entries`](Self::Entries), because a column of
    /// descriptions has no reason to hold one.
    ///
    /// That threshold errs toward refusing. A description column carrying a
    /// stray double space reads as [`Entries`](Self::Entries), and a bare code
    /// beside it is refused rather than paired. The other direction would pair
    /// it with something wrong and say nothing.
    ///
    /// The rule is defeated only by a column whose every entry pads with a
    /// single space, which takes codes of eight characters or more throughout.
    /// The longest code the documents print is `PU-ANC`, at six.
    fn of(grid: &Grid, column: usize) -> Self {
        let mut all_lone_tokens = true;
        for row in 0..grid.row_count() {
            let cell = grid.cell(row, column).trim();
            if cell.is_empty() {
                continue;
            }
            if pads_a_code(cell) {
                return Self::Entries;
            }
            if cell.contains(char::is_whitespace) {
                all_lone_tokens = false;
            }
        }
        if all_lone_tokens {
            Self::Codes
        } else {
            Self::Descriptions
        }
    }
}

/// Whether a cell holds a run of whitespace, which is how a document pads a code
/// out to its description's column.
///
/// The cell must be trimmed, so that only a run between two words counts.
fn pads_a_code(cell: &str) -> bool {
    let mut after_whitespace = false;
    for ch in cell.chars() {
        let whitespace = ch.is_whitespace();
        if whitespace && after_whitespace {
            return true;
        }
        after_whitespace = whitespace;
    }
    false
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
    fn refuses_to_rejoin_a_code_beside_a_column_holding_an_entry_padded_by_one_space() {
        // A code long enough to reach its description's column prints one space,
        // so on its own it reads exactly like prose. Judged cell by cell, the
        // entry beside `ICE` looks like a description, `ICE` swallows it whole,
        // and `LONGCODE` never enters the legend.
        //
        // The column gives it away. Its other entries pad with runs, so the
        // column holds entries, and the rejoin is refused. Every code in the
        // column would have to be eight characters or more to fool this.
        let grid = grid_of(&[
            &["ICE", "LONGCODE Motor Generator Unit Kinetic"],
            &["ES", "TC       Turbo Charger"],
        ]);

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
