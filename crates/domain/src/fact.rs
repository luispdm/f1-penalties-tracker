//! A [`Fact`]: one claim, from one document, about one component.
//!
//! Three independent FIA documents state the same running count, so the tracker
//! records each claim verbatim, tagged by its source, and lets the invariant
//! sweep cross-check them (Decision 4: store raw facts, compute the view, flag
//! disagreements, never guess). Nothing here computes; a fact is a witness.

use std::fmt;

/// A championship season, for example `2026`.
pub type Season = u16;

/// A car number.
pub type Car = u16;

/// An event's ordering within a season, counting from one.
pub type Round = u8;

/// A component code, for example `ICE` or `PU-CE`.
///
/// The code is data, never an enum. The component set changes across seasons:
/// 2021 adds an exhaust code, and 2026 drops MGU-H, adds PU-ANC, and renames
/// others. A season's valid components are exactly the rows the ruleset seeds
/// for it, so a fixed enum would encode a set that does not hold season to
/// season.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentCode(String);

impl ComponentCode {
    /// Wrap a raw code string.
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    /// The code as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ComponentCode {
    fn from(code: &str) -> Self {
        Self(code.to_owned())
    }
}

impl From<String> for ComponentCode {
    fn from(code: String) -> Self {
        Self(code)
    }
}

impl fmt::Display for ComponentCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether a document states a fitted element is within the allowance.
///
/// The new-elements document marks each new part in or out of conformity, and
/// the sweep checks that verdict against its own computed exceedance flag and
/// against the set of elements an infringement penalizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conformity {
    /// The document states the element is within its allowance.
    InConformity,
    /// The document states the element exceeds its allowance.
    NotInConformity,
}

/// What a fact claims about its `(season, round, car, component)`.
///
/// The variants keep two things apart that must never merge. Only
/// [`SnapshotCount`](Claim::SnapshotCount) and
/// [`ElementsFitted`](Claim::ElementsFitted) feed the components-used sum: the
/// count after an event is the prior snapshot plus that event's fitted parts.
/// An infringement's restated ordinal is a [`StatedOrdinal`](Claim::StatedOrdinal)
/// cross-check plus a [`Penalty`](Claim::Penalty), never a fitted increment, so
/// an element the FIA counts once where it is fitted is never counted a second
/// time where a penalty restates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    /// The snapshot's running total before this event's new parts:
    /// `snapshot(round)`. Feeds the components-used sum.
    SnapshotCount(u32),
    /// Parts fitted at this event, with the document's conformity verdict:
    /// `newPU(round)`. Feeds the components-used sum.
    ElementsFitted {
        /// How many of this component the driver fitted this event.
        count: u32,
        /// Whether the document states this fitting stays within the allowance.
        conformity: Conformity,
    },
    /// The new-elements document's "previously used" figure. A cross-check that
    /// must equal the prior snapshot; never part of the sum.
    PreviouslyUsed(u32),
    /// An infringement's restated ordinal, for example "the fifth of the four
    /// permitted". A cross-check against the computed count after the event;
    /// never a fitted increment.
    StatedOrdinal(u32),
    /// The penalty an infringement imposes, recorded verbatim. Its presence
    /// marks the component penalized; the text is not cross-checked.
    Penalty(String),
}

/// One claim from one document.
///
/// The key is `(season, car)`; there is no cross-season identity, so a team
/// rename is a non-event. `round` orders events within a season so the sweep can
/// relate consecutive ones in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// The season the claim belongs to, for example `2026`.
    pub season: Season,
    /// The event's ordering within the season, counting from one.
    pub round: Round,
    /// The car number the claim is about.
    pub car: Car,
    /// The component the claim is about.
    pub component: ComponentCode,
    /// What the fact claims.
    pub claim: Claim,
    /// The source document's number, as its header states it. Reconciliation
    /// (issue #31) supersedes an original by the highest document number.
    pub document: u32,
    /// Whether reconciliation has superseded this fact with a corrected
    /// document. The sweep skips superseded facts, so a correction is the
    /// expected outcome and never a conflict. Reconciliation sets this; issue
    /// #24 leaves it `false` and lets tests set it by hand.
    pub superseded: bool,
}

impl Fact {
    /// Build a live (not superseded) fact.
    pub fn new(
        season: Season,
        round: Round,
        car: Car,
        component: impl Into<ComponentCode>,
        claim: Claim,
        document: u32,
    ) -> Self {
        Self {
            season,
            round,
            car,
            component: component.into(),
            claim,
            document,
            superseded: false,
        }
    }
}
