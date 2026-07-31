//! Synthetic PDF fixtures for the extraction tests.
//!
//! The writer here is `printpdf`, an independent codebase from the `pdf_oxide`
//! reader the fixtures test. Writing and reading with one library could hide a
//! shared coordinate or font-metric bug, so the two stay apart. Nothing that
//! ships depends on this crate.
//!
//! Coordinates are page-space points: origin at the bottom-left, `y` increasing
//! upward. A cell's `baseline_pt` is where its text sits.

use printpdf::{
    BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Pt, TextItem,
    TextMatrix,
};

/// One string of text placed at an absolute baseline on the page.
#[derive(Debug, Clone)]
pub struct CellSpec {
    /// The text to draw.
    pub text: String,
    /// Left edge of the text, in points from the page's left edge.
    pub x_pt: f32,
    /// Baseline of the text, in points from the page's bottom edge.
    pub baseline_pt: f32,
}

impl CellSpec {
    /// Convenience constructor.
    #[must_use]
    pub fn new(text: &str, x_pt: f32, baseline_pt: f32) -> Self {
        Self {
            text: text.to_owned(),
            x_pt,
            baseline_pt,
        }
    }
}

/// A page of independently placed text cells to render as a PDF.
#[derive(Debug, Clone)]
pub struct TableSpec {
    /// Page width in millimetres.
    pub page_width_mm: f32,
    /// Page height in millimetres.
    pub page_height_mm: f32,
    /// Font size in points, shared by every cell.
    pub font_size_pt: f32,
    /// The cells, in any order.
    pub cells: Vec<CellSpec>,
}

/// Render a table spec to PDF bytes with the built-in Helvetica font.
///
/// Each cell is placed with an absolute text matrix, so its baseline lands
/// exactly at `baseline_pt`. That lets a spec put two columns of one logical row
/// on baselines a fraction of a point apart.
#[must_use]
pub fn render_table(spec: &TableSpec) -> Vec<u8> {
    let mut ops = vec![
        Op::StartTextSection,
        Op::SetFont {
            font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
            size: Pt(spec.font_size_pt),
        },
    ];
    for cell in &spec.cells {
        ops.push(Op::SetTextMatrix {
            matrix: TextMatrix::Translate(Pt(cell.x_pt), Pt(cell.baseline_pt)),
        });
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(cell.text.clone())],
        });
    }
    ops.push(Op::EndTextSection);

    let page = PdfPage::new(Mm(spec.page_width_mm), Mm(spec.page_height_mm), ops);
    let mut doc = PdfDocument::new("extract fixture");
    doc.with_pages(vec![page])
        .save(&PdfSaveOptions::default(), &mut Vec::new())
}

/// The soft hyphen that separates the two-part codes in [`pu_snapshot_spec`].
///
/// The 2026 documents separate `PU-CE` and `PU-ANC` with U+0002, not an
/// ordinary hyphen, so a parser matching a code against a hardcoded `"PU-CE"`
/// reads the wrong column. U+0002 cannot be written here: [`render_table`] draws
/// with the built-in Helvetica, whose WinAnsi encoding turns every unmappable
/// character into `?`. U+00AD is the closest character that survives the round
/// trip. It is invisible when rendered and it is not U+002D, so it breaks a
/// hardcoded literal exactly as U+0002 does. The mapping is asserted against the
/// real U+0002 in the `ingest` unit tests, which build a grid directly and need
/// no font.
pub const SOFT_HYPHEN: char = '\u{ad}';

/// The canonical multi-column, multi-baseline table committed as a fixture.
///
/// Four rows (a header and three data rows) across three columns. The first
/// data row splits across two baselines 0.24 points apart: the number sits on
/// one baseline, the team and time on another. A reader that assumes one
/// baseline per row would merge the number into the team or drop the row.
///
/// The expected [`Grid`](../../extract) is documented in `fixtures/README.md`
/// and asserted by the `extract` fixture test.
#[must_use]
pub fn table_grid_spec() -> TableSpec {
    // Column left edges.
    let (no_x, team_x, time_x) = (70.0, 130.0, 330.0);
    // Row baselines, top to bottom. Line spacing dwarfs the 0.24 split.
    let (header_y, row1_y, row2_y, row3_y) = (720.0, 690.0, 665.0, 640.0);
    // The split: the team and time of row 1 print 0.24 points below its number.
    let row1_split_y = row1_y - 0.24;

    TableSpec {
        page_width_mm: 210.0,
        page_height_mm: 297.0,
        font_size_pt: 11.0,
        cells: vec![
            CellSpec::new("No", no_x, header_y),
            CellSpec::new("Team", team_x, header_y),
            CellSpec::new("Time", time_x, header_y),
            CellSpec::new("1", no_x, row1_y),
            CellSpec::new("Falcon Racing", team_x, row1_split_y),
            CellSpec::new("1:31.201", time_x, row1_split_y),
            CellSpec::new("2", no_x, row2_y),
            CellSpec::new("Comet GP", team_x, row2_y),
            CellSpec::new("1:31.888", time_x, row2_y),
            CellSpec::new("3", no_x, row3_y),
            CellSpec::new("Vertex Motors", team_x, row3_y),
            CellSpec::new("1:32.044", time_x, row3_y),
        ],
    }
}

/// One legend line: the code, padded, then its description.
///
/// The real documents draw a legend entry as a single run of text, its code
/// padded out to the description's column. Drawing it that way here matters:
/// the padding spaces are glyphs, and they bind the code to its description in
/// one grid cell, which is what lets a legend row hold two entries side by side
/// without the columns blurring.
fn legend_line(code: &str, description: &str) -> String {
    const CODE_WIDTH: usize = 8;
    let padding = CODE_WIDTH.saturating_sub(code.chars().count()) + 1;
    format!("{code}{}{description}", " ".repeat(padding))
}

/// A snapshot page shaped like the 2026 `PU elements used per driver up to now`
/// documents, with invented drivers and teams.
///
/// It carries the two traps the column mapping has to survive, at the page
/// coordinates measured on the real documents.
///
/// **The header wraps over three baselines.** The long codes split: `MGU` sits
/// above `-K`, and `PU-` above `CE` and `ANC`. Only the short codes fit the
/// middle line, which therefore reads `ICE TC EXH ES` and puts `ES` fourth,
/// over the column that really holds `MGU-K`. Reading the labels along that line
/// mislabels two columns and yields a plausible, wrong table. Joining each
/// column's header cells down the page recovers the codes instead.
///
/// **The two-part codes use [`SOFT_HYPHEN`], not U+002D**, so matching a header
/// against a hardcoded `"PU-CE"` finds nothing.
///
/// The page also carries prose above the legend, as the real ones do. It spans
/// the full text width, so clustering the whole page collapses it to a single
/// column: a caller must slice the page into bands before the table's columns
/// appear at all.
#[must_use]
pub fn pu_snapshot_spec() -> TableSpec {
    let sh = SOFT_HYPHEN;
    let pu_ce = format!("PU{sh}CE");
    let pu_anc = format!("PU{sh}ANC");

    // Legend: two entries per line, left block and right block.
    let (legend_left_x, legend_right_x) = (42.5, 297.7);
    let legend_y = [489.0, 475.2, 461.4, 447.6];

    // Header: three baselines, 5.76 points apart. The gap clears the row
    // clustering threshold, so each is its own grid row.
    let (header_top_y, header_mid_y, header_bottom_y) = (421.9, 416.1, 410.4);

    // Column left edges, identity columns then one per component.
    let (no_x, team_x, driver_x) = (47.9, 74.0, 211.0);
    let (ice_x, tc_x, exh_x, mgu_k_x, es_x, pu_ce_x, pu_anc_x) =
        (309.2, 337.5, 368.3, 402.6, 434.5, 465.0, 497.7);

    let data_y = [398.9, 387.4, 375.9, 364.4];
    let rows: [(&str, &str, &str, [&str; 7]); 4] = [
        (
            "7",
            "Falcon Racing",
            "Ana Ferreira",
            ["2", "2", "2", "1", "2", "2", "3"],
        ),
        (
            "8",
            "Falcon Racing",
            "Piet Janssen",
            ["2", "2", "2", "2", "3", "3", "4"],
        ),
        (
            "9",
            "Comet GP",
            "Rosa Iglesias",
            ["3", "3", "3", "2", "3", "3", "5"],
        ),
        (
            "4",
            "Vertex Motors",
            "Kaito Mori",
            ["3", "3", "3", "1", "3", "3", "4"],
        ),
    ];

    let mut cells = vec![
        // Prose. The first line spans the text width and bridges every column.
        CellSpec::new("2027 SYNTHETIC GRAND PRIX", 173.5, 696.3),
        CellSpec::new(
            "The drivers entered in this synthetic championship have used the number of power unit elements listed below so far:",
            42.5,
            530.4,
        ),
        // Legend.
        CellSpec::new(
            &legend_line("ICE", "Internal Combustion Engine"),
            legend_left_x,
            legend_y[0],
        ),
        CellSpec::new(
            &legend_line("TC", "Turbo Charger"),
            legend_right_x,
            legend_y[0],
        ),
        CellSpec::new(
            &legend_line("EXH", "EXhaust set"),
            legend_left_x,
            legend_y[1],
        ),
        CellSpec::new(
            &legend_line("MGU-K", "Motor Generator Unit Kinetic"),
            legend_right_x,
            legend_y[1],
        ),
        CellSpec::new(
            &legend_line("ES", "Energy Store unit"),
            legend_left_x,
            legend_y[2],
        ),
        CellSpec::new(
            &legend_line(&pu_ce, "Power Unit Control Electronics unit"),
            legend_right_x,
            legend_y[2],
        ),
        CellSpec::new(
            &legend_line(&pu_anc, "Power Unit ANCillary component"),
            legend_left_x,
            legend_y[3],
        ),
        // Header, top line: the upper half of each split code.
        CellSpec::new("MGU", 393.7, header_top_y),
        CellSpec::new(&format!("PU{sh}"), 459.2, header_top_y),
        CellSpec::new(&format!("PU{sh}"), 491.8, header_top_y),
        // Header, middle line: the identity columns and every code short enough
        // to fit. Read along it, `ES` lands over the `MGU-K` column.
        CellSpec::new("N", no_x, header_mid_y),
        CellSpec::new("Car", team_x, header_mid_y),
        CellSpec::new("Driver", driver_x, header_mid_y),
        CellSpec::new("ICE", 303.5, header_mid_y),
        CellSpec::new("TC", 333.5, header_mid_y),
        CellSpec::new("EXH", 360.7, header_mid_y),
        CellSpec::new("ES", 430.6, header_mid_y),
        // Header, bottom line: the lower half of each split code.
        CellSpec::new("-K", 400.0, header_bottom_y),
        CellSpec::new("CE", 460.8, header_bottom_y),
        CellSpec::new("ANC", 489.6, header_bottom_y),
    ];

    let count_x = [ice_x, tc_x, exh_x, mgu_k_x, es_x, pu_ce_x, pu_anc_x];
    for (y, (no, team, driver, counts)) in data_y.into_iter().zip(rows) {
        cells.push(CellSpec::new(no, no_x, y));
        cells.push(CellSpec::new(team, team_x, y));
        cells.push(CellSpec::new(driver, driver_x, y));
        for (x, count) in count_x.into_iter().zip(counts) {
            cells.push(CellSpec::new(count, x, y));
        }
    }

    TableSpec {
        page_width_mm: 210.0,
        page_height_mm: 297.0,
        font_size_pt: 9.0,
        cells,
    }
}
