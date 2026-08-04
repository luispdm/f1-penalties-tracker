//! Regenerate the committed fixtures.
//!
//! Run from anywhere in the workspace:
//!
//! ```text
//! cargo run -p pdf-fixtures --bin regen
//! ```
//!
//! It writes every fixture into `crates/pdf-fixtures/fixtures`. Pass a
//! directory to write elsewhere.

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use pdf_fixtures::{
    pu_snapshot_spec, render_document, render_table, table_grid_spec, two_band_snapshot_spec,
};

fn main() -> ExitCode {
    let dir = std::env::args().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures"),
        PathBuf::from,
    );

    if write(&dir, "table_grid.pdf", render_table(&table_grid_spec()))
        && write(
            &dir,
            "pu_snapshot.pdf",
            render_document(&pu_snapshot_spec()),
        )
        && write(
            &dir,
            "two_band_snapshot.pdf",
            render_table(&two_band_snapshot_spec()),
        )
    {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Write `pdf` into `dir/name`, reporting whether it landed.
fn write(dir: &Path, name: &str, pdf: Vec<u8>) -> bool {
    let out = dir.join(name);
    match std::fs::write(&out, pdf) {
        Ok(()) => {
            println!("wrote {}", out.display());
            true
        }
        Err(err) => {
            eprintln!("failed to write {}: {err}", out.display());
            false
        }
    }
}
