//! The documents known to print a legend code their own table contradicts.
//!
//! One document of the 56 held locally refuses. The 2026 Monaco snapshot prints
//! `EX` in its legend code cell while its table header and the legend's own
//! description both say `EXH`: the line reads `EX  EXHaust set`, and every other
//! 2026 document prints `EXH  EXHaust`. The cell survives from the previous
//! season's template. [`label_columns`](crate::label_columns) refuses the
//! document rather than pick one of the two spellings, so the counts it states
//! never reach the sweep.
//!
//! Refusing loses no counts, because snapshots are cumulative and Barcelona
//! restates the totals five days later. It loses attribution: without Monaco, a
//! component change between the Canadian and Barcelona rounds cannot be pinned
//! to a weekend.
//!
//! # Why a correction names a season
//!
//! `EX` is not a misprint of `EXH`. It is the live 2025 code for the same
//! physical part: 2025 prints `EX  Engine EXhaust system` beside `MGU-H` and
//! `CE`, and 2026 prints `EXH  EXHaust set` beside `PU-CE` and `PU-ANC`. So a
//! global `EX` to `EXH` alias would merge two seasons' component sets, and a
//! round-only key would corrupt data outright: "at this round, rename `EX` to
//! `EXH`" fires on the 2025 document at the same round, whose legend is correct,
//! and rewrites it into a wrong one.
//!
//! The season is the whole key. Within 2026 the rename needs no round, because a
//! correction whose code the legend does not print stands down silently: every
//! other 2026 document prints `EXH` already and offers the entry nothing to
//! rename. That also covers a corrected reissue at the same round.
//!
//! # What a correction may do
//!
//! Rename legend codes. Never counts, never car numbers, never team names. The
//! emitted facts carry no mark that a correction ran, because the emitted `EXH`
//! is what the table header already prints, so they match a correctly printed
//! document's exactly.
//!
//! # Ruled out on evidence
//!
//! Recorded so nobody retries them:
//!
//! - A general rule of "one code prefixes the other, adopt the header spelling"
//!   breaks `label_columns`' header depth search, which rejects a too shallow
//!   depth precisely because `MGU` fails to match `MGU-K`.
//! - Deriving the code from the capitals in the description yields `EEX` from
//!   2025's `Engine EXhaust system`, not `EX`.
//!
//! The 2025 Chinese snapshot, which duplicates car 30, is left uncorrected on
//! purpose and sits outside what a rename can express. See
//! [`parse_snapshot`](crate::parse_snapshot) for why.

use domain::Season;

use crate::{error::LabelError, legend::Legend};

/// A legend code a document prints, and the code it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LegendRename {
    /// The stale code, exactly as the legend prints it.
    printed: &'static str,
    /// The code the rest of the document uses.
    meant: &'static str,
}

/// What one season's misprinted documents need before their columns can be
/// labelled.
///
/// Opaque, and obtainable only from [`for_season`]. The list of known bad
/// documents is the point of this module, so a correction nobody recorded here
/// cannot be conjured at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Correction {
    season: Season,
    renames: &'static [LegendRename],
}

impl Correction {
    /// Rewrite the stale codes this correction names.
    ///
    /// A rename whose printed code the legend does not carry stands down, and
    /// the legend passes through untouched.
    ///
    /// # Errors
    ///
    /// Returns [`LabelError::CorrectionWouldMergeCodes`] when the legend
    /// declares the stale code and the code it means side by side.
    pub(crate) fn apply(&self, legend: &mut Legend) -> Result<(), LabelError> {
        for rename in self.renames {
            legend.rename(rename.printed, rename.meant)?;
        }
        Ok(())
    }
}

/// Every document known to print a legend code its own table contradicts.
///
/// At most one entry per season; [`for_season`] takes the first that matches,
/// and `every_entry_names_a_distinct_season` pins the invariant.
static KNOWN: &[Correction] = &[
    // 2026. The Monaco snapshot's legend prints `EX` where its header and its
    // own description say `EXH`, left over from the 2025 template. No other 2026
    // document prints `EX`, so the rename stands down on all of them.
    Correction {
        season: 2026,
        renames: &[LegendRename {
            printed: "EX",
            meant: "EXH",
        }],
    },
];

/// The correction that applies to a season's documents, if any.
///
/// The caller resolves this once and passes the result to
/// [`parse_snapshot`](crate::parse_snapshot) for every document of that season,
/// so the parser needs no season, no document number, and no lookup of its own.
/// Passing it to a correctly printed document is safe: a rename whose code the
/// legend does not print stands down.
///
/// # Examples
///
/// ```
/// use ingest::corrections;
///
/// // 2026 carries the Monaco snapshot's stale legend code.
/// let correction = corrections::for_season(2026);
/// assert!(correction.is_some());
///
/// // 2025 prints that same code as a live one, so no correction reaches it.
/// assert!(corrections::for_season(2025).is_none());
/// ```
#[must_use]
pub fn for_season(season: Season) -> Option<&'static Correction> {
    KNOWN.iter().find(|correction| correction.season == season)
}

#[cfg(test)]
mod tests {
    //! The correction, driven through the seam it exists to unblock: a legend
    //! read from cells, corrected, then labelled against a header. No PDF.

    use domain::ComponentCode;
    use extract::{Column, Grid};

    use super::*;
    use crate::{ColumnLabels, LegendEntry, label_columns, legend::read_legend};

    /// The season the one recorded entry names.
    const MONACO_SEASON: Season = 2026;

    /// A grid of one cell per entry, laid out as the given rows of cells.
    fn grid_of(rows: &[&[&str]]) -> Grid {
        let width = rows.first().map_or(0, |row| row.len());
        let columns = (0..width)
            .map(|i| Column {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test geometry; the rules read cells, not coordinates"
                )]
                x0: i as f32 * 100.0,
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test geometry; the rules read cells, not coordinates"
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

    /// A 2026 legend spelling the exhaust code `exhaust`. Monaco prints `EX`
    /// there; every other 2026 document prints `EXH`.
    fn legend_of(exhaust: &str) -> Legend {
        read_legend(&grid_of(&[
            &["ICE  Internal Combustion Engine"],
            &[&format!("{exhaust}  EXHaust set")],
            &["ES  Energy Store unit"],
        ]))
        .expect("the legend must read")
    }

    /// The table the Monaco document really prints: a header spelling `EXH`
    /// over the column its legend calls `EX`.
    fn table() -> Grid {
        grid_of(&[&["Car", "ICE", "EXH", "ES"], &["7", "2", "3", "1"]])
    }

    fn monaco() -> &'static Correction {
        for_season(MONACO_SEASON).expect("2026 must carry a correction")
    }

    /// Every column's code, left to right, `None` where the header names no
    /// component.
    fn codes(labels: &ColumnLabels) -> Vec<Option<&str>> {
        (0..4)
            .map(|column| labels.code_at(column).map(ComponentCode::as_str))
            .collect()
    }

    #[test]
    fn labels_the_stale_column_by_the_code_its_header_spells() {
        // The seam, end to end: legend, correction, labelling. A correction that
        // landed on the legend while labelling ignored it would refuse here.
        let mut legend = legend_of("EX");

        monaco().apply(&mut legend).expect("the rename must apply");
        let labels = label_columns(&table(), &legend).expect("the columns must label");

        assert_eq!(codes(&labels), [None, Some("ICE"), Some("EXH"), Some("ES")]);
    }

    #[test]
    fn refuses_the_same_table_when_the_correction_does_not_run() {
        // What the correction buys, stated as the refusal it removes. Without
        // this the labelling test above would pass on a correction that did
        // nothing.
        let legend = legend_of("EX");

        assert_eq!(
            label_columns(&table(), &legend),
            Err(LabelError::HeaderDoesNotMatchLegend {
                unmatched: vec!["EX".to_owned()]
            })
        );
    }

    #[test]
    fn keeps_the_description_the_legend_prints_beside_the_stale_code() {
        // A rename touches the code alone. The description already says `EXH`,
        // and rewriting it would invent text the document never printed.
        let mut legend = legend_of("EX");

        monaco().apply(&mut legend).expect("the rename must apply");

        assert_eq!(
            legend.entry("EXH").map(LegendEntry::description),
            Some("EXHaust set")
        );
    }

    #[test]
    fn stands_down_on_a_legend_that_prints_the_code_it_means() {
        // Every 2026 document but Monaco, and a corrected Monaco reissue. The
        // correction resolves for the season, so it reaches all of them and must
        // leave each one exactly as read.
        let correct = legend_of("EXH");
        let mut corrected = correct.clone();

        monaco()
            .apply(&mut corrected)
            .expect("the rename must apply");

        assert_eq!(corrected, correct);
    }

    #[test]
    fn stands_down_on_a_legend_that_prints_neither_code() {
        let plain = read_legend(&grid_of(&[&["ICE  Internal Combustion Engine"]]))
            .expect("the legend must read");
        let mut corrected = plain.clone();

        monaco()
            .apply(&mut corrected)
            .expect("the rename must apply");

        assert_eq!(corrected, plain);
    }

    #[test]
    fn refuses_a_legend_that_declares_both_codes() {
        // A legend printing `EX` and `EXH` apart states two components, so the
        // rename would merge them and hand one column's counts to the other.
        // Standing down silently would hide a correction aimed at the wrong
        // document.
        let mut both = read_legend(&grid_of(&[
            &["EX   Engine EXhaust system"],
            &["EXH  EXHaust set"],
        ]))
        .expect("the legend must read");

        assert_eq!(
            monaco().apply(&mut both),
            Err(LabelError::CorrectionWouldMergeCodes {
                printed: "EX".to_owned(),
                meant: "EXH".to_owned()
            })
        );
    }

    #[test]
    fn renames_nothing_but_the_code() {
        // The whole of what a correction may express. Counts, car numbers, and
        // team names never reach it: `apply` takes a legend and nothing else.
        let mut legend = legend_of("EX");

        monaco().apply(&mut legend).expect("the rename must apply");

        let declared: Vec<&str> = legend
            .entries()
            .iter()
            .map(|entry| entry.code().as_str())
            .collect();
        assert_eq!(declared, ["ICE", "EXH", "ES"]);
    }

    #[test]
    fn resolves_the_correction_for_the_season_that_names_it() {
        assert!(for_season(MONACO_SEASON).is_some());
    }

    #[test]
    fn resolves_nothing_for_the_season_that_prints_the_stale_code_as_a_live_one() {
        // The case the season key exists for. 2025 prints `EX  Engine EXhaust
        // system` correctly, so a correction reaching it would rewrite a right
        // legend into a wrong one.
        assert_eq!(for_season(2025), None);
    }

    #[test]
    fn resolves_nothing_for_a_season_no_entry_names() {
        assert_eq!(for_season(2027), None);
    }

    #[test]
    fn every_entry_names_a_distinct_season() {
        // `for_season` takes the first match, so a second entry for one season
        // would never resolve.
        let mut seasons: Vec<Season> = KNOWN.iter().map(|entry| entry.season).collect();
        seasons.sort_unstable();
        let count = seasons.len();
        seasons.dedup();

        assert_eq!(seasons.len(), count, "two entries name one season");
    }

    #[test]
    fn every_rename_changes_the_code() {
        // A rename onto itself does nothing, and `Legend::rename` would refuse
        // it as a merge on every legend printing the code, so an entry carrying
        // one would break the season it was meant to repair.
        for entry in KNOWN {
            for rename in entry.renames {
                assert_ne!(
                    rename.printed, rename.meant,
                    "season {} renames a code to itself",
                    entry.season
                );
            }
        }
    }
}
