//! Error types for the document parsers.

/// A failure while slicing a page into bands.
///
/// Every variant is a refusal. A page the rules cannot read is reported and
/// dropped, because a band drawn in the wrong place yields a table that looks
/// whole and is not.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BandError {
    /// No row on the page prints like a row of a table.
    #[error("no row on the page prints like a table row")]
    NoTableBand,
    /// The page prints its table rows in more than one block, so any one band
    /// holds part of the table.
    #[error("the page prints table rows in {blocks} separate blocks, so no band holds them all")]
    SplitTable {
        /// How many blocks the table rows fall in.
        blocks: usize,
    },
    /// The one block of table rows is too short to carry a table.
    #[error("the table band holds too few rows for a header row and a data row: {rows}")]
    ShortTableBand {
        /// How many rows the block holds.
        rows: usize,
    },
    /// The table band starts at the top of the page, so nothing above it can
    /// name its columns.
    #[error("the table band starts at the top of the page, so no legend sits above it")]
    NoLegendBand,
}

/// A failure while reading a snapshot's legend or labelling its columns.
///
/// Every variant is a refusal, never a guess. A document that does not match
/// the shape the parser expects is reported and dropped, because a mapping that
/// silently labels one column wrong produces a plausible table that no later
/// check can distinguish from a correct one.
///
/// The last four variants report a failed header search. `label_columns` tries
/// each header depth in turn and reports the depth that came closest, so the
/// message names the likely fault rather than the first depth tried.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LabelError {
    /// The legend band held no entries at all.
    #[error("the legend block holds no entries")]
    EmptyLegend,
    /// A legend cell carried a code with nothing beside it.
    #[error("legend entry `{entry}` has a code but no description")]
    LegendEntryWithoutDescription {
        /// The cell text, as printed.
        entry: String,
    },
    /// One code appeared twice in the legend, so its column is ambiguous.
    #[error("the legend declares `{code}` twice")]
    DuplicateLegendCode {
        /// The repeated code, as printed.
        code: String,
    },
    /// No depth of header rows gave each legend code exactly one column.
    #[error("no header depth labels one column per legend code; unmatched: [{}]", unmatched.join(", "))]
    HeaderDoesNotMatchLegend {
        /// The codes that landed on no column, as printed.
        unmatched: Vec<String>,
    },
    /// Two columns spelled the same code, so neither one owns it.
    #[error("the header spells `{code}` in both column {first} and column {second}")]
    HeaderSpellsOneCodeTwice {
        /// The repeated code, as printed.
        code: String,
        /// The leftmost column spelling it.
        first: usize,
        /// The next column spelling it.
        second: usize,
    },
    /// A component column carried a header the legend never declared, so the
    /// legend is short of the table and that column has no code.
    #[error("header column {column} spells `{header}`, which the legend does not declare")]
    HeaderColumnNotInLegend {
        /// The column, counting from the left of the table.
        column: usize,
        /// Its header text, joined down the column.
        header: String,
    },
    /// A column carried no header text, so nothing names it and the header
    /// depth cannot be pinned.
    #[error("header column {column} carries no text")]
    HeaderColumnIsBlank {
        /// The column, counting from the left of the table.
        column: usize,
    },
}
