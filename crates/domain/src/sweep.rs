//! The invariant sweep: the oracle that cross-checks facts against each other.
//!
//! Three independent FIA documents state the same running count. The snapshot
//! publishes before an event, the new-elements document lands after it, and an
//! infringement restates the count of the element it penalizes. The sweep folds
//! the facts into per-component timelines and proves the oracle equations over
//! them, flagging every disagreement rather than guessing which witness is right
//! (Decision 4).
//!
//! It runs over surviving facts only. A fact a corrected document has superseded
//! is skipped, so a correction is the expected outcome and never a conflict.
//! Reconciliation (issue #31) computes supersession and marks the facts; the
//! sweep here honours the mark.
//!
//! The equations proved:
//!
//! - the count after an event equals the prior snapshot plus that event's new
//!   elements, and that count equals the next event's snapshot;
//! - the new-elements document's "previously used" figure equals the prior
//!   snapshot;
//! - the document's stated conformity matches the computed count-over-allowance
//!   flag;
//! - an infringement's stated ordinal equals the computed count after its event;
//! - the set of elements an infringement penalizes equals the set the
//!   new-elements document flags not in conformity.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    allowance::Allowances,
    fact::{Car, Claim, ComponentCode, Conformity, Fact, Round, Season},
};

/// A disagreement the sweep found between independent witnesses.
///
/// Each variant names where the disagreement sits and the two figures that
/// clash, so a conflict is actionable and never a bare boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// The count after one event does not equal the next event's snapshot.
    SnapshotDisagreement {
        /// The season the components belong to.
        season: Season,
        /// The car number.
        car: Car,
        /// The component whose totals clash.
        component: ComponentCode,
        /// The event whose computed count-after is under test.
        from_round: Round,
        /// The following event whose snapshot should match.
        to_round: Round,
        /// The computed count after `from_round`.
        count_after: u32,
        /// The snapshot the following event states.
        next_snapshot: u32,
    },
    /// A new-elements document's "previously used" figure does not equal the
    /// prior snapshot.
    PreviouslyUsedMismatch {
        /// The season the components belong to.
        season: Season,
        /// The car number.
        car: Car,
        /// The component whose figures clash.
        component: ComponentCode,
        /// The event the document belongs to.
        round: Round,
        /// The "previously used" figure the document states.
        previously_used: u32,
        /// The snapshot for the same event.
        snapshot: u32,
    },
    /// A document's stated conformity disagrees with the computed
    /// count-over-allowance flag.
    StatedExceedanceMismatch {
        /// The season the components belong to.
        season: Season,
        /// The car number.
        car: Car,
        /// The component under test.
        component: ComponentCode,
        /// The event the document belongs to.
        round: Round,
        /// Whether the document states the element is not in conformity.
        stated_not_in_conformity: bool,
        /// Whether the computed count exceeds the allowance.
        computed_exceeds: bool,
    },
    /// An infringement's stated ordinal does not equal the computed count after
    /// its event. A hard conflict.
    OrdinalMismatch {
        /// The season the components belong to.
        season: Season,
        /// The car number.
        car: Car,
        /// The penalized component.
        component: ComponentCode,
        /// The event the infringement belongs to.
        round: Round,
        /// The ordinal the infringement restates.
        stated_ordinal: u32,
        /// The computed count after the event.
        count_after: u32,
    },
    /// For one event, the set of elements infringements penalize does not equal
    /// the set the new-elements document flags not in conformity. Each element
    /// is a `(car, component)` pair.
    PenalizedSetMismatch {
        /// The season the event belongs to.
        season: Season,
        /// The event.
        round: Round,
        /// The elements infringements penalize.
        penalized: BTreeSet<(Car, ComponentCode)>,
        /// The elements the new-elements document flags not in conformity.
        not_in_conformity: BTreeSet<(Car, ComponentCode)>,
    },
    /// A fact references a component the season never seeds, so no allowance
    /// exists to check it against (Decision 6: a season's valid components are
    /// exactly its seeded rows).
    UnknownComponent {
        /// The season the fact belongs to.
        season: Season,
        /// The unseeded component.
        component: ComponentCode,
    },
}

/// What every claim about one `(season, car, component)` at one event folds to.
#[derive(Debug, Default)]
struct RoundData {
    snapshot: Option<u32>,
    fitted: Option<u32>,
    conformity: Option<Conformity>,
    previously_used: Option<u32>,
    stated_ordinal: Option<u32>,
    penalized: bool,
}

impl RoundData {
    /// Fold one claim into the event's data.
    fn absorb(&mut self, claim: &Claim) {
        match claim {
            Claim::SnapshotCount(count) => self.snapshot = Some(*count),
            Claim::ElementsFitted { count, conformity } => {
                self.fitted = Some(*count);
                self.conformity = Some(*conformity);
            }
            Claim::PreviouslyUsed(count) => self.previously_used = Some(*count),
            Claim::StatedOrdinal(ordinal) => self.stated_ordinal = Some(*ordinal),
            Claim::Penalty(_) => self.penalized = true,
        }
    }

    /// The count after the event: the snapshot plus this event's fitted parts.
    ///
    /// Only the snapshot and the fitted count feed the sum. An infringement's
    /// restated ordinal never does. `None` when no snapshot anchors the event.
    fn count_after(&self) -> Option<u32> {
        self.snapshot
            .map(|snapshot| snapshot + self.fitted.unwrap_or(0))
    }
}

/// The `(season, car, component)` a timeline belongs to.
type Series = (Season, Car, ComponentCode);

/// Cross-check `facts` against `allowances` and return every conflict found.
///
/// Superseded facts are skipped. A clean set of facts returns an empty vector.
/// The conflicts come back in a deterministic order.
#[must_use]
pub fn sweep(facts: &[Fact], allowances: &Allowances) -> Vec<Conflict> {
    let timelines = fold_live_facts(facts);

    let mut conflicts = Vec::new();
    conflicts.extend(local_conflicts(&timelines, allowances));
    conflicts.extend(snapshot_disagreements(&timelines));
    conflicts.extend(penalized_set_conflicts(&timelines));
    conflicts
}

/// Fold the live facts into per-component, per-event data.
fn fold_live_facts(facts: &[Fact]) -> BTreeMap<Series, BTreeMap<Round, RoundData>> {
    let mut timelines: BTreeMap<Series, BTreeMap<Round, RoundData>> = BTreeMap::new();
    for fact in facts.iter().filter(|fact| !fact.superseded) {
        let series = (fact.season, fact.car, fact.component.clone());
        timelines
            .entry(series)
            .or_default()
            .entry(fact.round)
            .or_default()
            .absorb(&fact.claim);
    }
    timelines
}

/// Flag the conflicts one event proves on its own: an unseeded component, a
/// previously-used figure that disagrees with the snapshot, a stated conformity
/// that disagrees with the computed exceedance, and a stated ordinal that
/// disagrees with the count after the event.
///
/// One flat pass over every `(series, round)` cell. Each unseeded
/// `(season, component)` yields one `UnknownComponent`, however many events
/// reference it.
fn local_conflicts(
    timelines: &BTreeMap<Series, BTreeMap<Round, RoundData>>,
    allowances: &Allowances,
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    let mut seen_unknown: BTreeSet<(Season, ComponentCode)> = BTreeSet::new();

    let cells = timelines
        .iter()
        .flat_map(|((season, car, component), rounds)| {
            rounds
                .iter()
                .map(move |(round, data)| (*season, *car, component, *round, data))
        });

    for (season, car, component, round, data) in cells {
        if allowances.allowance(season, component).is_none()
            && seen_unknown.insert((season, component.clone()))
        {
            conflicts.push(Conflict::UnknownComponent {
                season,
                component: component.clone(),
            });
        }

        let count_after = data.count_after();

        if let (Some(previously_used), Some(snapshot)) = (data.previously_used, data.snapshot)
            && previously_used != snapshot
        {
            conflicts.push(Conflict::PreviouslyUsedMismatch {
                season,
                car,
                component: component.clone(),
                round,
                previously_used,
                snapshot,
            });
        }

        if let (Some(conformity), Some(count_after)) = (data.conformity, count_after)
            && let Some(computed_exceeds) = allowances.exceeds(season, component, count_after)
        {
            let stated_not_in_conformity = conformity == Conformity::NotInConformity;
            if stated_not_in_conformity != computed_exceeds {
                conflicts.push(Conflict::StatedExceedanceMismatch {
                    season,
                    car,
                    component: component.clone(),
                    round,
                    stated_not_in_conformity,
                    computed_exceeds,
                });
            }
        }

        if let (Some(stated_ordinal), Some(count_after)) = (data.stated_ordinal, count_after)
            && stated_ordinal != count_after
        {
            conflicts.push(Conflict::OrdinalMismatch {
                season,
                car,
                component: component.clone(),
                round,
                stated_ordinal,
                count_after,
            });
        }
    }

    conflicts
}

/// Flag every adjacent event pair whose count after the earlier event disagrees
/// with the later event's snapshot.
///
/// One `windows(2)` over each timeline's present events, already sorted by the
/// map. A missing round leaves the events it separates adjacent; the sweep
/// checks the events it holds, not the rounds it lacks.
fn snapshot_disagreements(
    timelines: &BTreeMap<Series, BTreeMap<Round, RoundData>>,
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for ((season, car, component), rounds) in timelines {
        let ordered: Vec<(&Round, &RoundData)> = rounds.iter().collect();
        for pair in ordered.windows(2) {
            let (&from_round, from) = pair[0];
            let (&to_round, to) = pair[1];
            if let (Some(count_after), Some(next_snapshot)) = (from.count_after(), to.snapshot)
                && count_after != next_snapshot
            {
                conflicts.push(Conflict::SnapshotDisagreement {
                    season: *season,
                    car: *car,
                    component: component.clone(),
                    from_round,
                    to_round,
                    count_after,
                    next_snapshot,
                });
            }
        }
    }
    conflicts
}

/// The per-event equation: penalized elements equal the not-in-conformity set.
///
/// One map groups both sets per `(season, round)`. An event surfaces when its
/// two sets differ.
fn penalized_set_conflicts(
    timelines: &BTreeMap<Series, BTreeMap<Round, RoundData>>,
) -> Vec<Conflict> {
    #[derive(Default)]
    struct EventSets {
        penalized: BTreeSet<(Car, ComponentCode)>,
        not_in_conformity: BTreeSet<(Car, ComponentCode)>,
    }

    let mut events: BTreeMap<(Season, Round), EventSets> = BTreeMap::new();
    for ((season, car, component), rounds) in timelines {
        for (round, data) in rounds {
            if data.penalized {
                events
                    .entry((*season, *round))
                    .or_default()
                    .penalized
                    .insert((*car, component.clone()));
            }
            if data.conformity == Some(Conformity::NotInConformity) {
                events
                    .entry((*season, *round))
                    .or_default()
                    .not_in_conformity
                    .insert((*car, component.clone()));
            }
        }
    }

    events
        .into_iter()
        .filter_map(
            |(
                (season, round),
                EventSets {
                    penalized,
                    not_in_conformity,
                },
            )| {
                (penalized != not_in_conformity).then_some(Conflict::PenalizedSetMismatch {
                    season,
                    round,
                    penalized,
                    not_in_conformity,
                })
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    //! Hand-built facts drive the sweep. A clean set reports nothing; a single
    //! planted misparse surfaces as one named conflict.

    use super::*;

    const SEASON: Season = 2026;

    fn allowances() -> Allowances {
        Allowances::seed()
    }

    fn fact(round: Round, car: Car, component: &str, claim: Claim) -> Fact {
        Fact::new(SEASON, round, car, component, claim, u32::from(round))
    }

    fn snapshot(round: Round, car: Car, component: &str, count: u32) -> Fact {
        fact(round, car, component, Claim::SnapshotCount(count))
    }

    fn previously_used(round: Round, car: Car, component: &str, count: u32) -> Fact {
        fact(round, car, component, Claim::PreviouslyUsed(count))
    }

    fn fitted(round: Round, car: Car, component: &str, count: u32, conformity: Conformity) -> Fact {
        fact(
            round,
            car,
            component,
            Claim::ElementsFitted { count, conformity },
        )
    }

    fn ordinal(round: Round, car: Car, component: &str, value: u32) -> Fact {
        fact(round, car, component, Claim::StatedOrdinal(value))
    }

    fn penalty(round: Round, car: Car, component: &str) -> Fact {
        fact(
            round,
            car,
            component,
            Claim::Penalty("10 place grid drop".to_owned()),
        )
    }

    /// A coherent 2026 fixture over three events.
    ///
    /// Car 16 takes a fifth ICE at round 3 and is penalized for it; its MGU-K
    /// stays within the allowance. Car 4 stays clean throughout. Every oracle
    /// equation holds.
    fn clean_facts() -> Vec<Fact> {
        use Conformity::{InConformity, NotInConformity};
        vec![
            // Car 16, ICE: 1 -> 2 -> 3, then a fifth is fitted and penalized.
            snapshot(1, 16, "ICE", 1),
            previously_used(1, 16, "ICE", 1),
            fitted(1, 16, "ICE", 1, InConformity),
            snapshot(2, 16, "ICE", 2),
            previously_used(2, 16, "ICE", 2),
            fitted(2, 16, "ICE", 1, InConformity),
            snapshot(3, 16, "ICE", 3),
            previously_used(3, 16, "ICE", 3),
            fitted(3, 16, "ICE", 2, NotInConformity),
            ordinal(3, 16, "ICE", 5),
            penalty(3, 16, "ICE"),
            // Car 16, MGU-K: rises to the allowance of 3 without exceeding it.
            snapshot(1, 16, "MGU-K", 1),
            previously_used(1, 16, "MGU-K", 1),
            fitted(1, 16, "MGU-K", 1, InConformity),
            snapshot(2, 16, "MGU-K", 2),
            previously_used(2, 16, "MGU-K", 2),
            fitted(2, 16, "MGU-K", 1, InConformity),
            snapshot(3, 16, "MGU-K", 3),
            previously_used(3, 16, "MGU-K", 3),
            // Car 4, ICE: a clean climb, no penalty.
            snapshot(1, 4, "ICE", 0),
            previously_used(1, 4, "ICE", 0),
            fitted(1, 4, "ICE", 1, InConformity),
            snapshot(2, 4, "ICE", 1),
            previously_used(2, 4, "ICE", 1),
            fitted(2, 4, "ICE", 1, InConformity),
            snapshot(3, 4, "ICE", 2),
            previously_used(3, 4, "ICE", 2),
            fitted(3, 4, "ICE", 1, InConformity),
        ]
    }

    /// Mutable access to the facts matching one `(round, car, component)`.
    fn matching<'a>(
        facts: &'a mut [Fact],
        round: Round,
        car: Car,
        component: &'a str,
    ) -> impl Iterator<Item = &'a mut Fact> {
        facts.iter_mut().filter(move |fact| {
            fact.round == round && fact.car == car && fact.component.as_str() == component
        })
    }

    #[test]
    fn clean_facts_report_no_conflicts() {
        assert_eq!(sweep(&clean_facts(), &allowances()), Vec::new());
    }

    #[test]
    fn a_wrong_fitted_count_breaks_the_next_snapshot() {
        let mut facts = clean_facts();
        for fact in matching(&mut facts, 2, 16, "ICE") {
            if let Claim::ElementsFitted { count, .. } = &mut fact.claim {
                *count = 2;
            }
        }

        let conflicts = sweep(&facts, &allowances());

        assert_eq!(conflicts.len(), 1);
        assert!(matches!(
            conflicts[0],
            Conflict::SnapshotDisagreement {
                from_round: 2,
                to_round: 3,
                ..
            }
        ));
    }

    #[test]
    fn an_infringement_ordinal_that_disagrees_is_a_conflict() {
        let mut facts = clean_facts();
        for fact in matching(&mut facts, 3, 16, "ICE") {
            if let Claim::StatedOrdinal(value) = &mut fact.claim {
                *value = 4;
            }
        }

        let conflicts = sweep(&facts, &allowances());

        assert_eq!(conflicts.len(), 1);
        assert!(matches!(
            conflicts[0],
            Conflict::OrdinalMismatch {
                stated_ordinal: 4,
                count_after: 5,
                ..
            }
        ));
    }

    #[test]
    fn a_wrong_previously_used_figure_is_a_conflict() {
        let mut facts = clean_facts();
        for fact in matching(&mut facts, 2, 16, "ICE") {
            if let Claim::PreviouslyUsed(count) = &mut fact.claim {
                *count = 9;
            }
        }

        let conflicts = sweep(&facts, &allowances());

        assert_eq!(conflicts.len(), 1);
        assert!(matches!(
            conflicts[0],
            Conflict::PreviouslyUsedMismatch {
                previously_used: 9,
                snapshot: 2,
                ..
            }
        ));
    }

    #[test]
    fn a_conformity_verdict_against_the_count_is_a_conflict() {
        let mut facts = clean_facts();
        // Car 4's ICE reaches only 3 of 4 at round 3, yet claim it out of
        // conformity. The claim contradicts the count, but no penalty follows,
        // so only the exceedance witness clashes.
        for fact in matching(&mut facts, 3, 4, "ICE") {
            if let Claim::ElementsFitted { conformity, .. } = &mut fact.claim {
                *conformity = Conformity::NotInConformity;
            }
        }

        let conflicts = sweep(&facts, &allowances());

        assert!(conflicts.iter().any(|conflict| matches!(
            conflict,
            Conflict::StatedExceedanceMismatch {
                car: 4,
                stated_not_in_conformity: true,
                computed_exceeds: false,
                ..
            }
        )));
    }

    #[test]
    fn a_penalty_without_a_not_in_conformity_flag_is_a_conflict() {
        let mut facts = clean_facts();
        // Drop the not-in-conformity flag from the element the infringement
        // still penalizes: the two sets no longer agree.
        for fact in matching(&mut facts, 3, 16, "ICE") {
            if let Claim::ElementsFitted { conformity, .. } = &mut fact.claim {
                *conformity = Conformity::InConformity;
            }
        }

        let conflicts = sweep(&facts, &allowances());

        assert!(
            conflicts.iter().any(|conflict| matches!(
                conflict,
                Conflict::PenalizedSetMismatch { round: 3, .. }
            ))
        );
    }

    #[test]
    fn a_component_with_no_seeded_allowance_is_a_conflict() {
        let mut facts = clean_facts();
        facts.push(snapshot(1, 16, "GEARBOX", 1));

        let conflicts = sweep(&facts, &allowances());

        assert_eq!(conflicts.len(), 1);
        assert!(matches!(
            &conflicts[0],
            Conflict::UnknownComponent { season: 2026, component } if component.as_str() == "GEARBOX"
        ));
    }

    #[test]
    fn a_superseded_contradiction_raises_no_conflict() {
        let mut facts = clean_facts();
        facts.push(Fact {
            superseded: true,
            ..snapshot(3, 16, "ICE", 99)
        });

        assert_eq!(sweep(&facts, &allowances()), Vec::new());
    }

    #[test]
    fn the_same_contradiction_left_live_surfaces() {
        let mut facts = clean_facts();
        // The identical fact, unmarked, overwrites the good snapshot and blows
        // the equations, proving the superseded skip is not a vacuous pass.
        facts.push(snapshot(3, 16, "ICE", 99));

        assert!(!sweep(&facts, &allowances()).is_empty());
    }

    #[test]
    fn exceedance_flags_a_count_above_the_allowance() {
        assert_eq!(
            allowances().exceeds(2026, &ComponentCode::new("ICE"), 5),
            Some(true)
        );
    }

    #[test]
    fn exceedance_clears_a_count_at_the_allowance() {
        assert_eq!(
            allowances().exceeds(2026, &ComponentCode::new("ICE"), 4),
            Some(false)
        );
    }

    #[test]
    fn exceedance_clears_a_count_below_the_allowance() {
        assert_eq!(
            allowances().exceeds(2026, &ComponentCode::new("ICE"), 3),
            Some(false)
        );
    }

    #[test]
    fn exceedance_is_unknown_for_an_unseeded_component() {
        assert_eq!(
            allowances().exceeds(2026, &ComponentCode::new("GEARBOX"), 1),
            None
        );
    }
}
