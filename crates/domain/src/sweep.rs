//! The invariant sweep: the oracle that cross-checks facts against each other.
//!
//! Three independent FIA documents state the same running count. The snapshot
//! publishes before an event, the new-elements document lands after it, and an
//! infringement restates the count of the element it penalizes. The sweep folds
//! the facts into per-component timelines and proves the oracle equations over
//! them, flagging every disagreement rather than guessing which witness is right.
//!
//! It folds by seat, never by car number. A running count belongs to a team's
//! car entry, so a mid-season swap or a substitution hands it to whoever fills
//! the seat next. Keyed by car number the same swap merges two seats into one
//! total and fires a previously-used mismatch and a snapshot disagreement on
//! correct FIA data. [`resolve_seats`](crate::resolve_seats) builds the seat map
//! from the rosters; the sweep only reads it, and reports every diff the
//! resolver refused to guess rather than folding past it.
//!
//! Each document seats on the roster of the event it describes. A snapshot at
//! event N states the count reached before N's own parts, so it carries the
//! entry list of N-1 and seats on that roster; the new-elements and infringement
//! documents at N describe N itself and seat on N's roster. The season's first
//! event has nothing before it, so a snapshot there seats on its own roster. The
//! wrong roster misseats a swapped car.
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
//!   new-elements document flags not in conformity;
//! - the team a document prints equals the team the roster it seated on entered
//!   the car for.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    allowance::Allowances,
    fact::{Car, Claim, ComponentCode, Conformity, Fact, Round, Season, Team},
    seat::{Seat, SeatAmbiguity, Seats},
};

/// The first event of a season: the one a snapshot cannot look back past.
const FIRST_ROUND: Round = 1;

/// A disagreement the sweep found between independent witnesses.
///
/// Each variant names where the disagreement sits and the two figures that
/// clash, so a conflict is actionable and never a bare boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// The count after one event does not equal the next event's snapshot.
    SnapshotDisagreement {
        /// The seat whose totals clash.
        seat: Seat,
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
        /// The seat whose figures clash.
        seat: Seat,
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
        /// The seat under test.
        seat: Seat,
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
        /// The seat the infringement belongs to.
        seat: Seat,
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
    /// is a `(seat, component)` pair.
    PenalizedSetMismatch {
        /// The season the event belongs to.
        season: Season,
        /// The event.
        round: Round,
        /// The elements infringements penalize.
        penalized: BTreeSet<(Seat, ComponentCode)>,
        /// The elements the new-elements document flags not in conformity.
        not_in_conformity: BTreeSet<(Seat, ComponentCode)>,
    },
    /// A fact references a component the season never seeds, so no allowance
    /// exists to check it against (a season's valid components are
    /// exactly its seeded rows).
    UnknownComponent {
        /// The season the fact belongs to.
        season: Season,
        /// The unseeded component.
        component: ComponentCode,
    },
    /// The rosters seat no car of this number at the event the document
    /// describes, so the fact has no timeline to join. Reported once per
    /// `(document event, roster event, car)`: the window may hold no roster for
    /// the event consulted, the car may not have entered it, or its team's
    /// lineage may have stopped.
    UnknownSeat {
        /// The season the fact belongs to.
        season: Season,
        /// The event the document belongs to.
        document_round: Round,
        /// The event whose roster the sweep consulted.
        roster_round: Round,
        /// The car number the document prints.
        car: Car,
    },
    /// The seat resolver could not pair a team's roster diff, so it seated none
    /// of that team's cars from this event on. Surfaced whether or not a fact
    /// landed on the team: a diff nobody can pair is a mapping for a human to
    /// supply, never a guess.
    AmbiguousSeat(SeatAmbiguity),
    /// A document names a team the roster it seated on did not enter the car
    /// for. A snapshot is checked against the roster of the previous event, a
    /// new-elements or infringement document against its own. Reported once per
    /// `(document event, roster event, car)`, however many components the
    /// document's row covers. Two documents at one event that read different
    /// rosters stay two witnesses.
    PrintedTeamMismatch {
        /// The season the fact belongs to.
        season: Season,
        /// The event the document belongs to.
        document_round: Round,
        /// The event whose roster the sweep consulted.
        roster_round: Round,
        /// The car number the document prints.
        car: Car,
        /// The team the document prints.
        printed_team: Team,
        /// The team that roster entered the car for.
        roster_team: Team,
    },
}

/// What every claim about one `(seat, component)` at one event folds to.
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
            .map(|snapshot| snapshot.saturating_add(self.fitted.unwrap_or(0)))
    }
}

/// The `(seat, component)` a timeline belongs to.
type Series<'a> = (&'a Seat, &'a ComponentCode);

/// Every seated series, each holding the events it has data for.
type Timelines<'a> = BTreeMap<Series<'a>, BTreeMap<Round, RoundData>>;

/// Cross-check `facts` against `allowances` and the seat map, and return every
/// conflict found.
///
/// `seats` comes from [`resolve_seats`](crate::resolve_seats) over the rosters
/// covering the events the facts describe. A snapshot describes the event before
/// its own, so the window must reach one event back from the earliest snapshot,
/// unless that snapshot sits at the season's first event. A window that opens
/// later strands those snapshots: each comes back as
/// [`UnknownSeat`](Conflict::UnknownSeat) naming the roster event the window
/// lacks. The sweep never falls back to the nearest earlier roster, which would
/// silently misseat a car that moved in the gap.
///
/// Superseded facts are skipped. A clean set of facts over rosters the resolver
/// could pair returns an empty vector. The conflicts come back in a
/// deterministic order.
#[must_use]
pub fn sweep(facts: &[Fact], allowances: &Allowances, seats: &Seats) -> Vec<Conflict> {
    let Seated {
        timelines,
        conflicts: seating,
    } = seat_live_facts(facts, seats);

    let mut conflicts: Vec<Conflict> = ambiguous_seats(seats).collect();
    conflicts.extend(seating);
    conflicts.extend(local_conflicts(&timelines, allowances));
    conflicts.extend(snapshot_disagreements(&timelines));
    conflicts.extend(penalized_set_conflicts(&timelines));
    conflicts
}

/// The live facts folded onto their seats, and the conflicts seating them
/// raised.
struct Seated<'a> {
    timelines: Timelines<'a>,
    conflicts: Vec<Conflict>,
}

/// The event whose roster seats the car a fact's document prints.
///
/// A snapshot states the count reached before its own event's parts, so it
/// carries the entry list of the event before it. Every other claim comes from a
/// document about its own event. A season's first event has nothing before it,
/// so a snapshot there falls back to its own roster.
fn seating_round(fact: &Fact) -> Round {
    match fact.claim {
        Claim::SnapshotCount(_) if fact.round > FIRST_ROUND => fact.round - 1,
        // Every variant listed and no wildcard closing the match, so a new claim
        // kind must state which event it describes before this compiles.
        Claim::SnapshotCount(_)
        | Claim::ElementsFitted { .. }
        | Claim::PreviouslyUsed(_)
        | Claim::StatedOrdinal(_)
        | Claim::Penalty(_) => fact.round,
    }
}

/// Fold the live facts onto the seats the rosters give them, flagging every fact
/// the map cannot seat and every printed team the map contradicts.
///
/// A fact that cannot be seated joins no timeline, so leaving it unreported
/// would drop it from every equation and pass silently.
///
/// Both dedupe keys carry the roster event alongside the document's own, so a
/// snapshot and a new-elements document at one event stay separate.
fn seat_live_facts<'a>(facts: &'a [Fact], seats: &'a Seats) -> Seated<'a> {
    let mut timelines = Timelines::new();
    let mut conflicts = Vec::new();
    let mut unseated: BTreeSet<(Season, Round, Round, Car)> = BTreeSet::new();
    let mut misprinted: BTreeSet<(Season, Round, Round, Car, &Team, &Team)> = BTreeSet::new();

    for fact in facts.iter().filter(|fact| !fact.superseded) {
        let roster_round = seating_round(fact);

        let Some(seat) = seats.seat(fact.season, roster_round, fact.car) else {
            if unseated.insert((fact.season, fact.round, roster_round, fact.car)) {
                conflicts.push(Conflict::UnknownSeat {
                    season: fact.season,
                    document_round: fact.round,
                    roster_round,
                    car: fact.car,
                });
            }
            continue;
        };

        if let Some(printed) = fact.printed_team.as_ref()
            && printed != &seat.team
            && misprinted.insert((
                fact.season,
                fact.round,
                roster_round,
                fact.car,
                printed,
                &seat.team,
            ))
        {
            conflicts.push(Conflict::PrintedTeamMismatch {
                season: fact.season,
                document_round: fact.round,
                roster_round,
                car: fact.car,
                printed_team: printed.clone(),
                roster_team: seat.team.clone(),
            });
        }

        timelines
            .entry((seat, &fact.component))
            .or_default()
            .entry(fact.round)
            .or_default()
            .absorb(&fact.claim);
    }

    Seated {
        timelines,
        conflicts,
    }
}

/// Every roster diff the seat resolver refused to guess.
fn ambiguous_seats(seats: &Seats) -> impl Iterator<Item = Conflict> {
    seats
        .ambiguities()
        .iter()
        .cloned()
        .map(Conflict::AmbiguousSeat)
}

/// Flag the conflicts one event proves on its own: an unseeded component, a
/// previously-used figure that disagrees with the snapshot, a stated conformity
/// that disagrees with the computed exceedance, and a stated ordinal that
/// disagrees with the count after the event.
///
/// One flat pass over every `(series, round)` cell. Each unseeded
/// `(season, component)` yields one `UnknownComponent`, however many events
/// reference it.
fn local_conflicts<'a>(timelines: &Timelines<'a>, allowances: &Allowances) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    let mut seen_unknown: BTreeSet<(Season, &'a ComponentCode)> = BTreeSet::new();

    let cells = timelines.iter().flat_map(|(&(seat, component), rounds)| {
        rounds
            .iter()
            .map(move |(&round, data)| (seat, component, round, data))
    });

    for (seat, component, round, data) in cells {
        if allowances.allowance(seat.season, component).is_none()
            && seen_unknown.insert((seat.season, component))
        {
            conflicts.push(Conflict::UnknownComponent {
                season: seat.season,
                component: component.clone(),
            });
        }

        let count_after = data.count_after();

        if let (Some(previously_used), Some(snapshot)) = (data.previously_used, data.snapshot)
            && previously_used != snapshot
        {
            conflicts.push(Conflict::PreviouslyUsedMismatch {
                seat: seat.clone(),
                component: component.clone(),
                round,
                previously_used,
                snapshot,
            });
        }

        if let (Some(conformity), Some(count_after)) = (data.conformity, count_after)
            && let Some(computed_exceeds) = allowances.exceeds(seat.season, component, count_after)
        {
            let stated_not_in_conformity = conformity == Conformity::NotInConformity;
            if stated_not_in_conformity != computed_exceeds {
                conflicts.push(Conflict::StatedExceedanceMismatch {
                    seat: seat.clone(),
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
                seat: seat.clone(),
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
fn snapshot_disagreements(timelines: &Timelines<'_>) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for (&(seat, component), rounds) in timelines {
        let ordered: Vec<(&Round, &RoundData)> = rounds.iter().collect();
        for pair in ordered.windows(2) {
            let (&from_round, from) = pair[0];
            let (&to_round, to) = pair[1];
            if let (Some(count_after), Some(next_snapshot)) = (from.count_after(), to.snapshot)
                && count_after != next_snapshot
            {
                conflicts.push(Conflict::SnapshotDisagreement {
                    seat: seat.clone(),
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
/// two sets differ, and only then does either set need owning.
fn penalized_set_conflicts(timelines: &Timelines<'_>) -> Vec<Conflict> {
    #[derive(Default)]
    struct EventSets<'a> {
        penalized: BTreeSet<Series<'a>>,
        not_in_conformity: BTreeSet<Series<'a>>,
    }

    fn owned(elements: BTreeSet<Series<'_>>) -> BTreeSet<(Seat, ComponentCode)> {
        elements
            .into_iter()
            .map(|(seat, component)| (seat.clone(), component.clone()))
            .collect()
    }

    let mut events: BTreeMap<(Season, Round), EventSets<'_>> = BTreeMap::new();
    for (&(seat, component), rounds) in timelines {
        for (&round, data) in rounds {
            if data.penalized {
                events
                    .entry((seat.season, round))
                    .or_default()
                    .penalized
                    .insert((seat, component));
            }
            if data.conformity == Some(Conformity::NotInConformity) {
                events
                    .entry((seat.season, round))
                    .or_default()
                    .not_in_conformity
                    .insert((seat, component));
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
                (penalized != not_in_conformity).then(|| Conflict::PenalizedSetMismatch {
                    season,
                    round,
                    penalized: owned(penalized),
                    not_in_conformity: owned(not_in_conformity),
                })
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    //! Hand-built rosters and facts drive the sweep. A clean set reports
    //! nothing, and a single planted misparse surfaces as one named conflict.
    //! The fixture swaps two cars between teams and substitutes a third
    //! mid-season, so the same facts read against rosters that never record the
    //! moves raise the false conflicts the seat fold removes.

    use super::*;

    use crate::seat::{Roster, RosterEntry, Slot, resolve_seats};

    const SEASON: Season = 2026;
    const RED_BULL: &str = "Red Bull";
    const RACING_BULLS: &str = "Racing Bulls";
    const FERRARI: &str = "Ferrari";

    /// Every fact in the fixture is about the ICE, the component whose count the
    /// swap and the substitution carry across.
    const ICE: &str = "ICE";

    fn entry(car: Car, driver: &str, team: &str) -> RosterEntry {
        RosterEntry {
            car,
            driver: driver.to_owned(),
            team: team.into(),
        }
    }

    fn roster(round: Round, entries: Vec<RosterEntry>) -> Roster {
        Roster {
            season: SEASON,
            round,
            entries,
        }
    }

    fn seat(team: &str, slot: Slot) -> Seat {
        Seat {
            season: SEASON,
            team: team.into(),
            slot,
        }
    }

    /// The grid rounds 1 and 2 run: three teams, two cars each.
    fn opening_grid() -> Vec<RosterEntry> {
        vec![
            entry(1, "Verstappen", RED_BULL),
            entry(30, "Lawson", RED_BULL),
            entry(6, "Hadjar", RACING_BULLS),
            entry(22, "Tsunoda", RACING_BULLS),
            entry(16, "Leclerc", FERRARI),
            entry(44, "Hamilton", FERRARI),
        ]
    }

    /// The grid rounds 3 and 4 run: cars 22 and 30 swap teams, and car 43
    /// substitutes for car 44.
    fn swapped_grid() -> Vec<RosterEntry> {
        vec![
            entry(1, "Verstappen", RED_BULL),
            entry(22, "Tsunoda", RED_BULL),
            entry(6, "Hadjar", RACING_BULLS),
            entry(30, "Lawson", RACING_BULLS),
            entry(16, "Leclerc", FERRARI),
            entry(43, "Colapinto", FERRARI),
        ]
    }

    /// The four events the facts cover, with the moves taking effect at round 3.
    fn window() -> Vec<Roster> {
        vec![
            roster(1, opening_grid()),
            roster(2, opening_grid()),
            roster(3, swapped_grid()),
            roster(4, swapped_grid()),
        ]
    }

    fn seats() -> Seats {
        resolve_seats(&window())
    }

    /// The same four events with the opening grid throughout: rosters that never
    /// record the moves. Each of the six cars on that grid then holds one seat
    /// all season, so for them the sweep folds by car number and the seat key
    /// buys nothing. Car 43 enters no roster here: the substitute's facts drop
    /// out unseated.
    fn unchanged_seats() -> Seats {
        resolve_seats(&[
            roster(1, opening_grid()),
            roster(2, opening_grid()),
            roster(3, opening_grid()),
            roster(4, opening_grid()),
        ])
    }

    fn allowances() -> Allowances {
        Allowances::seed()
    }

    fn fact(round: Round, car: Car, team: &str, component: &str, claim: Claim) -> Fact {
        Fact::new(SEASON, round, car, component, claim, u32::from(round)).with_printed_team(team)
    }

    fn snapshot(round: Round, car: Car, team: &str, count: u32) -> Fact {
        fact(round, car, team, ICE, Claim::SnapshotCount(count))
    }

    fn previously_used(round: Round, car: Car, team: &str, count: u32) -> Fact {
        fact(round, car, team, ICE, Claim::PreviouslyUsed(count))
    }

    fn fitted(round: Round, car: Car, team: &str, count: u32, conformity: Conformity) -> Fact {
        fact(
            round,
            car,
            team,
            ICE,
            Claim::ElementsFitted { count, conformity },
        )
    }

    fn ordinal(round: Round, car: Car, team: &str, value: u32) -> Fact {
        fact(round, car, team, ICE, Claim::StatedOrdinal(value))
    }

    fn penalty(round: Round, car: Car, team: &str) -> Fact {
        fact(
            round,
            car,
            team,
            ICE,
            Claim::Penalty("10 place grid drop".to_owned()),
        )
    }

    /// A coherent 2026 fixture over the window's four events, for four seats.
    ///
    /// Red Bull's second seat climbs to three ICEs and Racing Bulls' to two, so
    /// the two seats the swap crosses hold different totals. Ferrari's second
    /// seat passes to a substitute at round 3. Ferrari's first seat takes a
    /// fifth ICE at round 3 and is penalized for it. Every oracle equation
    /// holds.
    ///
    /// Each round's snapshot prints the entry list of the round before it, and
    /// each round's new-elements document prints its own.
    fn clean_facts() -> Vec<Fact> {
        use Conformity::{InConformity, NotInConformity};
        vec![
            // Round 1. Nothing precedes it, so its snapshot prints its own grid.
            snapshot(1, 30, RED_BULL, 0),
            previously_used(1, 30, RED_BULL, 0),
            fitted(1, 30, RED_BULL, 1, InConformity),
            snapshot(1, 22, RACING_BULLS, 0),
            previously_used(1, 22, RACING_BULLS, 0),
            fitted(1, 22, RACING_BULLS, 1, InConformity),
            snapshot(1, 44, FERRARI, 0),
            previously_used(1, 44, FERRARI, 0),
            fitted(1, 44, FERRARI, 1, InConformity),
            snapshot(1, 16, FERRARI, 1),
            previously_used(1, 16, FERRARI, 1),
            fitted(1, 16, FERRARI, 1, InConformity),
            // Round 2. Car 22 fits nothing, so the new-elements document has no
            // row for it and its seat's total stands.
            snapshot(2, 30, RED_BULL, 1),
            previously_used(2, 30, RED_BULL, 1),
            fitted(2, 30, RED_BULL, 1, InConformity),
            snapshot(2, 22, RACING_BULLS, 1),
            snapshot(2, 44, FERRARI, 1),
            previously_used(2, 44, FERRARI, 1),
            fitted(2, 44, FERRARI, 1, InConformity),
            snapshot(2, 16, FERRARI, 2),
            previously_used(2, 16, FERRARI, 2),
            fitted(2, 16, FERRARI, 1, InConformity),
            // Round 3, where the moves take effect. The snapshot still prints
            // round 2's grid and totals; the new-elements document prints
            // round 3's grid, and each arriving car takes over the total of the
            // seat it fills.
            snapshot(3, 30, RED_BULL, 2),
            snapshot(3, 22, RACING_BULLS, 1),
            snapshot(3, 44, FERRARI, 2),
            snapshot(3, 16, FERRARI, 3),
            previously_used(3, 22, RED_BULL, 2),
            fitted(3, 22, RED_BULL, 1, InConformity),
            previously_used(3, 30, RACING_BULLS, 1),
            fitted(3, 30, RACING_BULLS, 1, InConformity),
            previously_used(3, 43, FERRARI, 2),
            fitted(3, 43, FERRARI, 1, InConformity),
            previously_used(3, 16, FERRARI, 3),
            fitted(3, 16, FERRARI, 2, NotInConformity),
            ordinal(3, 16, FERRARI, 5),
            penalty(3, 16, FERRARI),
            // Round 4. Its snapshot closes every seat's round 3 total.
            snapshot(4, 22, RED_BULL, 3),
            snapshot(4, 30, RACING_BULLS, 2),
            snapshot(4, 43, FERRARI, 3),
            snapshot(4, 16, FERRARI, 5),
        ]
    }

    /// Mutable access to the facts matching one `(round, car)` ICE cell.
    fn matching(facts: &mut [Fact], round: Round, car: Car) -> impl Iterator<Item = &mut Fact> {
        facts.iter_mut().filter(move |fact| {
            fact.round == round && fact.car == car && fact.component.as_str() == ICE
        })
    }

    #[test]
    fn clean_facts_over_the_rosters_that_record_the_moves_report_no_conflicts() {
        assert_eq!(sweep(&clean_facts(), &allowances(), &seats()), Vec::new());
    }

    #[test]
    fn rosters_that_miss_the_swap_raise_a_false_previously_used_mismatch() {
        let conflicts = sweep(&clean_facts(), &allowances(), &unchanged_seats());

        assert!(
            conflicts.contains(&Conflict::PreviouslyUsedMismatch {
                seat: seat(RACING_BULLS, 1),
                component: ComponentCode::new(ICE),
                round: 3,
                previously_used: 2,
                snapshot: 1,
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn rosters_that_miss_the_swap_raise_a_false_snapshot_disagreement() {
        let conflicts = sweep(&clean_facts(), &allowances(), &unchanged_seats());

        assert!(
            conflicts.contains(&Conflict::SnapshotDisagreement {
                seat: seat(RACING_BULLS, 1),
                component: ComponentCode::new(ICE),
                from_round: 3,
                to_round: 4,
                count_after: 2,
                next_snapshot: 3,
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn a_substitutes_previously_used_figure_is_read_against_the_seat_it_fills() {
        let mut facts = clean_facts();
        // Car 43 enters at round 3 with nothing of its own, so its stated
        // "previously used" can only come from the seat car 44 left.
        for fact in matching(&mut facts, 3, 43) {
            if let Claim::PreviouslyUsed(count) = &mut fact.claim {
                *count = 0;
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert!(
            conflicts.contains(&Conflict::PreviouslyUsedMismatch {
                seat: seat(FERRARI, 1),
                component: ComponentCode::new(ICE),
                round: 3,
                previously_used: 0,
                snapshot: 2,
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn a_substitutes_fitting_adds_to_the_seat_it_fills() {
        let mut facts = clean_facts();
        for fact in matching(&mut facts, 3, 43) {
            if let Claim::ElementsFitted { count, .. } = &mut fact.claim {
                *count = 2;
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert!(
            conflicts.contains(&Conflict::SnapshotDisagreement {
                seat: seat(FERRARI, 1),
                component: ComponentCode::new(ICE),
                from_round: 3,
                to_round: 4,
                count_after: 4,
                next_snapshot: 3,
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn a_seasons_first_snapshot_seats_on_its_own_roster() {
        assert_eq!(
            sweep(&[snapshot(1, 30, RED_BULL, 0)], &allowances(), &seats()),
            Vec::new()
        );
    }

    #[test]
    fn a_snapshot_naming_the_team_the_car_moves_to_is_a_conflict() {
        let mut facts = clean_facts();
        // Round 3's snapshot describes round 2, where car 22 was still a Racing
        // Bulls entry.
        for fact in matching(&mut facts, 3, 22) {
            if matches!(fact.claim, Claim::SnapshotCount(_)) {
                fact.printed_team = Some(RED_BULL.into());
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert!(
            conflicts.contains(&Conflict::PrintedTeamMismatch {
                season: SEASON,
                document_round: 3,
                roster_round: 2,
                car: 22,
                printed_team: RED_BULL.into(),
                roster_team: RACING_BULLS.into(),
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn a_new_elements_document_naming_the_team_the_car_left_is_a_conflict() {
        let mut facts = clean_facts();
        // Round 3's new-elements document describes round 3, where car 22 is a
        // Red Bull entry.
        for fact in matching(&mut facts, 3, 22) {
            if matches!(fact.claim, Claim::ElementsFitted { .. }) {
                fact.printed_team = Some(RACING_BULLS.into());
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert!(
            conflicts.contains(&Conflict::PrintedTeamMismatch {
                season: SEASON,
                document_round: 3,
                roster_round: 3,
                car: 22,
                printed_team: RACING_BULLS.into(),
                roster_team: RED_BULL.into(),
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn two_documents_at_one_event_reading_different_rosters_are_two_conflicts() {
        let mut facts = clean_facts();
        // Car 30 is a Red Bull entry at rounds 1 and 2, so round 2's snapshot and
        // its new-elements document read different rosters that agree on the
        // team. Both misprint it, and each stays its own witness.
        for fact in matching(&mut facts, 2, 30) {
            fact.printed_team = Some(FERRARI.into());
        }

        let conflicts = sweep(&facts, &allowances(), &seats());
        let misprints: Vec<&Conflict> = conflicts
            .iter()
            .filter(|conflict| matches!(conflict, Conflict::PrintedTeamMismatch { .. }))
            .collect();

        assert_eq!(
            misprints,
            vec![
                &Conflict::PrintedTeamMismatch {
                    season: SEASON,
                    document_round: 2,
                    roster_round: 1,
                    car: 30,
                    printed_team: FERRARI.into(),
                    roster_team: RED_BULL.into(),
                },
                &Conflict::PrintedTeamMismatch {
                    season: SEASON,
                    document_round: 2,
                    roster_round: 2,
                    car: 30,
                    printed_team: FERRARI.into(),
                    roster_team: RED_BULL.into(),
                },
            ]
        );
    }

    #[test]
    fn a_diff_the_resolver_could_not_pair_is_a_conflict() {
        // Both Red Bull occupants change at round 2, so no pairing follows from
        // the diff.
        let unpairable = resolve_seats(&[
            roster(1, opening_grid()),
            roster(
                2,
                vec![
                    entry(5, "Bortoleto", RED_BULL),
                    entry(7, "Doohan", RED_BULL),
                ],
            ),
        ]);

        assert_eq!(
            sweep(&[], &allowances(), &unpairable),
            vec![Conflict::AmbiguousSeat(SeatAmbiguity {
                season: SEASON,
                round: 2,
                team: RED_BULL.into(),
                vacated: vec![0, 1],
                arriving: vec![5, 7],
            })]
        );
    }

    #[test]
    fn a_fact_the_rosters_cannot_seat_is_a_conflict() {
        let mut facts = clean_facts();
        facts.push(snapshot(1, 77, "Alpine", 0));

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert_eq!(
            conflicts,
            vec![Conflict::UnknownSeat {
                season: SEASON,
                document_round: 1,
                roster_round: 1,
                car: 77,
            }]
        );
    }

    #[test]
    fn a_window_opening_at_a_snapshots_own_event_strands_it() {
        // Rounds 3 and 4 only: the round 3 snapshot describes round 2, whose
        // roster the window does not hold.
        let short = resolve_seats(&[roster(3, swapped_grid()), roster(4, swapped_grid())]);

        let conflicts = sweep(&[snapshot(3, 22, RACING_BULLS, 1)], &allowances(), &short);

        assert_eq!(
            conflicts,
            vec![Conflict::UnknownSeat {
                season: SEASON,
                document_round: 3,
                roster_round: 2,
                car: 22,
            }]
        );
    }

    #[test]
    fn a_wrong_fitted_count_breaks_the_next_snapshot() {
        let mut facts = clean_facts();
        for fact in matching(&mut facts, 2, 30) {
            if let Claim::ElementsFitted { count, .. } = &mut fact.claim {
                *count = 2;
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert_eq!(
            conflicts,
            vec![Conflict::SnapshotDisagreement {
                seat: seat(RED_BULL, 1),
                component: ComponentCode::new(ICE),
                from_round: 2,
                to_round: 3,
                count_after: 3,
                next_snapshot: 2,
            }]
        );
    }

    #[test]
    fn an_infringement_ordinal_that_disagrees_is_a_conflict() {
        let mut facts = clean_facts();
        for fact in matching(&mut facts, 3, 16) {
            if let Claim::StatedOrdinal(value) = &mut fact.claim {
                *value = 4;
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert_eq!(
            conflicts,
            vec![Conflict::OrdinalMismatch {
                seat: seat(FERRARI, 0),
                component: ComponentCode::new(ICE),
                round: 3,
                stated_ordinal: 4,
                count_after: 5,
            }]
        );
    }

    #[test]
    fn a_wrong_previously_used_figure_is_a_conflict() {
        let mut facts = clean_facts();
        for fact in matching(&mut facts, 2, 30) {
            if let Claim::PreviouslyUsed(count) = &mut fact.claim {
                *count = 9;
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert_eq!(
            conflicts,
            vec![Conflict::PreviouslyUsedMismatch {
                seat: seat(RED_BULL, 1),
                component: ComponentCode::new(ICE),
                round: 2,
                previously_used: 9,
                snapshot: 1,
            }]
        );
    }

    #[test]
    fn a_conformity_verdict_against_the_count_is_a_conflict() {
        let mut facts = clean_facts();
        // Red Bull's second seat reaches only 3 of 4 at round 3, yet claim it
        // out of conformity. The claim contradicts the count, but no penalty
        // follows, so the exceedance witness and the penalized set both clash.
        for fact in matching(&mut facts, 3, 22) {
            if let Claim::ElementsFitted { conformity, .. } = &mut fact.claim {
                *conformity = Conformity::NotInConformity;
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert!(
            conflicts.contains(&Conflict::StatedExceedanceMismatch {
                seat: seat(RED_BULL, 1),
                component: ComponentCode::new(ICE),
                round: 3,
                stated_not_in_conformity: true,
                computed_exceeds: false,
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn a_penalty_without_a_not_in_conformity_flag_is_a_conflict() {
        let mut facts = clean_facts();
        // Drop the not-in-conformity flag from the element the infringement
        // still penalizes: the two sets no longer agree.
        for fact in matching(&mut facts, 3, 16) {
            if let Claim::ElementsFitted { conformity, .. } = &mut fact.claim {
                *conformity = Conformity::InConformity;
            }
        }

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert!(
            conflicts.contains(&Conflict::PenalizedSetMismatch {
                season: SEASON,
                round: 3,
                penalized: BTreeSet::from([(seat(FERRARI, 0), ComponentCode::new(ICE))]),
                not_in_conformity: BTreeSet::new(),
            }),
            "{conflicts:?}"
        );
    }

    #[test]
    fn a_component_with_no_seeded_allowance_is_a_conflict() {
        let mut facts = clean_facts();
        facts.push(fact(1, 16, FERRARI, "GEARBOX", Claim::SnapshotCount(1)));

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert_eq!(
            conflicts,
            vec![Conflict::UnknownComponent {
                season: SEASON,
                component: ComponentCode::new("GEARBOX"),
            }]
        );
    }

    #[test]
    fn a_superseded_contradiction_raises_no_conflict() {
        let mut facts = clean_facts();
        facts.push(Fact {
            superseded: true,
            ..snapshot(3, 30, RED_BULL, 99)
        });

        assert_eq!(sweep(&facts, &allowances(), &seats()), Vec::new());
    }

    #[test]
    fn the_same_contradiction_left_live_surfaces() {
        let mut facts = clean_facts();
        // The identical fact, unmarked, overwrites the good snapshot and blows
        // the equations, proving the superseded skip is not a vacuous pass.
        facts.push(snapshot(3, 30, RED_BULL, 99));

        assert!(!sweep(&facts, &allowances(), &seats()).is_empty());
    }

    #[test]
    fn exceedance_flags_a_count_above_the_allowance() {
        assert_eq!(
            allowances().exceeds(SEASON, &ComponentCode::new(ICE), 5),
            Some(true)
        );
    }

    #[test]
    fn exceedance_clears_a_count_at_the_allowance() {
        assert_eq!(
            allowances().exceeds(SEASON, &ComponentCode::new(ICE), 4),
            Some(false)
        );
    }

    #[test]
    fn exceedance_clears_a_count_below_the_allowance() {
        assert_eq!(
            allowances().exceeds(SEASON, &ComponentCode::new(ICE), 3),
            Some(false)
        );
    }

    #[test]
    fn exceedance_is_unknown_for_an_unseeded_component() {
        assert_eq!(
            allowances().exceeds(SEASON, &ComponentCode::new("GEARBOX"), 1),
            None
        );
    }
}
