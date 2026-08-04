//! Error types for the document parsers.

use domain::{Car, ComponentCode};

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

/// A failure while locating a PU document's identity columns.
///
/// A refusal, never a guess. The three columns are found by position, so a
/// table of another shape leaves every role unassignable.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    /// The table prints some number of columns left of its first component
    /// column other than the three every PU document carries.
    #[error(
        "the table prints {found} columns left of its first component column, not the 3 a PU document carries"
    )]
    UnexpectedIdentityColumns {
        /// How many it prints.
        found: usize,
    },
}

/// A failure while parsing a PU snapshot document into count facts.
///
/// Every variant is a refusal. A snapshot is one of the three documents the
/// invariant sweep cross-checks, so a count it cannot read exactly is dropped
/// rather than guessed at: a row the sweep never sees raises nothing, while a
/// row it sees wrong raises a conflict somebody can act on.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SnapshotError {
    /// No page of the document yielded a table band.
    ///
    /// Carries every page's refusal, so a table broken in two on the page that
    /// should have carried it still reads out of the message rather than
    /// flattening into "no table".
    #[error("no page of the document carries a table: {}", refusals.iter().map(|(page, err)| format!("page {page}: {err}")).collect::<Vec<String>>().join("; "))]
    NoTablePage {
        /// Each page, with the reason its band split refused.
        refusals: Vec<(usize, BandError)>,
    },
    /// More than one page yielded a table band, so which one states the counts
    /// is ambiguous.
    #[error("pages [{}] each carry a table, so which one states the counts is ambiguous", pages.iter().map(usize::to_string).collect::<Vec<String>>().join(", "))]
    ManyTablePages {
        /// The pages that banded.
        pages: Vec<usize>,
    },
    /// No page states a document number, so the facts could not be tagged with
    /// the source reconciliation supersedes an original by.
    #[error("no page of the document states a document number")]
    NoDocumentNumber,
    /// Two pages state different document numbers.
    #[error("the document states more than one number: [{}]", numbers.iter().map(u32::to_string).collect::<Vec<String>>().join(", "))]
    ConflictingDocumentNumbers {
        /// The distinct numbers found, in the order the pages print them.
        numbers: Vec<u32>,
    },
    /// The legend or the column mapping refused.
    #[error(transparent)]
    Label(#[from] LabelError),
    /// The identity columns refused.
    #[error(transparent)]
    Identity(#[from] IdentityError),
    /// The table band holds a header and no data row, so the document would
    /// yield no fact at all while reading as whole.
    #[error("the table band is {header_rows} header rows and no data row")]
    NoDataRows {
        /// How deep the header runs.
        header_rows: usize,
    },
    /// A row printed something other than a number where the car number
    /// belongs.
    #[error("row {row} prints `{text}` where a car number belongs")]
    CarNotANumber {
        /// The row, counting from the top of the table band.
        row: usize,
        /// The cell text, as printed.
        text: String,
    },
    /// A row printed no team.
    ///
    /// A refusal rather than an absent witness. The sweep cross-checks a
    /// document's team grouping against the roster's, and a row with no team
    /// silently drops out of that check.
    #[error("row {row}, car {car}, prints no team")]
    MissingTeam {
        /// The row, counting from the top of the table band.
        row: usize,
        /// The car the row states.
        car: Car,
    },
    /// A row printed something other than a number under a component column.
    #[error("row {row}, car {car}, prints `{text}` under `{component}` where a count belongs")]
    CountNotANumber {
        /// The row, counting from the top of the table band.
        row: usize,
        /// The car the row states.
        car: Car,
        /// The component whose column it is.
        component: ComponentCode,
        /// The cell text, as printed.
        text: String,
    },
}
