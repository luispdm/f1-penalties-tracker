//! Component allowances and the one rule the domain implements.
//!
//! Allowances are seeded from the regulations, never parsed from document text.
//! A parser leaves holes until someone fits that part: the 2025
//! Belgian document omits the exhaust because nobody took one at Spa, so reading
//! the set from a document would record it short. The seed is complete from the
//! first ingest.
//!
//! The domain implements exactly one F1 rule: a count above its allowance is an
//! exceedance. Every other rule stays unimplemented; the tracker records what
//! the FIA states.

use std::collections::BTreeMap;

use crate::fact::ComponentCode;

/// One season's seeded allowance table: a component code maps to a permitted
/// count.
///
/// The season's valid components are exactly the rows present, so a lookup that
/// misses tells the sweep the component is unknown rather than assuming a
/// default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowances {
    by_component: BTreeMap<ComponentCode, u32>,
}

impl Allowances {
    /// Build an allowance table from `(component, allowance)` rows.
    pub fn from_rows(rows: impl IntoIterator<Item = (ComponentCode, u32)>) -> Self {
        Self {
            by_component: rows.into_iter().collect(),
        }
    }

    /// The permitted count for a component, or `None` when the component is not
    /// seeded.
    #[must_use]
    pub fn allowance(&self, component: &ComponentCode) -> Option<u32> {
        self.by_component.get(component).copied()
    }

    /// Whether `count` exceeds the allowance: the single domain rule.
    ///
    /// Returns `Some(true)` above the allowance and `Some(false)` at or below
    /// it. Returns `None` when the component is not seeded, so a caller cannot
    /// mistake an unknown component for a compliant one.
    #[must_use]
    pub fn exceeds(&self, component: &ComponentCode, count: u32) -> Option<bool> {
        self.allowance(component).map(|allowance| count > allowance)
    }

    /// The 2026 regulation allowances, the season the synthetic events exercise.
    ///
    /// The verified set is seven components: ICE, TC, EXH, MGU-K, ES, PU-CE,
    /// PU-ANC. Later seasons seed their own rows, which differ, so the table
    /// stays data.
    #[must_use]
    pub fn seed() -> Self {
        Self::from_rows([
            (ComponentCode::new("ICE"), 4),
            (ComponentCode::new("TC"), 4),
            (ComponentCode::new("EXH"), 4),
            (ComponentCode::new("MGU-K"), 3),
            (ComponentCode::new("ES"), 3),
            (ComponentCode::new("PU-CE"), 3),
            (ComponentCode::new("PU-ANC"), 6),
        ])
    }
}
