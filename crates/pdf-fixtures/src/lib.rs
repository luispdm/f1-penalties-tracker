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
    /// The text to draw, padding aside.
    pub text: String,
    /// Left edge of the text, in points from the page's left edge. The padding
    /// does not move it.
    pub x_pt: f32,
    /// Baseline of the text, in points from the page's bottom edge.
    pub baseline_pt: f32,
    /// Spaces drawn before the text, in the same run.
    pub padding_spaces: u16,
}

impl CellSpec {
    /// Convenience constructor.
    #[must_use]
    pub fn new(text: &str, x_pt: f32, baseline_pt: f32) -> Self {
        Self {
            text: text.to_owned(),
            x_pt,
            baseline_pt,
            padding_spaces: 0,
        }
    }

    /// A cell preceded by `spaces` spaces, its text still starting at `x_pt`.
    ///
    /// That is what lets a fixture carry the inter-cell space glyphs a real
    /// document prints while keeping every column at the position measured on
    /// that document. [`render_table`] does the placing, since it is what picks
    /// the font and the size the padding's advance depends on.
    #[must_use]
    pub fn padded(text: &str, x_pt: f32, baseline_pt: f32, spaces: u16) -> Self {
        Self {
            padding_spaces: spaces,
            ..Self::new(text, x_pt, baseline_pt)
        }
    }
}

/// The advance of a space in the built-in Helvetica, in ems.
///
/// [`render_table`] draws with that font, whose space is 278/1000 em wide. The
/// advance is exact, so a run of whole spaces can move an origin back and land
/// the text on the same point it started from.
const HELVETICA_SPACE_EM: f32 = 0.278;

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

/// Render a one-page table spec to PDF bytes with the built-in Helvetica font.
///
/// Each cell is placed with an absolute text matrix, so its baseline lands
/// exactly at `baseline_pt`. That lets a spec put two columns of one logical row
/// on baselines a fraction of a point apart.
///
/// A cell's padding is drawn ahead of its text and paid for by moving the
/// origin back by exactly the padding's advance, so
/// [`CellSpec::x_pt`] still lands the text where it asked. This is where that
/// happens, because this is what chooses Helvetica and `font_size_pt`.
#[must_use]
pub fn render_table(spec: &TableSpec) -> Vec<u8> {
    render_document(std::slice::from_ref(spec))
}

/// Render one spec per page to a single PDF, in the order given.
///
/// A snapshot is two pages, a cover and a table, and only the table page
/// carries the counts while only the cover states the document number. A parser
/// that takes a whole document therefore needs a fixture that is one, so a
/// fixture cannot be one page per file.
#[must_use]
pub fn render_document(pages: &[TableSpec]) -> Vec<u8> {
    let mut doc = PdfDocument::new("extract fixture");
    doc.with_pages(pages.iter().map(page_of).collect())
        .save(&PdfSaveOptions::default(), &mut Vec::new())
}

/// Draw one spec as a page.
fn page_of(spec: &TableSpec) -> PdfPage {
    let mut ops = vec![
        Op::StartTextSection,
        Op::SetFont {
            font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
            size: Pt(spec.font_size_pt),
        },
    ];
    for cell in &spec.cells {
        let padding = f32::from(cell.padding_spaces) * HELVETICA_SPACE_EM * spec.font_size_pt;
        ops.push(Op::SetTextMatrix {
            matrix: TextMatrix::Translate(Pt(cell.x_pt - padding), Pt(cell.baseline_pt)),
        });
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(format!(
                "{}{}",
                " ".repeat(usize::from(cell.padding_spaces)),
                cell.text
            ))],
        });
    }
    ops.push(Op::EndTextSection);

    PdfPage::new(Mm(spec.page_width_mm), Mm(spec.page_height_mm), ops)
}

/// The soft hyphen that separates the two-part codes in
/// [`pu_snapshot_table_spec`].
///
/// The 2026 documents separate `PU-CE` and `PU-ANC` with U+0002, not an
/// ordinary hyphen, so a parser matching a code against a hardcoded `"PU-CE"`
/// reads the wrong column. U+0002 cannot be written here: [`render_table`] draws
/// with the built-in Helvetica, whose WinAnsi encoding turns every unmappable
/// character into `?`. A writer that could carry it would not help either,
/// because pdf_oxide reads U+0002 back as U+FFFD however the file spells it.
/// U+00AD is the closest character that survives the round trip. It is
/// invisible when rendered and it is not U+002D, so it breaks a hardcoded
/// literal exactly as U+0002 does. The mapping is asserted against the real
/// U+0002 in the `ingest` unit tests, which build a grid directly and need no
/// font.
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

/// Font size of the snapshot pages, in points, as the documents print them.
const SNAPSHOT_FONT_SIZE_PT: f32 = 9.0;

/// Left edge of the legend's left hand block, in points.
const LEGEND_LEFT_X: f32 = 42.5;

/// Baselines of the legend's four lines, top to bottom.
///
/// They sit 13.8 points apart, and the line above them sits 27.6 away on the
/// real documents. That blank line is what bounds the legend band above, so the
/// spacing is part of what the fixtures test.
const LEGEND_Y: [f32; 4] = [489.0, 475.2, 461.4, 447.6];

/// The document number the snapshot's cover page states.
///
/// The real documents state it on the cover and nowhere else, so this is the
/// number a parser can only reach by reading the page it skips.
const COVER_DOCUMENT_NUMBER: u32 = 9;

/// A snapshot document: a cover page, then the page carrying the table.
///
/// The real documents are two pages, and which one carries the table is not
/// fixed by position but found: a parser feeds each page to the band split and
/// keeps the one that yields a table. This fixture is what proves it does,
/// because its cover page is a page the band split must refuse.
///
/// The split of labour between the two pages is the point. The counts print on
/// the table page and the document number prints on the cover, so neither page
/// alone carries what a snapshot fact needs.
#[must_use]
pub fn pu_snapshot_spec() -> Vec<TableSpec> {
    vec![pu_snapshot_cover_spec(), pu_snapshot_table_spec()]
}

/// The snapshot's cover page, at the coordinates measured on the real
/// documents.
///
/// Every string is invented except the document's own title and the header
/// labels a parser reads. It carries one trap.
///
/// **Exactly one line prints like a table row.** The band split calls a row a
/// table row when more than half its runs of ink are narrow, and on this page
/// only the time qualifies: `Time` and `08:58` are two narrow runs with nothing
/// wide beside them. Every other line pairs a narrow label with a wide value, or
/// runs wide on its own. So the page refuses with
/// `ShortTableBand { rows: 1 }`, which is what page selection has to survive.
///
/// Measured on the 56 snapshots held locally, 2024 through 2026, every cover
/// page refuses exactly this way, and every second page bands.
///
/// The margin is thin by construction, and a careless edit spends it. Shorten
/// the signature below 30 points and it becomes a second narrow run standing
/// alone, so the page prints its table rows in two blocks and refuses with
/// `SplitTable` instead. The fixture would still refuse, and would stop
/// carrying the shape it exists to carry.
#[must_use]
pub fn pu_snapshot_cover_spec() -> TableSpec {
    // The header block's two columns: the labels, then the values.
    const LABEL_X: f32 = 388.3;
    const VALUE_X: f32 = 450.9;

    TableSpec {
        page_width_mm: 210.0,
        page_height_mm: 297.0,
        font_size_pt: SNAPSHOT_FONT_SIZE_PT,
        cells: vec![
            CellSpec::new("2027 SYNTHETIC GRAND PRIX", 172.6, 718.4),
            CellSpec::new("03 - 05 July 2027", 246.7, 703.4),
            // The addressing block. `From` and `To` are narrow, but the name
            // beside each is wide, so neither line reads as a table row.
            CellSpec::new("From", 46.0, 661.9),
            CellSpec::new("The Synthetic Technical Delegate", 108.6, 661.9),
            CellSpec::new("Document", LABEL_X, 661.9),
            CellSpec::new(&COVER_DOCUMENT_NUMBER.to_string(), VALUE_X, 661.9),
            CellSpec::new("To", 46.0, 641.9),
            CellSpec::new("The Stewards", 108.6, 641.9),
            CellSpec::new("Date", LABEL_X, 641.9),
            CellSpec::new("03 July 2027", VALUE_X, 641.9),
            // The one line that prints like a table row: two narrow runs.
            CellSpec::new("Time", LABEL_X, 621.8),
            CellSpec::new("08:58", VALUE_X, 621.8),
            CellSpec::new("Title", 46.0, 584.3),
            CellSpec::new("Synthetic Delegate's Report", 118.6, 584.3),
            // One run: the label sits a single space from its value, as the
            // documents print it, so the two never separate.
            CellSpec::new(
                "Description PU elements used per driver up to now",
                46.0,
                561.8,
            ),
            CellSpec::new("Enclosed", 46.0, 539.3),
            CellSpec::new("09 SYN GP 27 TDR1.pdf", 118.6, 539.3),
            // The signature. Wide enough to read as prose; see the doc above.
            CellSpec::new("A. Delegate", 46.0, 487.7),
            CellSpec::new("The Synthetic Technical Delegate", 46.0, 457.2),
        ],
    }
}

/// The snapshot's table page, shaped like the 2026
/// `PU elements used per driver up to now` documents, with invented drivers and
/// teams.
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
/// **The data rows are padded.** A real snapshot draws a row as one run of text
/// whose cells are spaced apart, so a space glyph sits in every gap between
/// columns and a clustering that reads whitespace midpoints walks the padding
/// from one column to the next until ten columns come out as one. The padding
/// here is drawn by [`CellSpec::padded`], which leaves every column's left edge
/// on the point measured from the documents.
///
/// The page also carries prose above the legend, as the real ones do. It spans
/// the full text width, so clustering the whole page collapses it to a single
/// column: a caller must slice the page into bands before the table's columns
/// appear at all.
#[must_use]
pub fn pu_snapshot_table_spec() -> TableSpec {
    // The legend's right hand block, at the position measured on the documents.
    const LEGEND_RIGHT_X: f32 = 297.7;

    let mut cells = vec![
        // Prose. The first line spans the text width and bridges every column.
        CellSpec::new("2027 SYNTHETIC GRAND PRIX", 173.5, 696.3),
        CellSpec::new(
            "The drivers entered in this synthetic championship have used the number of power unit elements listed below so far:",
            LEGEND_LEFT_X,
            530.4,
        ),
    ];
    cells.extend(legend_cells(LEGEND_RIGHT_X));
    cells.extend(table_cells());

    TableSpec {
        page_width_mm: 210.0,
        page_height_mm: 297.0,
        font_size_pt: SNAPSHOT_FONT_SIZE_PT,
        cells,
    }
}

/// The same snapshot page carrying its legend band and its table band alone.
///
/// [`pu_snapshot_table_spec`] proves that a caller must *select* the bands,
/// since its prose spans the text width and collapses the page to one column. This
/// page proves the other half: that a caller must *cluster* each band on its
/// own. Select both bands together, cluster once, and the table still does not
/// appear.
///
/// The reason is one wide legend entry. `PU-CE`'s description runs from x 317.7
/// to x 497, past the left edge of every component column beneath it and within
/// a point of the last one's ink, so single-linkage clustering steps across all
/// seven boundaries. Both bands together therefore yield **2** columns against
/// the table's **10**, which is what the real 2026 documents measure, where the
/// same entry runs from x 347 to x 533.
///
/// The table is the one [`pu_snapshot_table_spec`] prints, padding and wrapped
/// header included, at the same coordinates.
#[must_use]
pub fn two_band_snapshot_spec() -> TableSpec {
    // The legend's right hand block, 20 points right of where
    // `pu_snapshot_table_spec` puts it. That is what carries its widest entry
    // across the last component column, and it sits nearer the real documents,
    // which print the block's description from x 347.
    const LEGEND_RIGHT_X: f32 = 317.7;

    let mut cells = legend_cells(LEGEND_RIGHT_X);
    cells.extend(table_cells());

    TableSpec {
        page_width_mm: 210.0,
        page_height_mm: 297.0,
        font_size_pt: SNAPSHOT_FONT_SIZE_PT,
        cells,
    }
}

/// The legend block: two entries a line, the right hand one starting at
/// `right_x`.
///
/// The 2026 component set, whose two-part codes carry [`SOFT_HYPHEN`].
fn legend_cells(right_x: f32) -> Vec<CellSpec> {
    let sh = SOFT_HYPHEN;
    let pu_ce = format!("PU{sh}CE");
    let pu_anc = format!("PU{sh}ANC");

    vec![
        CellSpec::new(
            &legend_line("ICE", "Internal Combustion Engine"),
            LEGEND_LEFT_X,
            LEGEND_Y[0],
        ),
        CellSpec::new(&legend_line("TC", "Turbo Charger"), right_x, LEGEND_Y[0]),
        CellSpec::new(
            &legend_line("EXH", "EXhaust set"),
            LEGEND_LEFT_X,
            LEGEND_Y[1],
        ),
        CellSpec::new(
            &legend_line("MGU-K", "Motor Generator Unit Kinetic"),
            right_x,
            LEGEND_Y[1],
        ),
        CellSpec::new(
            &legend_line("ES", "Energy Store unit"),
            LEGEND_LEFT_X,
            LEGEND_Y[2],
        ),
        CellSpec::new(
            &legend_line(&pu_ce, "Power Unit Control Electronics unit"),
            right_x,
            LEGEND_Y[2],
        ),
        CellSpec::new(
            &legend_line(&pu_anc, "Power Unit ANCillary component"),
            LEGEND_LEFT_X,
            LEGEND_Y[3],
        ),
    ]
}

/// The table block: the wrapped header over four padded data rows.
fn table_cells() -> Vec<CellSpec> {
    // Padding before each data cell but the row's first, in spaces. Each count
    // is the longest run that still opens clear of the column to its left,
    // measured from the rendered fixture: the runs start between 0.3 and 2.4
    // points past that column's ink, far inside the 12-point column gap, so
    // whitespace midpoints chain every boundary. Inked midpoints do not move.
    const TEAM_PAD: u16 = 7;
    const DRIVER_PAD: u16 = 31;
    const COUNT_PAD: [u16; 7] = [17, 7, 9, 9, 7, 8, 8];

    let sh = SOFT_HYPHEN;

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
        cells.push(CellSpec::padded(team, team_x, y, TEAM_PAD));
        cells.push(CellSpec::padded(driver, driver_x, y, DRIVER_PAD));
        for ((x, pad), count) in count_x.into_iter().zip(COUNT_PAD).zip(counts) {
            cells.push(CellSpec::padded(count, x, y, pad));
        }
    }
    cells
}
