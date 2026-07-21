//! Layer 2: the committed synthetic fixture, parsed through pdf_oxide and the
//! clustering into the documented grid. See
//! `crates/pdf-fixtures/fixtures/README.md` for the expected content and the
//! table spec.

use extract::{ClusterConfig, GlyphSource, PdfOxideEngine, cluster};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../pdf-fixtures/fixtures/table_grid.pdf"
);

// The documented expected grid. Row 1 is the multi-baseline row: its number
// prints 0.24 points above its team and time.
const EXPECTED: [[&str; 3]; 4] = [
    ["No", "Team", "Time"],
    ["1", "Falcon Racing", "1:31.201"],
    ["2", "Comet GP", "1:31.888"],
    ["3", "Vertex Motors", "1:32.044"],
];

#[test]
fn committed_fixture_parses_into_documented_grid() {
    let bytes = std::fs::read(FIXTURE).expect("committed fixture must exist");
    let engine = PdfOxideEngine::from_bytes(bytes).expect("fixture must parse");
    let glyphs = engine.glyphs(0).expect("page 0 must extract");

    // The multi-baseline split is present in the raw glyphs: within the first
    // data row two baselines sit a fraction of a point apart.
    let mut row1_baselines: Vec<f32> = glyphs
        .iter()
        .filter(|g| g.y > 689.0 && g.y < 691.0)
        .map(|g| g.y)
        .collect();
    row1_baselines.sort_by(f32::total_cmp);
    row1_baselines.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert_eq!(
        row1_baselines.len(),
        2,
        "row 1 should carry two baselines, got {row1_baselines:?}",
    );
    let split = row1_baselines[1] - row1_baselines[0];
    assert!(
        (0.1..0.5).contains(&split),
        "the split should be about 0.24 points, got {split}",
    );

    // Geometry alone recovers the documented grid: no dropped rows, and no
    // number merged into a team.
    let grid = cluster(&glyphs, &ClusterConfig::default());

    assert_eq!(grid.row_count(), EXPECTED.len());
    assert_eq!(grid.columns().len(), EXPECTED[0].len());
    for (r, row) in EXPECTED.iter().enumerate() {
        for (c, expected) in row.iter().enumerate() {
            assert_eq!(grid.cell(r, c), *expected, "cell ({r}, {c})");
        }
    }
}
