//! Parsing a `PU elements used per driver up to now` document into count facts.
//!
//! The snapshot is the running total per driver, published before each event.
//! It is one of the three documents the invariant sweep cross-checks, so its
//! parse must be exact: a count read wrong here contradicts the other two and
//! surfaces, but a row never read at all raises nothing.
//!
//! # A snapshot is a document, not a page
//!
//! Two pages, and neither one alone carries what a fact needs. The table prints
//! on the second page; the document number prints in the first page's header
//! block and nowhere else. Measured over the 56 snapshots held locally, 2024
//! through 2026, all 56 print `Document N` on the cover and none repeat it on
//! the table page. So [`parse_snapshot`] takes every page.
//!
//! The two are read from opposite ends. The table page is found by banding every
//! page, because which page carries the table is a property of the page. The
//! number is read from page 0 alone, because which page carries the header is a
//! property of the document, and every document puts it first.
//!
//! # Finding the table page
//!
//! Not by position. The parser feeds each page to the band split and keeps the
//! one that yields a table, which reads the page rather than trusting a page
//! order no document guarantees. Measured over the same 56 documents, the cover
//! refuses every time and the second page bands every time.
//!
//! A page whose band split refuses is not the table page, whatever the refusal.
//! A cover page refuses with `ShortTableBand { rows: 1 }`, because the time it
//! was published is two narrow runs and reads as one table row. A table cut to a
//! single row refuses identically, and no row count separates the two:
//! `ShortTableBand` can only ever carry `rows: 1`, since the band split's floor
//! is two rows and a block of rows is never empty.
//!
//! Skipping both is still safe. A one-row table is skipped, no other page
//! bands, and the document refuses with [`SnapshotError::NoTablePage`] carrying
//! every page's reason. A table cut to two rows or more never raises
//! `ShortTableBand` at all, so keying on the variant would not have caught it
//! either. What changes is the wording of the refusal, never whether one comes.
//!
//! Two pages that both band is refused rather than resolved by taking the
//! first. Picking one of two plausible tables is the failure this epic exists to
//! prevent, and no document in the archive produces the shape.

use domain::{Car, Claim, Fact, Round};
use extract::{ClusterConfig, Glyph, Grid, cluster};

use crate::{
    bands::{Bands, bands},
    error::SnapshotError,
    identity::identity_columns,
    labels::label_columns,
    legend::read_legend,
};

/// The label a document's header prints before its number.
///
/// Matched case-insensitively. What follows it is what makes a match mean
/// anything; see [`numbers_in`].
const DOCUMENT_LABEL: &str = "document";

/// The page whose header states the document number.
///
/// The header opens the document, so the number is read from the first page and
/// no other. All 56 snapshots held locally, 2024 through 2026, print it there.
const HEADER_PAGE: usize = 0;

/// Parse a snapshot document into one count fact per driver per component.
///
/// `pages` is every page of one document, in order, each the glyphs of that
/// page. `round` is the caller's: a snapshot names its event and never numbers
/// it, and mapping an event to a round belongs to the scraper. `config` is the
/// clustering the band split runs with.
///
/// Every fact carries [`Claim::SnapshotCount`], the running total the page
/// states, so a driver who fitted nothing this event still reports the count
/// carried forward. Every fact also carries the team string printed on its row,
/// verbatim, and the document number the header states, which is what lets
/// reconciliation supersede an original by the highest number.
///
/// Facts come out in printed order: each data row top to bottom, each component
/// column left to right within a row.
///
/// Rows are read as printed, not deduplicated. A document that prints one car
/// twice yields two facts, because the 2025 Chinese snapshot really does print
/// car 30 in two team blocks and was never corrected. Hiding that here would
/// take the contradiction away from the sweep, which is the only thing that can
/// report it.
///
/// # Errors
///
/// Returns [`SnapshotError`]. Either no page carries a table or several do, the
/// header states no number or more than one, the legend or the column mapping
/// refuses, the identity columns are not the three a PU document prints, the
/// table holds no data row, or a row prints something other than a number where
/// a car or a count belongs, or prints no team.
pub fn parse_snapshot(
    pages: &[Vec<Glyph>],
    round: Round,
    config: &ClusterConfig,
) -> Result<Vec<Fact>, SnapshotError> {
    let bands = table_page(pages, config)?;
    let document = document_number(pages, config)?;

    let legend = read_legend(bands.legend())?;
    let labels = label_columns(bands.table(), &legend)?;
    let identity = identity_columns(&labels)?;

    let table = bands.table();
    let header_rows = labels.header_rows();
    if header_rows >= table.row_count() {
        return Err(SnapshotError::NoDataRows { header_rows });
    }

    let mut facts = Vec::new();
    for row in header_rows..table.row_count() {
        let printed = table.cell(row, identity.car()).trim();
        let car: Car = printed.parse().map_err(|_| SnapshotError::CarNotANumber {
            row,
            text: printed.to_owned(),
        })?;

        let team = table.cell(row, identity.team()).trim();
        if team.is_empty() {
            return Err(SnapshotError::MissingTeam { row, car });
        }

        for (column, component) in labels.components() {
            let printed = table.cell(row, *column).trim();
            let count: u32 = printed
                .parse()
                .map_err(|_| SnapshotError::CountNotANumber {
                    row,
                    car,
                    component: component.clone(),
                    text: printed.to_owned(),
                })?;

            facts.push(
                Fact::new(
                    round,
                    car,
                    component.clone(),
                    Claim::SnapshotCount(count),
                    document,
                )
                .with_printed_team(team),
            );
        }
    }

    Ok(facts)
}

/// The one page of the document whose band split yields a table.
///
/// A page that refuses is not the table page, whatever it refuses with; see the
/// module doc for why no refusal is worth telling apart. No page banding and
/// several pages banding are both refusals.
fn table_page(pages: &[Vec<Glyph>], config: &ClusterConfig) -> Result<Bands, SnapshotError> {
    let mut banded: Vec<(usize, Bands)> = Vec::new();
    let mut refusals = Vec::new();
    for (page, glyphs) in pages.iter().enumerate() {
        match bands(glyphs, config) {
            Ok(bands) => banded.push((page, bands)),
            Err(refusal) => refusals.push((page, refusal)),
        }
    }

    if banded.len() > 1 {
        return Err(SnapshotError::ManyTablePages {
            pages: banded.into_iter().map(|(page, _)| page).collect(),
        });
    }
    banded
        .pop()
        .map(|(_, bands)| bands)
        .ok_or(SnapshotError::NoTablePage { refusals })
}

/// The document number the header states, read from [`HEADER_PAGE`] alone.
///
/// The narrow page is the point. [`numbers_in`] asks nothing of what precedes
/// the label, because the header welds it to the text on its left, so the rule
/// reduces to "a line holding `document` followed by digits". That is safe on a
/// header block and unsafe anywhere else: a footer or a sentence naming another
/// document matches it just as well, and the match would either overwrite a good
/// number or refuse a sound document. Reading one page keeps the loose rule
/// pointed at the only text it was measured on.
///
/// Keying on the page index rather than on the pages selection skipped matters
/// for the same reason it is safe. A document printing its header and its table
/// on one page still states its number on page 0, while a skipped-pages framing
/// would find nothing to read.
///
/// A line stating the label without a number after it is passed over rather than
/// refused, since prose may use the word. Two numbers in the header refuse: the
/// number is the key reconciliation supersedes an original by, so guessing which
/// one is meant would silently pick which document wins.
fn document_number(pages: &[Vec<Glyph>], config: &ClusterConfig) -> Result<u32, SnapshotError> {
    // Unreachable as `parse_snapshot` calls it: `table_page` runs first and
    // refuses an empty document with `NoTablePage`, so the page is always
    // there. Call the two the other way round and this would report a missing
    // number for a document that has no pages at all, which names the wrong
    // fault. `refuses_a_document_with_no_pages` pins the order.
    let header = pages
        .get(HEADER_PAGE)
        .ok_or(SnapshotError::NoDocumentNumber)?;
    let page = cluster(header, config);

    let mut found: Vec<u32> = Vec::new();
    for row in 0..page.row_count() {
        for number in numbers_in(&line_of(&page, row)) {
            if !found.contains(&number) {
                found.push(number);
            }
        }
    }

    match found.as_slice() {
        [] => Err(SnapshotError::NoDocumentNumber),
        [only] => Ok(*only),
        _ => Err(SnapshotError::AmbiguousDocumentNumber { numbers: found }),
    }
}

/// A grid row's cells joined left to right, which is the line the page prints.
///
/// Joining with a space keeps two cells apart wherever the clustering split
/// them, so a label in one column and its value in the next still read as two
/// tokens.
fn line_of(page: &Grid, row: usize) -> String {
    (0..page.columns().len())
        .map(|column| page.cell(row, column))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Every document number a line states, left to right.
///
/// What follows the label is the whole rule: optional whitespace, then digits.
/// A line that puts anything else there states no number and is passed over, so
/// `documented`, `documents 5`, and prose that merely uses the word all yield
/// nothing. Prose does not put a bare number straight after the word; a header
/// does.
///
/// Nothing is asked of what precedes the label, and that is measured, not
/// conceded. Over the 56 snapshots held locally the header welds the label to
/// the text on its left on 2 of them, printing `DelegateDocument 5`, because the
/// two sit in adjacent columns that the clustering joins into one cell. A left
/// boundary check would refuse those documents to guard against a word ending in
/// `document`, which no document prints.
///
/// The same welding happens on the right on 6 of the 56, printing `Document7`,
/// which is why the whitespace between label and number is optional.
///
/// Dropping the left boundary is what confines the search to the header page.
/// The rule reduces to "a line holding `document` followed by digits", which a
/// footer or a passing reference elsewhere in a document would also satisfy. See
/// [`document_number`].
fn numbers_in(line: &str) -> Vec<u32> {
    let lowered = line.to_lowercase();
    lowered
        .match_indices(DOCUMENT_LABEL)
        .filter_map(|(at, _)| {
            let digits: String = lowered[at + DOCUMENT_LABEL.len()..]
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! The parser, driven with hand-built glyphs. No PDF: the rules read
    //! geometry and cell text, and both are what a test can state outright. The
    //! committed two-page fixture is exercised in `tests/pu_snapshot.rs`.

    use domain::{ComponentCode, Team};

    use super::*;
    use crate::error::{BandError, IdentityError};

    /// Advance and width of a glyph, in points. Close to the 9-point Helvetica
    /// the documents print.
    const ADVANCE: f32 = 6.0;
    const WIDTH: f32 = 5.0;

    /// Lay a string out left to right from `x0` on baseline `y`, one glyph per
    /// character.
    fn word(text: &str, x0: f32, y: f32) -> Vec<Glyph> {
        text.chars()
            .enumerate()
            .map(|(index, ch)| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "test geometry; a fixture row is a handful of glyphs"
                )]
                let left = x0 + index as f32 * ADVANCE;
                Glyph {
                    ch,
                    x0: left,
                    x1: left + WIDTH,
                    y,
                }
            })
            .collect()
    }

    /// A cover page: a header block stating `document`, and one line that reads
    /// as a table row, exactly as the documents print one.
    fn cover(number: &str) -> Vec<Glyph> {
        let mut glyphs = word("From", 42.0, 661.9);
        glyphs.extend(word("The Synthetic Technical Delegate", 108.6, 661.9));
        glyphs.extend(word("Document", 388.3, 661.9));
        glyphs.extend(word(number, 450.9, 661.9));
        glyphs.extend(word("Time", 388.3, 621.8));
        glyphs.extend(word("08:58", 450.9, 621.8));
        glyphs
    }

    /// Left edge of each table column: the three identity ones, then one per
    /// component.
    const DRIVER_X: f32 = 200.0;
    const COUNT_X: [f32; 4] = [309.0, 338.0, 371.0, 403.0];

    /// One legend entry drawn as the documents draw it: the code padded out to
    /// its description, both in one run, so the pair binds into one cell.
    fn entry(code: &str, description: &str, x0: f32, y: f32) -> Vec<Glyph> {
        let padding = 9_usize.saturating_sub(code.chars().count());
        word(
            &format!("{code}{}{description}", " ".repeat(padding)),
            x0,
            y,
        )
    }

    /// A table page: prose, a legend, then the table.
    fn table_page_glyphs(rows: &[(&str, &str, [&str; 4])]) -> Vec<Glyph> {
        let mut glyphs = word(
            "The drivers entered in this synthetic championship have used",
            42.0,
            530.4,
        );
        glyphs.extend(entry("ICE", "Internal Combustion Engine", 42.0, 489.0));
        glyphs.extend(entry("TC", "Turbo Charger", 317.0, 489.0));
        glyphs.extend(entry("ES", "Energy Store unit", 42.0, 475.2));
        glyphs.extend(entry(
            "PU-CE",
            "Power Unit Control Electronics",
            317.0,
            475.2,
        ));

        glyphs.extend(word("N", 48.0, 421.9));
        glyphs.extend(word("Car", 74.0, 421.9));
        glyphs.extend(word("Driver", DRIVER_X, 421.9));
        glyphs.extend(word("ICE", 303.0, 421.9));
        glyphs.extend(word("TC", 333.0, 421.9));
        glyphs.extend(word("ES", 368.0, 421.9));
        glyphs.extend(word("PU-CE", 400.0, 421.9));

        for (index, (car, team, counts)) in rows.iter().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "test geometry; a few rows")]
            let y = 398.9 - 11.5 * index as f32;
            glyphs.extend(word(car, 48.0, y));
            glyphs.extend(word(team, 74.0, y));
            glyphs.extend(word("Ana Ferreira", DRIVER_X, y));
            for (x, count) in COUNT_X.into_iter().zip(counts) {
                glyphs.extend(word(count, x, y));
            }
        }
        glyphs
    }

    /// Two drivers of one team, with a count under each of the four components.
    fn two_rows() -> Vec<Glyph> {
        table_page_glyphs(&[
            ("7", "Falcon Racing", ["2", "2", "1", "3"]),
            ("8", "Falcon Racing", ["2", "3", "2", "4"]),
        ])
    }

    /// A table page whose header wraps, up to the last header row and no
    /// further.
    ///
    /// `MGU` prints above `-K`, so the line carrying the most codes reads
    /// `N Car Driver ICE ES` and puts `ES` over the column that really holds
    /// `MGU-K`. Reading down each column spells it correctly.
    ///
    /// Two tests share this page, and the pair is the point: one takes it as it
    /// stands, a band that is all header, and the other adds a data row. So the
    /// page a wrapped header makes and the same page with data are the same
    /// geometry by construction, and moving a baseline moves both.
    fn wrapped_header_glyphs() -> Vec<Glyph> {
        let mut glyphs = word(
            "The drivers entered in this synthetic championship have used",
            42.0,
            530.4,
        );
        glyphs.extend(entry("ICE", "Internal Combustion Engine", 42.0, 489.0));
        glyphs.extend(entry("MGU-K", "Motor Generator Unit Kinetic", 317.0, 489.0));
        glyphs.extend(entry("ES", "Energy Store unit", 42.0, 475.2));
        glyphs.extend(word("MGU", 330.0, 427.6));
        glyphs.extend(word("N", 48.0, 421.9));
        glyphs.extend(word("Car", 74.0, 421.9));
        glyphs.extend(word("Driver", DRIVER_X, 421.9));
        glyphs.extend(word("ICE", 303.0, 421.9));
        glyphs.extend(word("ES", 368.0, 421.9));
        glyphs.extend(word("-K", 333.0, 416.1));
        glyphs
    }

    /// The whole document: a cover, then the table page.
    fn document() -> Vec<Vec<Glyph>> {
        vec![cover("9"), two_rows()]
    }

    fn parsed(pages: &[Vec<Glyph>]) -> Vec<Fact> {
        parse_snapshot(pages, 9, &ClusterConfig::default()).expect("the snapshot must parse")
    }

    /// Each fact as `(car, component, count)`, which is what the table states.
    fn counts(facts: &[Fact]) -> Vec<(Car, &str, u32)> {
        facts
            .iter()
            .filter_map(|fact| match fact.claim {
                Claim::SnapshotCount(count) => Some((fact.car, fact.component.as_str(), count)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn reads_every_count_off_the_table() {
        let facts = parsed(&document());

        assert_eq!(
            counts(&facts),
            [
                (7, "ICE", 2),
                (7, "TC", 2),
                (7, "ES", 1),
                (7, "PU-CE", 3),
                (8, "ICE", 2),
                (8, "TC", 3),
                (8, "ES", 2),
                (8, "PU-CE", 4),
            ]
        );
    }

    #[test]
    fn every_fact_claims_a_snapshot_count() {
        // Only `SnapshotCount` and `ElementsFitted` feed the components-used
        // sum, and a snapshot states the running total before this event's
        // parts. Any other claim kind here would double count.
        let facts = parsed(&document());

        assert!(
            facts
                .iter()
                .all(|fact| matches!(fact.claim, Claim::SnapshotCount(_))),
            "a snapshot states running totals and nothing else"
        );
    }

    #[test]
    fn carries_forward_the_count_of_a_driver_who_fitted_nothing() {
        // The snapshot is a running total, so a driver who took no new part
        // still prints the count from before. There is no blank cell to skip
        // and no fact to omit.
        let pages = vec![
            cover("9"),
            table_page_glyphs(&[
                ("7", "Falcon Racing", ["2", "2", "1", "3"]),
                ("8", "Falcon Racing", ["2", "2", "1", "3"]),
            ]),
        ];

        let facts = parsed(&pages);

        // The values, not the count. Car 8 repeats car 7's row exactly, so a
        // parser that carried a count forward wrongly would still return eight
        // facts and only the numbers would give it away.
        assert_eq!(
            counts(&facts),
            [
                (7, "ICE", 2),
                (7, "TC", 2),
                (7, "ES", 1),
                (7, "PU-CE", 3),
                (8, "ICE", 2),
                (8, "TC", 2),
                (8, "ES", 1),
                (8, "PU-CE", 3),
            ]
        );
    }

    #[test]
    fn tags_every_fact_with_the_document_number_the_cover_states() {
        // The number prints on the page the parser skips, so a parser reading
        // the table page alone could not emit it.
        let facts = parsed(&document());

        assert!(facts.iter().all(|fact| fact.document == 9));
    }

    #[test]
    fn takes_the_document_number_from_a_header_that_pads_it() {
        let pages = vec![cover("14"), two_rows()];

        let facts = parsed(&pages);

        assert!(facts.iter().all(|fact| fact.document == 14));
    }

    #[test]
    fn records_the_team_each_row_prints() {
        let facts = parsed(&document());

        assert!(
            facts
                .iter()
                .all(|fact| fact.printed_team == Some(Team::from("Falcon Racing"))),
            "every row of this table prints one team"
        );
    }

    #[test]
    fn keeps_a_team_string_exactly_as_printed() {
        // The sweep compares which cars share a string, never the strings
        // themselves, so tidying one breaks the check. Folding this name's
        // accent or its case would leave two cars of one team under two
        // strings, and the grouping check would call teammates strangers.
        let pages = vec![
            cover("9"),
            table_page_glyphs(&[("7", "Fálcon Racing", ["2", "2", "1", "3"])]),
        ];

        let facts = parsed(&pages);

        assert_eq!(facts[0].printed_team, Some(Team::from("Fálcon Racing")));
    }

    #[test]
    fn emits_a_car_the_document_prints_twice_twice() {
        // The 2025 Chinese snapshot prints car 30 in two team blocks and was
        // never corrected. Deduplicating here would take the contradiction away
        // from the sweep, the only thing that can report it.
        let pages = vec![
            cover("9"),
            table_page_glyphs(&[
                ("7", "Falcon Racing", ["2", "2", "1", "3"]),
                ("7", "Comet GP", ["3", "3", "2", "4"]),
            ]),
        ];

        let facts = parsed(&pages);

        let teams: Vec<Option<&Team>> = facts
            .iter()
            .filter(|fact| fact.component == ComponentCode::new("ICE"))
            .map(|fact| fact.printed_team.as_ref())
            .collect();
        assert_eq!(
            teams,
            [
                Some(&Team::from("Falcon Racing")),
                Some(&Team::from("Comet GP")),
            ]
        );
    }

    #[test]
    fn labels_the_columns_from_the_legend_not_the_header_line() {
        // The wrapped header swap. `ES` prints on the line carrying the most
        // codes, one column left of where it belongs, so a parser reading along
        // that line would hand this column's count to `ES`.
        let mut glyphs = wrapped_header_glyphs();
        glyphs.extend(word("7", 48.0, 398.9));
        glyphs.extend(word("Falcon Racing", 74.0, 398.9));
        glyphs.extend(word("Ana Ferreira", DRIVER_X, 398.9));
        glyphs.extend(word("2", 309.0, 398.9));
        glyphs.extend(word("5", 338.0, 398.9));
        glyphs.extend(word("1", 371.0, 398.9));

        let facts = parsed(&[cover("9"), glyphs]);

        assert_eq!(
            counts(&facts),
            [(7, "ICE", 2), (7, "MGU-K", 5), (7, "ES", 1)],
            "the 5 belongs to MGU-K; a header-line reader gives it to ES"
        );
    }

    #[test]
    fn reads_the_table_page_and_skips_the_cover() {
        // The cover carries one line that prints like a table row. Band on it
        // and the parse would read a header and no data.
        let facts = parsed(&document());

        assert_eq!(counts(&facts).len(), 8);
    }

    #[test]
    fn finds_the_table_page_wherever_it_sits() {
        // Selection reads the pages rather than trusting a position no document
        // guarantees, so the table is found at index 2 here. The header is a
        // different matter: it opens the document, so its page is fixed.
        let facts = parsed(&[cover("9"), Vec::new(), two_rows()]);

        assert_eq!(counts(&facts).len(), 8);
    }

    #[test]
    fn refuses_a_document_whose_table_page_is_cut_to_one_row() {
        // The shape a cover page shares. Skipping it leaves no page banding, so
        // the document refuses rather than returning a table short of its
        // drivers. Both pages' reasons come back.
        let mut truncated = word("ICE", 42.0, 489.0);
        truncated.extend(word("Internal Combustion Engine", 92.0, 489.0));
        truncated.extend(word("1", 300.0, 60.0));

        let refusal = parse_snapshot(&[cover("9"), truncated], 9, &ClusterConfig::default());

        assert_eq!(
            refusal,
            Err(SnapshotError::NoTablePage {
                refusals: vec![
                    (0, BandError::ShortTableBand { rows: 1 }),
                    (1, BandError::ShortTableBand { rows: 1 }),
                ]
            })
        );
    }

    #[test]
    fn reports_a_split_table_in_the_refusal_it_returns() {
        // The reason every page's refusal is carried. A table broken in two is
        // a stronger signal than a cover page, and flattening it into "no
        // table" would throw that away.
        let mut split = two_rows();
        split.extend(word("Cars below this line of prose", 42.0, 340.0));
        split.extend(word("9", 48.0, 320.0));
        split.extend(word("Comet GP", 74.0, 320.0));
        split.extend(word("Rosa Iglesias", 160.0, 320.0));
        for x in [309.0, 338.0, 371.0, 403.0] {
            split.extend(word("3", x, 320.0));
        }

        let refusal = parse_snapshot(&[cover("9"), split], 9, &ClusterConfig::default());

        assert!(
            matches!(
                refusal,
                Err(SnapshotError::NoTablePage { ref refusals })
                    if refusals.contains(&(1, BandError::SplitTable { blocks: 2 }))
            ),
            "the split table must survive into the refusal: {refusal:?}"
        );
    }

    #[test]
    fn refuses_a_document_whose_pages_both_carry_a_table() {
        // Taking the first would pick one of two plausible tables with nothing
        // to tell them apart, which is the failure the epic exists to prevent.
        let refusal = parse_snapshot(&[two_rows(), two_rows()], 9, &ClusterConfig::default());

        assert_eq!(
            refusal,
            Err(SnapshotError::ManyTablePages { pages: vec![0, 1] })
        );
    }

    #[test]
    fn refuses_a_document_that_states_no_number() {
        let refusal = parse_snapshot(&[two_rows()], 9, &ClusterConfig::default());

        assert_eq!(refusal, Err(SnapshotError::NoDocumentNumber));
    }

    #[test]
    fn refuses_a_header_that_states_two_numbers() {
        // The number is the key reconciliation supersedes an original by, so
        // choosing between two would silently pick which document wins. Same
        // rule as `ManyTablePages`: refuse the ambiguity, never resolve it.
        let mut ambiguous = cover("12");
        ambiguous.extend(word("Document", 388.3, 600.0));
        ambiguous.extend(word("14", 450.9, 600.0));

        let refusal = parse_snapshot(&[ambiguous, two_rows()], 9, &ClusterConfig::default());

        assert_eq!(
            refusal,
            Err(SnapshotError::AmbiguousDocumentNumber {
                numbers: vec![12, 14]
            })
        );
    }

    #[test]
    fn ignores_a_number_stated_off_the_header_page() {
        // The reason the search is one page wide. `numbers_in` asks nothing of
        // what precedes the label, so this footer satisfies it exactly as a
        // header would. Searching every page would refuse this document as
        // ambiguous, or take the wrong number if the header stated none.
        let mut footer = two_rows();
        footer.extend(word("Supersedes Document 3", 42.0, 60.0));

        let facts = parsed(&[cover("9"), footer]);

        assert!(facts.iter().all(|fact| fact.document == 9));
    }

    #[test]
    fn passes_over_prose_that_merely_uses_the_word() {
        // `document` in a sentence states no number, so it must not refuse as a
        // conflict alongside the header's.
        let mut wordy = cover("9");
        wordy.extend(word(
            "this document supersedes the previous one",
            42.0,
            600.0,
        ));

        let facts = parsed(&[wordy, two_rows()]);

        assert!(facts.iter().all(|fact| fact.document == 9));
    }

    #[test]
    fn passes_over_a_word_that_merely_ends_in_the_label() {
        let mut wordy = cover("9");
        wordy.extend(word("documents 5 and 6 are superseded", 42.0, 600.0));

        let facts = parsed(&[wordy, two_rows()]);

        assert!(facts.iter().all(|fact| fact.document == 9));
    }

    #[test]
    fn takes_a_number_the_header_welds_to_its_label() {
        // 6 of the 56 snapshots held locally print `Document7`: the label and
        // the number sit near enough that the clustering joins them into one
        // cell, and no space glyph separates them.
        let mut welded = word("From", 42.0, 661.9);
        welded.extend(word("The Synthetic Technical Delegate", 108.6, 661.9));
        welded.extend(word("Document7", 388.3, 661.9));
        welded.extend(word("Time", 388.3, 621.8));
        welded.extend(word("08:58", 450.9, 621.8));

        let facts = parsed(&[welded, two_rows()]);

        assert!(facts.iter().all(|fact| fact.document == 7));
    }

    #[test]
    fn takes_a_label_the_header_welds_to_the_text_on_its_left() {
        // 2 of the 56 print `DelegateDocument 5`, the label joined to the cell
        // to its left. A left boundary check would refuse those documents.
        let mut welded = word("From", 42.0, 661.9);
        welded.extend(word("The Synthetic DelegateDocument 5", 108.6, 661.9));
        welded.extend(word("Time", 388.3, 621.8));
        welded.extend(word("08:58", 450.9, 621.8));

        let facts = parsed(&[welded, two_rows()]);

        assert!(facts.iter().all(|fact| fact.document == 5));
    }

    #[test]
    fn refuses_a_row_that_prints_no_car_number() {
        let pages = vec![
            cover("9"),
            table_page_glyphs(&[("TBC", "Falcon Racing", ["2", "2", "1", "3"])]),
        ];

        assert_eq!(
            parse_snapshot(&pages, 9, &ClusterConfig::default()),
            Err(SnapshotError::CarNotANumber {
                row: 1,
                text: "TBC".to_owned()
            })
        );
    }

    #[test]
    fn refuses_a_row_that_prints_no_team() {
        // Emitting `printed_team: None` instead would drop this row out of the
        // sweep's grouping check silently. Emitting an empty string would be
        // worse: every teamless row would group under one name the document
        // never printed, and the sweep would check a grouping that does not
        // exist.
        let pages = vec![
            cover("9"),
            table_page_glyphs(&[("7", "", ["2", "2", "1", "3"])]),
        ];

        assert_eq!(
            parse_snapshot(&pages, 9, &ClusterConfig::default()),
            Err(SnapshotError::MissingTeam { row: 1, car: 7 })
        );
    }

    #[test]
    fn refuses_a_table_band_that_is_all_header() {
        // The page above without its data row. Every legend code lands on a
        // column, so the mapping is happy and the depth runs to the whole band.
        // Returning an empty fact list would read as a document nobody fitted a
        // part at, which the sweep cannot tell from one it never saw.
        assert_eq!(
            parse_snapshot(
                &[cover("9"), wrapped_header_glyphs()],
                9,
                &ClusterConfig::default()
            ),
            Err(SnapshotError::NoDataRows { header_rows: 3 })
        );
    }

    #[test]
    fn refuses_a_row_whose_count_is_not_a_number() {
        let pages = vec![
            cover("9"),
            table_page_glyphs(&[("7", "Falcon Racing", ["2", "-", "1", "3"])]),
        ];

        assert_eq!(
            parse_snapshot(&pages, 9, &ClusterConfig::default()),
            Err(SnapshotError::CountNotANumber {
                row: 1,
                car: 7,
                component: ComponentCode::new("TC"),
                text: "-".to_owned()
            })
        );
    }

    #[test]
    fn refuses_a_table_whose_identity_columns_are_not_the_three() {
        // Without a driver column the roles cannot be assigned by position, so
        // the team witness would come from whichever column happened to sit
        // second.
        let mut glyphs = word(
            "The drivers entered in this synthetic championship have used",
            42.0,
            530.4,
        );
        glyphs.extend(entry("ICE", "Internal Combustion Engine", 42.0, 489.0));
        glyphs.extend(entry("TC", "Turbo Charger", 42.0, 475.2));
        glyphs.extend(word("N", 48.0, 421.9));
        glyphs.extend(word("Car", 74.0, 421.9));
        glyphs.extend(word("ICE", 303.0, 421.9));
        glyphs.extend(word("TC", 333.0, 421.9));
        glyphs.extend(word("7", 48.0, 398.9));
        glyphs.extend(word("Falcon Racing", 74.0, 398.9));
        glyphs.extend(word("2", 309.0, 398.9));
        glyphs.extend(word("3", 338.0, 398.9));

        assert_eq!(
            parse_snapshot(&[cover("9"), glyphs], 9, &ClusterConfig::default()),
            Err(SnapshotError::Identity(
                IdentityError::UnexpectedIdentityColumns { found: 2 }
            ))
        );
    }

    #[test]
    fn tags_every_fact_with_the_round_the_caller_passes() {
        // A snapshot names its event and never numbers it, so the round is the
        // caller's to supply.
        let facts = parse_snapshot(&document(), 4, &ClusterConfig::default()).expect("must parse");

        assert!(facts.iter().all(|fact| fact.round == 4));
    }

    #[test]
    fn leaves_every_fact_live() {
        // Reconciliation sets `superseded`, not the parser. A parser that
        // guessed would take a correction away from issue #31.
        let facts = parsed(&document());

        assert!(facts.iter().all(|fact| !fact.superseded));
    }

    #[test]
    fn refuses_a_document_with_no_pages() {
        assert_eq!(
            parse_snapshot(&[], 9, &ClusterConfig::default()),
            Err(SnapshotError::NoTablePage { refusals: vec![] })
        );
    }
}
