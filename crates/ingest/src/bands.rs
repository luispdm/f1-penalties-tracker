//! Slicing a snapshot page into the bands its parsers read.
//!
//! [`read_legend`](crate::read_legend) and
//! [`label_columns`](crate::label_columns) each take a grid over one region of
//! the page, and nothing stops a caller handing both the same one. Clustering
//! the page whole is worse than useless: the prose spans the text width, so
//! every column boundary under it disappears and a ten-column table comes out
//! as one. Selecting the two regions is not enough either. A legend
//! description runs across the columns beneath it without an internal gap, so
//! the two bands clustered together still yield two columns against the table's
//! ten. Each band has to be clustered on its own, and that is what this module
//! does.
//!
//! The work runs on `&[Glyph]`, never on a [`Grid`]. A grid exposes its columns
//! as x ranges and carries no y at all, so the y a band is chosen by is gone by
//! the time you hold one. Grouping the glyphs into rows here therefore repeats
//! the baseline pass inside `extract`. That duplication is deliberate: deciding
//! which band is the legend is F1 knowledge, and `extract` carries none. The
//! alternative is widening the extraction API until it names document regions.
//!
//! # The rules
//!
//! **Density finds the table.** Split each row's ink into runs, a run being a
//! maximal group no more than `column_gap` apart, and a table row is the row
//! whose runs are mostly narrow. Measured on the 2022, 2024, 2025, and 2026
//! snapshots, every row of a table, wrapped header lines included, holds runs of
//! a median 13 to 17 points, and every row above one holds runs of a median 43
//! or more. Prose is a few wide runs; a table row is many narrow ones. The rule
//! reads geometry alone, so it survives the drift a fixed y would not: the
//! legend's length follows the component set, which gained an exhaust code in
//! 2021 and dropped MGU-H for PU-ANC in 2026, and every such change moves the
//! table up or down the page.
//!
//! **A blank line bounds the legend.** Density cannot find the legend's top
//! edge, because a legend line and a line of prose measure the same: both are a
//! few wide runs. The page says it another way. The legend is a paragraph, set
//! off above and below by a blank line: 13.8 points between its own lines
//! against 27.6 to the prose above, on every document measured from 2022 to
//! 2026. So the legend band starts at the row above the table and grows upward
//! while each gap stays within [`BLANK_LINE`] times the smallest gap already
//! inside it.
//!
//! One bound comes with that. A legend of a single line has no internal spacing
//! to measure, so the walk takes the line above it. No season prints one: the
//! component set runs to six codes or more, two to a line.
//!
//! # Whitespace opens no row
//!
//! Both rules run on the inked glyphs, and a row here is a row of ink. The
//! documents print space glyphs on baselines that carry nothing else, three of
//! them between the prose and the legend of a 2026 snapshot. Count those as
//! rows and the blank line above the legend arrives as two short gaps rather
//! than one long one, so the walk climbs straight through it and the legend band
//! swallows the page. The bands are therefore chosen by where the ink is, and
//! every glyph inside a band's baselines, spaces included, then joins it. A
//! cell's padding survives, since it prints on the same baseline as the cell.

use std::ops::{Range, RangeInclusive};

use extract::{ClusterConfig, Glyph, Grid, cluster};

use crate::error::BandError;

/// Widest run of ink still counted narrow, in points.
///
/// It sits between the two things it has to tell apart. The widest header cell
/// the documents print is `Driver` at 29 points, and the narrowest legend
/// description is `EXhaust set` at 49, so a header row counts as all narrow and
/// a legend line as half.
const NARROW_RUN_PT: f32 = 30.0;

/// How far a vertical gap must exceed a block's line spacing to read as a blank
/// line.
///
/// The documents leave a full blank line, so the real gap is double the
/// spacing: 27.6 points against 13.8. Half a line of margin either way.
const BLANK_LINE: f32 = 1.5;

/// The two bands of a snapshot page, each clustered on its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Bands {
    legend: Grid,
    table: Grid,
}

impl Bands {
    /// The legend band, for [`read_legend`](crate::read_legend).
    #[must_use]
    pub fn legend(&self) -> &Grid {
        &self.legend
    }

    /// The table band, header rows first, for
    /// [`label_columns`](crate::label_columns).
    #[must_use]
    pub fn table(&self) -> &Grid {
        &self.table
    }
}

/// Slice a page's glyphs into a legend band and a table band, clustering each
/// on its own.
///
/// `glyphs` is one whole page. `config` sets both the banding and the
/// clustering: rows come from `row_gap` and runs from `column_gap`, which are
/// the same questions the clustering asks of the same page.
///
/// The table band is the longest block of consecutive table rows, and the
/// topmost of equals. A page prints one table, so a second block of the same
/// length would mean the rules had already failed.
///
/// # Errors
///
/// Returns [`BandError::NoTableBand`] when no row on the page prints like a
/// table row, and [`BandError::NoLegendBand`] when the table band starts at the
/// top of the page with nothing above it to read.
pub fn bands(glyphs: &[Glyph], config: &ClusterConfig) -> Result<Bands, BandError> {
    let rows = rows_of_ink(glyphs, config.row_gap);
    let table_row: Vec<bool> = rows
        .iter()
        .map(|row| is_table_row(row, config.column_gap))
        .collect();

    let table = longest_block(&table_row).ok_or(BandError::NoTableBand)?;
    let legend = legend_band(&rows, &table_row, table.start).ok_or(BandError::NoLegendBand)?;

    Ok(Bands {
        legend: cluster(&within(glyphs, &baselines(&rows[legend])), config),
        table: cluster(&within(glyphs, &baselines(&rows[table])), config),
    })
}

/// Group the inked glyphs into rows by baseline, top to bottom.
///
/// Single linkage over the baselines, splitting where the gap exceeds
/// `row_gap`, which is what the clustering's row pass does over every glyph.
/// This pass drops the whitespace first; see the module doc on why a space must
/// not open a row of its own here.
fn rows_of_ink(glyphs: &[Glyph], row_gap: f32) -> Vec<Vec<Glyph>> {
    let mut sorted: Vec<Glyph> = glyphs
        .iter()
        .copied()
        .filter(|glyph| !glyph.ch.is_whitespace())
        .collect();
    sorted.sort_by(|a, b| b.y.total_cmp(&a.y));

    let mut rows: Vec<Vec<Glyph>> = Vec::new();
    let mut previous: Option<f32> = None;
    for glyph in sorted {
        match rows.last_mut() {
            Some(row) if previous.is_some_and(|prev| (prev - glyph.y).abs() <= row_gap) => {
                row.push(glyph);
            }
            _ => rows.push(vec![glyph]),
        }
        previous = Some(glyph.y);
    }
    rows
}

/// The topmost baseline in a row.
///
/// A row can print across two baselines a fraction of a point apart, so it
/// takes one of them to stand for the row. The topmost keeps the spacing
/// between rows measured from the same edge.
fn top_baseline(row: &[Glyph]) -> f32 {
    row.iter()
        .map(|glyph| glyph.y)
        .fold(f32::NEG_INFINITY, f32::max)
}

/// Whether a row prints like a row of a table: more than half its runs narrow.
///
/// The majority is strict, so a legend line of one code beside one description
/// counts as prose. That is the common shape, and half its runs are narrow.
fn is_table_row(row: &[Glyph], column_gap: f32) -> bool {
    let widths = run_widths(row, column_gap);
    let narrow = widths
        .iter()
        .filter(|width| **width < NARROW_RUN_PT)
        .count();
    narrow * 2 > widths.len()
}

/// The width of each run of ink in a row, left to right.
///
/// A run is a maximal group of glyphs no more than `column_gap` apart, so it is
/// what the column pass would join. The row carries ink alone, which is what
/// makes the measure mean anything: a padded run of spaces reaches from one
/// column to the next and would weld every run into one.
fn run_widths(row: &[Glyph], column_gap: f32) -> Vec<f32> {
    let mut ink: Vec<Glyph> = row.to_vec();
    ink.sort_by(|a, b| a.x0.total_cmp(&b.x0));

    let mut runs: Vec<(f32, f32)> = Vec::new();
    for glyph in ink {
        match runs.last_mut() {
            Some(run) if glyph.x0 - run.1 <= column_gap => run.1 = run.1.max(glyph.x1),
            _ => runs.push((glyph.x0, glyph.x1)),
        }
    }
    runs.into_iter().map(|(x0, x1)| x1 - x0).collect()
}

/// The longest block of consecutive `true` flags, the topmost of equals.
fn longest_block(flags: &[bool]) -> Option<Range<usize>> {
    let mut longest: Option<Range<usize>> = None;
    let mut start: Option<usize> = None;
    for (index, flag) in flags.iter().copied().chain([false]).enumerate() {
        match (flag, start) {
            (true, None) => start = Some(index),
            (false, Some(open)) => {
                if longest
                    .as_ref()
                    .is_none_or(|best| index - open > best.len())
                {
                    longest = Some(open..index);
                }
                start = None;
            }
            _ => {}
        }
    }
    longest
}

/// The rows above the table band that belong to the legend.
///
/// The walk starts at the row directly above the table and climbs while the gap
/// to the next row up stays within [`BLANK_LINE`] times the smallest gap already
/// crossed. The first step has nothing to compare against and is always taken;
/// see the module doc on what that costs.
///
/// Returns `None` when the table band starts at the top of the page.
fn legend_band(
    rows: &[Vec<Glyph>],
    table_row: &[bool],
    table_start: usize,
) -> Option<Range<usize>> {
    let mut top = table_start.checked_sub(1)?;
    let mut spacing: Option<f32> = None;

    while top > 0 {
        let above = top - 1;
        // A second table stops the walk. Prose or a legend line above one is
        // not this table's legend.
        if table_row[above] {
            break;
        }
        let gap = top_baseline(&rows[above]) - top_baseline(&rows[top]);
        if let Some(seen) = spacing
            && gap > BLANK_LINE * seen
        {
            break;
        }
        spacing = Some(spacing.map_or(gap, |seen: f32| seen.min(gap)));
        top = above;
    }

    Some(top..table_start)
}

/// The baselines a block of rows spans, lowest to highest.
fn baselines(rows: &[Vec<Glyph>]) -> RangeInclusive<f32> {
    let all = rows.iter().flatten().map(|glyph| glyph.y);
    let (low, high) = all.fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), y| {
        (low.min(y), high.max(y))
    });
    low..=high
}

/// Every glyph printed on those baselines, whitespace included.
fn within(glyphs: &[Glyph], baselines: &RangeInclusive<f32>) -> Vec<Glyph> {
    glyphs
        .iter()
        .copied()
        .filter(|glyph| baselines.contains(&glyph.y))
        .collect()
}

#[cfg(test)]
mod tests {
    //! The banding, driven with hand-built glyphs. No PDF: the rules read
    //! geometry, and geometry is what a test can state outright.

    use super::*;

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

    /// A page shaped like a snapshot: prose, then a legend, then a table.
    ///
    /// The baselines are the documents': the legend's lines sit 13.8 points
    /// apart and the prose sits 41.4 above them, a blank line clear. `PU-CE`'s
    /// description spans x 347 to 527, past every component column beneath it,
    /// so the two bands clustered together lose the table.
    fn page() -> Vec<Glyph> {
        let mut glyphs = word(
            "The drivers entered in this synthetic championship have used",
            42.0,
            530.4,
        );
        glyphs.extend(word("ICE", 42.0, 489.0));
        glyphs.extend(word("Internal Combustion Engine", 92.0, 489.0));
        glyphs.extend(word("TC", 317.0, 489.0));
        glyphs.extend(word("Turbo Charger", 347.0, 489.0));
        glyphs.extend(word("ES", 42.0, 475.2));
        glyphs.extend(word("Energy Store unit", 92.0, 475.2));
        glyphs.extend(word("PU-CE", 317.0, 475.2));
        glyphs.extend(word("Power Unit Control Electronics", 347.0, 475.2));
        glyphs.extend(table_rows());
        glyphs
    }

    /// The table alone: a header line over two data rows.
    fn table_rows() -> Vec<Glyph> {
        let mut glyphs = word("N", 48.0, 421.9);
        glyphs.extend(word("Car", 74.0, 421.9));
        glyphs.extend(word("ICE", 303.0, 421.9));
        glyphs.extend(word("TC", 333.0, 421.9));
        glyphs.extend(word("ES", 368.0, 421.9));
        glyphs.extend(word("PU-CE", 400.0, 421.9));
        for (index, y) in [398.9, 387.4].into_iter().enumerate() {
            let count = if index == 0 { "2" } else { "3" };
            glyphs.extend(word("7", 48.0, y));
            glyphs.extend(word("Falcon Racing", 74.0, y));
            for x in [309.0, 338.0, 371.0, 403.0] {
                glyphs.extend(word(count, x, y));
            }
        }
        glyphs
    }

    fn banded(glyphs: &[Glyph]) -> Bands {
        bands(glyphs, &ClusterConfig::default()).expect("the page must band")
    }

    /// Every cell of a grid, joined, so a test can ask what a band swallowed.
    fn text_of(grid: &Grid) -> String {
        (0..grid.row_count())
            .flat_map(|row| (0..grid.columns().len()).map(move |column| (row, column)))
            .map(|(row, column)| grid.cell(row, column))
            .collect::<Vec<&str>>()
            .join("|")
    }

    #[test]
    fn the_table_band_recovers_every_column() {
        let bands = banded(&page());

        assert_eq!(bands.table().columns().len(), 6);
    }

    #[test]
    fn clustering_the_page_as_one_band_loses_the_table() {
        // The point of the module. The prose spans the text width and the wide
        // legend entry spans the component columns, so one pass over the page
        // yields a single column where the table has six. An implementation
        // that bands by selection alone, or not at all, fails the test above.
        let grid = cluster(&page(), &ClusterConfig::default());

        assert_eq!(grid.columns().len(), 1);
    }

    #[test]
    fn clustering_both_bands_together_still_loses_the_table() {
        // Selecting the bands is not enough: drop the prose, cluster the legend
        // and the table in one pass, and `PU-CE`'s description still bridges
        // all four component columns into one. Three columns come out where the
        // table has six.
        let page = page();
        let below_the_prose: Vec<Glyph> = page
            .iter()
            .copied()
            .filter(|glyph| glyph.y < 500.0)
            .collect();

        let grid = cluster(&below_the_prose, &ClusterConfig::default());

        assert_eq!(grid.columns().len(), 3);
    }

    #[test]
    fn the_legend_band_holds_the_legend_alone() {
        let bands = banded(&page());

        assert_eq!(bands.legend().row_count(), 2);
    }

    #[test]
    fn the_prose_above_the_legend_stays_out_of_the_legend_band() {
        // A blank line separates the two. Take the prose in and the band
        // clusters to one column, which glues each line's second entry onto the
        // first one's description.
        let bands = banded(&page());

        assert!(
            !text_of(bands.legend()).contains("drivers"),
            "the prose must not reach the legend: {}",
            text_of(bands.legend())
        );
    }

    #[test]
    fn the_wide_legend_entry_never_reaches_the_table_band() {
        let bands = banded(&page());

        assert!(
            !text_of(bands.table()).contains("Power Unit"),
            "the legend entry spanning the columns must stay in the legend band: {}",
            text_of(bands.table())
        );
    }

    #[test]
    fn the_wide_legend_entry_lands_in_the_legend_band() {
        let bands = banded(&page());

        assert!(text_of(bands.legend()).contains("Power Unit Control Electronics"));
    }

    #[test]
    fn the_table_band_keeps_its_header_row() {
        // `label_columns` reads the codes down the header, so a band that
        // starts at the first data row would leave every column unnamed.
        let bands = banded(&page());

        assert_eq!(bands.table().cell(0, 1), "Car");
    }

    #[test]
    fn a_wider_gap_than_the_legend_prints_does_not_end_the_legend_band() {
        // The margin. The rule allows half a line before it calls a gap blank,
        // so a legend line printed 1.4 times the spacing below the one above
        // stays in the band.
        let mut glyphs = word("ICE", 42.0, 494.0);
        glyphs.extend(word("Internal Combustion Engine", 92.0, 494.0));
        glyphs.extend(word("ES", 42.0, 475.2));
        glyphs.extend(word("Energy Store unit", 92.0, 475.2));
        glyphs.extend(word("TC", 42.0, 461.4));
        glyphs.extend(word("Turbo Charger", 92.0, 461.4));
        glyphs.extend(table_rows());

        let bands = banded(&glyphs);

        assert_eq!(bands.legend().row_count(), 3);
    }

    #[test]
    fn refuses_a_page_with_no_table() {
        let mut glyphs = word("ICE", 42.0, 489.0);
        glyphs.extend(word("Internal Combustion Engine", 92.0, 489.0));

        assert_eq!(
            bands(&glyphs, &ClusterConfig::default()),
            Err(BandError::NoTableBand)
        );
    }

    #[test]
    fn refuses_a_page_that_is_all_table() {
        // Nothing above the table means no legend, and a table whose columns
        // nobody can name is worse than none.
        assert_eq!(
            bands(&table_rows(), &ClusterConfig::default()),
            Err(BandError::NoLegendBand)
        );
    }

    #[test]
    fn refuses_an_empty_page() {
        assert_eq!(
            bands(&[], &ClusterConfig::default()),
            Err(BandError::NoTableBand)
        );
    }
}
