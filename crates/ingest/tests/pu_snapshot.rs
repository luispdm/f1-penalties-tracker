//! The committed snapshot fixture, parsed through pdf_oxide and the clustering
//! into a legend, a column mapping, and count facts.
//!
//! The fixture is shaped like the 2026 `PU elements used per driver up to now`
//! documents, at their measured page coordinates, with invented drivers and
//! teams. It is two pages, as the real ones are: a cover that states the
//! document number and a table page that states the counts. See
//! `crates/pdf-fixtures/fixtures/README.md`.

mod common;

use common::text_of;
use domain::{Car, Claim, ComponentCode, Fact, Team};
use extract::{ClusterConfig, Glyph, GlyphSource, Grid, PdfOxideEngine, cluster};
use ingest::{
    Bands, LabelError, Legend, SnapshotError, bands, label_columns, parse_snapshot, read_legend,
};
use pdf_fixtures::SOFT_HYPHEN;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../pdf-fixtures/fixtures/pu_snapshot.pdf"
);

// Where the fixture prints its table. The parser finds this by banding each
// page, never by index; the constant is here for the tests that measure one
// page's own geometry.
const TABLE_PAGE: usize = 1;

// The round the caller passes. A snapshot names its event and never numbers it.
const ROUND: u8 = 9;

// What the fixture's cover page states as its document number.
const DOCUMENT: u32 = 9;

// Where the fixture prints its table. `bands` derives this from the page, so no
// parser is given it. It stays for the tests that measure the fixture's own
// ink, which read the glyphs rather than the grid, and for the one that clips a
// band on purpose.
const TABLE_BAND: std::ops::Range<f32> = 360.0..425.0;

// The fixture's table: car number, team, and driver, then one column per
// component.
const FIRST_COMPONENT_COLUMN: usize = 3;

// The header is three rows deep, so the first data row is the fourth.
const FIRST_DATA_ROW: usize = 3;

// The left edge of each table column, in points, as the fixture prints it.
// Padding must not move them: the point of deriving boundaries from ink is that
// the spaces between the columns leave the columns where they are.
const COLUMN_X0: [f32; 10] = [
    47.9, 74.0, 211.0, 303.5, 333.5, 360.7, 393.7, 430.6, 459.2, 489.6,
];

/// Every page of the fixture, in order.
fn pages() -> Vec<Vec<Glyph>> {
    let bytes = std::fs::read(FIXTURE).expect("committed fixture must exist");
    let engine = PdfOxideEngine::from_bytes(bytes).expect("fixture must parse");
    (0..).map_while(|page| engine.glyphs(page).ok()).collect()
}

/// The table page's glyphs, for the tests that measure that page's geometry.
fn glyphs() -> Vec<Glyph> {
    pages()
        .into_iter()
        .nth(TABLE_PAGE)
        .expect("the fixture must carry a table page")
}

fn band_glyphs(glyphs: &[Glyph], band: std::ops::Range<f32>) -> Vec<Glyph> {
    glyphs
        .iter()
        .copied()
        .filter(|glyph| band.contains(&glyph.y))
        .collect()
}

fn band(glyphs: &[Glyph], band: std::ops::Range<f32>) -> Grid {
    cluster(&band_glyphs(glyphs, band), &ClusterConfig::default())
}

fn banded(glyphs: &[Glyph]) -> Bands {
    bands(glyphs, &ClusterConfig::default()).expect("the page must band")
}

/// The glyphs grouped by baseline, each group ordered by `x0`, which is the
/// order `assemble_cells` reads a cell in.
fn rows_by_baseline(glyphs: &[Glyph]) -> Vec<Vec<Glyph>> {
    let mut baselines: Vec<f32> = glyphs.iter().map(|glyph| glyph.y).collect();
    baselines.sort_by(f32::total_cmp);
    baselines.dedup_by(|a, b| (*a - *b).abs() < 0.01);

    baselines
        .into_iter()
        .map(|baseline| {
            let mut row: Vec<Glyph> = glyphs
                .iter()
                .copied()
                .filter(|glyph| (glyph.y - baseline).abs() < 0.01)
                .collect();
            row.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            row
        })
        .collect()
}

/// Each padding run in a row, as the x its first space opens at paired with the
/// right edge of the nearest ink to its left.
///
/// A run of two counts as padding. The only other spaces the fixture prints sit
/// inside a name, one at a time.
fn padding_runs(row: &[Glyph]) -> Vec<(f32, f32)> {
    let mut runs = Vec::new();
    let mut ink_ends_at = f32::NEG_INFINITY;
    let mut index = 0;
    while index < row.len() {
        if !row[index].ch.is_whitespace() {
            ink_ends_at = ink_ends_at.max(row[index].x1);
            index += 1;
            continue;
        }
        let opens = index;
        while index < row.len() && row[index].ch.is_whitespace() {
            index += 1;
        }
        if index - opens >= 2 {
            runs.push((row[opens].x0, ink_ends_at));
        }
    }
    runs
}

fn legend_of(glyphs: &[Glyph]) -> Legend {
    read_legend(banded(glyphs).legend()).expect("the legend must read")
}

/// The codes the fixture declares, in printed order, with the separator the
/// fixture can actually carry.
fn expected_codes() -> Vec<String> {
    vec![
        "ICE".to_owned(),
        "TC".to_owned(),
        "EXH".to_owned(),
        "MGU-K".to_owned(),
        "ES".to_owned(),
        format!("PU{SOFT_HYPHEN}CE"),
        format!("PU{SOFT_HYPHEN}ANC"),
    ]
}

/// The codes a naive reader takes off the header line carrying the most of
/// them, left to right.
fn naive_header_line(table: &Grid, legend: &Legend) -> Vec<String> {
    (0..table.columns().len())
        .map(|column| table.cell(1, column).trim().to_owned())
        .filter(|text| legend.entry(text).is_some())
        .collect()
}

/// The table the fixture prints, row by row: car, team, then a count per
/// component in printed column order.
///
/// Stated here rather than imported, so a change to the fixture's spec cannot
/// quietly move what the test expects of it.
const PRINTED_TABLE: [(Car, &str, [u32; 7]); 4] = [
    (7, "Falcon Racing", [2, 2, 2, 1, 2, 2, 3]),
    (8, "Falcon Racing", [2, 2, 2, 2, 3, 3, 4]),
    (9, "Comet GP", [3, 3, 3, 2, 3, 3, 5]),
    (4, "Vertex Motors", [3, 3, 3, 1, 3, 3, 4]),
];

fn parsed() -> Vec<Fact> {
    parse_snapshot(&pages(), ROUND, &ClusterConfig::default())
        .expect("the snapshot document must parse")
}

/// Every fact as `(car, component, count)`, which is what the table states.
fn counts(facts: &[Fact]) -> Vec<(Car, String, u32)> {
    facts
        .iter()
        .filter_map(|fact| match fact.claim {
            Claim::SnapshotCount(count) => Some((fact.car, fact.component.to_string(), count)),
            _ => None,
        })
        .collect()
}

#[test]
fn every_count_fact_matches_the_table_the_fixture_prints() {
    let facts = parsed();

    let expected: Vec<(Car, String, u32)> = PRINTED_TABLE
        .iter()
        .flat_map(|(car, _, row)| {
            expected_codes()
                .into_iter()
                .zip(*row)
                .map(move |(code, count)| (*car, code, count))
        })
        .collect();
    assert_eq!(counts(&facts), expected);
}

#[test]
fn the_parse_reads_the_table_page_and_skips_the_cover() {
    // The cover prints its publication time as two narrow runs, which reads as
    // one table row. Band on it and the parse would return a table of one row
    // and no counts at all.
    let refusal = bands(&pages()[0], &ClusterConfig::default());

    assert!(
        refusal.is_err(),
        "the cover must refuse, or page selection proves nothing: {refusal:?}"
    );
    assert_eq!(counts(&parsed()).len(), PRINTED_TABLE.len() * 7);
}

#[test]
fn a_short_table_page_is_refused_rather_than_skipped_as_a_cover() {
    // A table cut to one row refuses exactly as the cover does, so selection
    // skips both and no page bands. The document refuses rather than returning
    // a table short of its drivers, and every page's reason comes back.
    let cover = pages().swap_remove(0);
    let truncated: Vec<Glyph> = glyphs()
        .into_iter()
        .filter(|glyph| glyph.y > 470.0 || (395.0..402.0).contains(&glyph.y))
        .collect();

    let refusal = parse_snapshot(&[cover, truncated], ROUND, &ClusterConfig::default());

    assert!(
        matches!(refusal, Err(SnapshotError::NoTablePage { ref refusals }) if refusals.len() == 2),
        "expected both pages' refusals: {refusal:?}"
    );
}

#[test]
fn every_count_fact_carries_the_team_its_row_prints() {
    let facts = parsed();

    let teams: Vec<Option<Team>> = PRINTED_TABLE
        .iter()
        .flat_map(|(_, team, row)| row.iter().map(move |_| Some(Team::from(*team))))
        .collect();
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.printed_team.clone())
            .collect::<Vec<Option<Team>>>(),
        teams
    );
}

#[test]
fn every_count_fact_carries_the_document_number_the_cover_states() {
    // The number prints on the page the parse skips, so a parser reading the
    // table page alone could not emit it.
    let facts = parsed();

    assert!(facts.iter().all(|fact| fact.document == DOCUMENT));
}

#[test]
fn a_driver_who_fitted_nothing_still_reports_the_carried_forward_count() {
    // Car 4 and car 7 each print 1 for MGU-K, the count they carried in. The
    // snapshot is a running total, so there is no blank cell and no fact to
    // omit.
    let facts = parsed();

    let carried: Vec<(Car, u32)> = facts
        .iter()
        .filter_map(|fact| match fact.claim {
            Claim::SnapshotCount(count) if fact.component.as_str() == "MGU-K" => {
                Some((fact.car, count))
            }
            _ => None,
        })
        .collect();
    assert_eq!(carried, [(7, 1), (8, 2), (9, 2), (4, 1)]);
}

#[test]
fn the_wrapped_header_swap_does_not_corrupt_a_count() {
    // The trap the legend-derived mapping exists for. Reading labels along the
    // header line gives this column to `ES`, so car 7's 1 would be recorded as
    // one energy store rather than one MGU-K, and car 7's real ES count of 2
    // would shift right.
    let facts = parsed();

    let car_7: Vec<(String, u32)> = facts
        .iter()
        .filter(|fact| fact.car == 7)
        .filter_map(|fact| match fact.claim {
            Claim::SnapshotCount(count) => Some((fact.component.to_string(), count)),
            _ => None,
        })
        .collect();
    assert_eq!(
        car_7,
        expected_codes()
            .into_iter()
            .zip([2, 2, 2, 1, 2, 2, 3])
            .collect::<Vec<(String, u32)>>()
    );
}

#[test]
fn clustering_the_whole_page_gives_one_column() {
    // Why the bands exist. The prose and the legend descriptions run across the
    // page with no gap wide enough to split, so single-linkage clustering
    // bridges every column boundary. A caller that hands the page straight to
    // the parsers gets one column and no table at all.
    let grid = cluster(&glyphs(), &ClusterConfig::default());

    assert_eq!(grid.columns().len(), 1);
}

#[test]
fn the_prose_stays_out_of_the_legend_band() {
    // The rule that bounds the legend above. A blank line separates it from the
    // sentence overhead: 41.4 points against the 13.8 between its own lines.
    // Take the prose in and the band spans the text width, which collapses it
    // to one column and glues each line's second entry onto the first one's
    // description.
    let bands = banded(&glyphs());

    assert!(
        !text_of(bands.legend()).contains("drivers"),
        "the prose reached the legend band: {}",
        text_of(bands.legend())
    );
}

#[test]
fn the_legend_band_holds_the_four_legend_lines() {
    let bands = banded(&glyphs());

    assert_eq!(bands.legend().row_count(), 4);
}

#[test]
fn the_table_band_clusters_into_its_ten_documented_columns() {
    let bands = banded(&glyphs());

    let found: Vec<f32> = bands
        .table()
        .columns()
        .iter()
        .map(|column| column.x0)
        .collect();
    assert_eq!(found.len(), COLUMN_X0.len(), "columns: {found:?}");
    for (found, printed) in found.iter().zip(COLUMN_X0) {
        assert!(
            (found - printed).abs() < 0.01,
            "column {printed} moved to {found}",
        );
    }
}

#[test]
fn the_padded_table_band_carries_the_whitespace_trap() {
    // The fixture proves the rule only while its rows are padded. Cluster the
    // band's midpoints with the space glyphs left in and single linkage walks
    // the padding across every boundary, which is what the boundaries derived
    // from ink alone avoid. Drop the padding and the fixture passes either way.
    let mut midpoints: Vec<f32> = band_glyphs(&glyphs(), TABLE_BAND)
        .iter()
        .map(|glyph| glyph.mid_x())
        .collect();
    midpoints.sort_by(f32::total_cmp);

    let columns = 1 + midpoints
        .windows(2)
        .filter(|pair| pair[1] - pair[0] > ClusterConfig::default().column_gap)
        .count();

    assert_eq!(
        columns, 1,
        "the padding must bridge every column, or the fixture proves nothing"
    );
}

#[test]
fn every_padding_run_opens_clear_of_the_ink_to_its_left() {
    // The runs nearly fill their gaps, and they have to: a space advances
    // 2.502 pt against a 12 pt `column_gap`, so a shorter run breaks the chain
    // and weakens the trap. What that costs is margin. Lengthen a team or driver
    // name and its row's next run would open left of that name's last glyph.
    // `assemble_cells` orders a cell's glyphs by `x0`, so the padding would
    // interleave mid-name and the cell would come out split by spaces.
    let rows = rows_by_baseline(&band_glyphs(&glyphs(), TABLE_BAND));

    let mut checked = 0;
    for row in &rows {
        for (opens_at, ink_ends_at) in padding_runs(row) {
            assert!(
                opens_at > ink_ends_at,
                "a padding run opens at {opens_at} but the ink to its left runs to {ink_ends_at}",
            );
            checked += 1;
        }
    }

    // Nine gaps a row, over the four data rows. The header rows print no run.
    assert_eq!(checked, 36, "the fixture must still print its padding");
}

#[test]
fn multi_word_cells_keep_their_space_and_pick_up_no_padding() {
    let bands = banded(&glyphs());
    let table = bands.table();

    assert_eq!(table.cell(FIRST_DATA_ROW, 1), "Falcon Racing");
    assert_eq!(table.cell(FIRST_DATA_ROW, 2), "Ana Ferreira");
    assert_eq!(table.cell(FIRST_DATA_ROW + 3, 1), "Vertex Motors");
}

#[test]
fn the_legend_band_yields_every_component_code_in_printed_order() {
    let legend = legend_of(&glyphs());

    let codes: Vec<String> = legend
        .entries()
        .iter()
        .map(|entry| entry.code().to_string())
        .collect();
    assert_eq!(codes, expected_codes());
}

#[test]
fn the_legend_keeps_each_description_beside_its_code() {
    let legend = legend_of(&glyphs());

    assert_eq!(
        legend.entry("ICE").map(|entry| entry.description()),
        Some("Internal Combustion Engine")
    );
}

#[test]
fn every_component_column_gets_its_code() {
    let glyphs = glyphs();
    let legend = legend_of(&glyphs);

    let labels = label_columns(banded(&glyphs).table(), &legend).expect("columns must label");

    let codes: Vec<Option<String>> = (0..10)
        .map(|column| labels.code_at(column).map(ComponentCode::to_string))
        .collect();
    let expected: Vec<Option<String>> = [None, None, None]
        .into_iter()
        .chain(expected_codes().into_iter().map(Some))
        .collect();
    assert_eq!(codes, expected);
}

#[test]
fn the_header_is_three_rows_deep() {
    let glyphs = glyphs();
    let legend = legend_of(&glyphs);

    let labels = label_columns(banded(&glyphs).table(), &legend).expect("columns must label");

    assert_eq!(labels.header_rows(), 3);
}

#[test]
fn the_wrapped_header_line_carries_only_the_short_codes() {
    // The long codes wrapped away from the middle header line, so it reads
    // `ICE TC EXH ES` and its fourth code is `ES`.
    let glyphs = glyphs();
    let legend = legend_of(&glyphs);

    let naive = naive_header_line(banded(&glyphs).table(), &legend);

    assert_eq!(naive, ["ICE", "TC", "EXH", "ES"]);
}

#[test]
fn a_naive_header_reader_mislabels_the_column_this_mapping_gets_right() {
    // The naive parser hands the nth code of that line to the nth component
    // column, which gives this column `ES`.
    let glyphs = glyphs();
    let legend = legend_of(&glyphs);

    let labels = label_columns(banded(&glyphs).table(), &legend).expect("columns must label");

    assert_eq!(
        labels
            .code_at(FIRST_COMPONENT_COLUMN + 3)
            .map(ComponentCode::as_str),
        Some("MGU-K"),
        "reading down the column spells MGU then -K, so the naive label is wrong"
    );
}

#[test]
fn a_clipped_legend_band_refuses_rather_than_dropping_a_column() {
    // The band that slices the legend is the caller's, derived from the page.
    // Raising its lower edge to 448 clips the last legend line, so `PU-ANC`
    // never reaches the legend while the table still prints its column.
    // Labelling the remaining six and leaving the seventh unlabelled would hand
    // the caller a plausible, wrong table.
    //
    // The clip also splits the legend's codes from their descriptions. `PU-ANC`
    // is the longest code in the band and the only one reaching past where the
    // descriptions start, so cutting its line leaves nothing to bridge the two,
    // and boundaries drawn from ink put the code in one column and the
    // description in the next. `read_legend` rejoins them, which is why the
    // refusal below still comes from the labelling and not from the legend.
    let glyphs = glyphs();
    let clipped = read_legend(&band(&glyphs, 448.0..500.0)).expect("the legend must read");

    let refusal = label_columns(banded(&glyphs).table(), &clipped);

    assert_eq!(
        refusal,
        Err(LabelError::HeaderColumnNotInLegend {
            column: 9,
            header: format!("PU{SOFT_HYPHEN}ANC"),
        })
    );
}

#[test]
fn a_hardcoded_ascii_hyphen_finds_no_column() {
    let glyphs = glyphs();
    let legend = legend_of(&glyphs);

    let labels = label_columns(banded(&glyphs).table(), &legend).expect("columns must label");

    // The fixture separates the two-part codes with U+00AD, standing in for the
    // U+0002 the 2026 documents carry. Either way a literal misses.
    assert_eq!(labels.column_of("PU-CE"), None);
    assert!(
        labels.column_of(&format!("PU{SOFT_HYPHEN}CE")).is_some(),
        "the code read from the legend must find its column"
    );
}
