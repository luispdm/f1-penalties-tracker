//! Per-season component allowances and the one rule the domain implements.
//!
//! Allowances are seeded from the regulations, never parsed from document text
//! (Decision 6). A parser leaves holes until someone fits that part: the 2025
//! Belgian document omits the exhaust because nobody took one at Spa, so reading
//! the set from a document would record it short. The seed is complete from the
//! first ingest.
//!
//! The domain implements exactly one F1 rule: a count above its allowance is an
//! exceedance. Every other rule stays unimplemented; the tracker records what
//! the FIA states.

use std::collections::BTreeMap;

use crate::fact::ComponentCode;

/// The seeded allowance table: a season and component code map to a permitted
/// count.
///
/// A season's valid components are exactly the rows present for it, so a lookup
/// that misses tells the sweep the component is unknown for that season rather
/// than assuming a default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowances {
    by_season_component: BTreeMap<(u16, ComponentCode), u32>,
}

impl Allowances {
    /// Build an allowance table from `(season, component, allowance)` rows.
    pub fn from_rows(rows: impl IntoIterator<Item = (u16, ComponentCode, u32)>) -> Self {
        Self {
            by_season_component: rows
                .into_iter()
                .map(|(season, component, allowance)| ((season, component), allowance))
                .collect(),
        }
    }

    /// The permitted count for a component in a season, or `None` when the
    /// component is not seeded for that season.
    #[must_use]
    pub fn allowance(&self, season: u16, component: &ComponentCode) -> Option<u32> {
        self.by_season_component
            .get(&(season, component.clone()))
            .copied()
    }

    /// Whether `count` exceeds the allowance: the single domain rule.
    ///
    /// Returns `Some(true)` above the allowance and `Some(false)` at or below
    /// it. Returns `None` when the component is not seeded for the season, so a
    /// caller cannot mistake an unknown component for a compliant one.
    #[must_use]
    pub fn exceeds(&self, season: u16, component: &ComponentCode, count: u32) -> Option<bool> {
        self.allowance(season, component)
            .map(|allowance| count > allowance)
    }

    /// The regulation allowances for the seasons the tracker currently covers.
    ///
    /// 2026 only, the season the synthetic events exercise. The verified set is
    /// seven components: ICE, TC, EXH, MGU-K, ES, PU-CE, PU-ANC. Later seasons
    /// seed their own rows, which differ, so the table stays data.
    #[must_use]
    pub fn seed() -> Self {
        Self::from_rows([
            (2026, ComponentCode::new("ICE"), 4),
            (2026, ComponentCode::new("TC"), 4),
            (2026, ComponentCode::new("EXH"), 4),
            (2026, ComponentCode::new("MGU-K"), 3),
            (2026, ComponentCode::new("ES"), 3),
            (2026, ComponentCode::new("PU-CE"), 3),
            (2026, ComponentCode::new("PU-ANC"), 6),
        ])
    }
}
