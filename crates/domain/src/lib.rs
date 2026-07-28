//! Domain layer: components, allowances, penalties, invariants.
//!
//! Pure, no IO. The crate works on [`Fact`] values, built by hand in tests and,
//! later, by the document parsers. It carries three things:
//!
//! - [`Fact`] and its [`Claim`] kinds, the verbatim record of what one document
//!   states about one component (store raw facts, compute the view).
//! - [`Allowances`], the per-season regulation allowances and the single rule
//!   the domain implements, that a count above its allowance is an exceedance.
//! - [`resolve_seats`], which derives the [`Seat`] each car entry occupies from
//!   a window of per-event [`Roster`]s, so a mid-season swap passes a car's
//!   running count to the incoming driver instead of splitting it.
//! - [`sweep`], the invariant oracle that cross-checks the facts against each
//!   other and returns every [`Conflict`] it finds.
//!
//! Checking a set of facts is two steps: resolve the rosters into a seat map,
//! then sweep the facts against it. The sweep folds by seat, so it needs the map
//! and never builds one of its own.
//!
//! The types firm up as the parsers reveal what the documents state; new claim
//! kinds land with the parser that needs them.

mod allowance;
mod fact;
mod seat;
mod sweep;

pub use allowance::Allowances;
pub use fact::{Car, Claim, ComponentCode, Conformity, Fact, Round, Season, Team};
pub use seat::{Driver, Roster, RosterEntry, Seat, SeatAmbiguity, Seats, Slot, resolve_seats};
pub use sweep::{Conflict, sweep};
