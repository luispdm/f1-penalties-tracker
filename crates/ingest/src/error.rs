//! Error type for the document parsers.

/// A failure while reading a snapshot's legend or labelling its columns.
///
/// Every variant is a refusal, never a guess. A document that does not match
/// the shape the parser expects is reported and dropped, because a mapping that
/// silently labels one column wrong produces a plausible table that no later
/// check can distinguish from a correct one.
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
        /// The codes the header never spelled at any depth.
        unmatched: Vec<String>,
    },
}
