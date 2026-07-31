//! The invariant sweep: the oracle that cross-checks facts against each other.
//!
//! Three independent FIA documents state the same running count. The snapshot
//! publishes before an event, the new-elements document lands after it, and an
//! infringement restates the count of the element it penalizes. The sweep folds
//! the facts into per-component timelines and proves the oracle equations over
//! them, flagging every disagreement rather than guessing which witness is right.
//!
//! One call covers one season. Counts reset each season and every caller holds
//! a single one, so nothing here carries a season and nothing checks for one.
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
//! The printed team is checked by grouping, and no team name is ever compared to
//! another source's. The documents print "Red Bull Racing Honda RBPT" where the
//! roster feed returns "Red Bull Racing", so name equality would fire on nearly
//! every row and bury the disagreements worth reading. Only the grouping carries
//! information: over the cars one document lists, two printed under one string
//! must be teammates in the roster it seated on, and two printed under different
//! strings must not be. A label that never reaches a seat costs nothing when it
//! is wrong, so there is no alias table and no normalisation to keep current.
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
//! - a document's team strings group the cars it lists the way the roster it
//!   seated on groups them.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    allowance::Allowances,
    fact::{Car, Claim, ComponentCode, Conformity, Fact, Round, Team},
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
        /// The event.
        round: Round,
        /// The elements infringements penalize.
        penalized: BTreeSet<(Seat, ComponentCode)>,
        /// The elements the new-elements document flags not in conformity.
        not_in_conformity: BTreeSet<(Seat, ComponentCode)>,
    },
    /// A fact references a component the allowances never seed, so no allowance
    /// exists to check it against (the season's valid components are exactly its
    /// seeded rows).
    UnknownComponent {
        /// The unseeded component.
        component: ComponentCode,
    },
    /// The rosters seat no car of this number at the event the document
    /// describes, so the fact has no timeline to join. Reported once per
    /// `(document, car)`, however many components the document lists for it: the
    /// window may hold no roster for the event consulted, the car may not have
    /// entered it, or its team's lineage may have stopped.
    UnknownSeat {
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
    /// A document's team strings group the cars it lists differently from the
    /// roster it seated on: one string covers cars the roster splits between
    /// teams, or one team's cars come under two strings. A snapshot is grouped
    /// against the roster of the previous event, a new-elements or infringement
    /// document against its own.
    ///
    /// Both maps hold the cars that disagree, so the payload names them: the
    /// document's grouping keyed by the string it prints, the roster's keyed by
    /// the team it entered them for. A car the document prints under two strings
    /// appears under both. The cars whose grouping both sides agree on stay out.
    ///
    /// Reported once per document and roster read, however many rows disagree.
    /// A document carrying a snapshot claim beside another kind, above the
    /// season's first event, reads two rosters and raises a conflict against
    /// each. Two documents at one event stay two witnesses, whether or not they
    /// read the same roster.
    TeamGroupingMismatch {
        /// The event the document belongs to.
        document_round: Round,
        /// The event whose roster the sweep consulted.
        roster_round: Round,
        /// The disagreeing cars the document prints under each team string.
        printed: BTreeMap<Team, BTreeSet<Car>>,
        /// The same cars, under the team the roster entered each of them for.
        entered: BTreeMap<Team, BTreeSet<Car>>,
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

/// The document a fact came from: the event it belongs to and the number its
/// header states. Document numbers restart each event, so the round is part of
/// the identity.
type Document = (Round, u32);

/// Every seated series, each holding the events it has data for.
type Timelines<'a> = BTreeMap<Series<'a>, BTreeMap<Round, RoundData>>;

/// One document read against one event's roster: the unit the team grouping is
/// compared over.
///
/// Every document describes one event, so it reads one roster and a reading is
/// the document. A document whose claims looked back past different events would
/// group each roster's rows on their own instead of cutting two grids together.
type Reading = (Document, Round);

/// The cars one reading puts under each `(printed team, entered team)` pair.
///
/// The pair keys the cars, not the other way round, because one document can
/// print one car under two team strings. That is the misparse this catches, so
/// nothing may assume a car sits under a single label.
type Cells<'a> = BTreeMap<(&'a Team, &'a Team), BTreeSet<Car>>;

/// Every document's grouping, by the roster each read.
type Readings<'a> = BTreeMap<Reading, Cells<'a>>;

/// Cross-check `facts` against `allowances` and the seat map, and return every
/// conflict found.
///
/// The facts, the allowances, and the rosters behind `seats` all belong to one
/// season. Nothing checks it: every caller holds one season by construction, so
/// a batch merging two would return a wrong answer rather than trip a guard.
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
        Claim::SnapshotCount(_)
        | Claim::ElementsFitted { .. }
        | Claim::PreviouslyUsed(_)
        | Claim::StatedOrdinal(_)
        | Claim::Penalty(_) => fact.round,
    }
}

/// Fold the live facts onto the seats the rosters give them, flagging every fact
/// the map cannot seat and every document whose team grouping the rosters
/// contradict.
///
/// A fact that cannot be seated joins no timeline, so leaving it unreported
/// would drop it from every equation and pass silently. It joins no grouping
/// either: a car with no seat has no team to be grouped against, and it already
/// reports once as unseated.
///
/// A row with no printed team constrains no grouping, so it is left out of one.
///
/// The unseated dedupe key and the grouping key both name the document, so one
/// document's several component rows fold into one conflict while two documents
/// stay two witnesses. The grouping key names the roster read as well, so a
/// document that reads two of them groups each on its own: see [`Reading`].
fn seat_live_facts<'a>(facts: &'a [Fact], seats: &'a Seats) -> Seated<'a> {
    let mut timelines = Timelines::new();
    let mut conflicts = Vec::new();
    let mut unseated: BTreeSet<(Document, Car)> = BTreeSet::new();
    let mut readings = Readings::new();

    for fact in facts.iter().filter(|fact| !fact.superseded) {
        let document: Document = (fact.round, fact.document);
        let roster_round = seating_round(fact);

        let Some(seat) = seats.seat(roster_round, fact.car) else {
            if unseated.insert((document, fact.car)) {
                conflicts.push(Conflict::UnknownSeat {
                    document_round: fact.round,
                    roster_round,
                    car: fact.car,
                });
            }
            continue;
        };

        if let Some(printed) = fact.printed_team.as_ref() {
            readings
                .entry((document, roster_round))
                .or_default()
                .entry((printed, &seat.team))
                .or_default()
                .insert(fact.car);
        }

        timelines
            .entry((seat, &fact.component))
            .or_default()
            .entry(fact.round)
            .or_default()
            .absorb(&fact.claim);
    }

    conflicts.extend(grouping_conflicts(&readings));

    Seated {
        timelines,
        conflicts,
    }
}

/// Flag every document whose team strings group its cars differently from the
/// roster it read.
fn grouping_conflicts(readings: &Readings<'_>) -> impl Iterator<Item = Conflict> {
    readings
        .iter()
        .filter_map(|(&((document_round, _), roster_round), cells)| {
            grouping_conflict(document_round, roster_round, cells)
        })
}

/// The conflict one reading raises, if the two groupings disagree.
///
/// They agree when each printed string covers one entered team and each entered
/// team sits under one printed string: one class against one class, so both sides
/// cut the reading's cars the same way, whatever the classes are called. Any
/// other shape merges teams the roster splits, splits a team the roster keeps
/// whole, or both.
///
/// The cars in the cells that break the correspondence are the cars that
/// disagree. A cell whose two ends each stand alone agrees, so it stays out of
/// the payload and a document with one wrong label among right ones reports only
/// the rows at issue.
fn grouping_conflict(
    document_round: Round,
    roster_round: Round,
    cells: &Cells<'_>,
) -> Option<Conflict> {
    let mut entered_per_printed: BTreeMap<&Team, BTreeSet<&Team>> = BTreeMap::new();
    let mut printed_per_entered: BTreeMap<&Team, BTreeSet<&Team>> = BTreeMap::new();
    for &(printed, entered) in cells.keys() {
        entered_per_printed
            .entry(printed)
            .or_default()
            .insert(entered);
        printed_per_entered
            .entry(entered)
            .or_default()
            .insert(printed);
    }
    let merging = spanning_teams(&entered_per_printed);
    let split = spanning_teams(&printed_per_entered);

    let mut printed_groups: BTreeMap<Team, BTreeSet<Car>> = BTreeMap::new();
    let mut entered_groups: BTreeMap<Team, BTreeSet<Car>> = BTreeMap::new();
    for (&(printed, entered), cars) in cells {
        if merging.contains(printed) || split.contains(entered) {
            printed_groups
                .entry(printed.clone())
                .or_default()
                .extend(cars);
            entered_groups
                .entry(entered.clone())
                .or_default()
                .extend(cars);
        }
    }

    (!printed_groups.is_empty()).then_some(Conflict::TeamGroupingMismatch {
        document_round,
        roster_round,
        printed: printed_groups,
        entered: entered_groups,
    })
}

/// The teams on one side of the grouping that reach more than one team on the
/// other: the classes the two sides cut differently.
fn spanning_teams<'a>(relation: &BTreeMap<&'a Team, BTreeSet<&'a Team>>) -> BTreeSet<&'a Team> {
    relation
        .iter()
        .filter(|(_, opposite)| opposite.len() > 1)
        .map(|(&team, _)| team)
        .collect()
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
/// One flat pass over every `(series, round)` cell. Each unseeded component
/// yields one `UnknownComponent`, however many events reference it.
fn local_conflicts<'a>(timelines: &Timelines<'a>, allowances: &Allowances) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    let mut seen_unknown: BTreeSet<&'a ComponentCode> = BTreeSet::new();

    let cells = timelines.iter().flat_map(|(&(seat, component), rounds)| {
        rounds
            .iter()
            .map(move |(&round, data)| (seat, component, round, data))
    });

    for (seat, component, round, data) in cells {
        if allowances.allowance(component).is_none() && seen_unknown.insert(component) {
            conflicts.push(Conflict::UnknownComponent {
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
            && let Some(computed_exceeds) = allowances.exceeds(component, count_after)
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
/// One map groups both sets per round. An event surfaces when its two sets
/// differ, and only then does either set need owning.
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

    let mut events: BTreeMap<Round, EventSets<'_>> = BTreeMap::new();
    for (&(seat, component), rounds) in timelines {
        for (&round, data) in rounds {
            if data.penalized {
                events
                    .entry(round)
                    .or_default()
                    .penalized
                    .insert((seat, component));
            }
            if data.conformity == Some(Conformity::NotInConformity) {
                events
                    .entry(round)
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
                round,
                EventSets {
                    penalized,
                    not_in_conformity,
                },
            )| {
                (penalized != not_in_conformity).then(|| Conflict::PenalizedSetMismatch {
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

    /// The teams as the rosters enter them.
    const RED_BULL: &str = "Red Bull";
    const RACING_BULLS: &str = "Racing Bulls";
    const FERRARI: &str = "Ferrari";

    /// The same teams as the documents print them, and the only strings the
    /// facts here carry: the sponsor names the roster feed leaves out, so no
    /// printed string equals a roster string.
    const RED_BULL_PRINTED: &str = "Red Bull Racing Honda RBPT";
    const RACING_BULLS_PRINTED: &str = "Racing Bulls Honda RBPT";
    const FERRARI_PRINTED: &str = "Scuderia Ferrari HP";

    /// A team no roster in the window enters, for the cars the sweep cannot
    /// seat.
    const ALPINE_PRINTED: &str = "BWT Alpine Formula One Team";

    /// Every fact in the fixture is about the ICE, the component whose count the
    /// swap and the substitution carry across.
    const ICE: &str = "ICE";

    /// A second seeded component, for the rows one document lists beside the
    /// ICE.
    const TC: &str = "TC";

    /// The three documents an event publishes, by the number each header states.
    /// Numbering restarts each event, so the same three numbers recur every
    /// round.
    const SNAPSHOT_DOC: u32 = 1;
    const NEW_ELEMENTS_DOC: u32 = 2;
    const INFRINGEMENT_DOC: u32 = 3;

    fn entry(car: Car, driver: &str, team: &str) -> RosterEntry {
        RosterEntry {
            car,
            driver: driver.to_owned(),
            team: team.into(),
        }
    }

    fn roster(round: Round, entries: Vec<RosterEntry>) -> Roster {
        Roster { round, entries }
    }

    fn seat(team: &str, slot: Slot) -> Seat {
        Seat {
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

    fn fact(
        round: Round,
        car: Car,
        team: &str,
        component: &str,
        claim: Claim,
        document: u32,
    ) -> Fact {
        Fact::new(round, car, component, claim, document).with_printed_team(team)
    }

    fn snapshot(round: Round, car: Car, team: &str, count: u32) -> Fact {
        fact(
            round,
            car,
            team,
            ICE,
            Claim::SnapshotCount(count),
            SNAPSHOT_DOC,
        )
    }

    fn previously_used(round: Round, car: Car, team: &str, count: u32) -> Fact {
        fact(
            round,
            car,
            team,
            ICE,
            Claim::PreviouslyUsed(count),
            NEW_ELEMENTS_DOC,
        )
    }

    fn fitted(round: Round, car: Car, team: &str, count: u32, conformity: Conformity) -> Fact {
        fitted_component(round, car, team, ICE, count, conformity)
    }

    fn fitted_component(
        round: Round,
        car: Car,
        team: &str,
        component: &str,
        count: u32,
        conformity: Conformity,
    ) -> Fact {
        fact(
            round,
            car,
            team,
            component,
            Claim::ElementsFitted { count, conformity },
            NEW_ELEMENTS_DOC,
        )
    }

    /// A new-elements row from a document that prints no team against the car.
    fn fitted_unprinted(round: Round, car: Car) -> Fact {
        Fact::new(
            round,
            car,
            ICE,
            Claim::ElementsFitted {
                count: 1,
                conformity: Conformity::InConformity,
            },
            NEW_ELEMENTS_DOC,
        )
    }

    fn ordinal(round: Round, car: Car, team: &str, value: u32) -> Fact {
        fact(
            round,
            car,
            team,
            ICE,
            Claim::StatedOrdinal(value),
            INFRINGEMENT_DOC,
        )
    }

    fn penalty(round: Round, car: Car, team: &str) -> Fact {
        fact(
            round,
            car,
            team,
            ICE,
            Claim::Penalty("10 place grid drop".to_owned()),
            INFRINGEMENT_DOC,
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
    /// each round's new-elements document prints its own, both in the form the
    /// documents print teams.
    fn clean_facts() -> Vec<Fact> {
        use Conformity::{InConformity, NotInConformity};
        vec![
            // Round 1. Nothing precedes it, so its snapshot prints its own grid.
            snapshot(1, 30, RED_BULL_PRINTED, 0),
            previously_used(1, 30, RED_BULL_PRINTED, 0),
            fitted(1, 30, RED_BULL_PRINTED, 1, InConformity),
            snapshot(1, 22, RACING_BULLS_PRINTED, 0),
            previously_used(1, 22, RACING_BULLS_PRINTED, 0),
            fitted(1, 22, RACING_BULLS_PRINTED, 1, InConformity),
            snapshot(1, 44, FERRARI_PRINTED, 0),
            previously_used(1, 44, FERRARI_PRINTED, 0),
            fitted(1, 44, FERRARI_PRINTED, 1, InConformity),
            snapshot(1, 16, FERRARI_PRINTED, 1),
            previously_used(1, 16, FERRARI_PRINTED, 1),
            fitted(1, 16, FERRARI_PRINTED, 1, InConformity),
            // Round 2. Car 22 fits nothing, so the new-elements document has no
            // row for it and its seat's total stands.
            snapshot(2, 30, RED_BULL_PRINTED, 1),
            previously_used(2, 30, RED_BULL_PRINTED, 1),
            fitted(2, 30, RED_BULL_PRINTED, 1, InConformity),
            snapshot(2, 22, RACING_BULLS_PRINTED, 1),
            snapshot(2, 44, FERRARI_PRINTED, 1),
            previously_used(2, 44, FERRARI_PRINTED, 1),
            fitted(2, 44, FERRARI_PRINTED, 1, InConformity),
            snapshot(2, 16, FERRARI_PRINTED, 2),
            previously_used(2, 16, FERRARI_PRINTED, 2),
            fitted(2, 16, FERRARI_PRINTED, 1, InConformity),
            // Round 3, where the moves take effect. The snapshot still prints
            // round 2's grid and totals; the new-elements document prints
            // round 3's grid, and each arriving car takes over the total of the
            // seat it fills.
            snapshot(3, 30, RED_BULL_PRINTED, 2),
            snapshot(3, 22, RACING_BULLS_PRINTED, 1),
            snapshot(3, 44, FERRARI_PRINTED, 2),
            snapshot(3, 16, FERRARI_PRINTED, 3),
            previously_used(3, 22, RED_BULL_PRINTED, 2),
            fitted(3, 22, RED_BULL_PRINTED, 1, InConformity),
            previously_used(3, 30, RACING_BULLS_PRINTED, 1),
            fitted(3, 30, RACING_BULLS_PRINTED, 1, InConformity),
            previously_used(3, 43, FERRARI_PRINTED, 2),
            fitted(3, 43, FERRARI_PRINTED, 1, InConformity),
            previously_used(3, 16, FERRARI_PRINTED, 3),
            fitted(3, 16, FERRARI_PRINTED, 2, NotInConformity),
            ordinal(3, 16, FERRARI_PRINTED, 5),
            penalty(3, 16, FERRARI_PRINTED),
            // Round 4. Its snapshot closes every seat's round 3 total.
            snapshot(4, 22, RED_BULL_PRINTED, 3),
            snapshot(4, 30, RACING_BULLS_PRINTED, 2),
            snapshot(4, 43, FERRARI_PRINTED, 3),
            snapshot(4, 16, FERRARI_PRINTED, 5),
        ]
    }

    /// Mutable access to the facts matching one `(round, car)` ICE cell.
    fn matching(facts: &mut [Fact], round: Round, car: Car) -> impl Iterator<Item = &mut Fact> {
        facts.iter_mut().filter(move |fact| {
            fact.round == round && fact.car == car && fact.component.as_str() == ICE
        })
    }

    /// Rewrite the team every row of one document prints against `car`.
    fn reprint(facts: &mut [Fact], round: Round, document: u32, car: Car, team: &str) {
        for fact in facts
            .iter_mut()
            .filter(|fact| fact.round == round && fact.document == document && fact.car == car)
        {
            fact.printed_team = Some(team.into());
        }
    }

    /// One side of a grouping: the cars under each team.
    fn grouped<const N: usize>(groups: [(&str, &[Car]); N]) -> BTreeMap<Team, BTreeSet<Car>> {
        groups
            .into_iter()
            .map(|(team, cars)| (team.into(), cars.iter().copied().collect()))
            .collect()
    }

    #[test]
    fn clean_facts_over_the_rosters_that_record_the_moves_report_no_conflicts() {
        // Every string these documents print carries the sponsor names the
        // roster feed leaves out, so not one of them equals a roster team. The
        // grouping is the same either way.
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
            sweep(
                &[snapshot(1, 30, RED_BULL_PRINTED, 0)],
                &allowances(),
                &seats()
            ),
            Vec::new()
        );
    }

    #[test]
    fn a_document_naming_one_cars_team_wrongly_raises_nothing() {
        // A label on a lone car groups nothing with anything, so the roster has
        // no grouping to contradict, however wrong the label.
        let facts = vec![fitted_component(
            2,
            30,
            FERRARI_PRINTED,
            ICE,
            1,
            Conformity::InConformity,
        )];

        assert_eq!(sweep(&facts, &allowances(), &seats()), Vec::new());
    }

    #[test]
    fn two_cars_the_roster_keeps_together_under_one_string_raise_nothing() {
        // Cars 1 and 30 are both Red Bull entries at round 2, and the document
        // prints both under one string.
        let facts = vec![
            fitted_component(2, 1, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
            fitted_component(2, 30, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
        ];

        assert_eq!(sweep(&facts, &allowances(), &seats()), Vec::new());
    }

    #[test]
    fn a_document_grouping_two_cars_the_roster_splits_is_a_conflict() {
        let mut facts = clean_facts();
        // Round 3's new-elements document describes round 3, where car 22 is a
        // Red Bull entry and car 30 a Racing Bulls one. Printing 22 under Racing
        // Bulls puts the two under one string the roster splits.
        reprint(&mut facts, 3, NEW_ELEMENTS_DOC, 22, RACING_BULLS_PRINTED);

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![Conflict::TeamGroupingMismatch {
                document_round: 3,
                roster_round: 3,
                printed: grouped([(RACING_BULLS_PRINTED, &[22, 30])]),
                entered: grouped([(RED_BULL, &[22]), (RACING_BULLS, &[30])]),
            }]
        );
    }

    #[test]
    fn a_snapshot_is_grouped_against_the_previous_events_roster() {
        let mut facts = clean_facts();
        // Round 3's snapshot describes round 2, where car 22 was still a Racing
        // Bulls entry and car 30 a Red Bull one. Printing 22 under Red Bull
        // groups it with 30, which that roster does not.
        reprint(&mut facts, 3, SNAPSHOT_DOC, 22, RED_BULL_PRINTED);

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![Conflict::TeamGroupingMismatch {
                document_round: 3,
                roster_round: 2,
                printed: grouped([(RED_BULL_PRINTED, &[22, 30])]),
                entered: grouped([(RED_BULL, &[30]), (RACING_BULLS, &[22])]),
            }]
        );
    }

    #[test]
    fn a_car_one_document_prints_under_two_teams_is_a_conflict() {
        // The 2025 Chinese GP snapshot in miniature: car 30 appears in two team
        // blocks, one with its own teammate and one with car 22, which the
        // roster enters for another team. No pair of names disagrees, so only the
        // grouping catches it.
        let facts = vec![
            snapshot(2, 1, RED_BULL_PRINTED, 0),
            snapshot(2, 30, RED_BULL_PRINTED, 0),
            fact(
                2,
                30,
                RACING_BULLS_PRINTED,
                TC,
                Claim::SnapshotCount(0),
                SNAPSHOT_DOC,
            ),
            snapshot(2, 22, RACING_BULLS_PRINTED, 0),
        ];

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![Conflict::TeamGroupingMismatch {
                document_round: 2,
                roster_round: 1,
                printed: grouped([
                    (RED_BULL_PRINTED, &[1, 30]),
                    (RACING_BULLS_PRINTED, &[22, 30]),
                ]),
                entered: grouped([(RED_BULL, &[1, 30]), (RACING_BULLS, &[22])]),
            }]
        );
    }

    #[test]
    fn a_document_listing_part_of_the_grid_is_checked_over_the_cars_it_lists() {
        // Two of the grid's six cars, from two teams, under one string.
        let facts = vec![
            fitted_component(2, 30, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
            fitted_component(2, 22, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
        ];

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![Conflict::TeamGroupingMismatch {
                document_round: 2,
                roster_round: 2,
                printed: grouped([(RED_BULL_PRINTED, &[22, 30])]),
                entered: grouped([(RED_BULL, &[30]), (RACING_BULLS, &[22])]),
            }]
        );
    }

    #[test]
    fn rows_with_no_printed_team_join_no_grouping() {
        // Cars 30 and 22 are the pair the roster splits, and 22 prints no team,
        // so the string left on 30 groups it with nothing. Car 16 prints none
        // either, from a third team: gathering the two unprinted rows under one
        // placeholder would merge teams the roster keeps apart.
        let facts = vec![
            fitted_component(2, 30, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
            fitted_unprinted(2, 22),
            fitted_unprinted(2, 16),
        ];

        assert_eq!(sweep(&facts, &allowances(), &seats()), Vec::new());
    }

    #[test]
    fn one_documents_component_rows_are_one_conflict() {
        // One new-elements document lists each car once per component, and
        // groups the two cars alike on every row.
        let facts = vec![
            fitted_component(2, 30, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
            fitted_component(2, 30, RED_BULL_PRINTED, TC, 1, Conformity::InConformity),
            fitted_component(2, 22, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
            fitted_component(2, 22, RED_BULL_PRINTED, TC, 1, Conformity::InConformity),
        ];

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![Conflict::TeamGroupingMismatch {
                document_round: 2,
                roster_round: 2,
                printed: grouped([(RED_BULL_PRINTED, &[22, 30])]),
                entered: grouped([(RED_BULL, &[30]), (RACING_BULLS, &[22])]),
            }]
        );
    }

    #[test]
    fn two_documents_at_one_event_reading_different_rosters_are_two_conflicts() {
        let mut facts = clean_facts();
        // Car 30 is a Red Bull entry at rounds 1 and 2, so round 2's snapshot and
        // its new-elements document read different rosters that agree on the
        // team. Both print it in Ferrari's block, and each stays its own witness.
        reprint(&mut facts, 2, SNAPSHOT_DOC, 30, FERRARI_PRINTED);
        reprint(&mut facts, 2, NEW_ELEMENTS_DOC, 30, FERRARI_PRINTED);

        let misgrouped = |roster_round| Conflict::TeamGroupingMismatch {
            document_round: 2,
            roster_round,
            printed: grouped([(FERRARI_PRINTED, &[16, 30, 44])]),
            entered: grouped([(RED_BULL, &[30]), (FERRARI, &[16, 44])]),
        };

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![misgrouped(1), misgrouped(2)]
        );
    }

    #[test]
    fn two_documents_at_the_seasons_first_event_are_two_conflicts() {
        let mut facts = clean_facts();
        // Round 1 has nothing before it, so its snapshot and its new-elements
        // document both read roster 1. Both print car 30 in Ferrari's block, and
        // each stays its own witness.
        reprint(&mut facts, 1, SNAPSHOT_DOC, 30, FERRARI_PRINTED);
        reprint(&mut facts, 1, NEW_ELEMENTS_DOC, 30, FERRARI_PRINTED);

        // The payload names the two rounds, not the document, so the two
        // witnesses read alike.
        let misgrouped = Conflict::TeamGroupingMismatch {
            document_round: 1,
            roster_round: 1,
            printed: grouped([(FERRARI_PRINTED, &[16, 30, 44])]),
            entered: grouped([(RED_BULL, &[30]), (FERRARI, &[16, 44])]),
        };

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![misgrouped.clone(), misgrouped]
        );
    }

    #[test]
    fn one_document_number_at_two_events_misgrouping_two_cars_is_two_conflicts() {
        // Numbers restart each event, so both snapshots print number 1. Round
        // 2's snapshot describes round 1, so both read roster 1, and both group
        // cars 30 and 22 under one string. Only the round tells the two documents
        // apart.
        let facts = vec![
            snapshot(1, 30, RED_BULL_PRINTED, 0),
            snapshot(1, 22, RED_BULL_PRINTED, 0),
            snapshot(2, 30, RED_BULL_PRINTED, 0),
            snapshot(2, 22, RED_BULL_PRINTED, 0),
        ];

        let misgrouped = |document_round| Conflict::TeamGroupingMismatch {
            document_round,
            roster_round: 1,
            printed: grouped([(RED_BULL_PRINTED, &[22, 30])]),
            entered: grouped([(RED_BULL, &[30]), (RACING_BULLS, &[22])]),
        };

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![misgrouped(1), misgrouped(2)]
        );
    }

    #[test]
    fn a_car_the_rosters_cannot_seat_joins_no_grouping() {
        // Car 77 shares a string with car 30, which the roster seats at Red Bull.
        // Seated anywhere else it would misgroup; unseated it reports once, as
        // unseated.
        let facts = vec![
            fitted_component(1, 30, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
            fitted_component(1, 77, RED_BULL_PRINTED, ICE, 1, Conformity::InConformity),
        ];

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![Conflict::UnknownSeat {
                document_round: 1,
                roster_round: 1,
                car: 77,
            }]
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
        facts.push(snapshot(1, 77, ALPINE_PRINTED, 0));

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert_eq!(
            conflicts,
            vec![Conflict::UnknownSeat {
                document_round: 1,
                roster_round: 1,
                car: 77,
            }]
        );
    }

    #[test]
    fn one_documents_rows_for_an_unseated_car_are_one_conflict() {
        // One new-elements document lists car 77 once per component. No roster
        // enters it.
        let facts = vec![
            fitted_component(1, 77, ALPINE_PRINTED, ICE, 1, Conformity::InConformity),
            fitted_component(1, 77, ALPINE_PRINTED, TC, 1, Conformity::InConformity),
        ];

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![Conflict::UnknownSeat {
                document_round: 1,
                roster_round: 1,
                car: 77,
            }]
        );
    }

    #[test]
    fn two_documents_naming_one_unseated_car_are_two_conflicts() {
        // Round 1's snapshot and its new-elements document both read roster 1,
        // and neither can seat car 77.
        let facts = vec![
            snapshot(1, 77, ALPINE_PRINTED, 0),
            fitted(1, 77, ALPINE_PRINTED, 1, Conformity::InConformity),
        ];

        let unseated = Conflict::UnknownSeat {
            document_round: 1,
            roster_round: 1,
            car: 77,
        };

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![unseated.clone(), unseated]
        );
    }

    #[test]
    fn one_document_number_at_two_events_naming_an_unseated_car_is_two_conflicts() {
        // Numbers restart each event, so both snapshots print number 1. Round
        // 2's snapshot describes round 1, so both read roster 1, and neither can
        // seat car 77. Only the round tells the two documents apart.
        let facts = vec![
            snapshot(1, 77, ALPINE_PRINTED, 0),
            snapshot(2, 77, ALPINE_PRINTED, 0),
        ];

        assert_eq!(
            sweep(&facts, &allowances(), &seats()),
            vec![
                Conflict::UnknownSeat {
                    document_round: 1,
                    roster_round: 1,
                    car: 77,
                },
                Conflict::UnknownSeat {
                    document_round: 2,
                    roster_round: 1,
                    car: 77,
                },
            ]
        );
    }

    #[test]
    fn a_window_opening_at_a_snapshots_own_event_strands_it() {
        // Rounds 3 and 4 only: the round 3 snapshot describes round 2, whose
        // roster the window does not hold.
        let short = resolve_seats(&[roster(3, swapped_grid()), roster(4, swapped_grid())]);

        let conflicts = sweep(
            &[snapshot(3, 22, RACING_BULLS_PRINTED, 1)],
            &allowances(),
            &short,
        );

        assert_eq!(
            conflicts,
            vec![Conflict::UnknownSeat {
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
        facts.push(fact(
            1,
            16,
            FERRARI_PRINTED,
            "GEARBOX",
            Claim::SnapshotCount(1),
            SNAPSHOT_DOC,
        ));

        let conflicts = sweep(&facts, &allowances(), &seats());

        assert_eq!(
            conflicts,
            vec![Conflict::UnknownComponent {
                component: ComponentCode::new("GEARBOX"),
            }]
        );
    }

    #[test]
    fn a_superseded_contradiction_raises_no_conflict() {
        let mut facts = clean_facts();
        facts.push(Fact {
            superseded: true,
            ..snapshot(3, 30, RED_BULL_PRINTED, 99)
        });

        assert_eq!(sweep(&facts, &allowances(), &seats()), Vec::new());
    }

    #[test]
    fn the_same_contradiction_left_live_surfaces() {
        let mut facts = clean_facts();
        // The identical fact, unmarked, overwrites the good snapshot and blows
        // the equations, proving the superseded skip is not a vacuous pass.
        facts.push(snapshot(3, 30, RED_BULL_PRINTED, 99));

        assert!(!sweep(&facts, &allowances(), &seats()).is_empty());
    }

    #[test]
    fn exceedance_flags_a_count_above_the_allowance() {
        assert_eq!(
            allowances().exceeds(&ComponentCode::new(ICE), 5),
            Some(true)
        );
    }

    #[test]
    fn exceedance_clears_a_count_at_the_allowance() {
        assert_eq!(
            allowances().exceeds(&ComponentCode::new(ICE), 4),
            Some(false)
        );
    }

    #[test]
    fn exceedance_clears_a_count_below_the_allowance() {
        assert_eq!(
            allowances().exceeds(&ComponentCode::new(ICE), 3),
            Some(false)
        );
    }

    #[test]
    fn exceedance_is_unknown_for_an_unseeded_component() {
        assert_eq!(
            allowances().exceeds(&ComponentCode::new("GEARBOX"), 1),
            None
        );
    }
}
