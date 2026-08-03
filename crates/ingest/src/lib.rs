//! Ingest layer: scrape, fetch, pdf extraction, parse.
//!
//! The document parsers live here, on top of the positioned grids `extract`
//! emits. This is the first layer that carries F1 knowledge: `extract` names no
//! column, and turning one into a component code takes a legend, a header, and
//! the rules for reading them.
//!
//! So far it carries the column mapping a snapshot table needs:
//!
//! - [`read_legend`], which pairs each code the legend declares with its
//!   description. The legend is the only complete source of a season's
//!   component set.
//! - [`label_columns`], which gives each component column its code by reading
//!   down the column rather than along the wrapped header line.
//!
//! - [`bands`], which slices a page into those two regions and clusters each on
//!   its own.
//!
//! Both parsers take a grid covering one band of the page. Clustering a whole
//! page yields a single column, because the prose and the legend descriptions
//! run across it without a gap wide enough to split, and clustering the two
//! bands together yields two, because a legend description spans the columns
//! beneath it. So [`bands`] slices the page and clusters each band apart.

mod bands;
mod error;
mod labels;
mod legend;

pub use bands::{Bands, bands};
pub use error::{BandError, LabelError};
pub use labels::{ColumnLabels, label_columns};
pub use legend::{Legend, LegendEntry, read_legend};
