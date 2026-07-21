//! Regenerate the committed extraction fixture.
//!
//! Run from anywhere in the workspace:
//!
//! ```text
//! cargo run -p pdf-fixtures --bin regen
//! ```
//!
//! It writes `crates/pdf-fixtures/fixtures/table_grid.pdf`. Pass a path to
//! write elsewhere.

use std::{path::PathBuf, process::ExitCode};

use pdf_fixtures::{render_table, table_grid_spec};

fn main() -> ExitCode {
    let out = std::env::args().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/table_grid.pdf"),
        PathBuf::from,
    );

    let bytes = render_table(&table_grid_spec());
    match std::fs::write(&out, bytes) {
        Ok(()) => {
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed to write {}: {err}", out.display());
            ExitCode::FAILURE
        }
    }
}
