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
