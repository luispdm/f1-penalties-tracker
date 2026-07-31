//! The committed snapshot fixture, parsed through pdf_oxide and the clustering
//! into a legend and a column mapping.
//!
//! The fixture is shaped like the 2026 `PU elements used per driver up to now`
//! documents, at their measured page coordinates, with invented drivers and
//! teams. See `crates/pdf-fixtures/fixtures/README.md`.

use domain::ComponentCode;
use extract::{ClusterConfig, Glyph, GlyphSource, Grid, PdfOxideEngine, cluster};
use ingest::{Legend, label_columns, read_legend};
use pdf_fixtures::SOFT_HYPHEN;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../pdf-fixtures/fixtures/pu_snapshot.pdf"
);

// The page bands. Slicing is the caller's job: the parsers take a grid over one
// band, never the page.
const LEGEND_BAND: std::ops::Range<f32> = 440.0..500.0;
const TABLE_BAND: std::ops::Range<f32> = 360.0..425.0;

// The fixture's table: car number, team, and driver, then one column per
// component.
const FIRST_COMPONENT_COLUMN: usize = 3;

fn glyphs() -> Vec<Glyph> {
    let bytes = std::fs::read(FIXTURE).expect("committed fixture must exist");
    let engine = PdfOxideEngine::from_bytes(bytes).expect("fixture must parse");
    engine.glyphs(0).expect("page 0 must extract")
}

fn band(glyphs: &[Glyph], band: std::ops::Range<f32>) -> Grid {
    let sliced: Vec<Glyph> = glyphs
        .iter()
        .copied()
        .filter(|glyph| band.contains(&glyph.y))
        .collect();
    cluster(&sliced, &ClusterConfig::default())
}

fn legend_of(glyphs: &[Glyph]) -> Legend {
    read_legend(&band(glyphs, LEGEND_BAND)).expect("the legend must read")
}

/// The codes the fixture declares, in printed order, with the separator the
/// fixture can actually carry.
fn expected_codes() -> Vec<String> {
    let sh = SOFT_HYPHEN;
    ["ICE", "TC", "EXH", "MGU-K", "ES", "PU{SH}CE", "PU{SH}ANC"]
        .iter()
        .map(|code| code.replace("{SH}", &sh.to_string()))
        .collect()
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

    let labels = label_columns(&band(&glyphs, TABLE_BAND), &legend).expect("columns must label");

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

    let labels = label_columns(&band(&glyphs, TABLE_BAND), &legend).expect("columns must label");

    assert_eq!(labels.header_rows(), 3);
}

#[test]
fn a_naive_header_reader_mislabels_the_column_this_mapping_gets_right() {
    let glyphs = glyphs();
    let legend = legend_of(&glyphs);
    let table = band(&glyphs, TABLE_BAND);
    let labels = label_columns(&table, &legend).expect("columns must label");

    // The naive parser: take the header line carrying the most codes and hand
    // the nth code to the nth component column. The fixture's middle header
    // line reads `ICE TC EXH ES`, because the long codes wrapped away from it.
    let naive: Vec<String> = (0..table.columns().len())
        .map(|column| table.cell(1, column).trim().to_owned())
        .filter(|text| legend.entry(text).is_some())
        .collect();
    assert_eq!(naive, ["ICE", "TC", "EXH", "ES"], "the wrapped header line");

    // Its fourth code lands on the fourth component column.
    let mislabelled = FIRST_COMPONENT_COLUMN + 3;
    assert_eq!(naive[3], "ES");
    assert_eq!(
        labels.code_at(mislabelled).map(ComponentCode::as_str),
        Some("MGU-K"),
        "reading down the column spells MGU then -K, so the naive label is wrong"
    );
}

#[test]
fn a_hardcoded_ascii_hyphen_finds_no_column() {
    let glyphs = glyphs();
    let legend = legend_of(&glyphs);

    let labels = label_columns(&band(&glyphs, TABLE_BAND), &legend).expect("columns must label");

    // The fixture separates the two-part codes with U+00AD, standing in for the
    // U+0002 the 2026 documents carry. Either way a literal misses.
    assert_eq!(labels.column_of(&ComponentCode::new("PU-CE")), None);
    assert!(
        labels
            .column_of(&ComponentCode::new(format!("PU{SOFT_HYPHEN}CE")))
            .is_some(),
        "the code read from the legend must find its column"
    );
}
