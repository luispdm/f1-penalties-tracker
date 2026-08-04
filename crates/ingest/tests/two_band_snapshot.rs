//! The two-band fixture, banded and parsed.
//!
//! The page prints a legend over a table and nothing else, so it isolates one
//! claim: a caller must cluster each band on its own, not merely select the two.
//! One legend entry spans every component column beneath it, so both bands in a
//! single pass yield two columns against the table's ten.
//!
//! See `crates/pdf-fixtures/fixtures/README.md`.

mod common;

use common::text_of;
use domain::ComponentCode;
use extract::{ClusterConfig, Glyph, GlyphSource, PdfOxideEngine, cluster};
use ingest::{Bands, bands, label_columns, read_legend};
use pdf_fixtures::SOFT_HYPHEN;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../pdf-fixtures/fixtures/two_band_snapshot.pdf"
);

// The left edge of each table column, in points, as the fixture prints them.
const COLUMN_X0: [f32; 10] = [
    47.9, 74.0, 211.0, 303.5, 333.5, 360.7, 393.7, 430.6, 459.2, 489.6,
];

fn glyphs() -> Vec<Glyph> {
    let bytes = std::fs::read(FIXTURE).expect("committed fixture must exist");
    let engine = PdfOxideEngine::from_bytes(bytes).expect("fixture must parse");
    engine.glyphs(0).expect("page 0 must extract")
}

fn banded() -> Bands {
    bands(&glyphs(), &ClusterConfig::default()).expect("the page must band")
}

/// The codes the fixture declares, in printed order, with the separator the
/// fixture can carry.
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

#[test]
fn clustering_the_two_bands_as_one_gives_two_columns() {
    // Why the bands are clustered apart rather than selected. `PU-CE`'s
    // description runs from x 317.7 to x 497 and the last column's ink opens at
    // 498, so single linkage walks it across all seven component boundaries.
    // The identity columns go the same way under the legend's left block.
    let grid = cluster(&glyphs(), &ClusterConfig::default());

    assert_eq!(grid.columns().len(), 2);
}

#[test]
fn the_table_band_clusters_into_its_ten_documented_columns() {
    let bands = banded();

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
fn the_legend_band_holds_the_four_legend_lines() {
    let bands = banded();

    assert_eq!(bands.legend().row_count(), 4);
}

#[test]
fn the_legend_band_clusters_into_its_two_printed_blocks() {
    // One column an entry, which is what lets a line carry two entries without
    // the columns blurring.
    let bands = banded();

    assert_eq!(bands.legend().columns().len(), 2);
}

#[test]
fn the_wide_legend_entry_never_reaches_the_table_band() {
    let bands = banded();

    assert!(
        !text_of(bands.table()).contains("Power Unit"),
        "the entry spanning the columns must stay in the legend band: {}",
        text_of(bands.table())
    );
}

#[test]
fn the_legend_band_yields_every_component_code_in_printed_order() {
    let legend = read_legend(banded().legend()).expect("the legend must read");

    let codes: Vec<String> = legend
        .entries()
        .iter()
        .map(|entry| entry.code().to_string())
        .collect();
    assert_eq!(codes, expected_codes());
}

#[test]
fn every_component_column_gets_its_code() {
    let bands = banded();
    let legend = read_legend(bands.legend()).expect("the legend must read");

    let labels = label_columns(bands.table(), &legend).expect("columns must label");

    let codes: Vec<Option<String>> = (0..10)
        .map(|column| labels.code_at(column).map(ComponentCode::to_string))
        .collect();
    let expected: Vec<Option<String>> = [None, None, None]
        .into_iter()
        .chain(expected_codes().into_iter().map(Some))
        .collect();
    assert_eq!(codes, expected);
}
