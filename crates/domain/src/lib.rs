//! Domain layer: components, allowances, penalties, invariants.
//!
//! Pure, no IO. The crate works on [`Fact`] values, built by hand in tests and,
//! later, by the document parsers. It carries three things:
//!
//! - [`Fact`] and its [`Claim`] kinds, the verbatim record of what one document
//!   states about one component (Decision 4: store raw facts, compute the view).
//! - [`Allowances`], the per-season regulation allowances and the single rule
//!   the domain implements, that a count above its allowance is an exceedance.
//! - [`sweep`], the invariant oracle that cross-checks the facts against each
//!   other and returns every [`Conflict`] it finds.
//!
//! The types firm up as the parsers reveal what the documents state; new claim
//! kinds land with the parser that needs them.

mod allowance;
mod fact;
mod sweep;

pub use allowance::Allowances;
pub use fact::{Car, Claim, ComponentCode, Conformity, Fact, Round, Season};
pub use sweep::{Conflict, sweep};
