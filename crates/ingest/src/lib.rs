//! Ingest layer: scrape, fetch, pdf extraction, parse.
//!
//! The document parsers live here, on top of the positioned grids `extract`
//! emits. This is the first layer that carries F1 knowledge: `extract` names no
//! column, and turning one into a component code takes a legend, a header, and
//! the rules for reading them.
//!
//! So far it carries the snapshot parser and the steps it is built from:
//!
//! - [`read_legend`], which pairs each code the legend declares with its
//!   description. The legend is the only complete source of a season's
//!   component set.
//! - [`label_columns`], which gives each component column its code by reading
//!   down the column rather than along the wrapped header line.
//! - [`bands`], which slices a page into the two regions those parsers read and
//!   clusters each on its own.
//! - [`identity_columns`], which names the three columns every PU document
//!   prints left of its components, by position rather than header text.
//! - [`parse_snapshot`], which turns a whole `PU elements used per driver up to
//!   now` document into one count fact per driver per component.
//! - [`corrections`], the list of documents known to print a legend code their
//!   own table contradicts. The caller resolves the entry for its season and
//!   hands it to the parser, so no parser here holds a season or looks one up.
//!
//! Each step below the parser takes a grid covering one band. Clustering a whole
//! page yields a single column, because the prose and the legend descriptions
//! run across it without a gap wide enough to split, and clustering the two
//! bands together yields two, because a legend description spans the columns
//! beneath it. That is why the bands are cut and clustered apart.
//!
//! [`parse_snapshot`] is the layer above: it takes every page of a document,
//! because a snapshot states its counts on one page and its document number on
//! another.

mod bands;
pub mod corrections;
mod error;
mod identity;
mod labels;
mod legend;
mod snapshot;

pub use bands::{Bands, bands};
pub use error::{BandError, IdentityError, LabelError, SnapshotError};
pub use identity::{IdentityColumns, identity_columns};
pub use labels::{ColumnLabels, label_columns};
pub use legend::{Legend, LegendEntry, read_legend};
pub use snapshot::parse_snapshot;
