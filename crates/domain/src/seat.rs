//! Seat resolution: which car entry a season's running counts belong to.
//!
//! A PU element count belongs to the seat, a team's car entry, not to the driver
//! in it. Fold a season by seat and a mid-season swap keeps running total;
//! fold it by car number and the swap mixes two seats, then raises a false
//! conflict on correct FIA data.
//!
//! A [`Seat`] is `(season, team, slot)`. One rule produces the
//! slot at every event: a car entering a team with no vacated seat to inherit
//! takes a fresh slot, and arrivals are seated in ascending car-number order. A
//! team's first event vacates nothing, so its cars number by car number; every
//! later event carries those seats forward.
//!
//! [`resolve_seats`] diffs each team between consecutive events. One car out and
//! one car in transfers the seat, which covers a mid-season swap and an
//! outside-grid substitute alike, with no special path. A car the team has
//! seated before is matched on the slot it last held, never paired by count into
//! another car's seat. A diff that admits more than one pairing of arriving cars
//! to vacated seats becomes a [`SeatAmbiguity`] and stops that team's lineage; a
//! human supplies the mapping.
//!
//! The seat is computed here and never written onto a fact. It
//! groups within one season.

use std::collections::{BTreeMap, BTreeSet};

use crate::fact::{Car, Round, Season, Team};

/// A driver name, as the roster states it.
///
/// Display only. Seat resolution keys on the car number and the team, so a
/// renamed or misspelled driver never shifts a seat.
pub type Driver = String;

/// A seat's index within its team, counting from zero.
pub type Slot = u8;

/// A team's car entry within one season: the identity a running count follows.
///
/// The car number and the printed driver stay recorded for display; neither is
/// the identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seat {
    /// The season the seat belongs to.
    pub season: Season,
    /// The team fielding the entry.
    pub team: Team,
    /// The entry's index within the team.
    pub slot: Slot,
}

/// One entered race driver at one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterEntry {
    /// The car number entered.
    pub car: Car,
    /// The driver entered in it.
    pub driver: Driver,
    /// The team entering it.
    pub team: Team,
}

/// The entered race drivers for one event.
///
/// Entered race drivers only: an FP1-only cameo never shifts a
/// seat. One entry per car; a car repeated within an event keeps its first
/// entry, so a car never sits in two teams at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster {
    /// The season the event belongs to.
    pub season: Season,
    /// The event's ordering within the season, counting from one.
    pub round: Round,
    /// The entered race drivers.
    pub entries: Vec<RosterEntry>,
}

/// A per-team diff the resolver could not pair without guessing.
///
/// Two diffs reach here. Both of a team's occupants change at once, so the diff
/// cannot tell which arriving car took which vacated seat; or a car returns to a
/// slot another car entered at this event now occupies, so the diff cannot tell
/// whether it resumes that slot or takes the vacant one. Either way the resolver
/// flags the event and seats none of the team's cars from it onward, leaving the
/// mapping to a human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatAmbiguity {
    /// The season the event belongs to.
    pub season: Season,
    /// The event whose diff could not be paired.
    pub round: Round,
    /// The team whose lineage stops here.
    pub team: Team,
    /// The slots whose occupants did not enter this event, ascending.
    pub vacated: Vec<Slot>,
    /// The cars entered for the team with no seat to carry forward, ascending
    /// by car number.
    pub arriving: Vec<Car>,
}

/// The seats a window of rosters resolves to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seats {
    by_entry: BTreeMap<(Season, Round, Car), Seat>,
    ambiguities: Vec<SeatAmbiguity>,
}

impl Seats {
    /// The seat `car` occupies at `(season, round)`.
    ///
    /// `None` when the window holds no roster for that event, when the car did
    /// not enter it, or when an ambiguous diff stopped its team's lineage.
    #[must_use]
    pub fn seat(&self, season: Season, round: Round, car: Car) -> Option<&Seat> {
        self.by_entry.get(&(season, round, car))
    }

    /// Every diff the resolver refused to guess, ordered by season, then round,
    /// then team. One per team: the first unpairable diff stops the lineage.
    #[must_use]
    pub fn ambiguities(&self) -> &[SeatAmbiguity] {
        &self.ambiguities
    }
}

/// Resolve the seat of every car entry in a window of rosters.
///
/// Seasons resolve independently, and the window is order
/// independent: the rosters are indexed by season and round before any diff
/// runs.
#[must_use]
pub fn resolve_seats(rosters: &[Roster]) -> Seats {
    let mut seats = Seats::default();
    for (season, events) in index_window(rosters) {
        resolve_season(season, &events, &mut seats);
    }
    seats
}

/// One event's entered cars, each mapped to the team that entered it.
type Entries = BTreeMap<Car, Team>;

/// Index the window by season and round, one team per car per event.
fn index_window(rosters: &[Roster]) -> BTreeMap<Season, BTreeMap<Round, Entries>> {
    let mut window: BTreeMap<Season, BTreeMap<Round, Entries>> = BTreeMap::new();
    for roster in rosters {
        let event = window
            .entry(roster.season)
            .or_default()
            .entry(roster.round)
            .or_default();
        for entry in &roster.entries {
            event.entry(entry.car).or_insert_with(|| entry.team.clone());
        }
    }
    window
}

/// One event's entered cars, grouped by team.
fn group_by_team(entries: &Entries) -> BTreeMap<&Team, BTreeSet<Car>> {
    let mut teams: BTreeMap<&Team, BTreeSet<Car>> = BTreeMap::new();
    for (car, team) in entries {
        teams.entry(team).or_default().insert(*car);
    }
    teams
}

/// Carry every team's lineage across one season's events, recording each seated
/// entry and every diff that could not be paired.
fn resolve_season(season: Season, events: &BTreeMap<Round, Entries>, seats: &mut Seats) {
    let mut lineages: BTreeMap<Team, Lineage> = BTreeMap::new();

    for (&round, entries) in events {
        for (team, entered) in group_by_team(entries) {
            let lineage = lineages.entry(team.clone()).or_default();

            match lineage.carry(&entered) {
                Ok(seated) => {
                    for (slot, car) in seated {
                        let seat = Seat {
                            season,
                            team: team.clone(),
                            slot,
                        };
                        seats.by_entry.insert((season, round, car), seat);
                    }
                }
                Err(Unpaired { vacated, arriving }) => seats.ambiguities.push(SeatAmbiguity {
                    season,
                    round,
                    team: team.clone(),
                    vacated,
                    arriving,
                }),
            }
        }
    }
}

/// The two halves of a diff that admits more than one pairing.
struct Unpaired {
    vacated: Vec<Slot>,
    arriving: Vec<Car>,
}

/// One team's seat lineage as a season's events carry it forward.
#[derive(Default)]
struct Lineage {
    /// The car occupying each of the team's slots. A slot keeps its occupant
    /// while that car sits out unreplaced, so a team entering one car for an
    /// event gives up no seat and needs no transfer to get it back.
    occupants: BTreeMap<Slot, Car>,
    /// The slot each car last held for this team, kept after the car leaves it.
    /// A returning car is recognised by that slot instead of paired by count.
    held: BTreeMap<Car, Slot>,
    /// Set by the first diff the lineage could not pair. The team resolves no
    /// further event.
    stopped: bool,
}

impl Lineage {
    /// Carry the lineage into an event whose entered cars are `entered`, and
    /// return the cars it seats there with their slots.
    ///
    /// Cars already occupying a slot keep it. The rest pair against the slots
    /// this event vacates: no vacancy seats every arrival afresh, one vacancy
    /// and one arrival transfers the seat, and anything else is unpairable, so
    /// the lineage stops rather than choose. A stopped lineage seats nothing and
    /// returns no second error.
    fn carry(&mut self, entered: &BTreeSet<Car>) -> Result<Vec<(Slot, Car)>, Unpaired> {
        if self.stopped {
            return Ok(Vec::new());
        }

        let occupied: BTreeSet<Car> = self.occupants.values().copied().collect();
        let vacated: Vec<Slot> = self
            .occupants
            .iter()
            .filter(|(_, car)| !entered.contains(*car))
            .map(|(&slot, _)| slot)
            .collect();
        let arriving: Vec<Car> = entered
            .iter()
            .copied()
            .filter(|car| !occupied.contains(car))
            .collect();

        if self.contested(entered, &arriving) {
            self.stopped = true;
            return Err(Unpaired { vacated, arriving });
        }

        match (vacated.len(), arriving.len()) {
            (_, 0) => {}
            (0, _) => {
                for &car in &arriving {
                    self.seat_in_new_slot(car);
                }
            }
            (1, 1) => {
                for (slot, car) in vacated.iter().copied().zip(arriving.iter().copied()) {
                    self.seat_in(slot, car);
                }
            }
            _ => {
                self.stopped = true;
                return Err(Unpaired { vacated, arriving });
            }
        }
        Ok(self.seated(entered))
    }

    /// Whether an arriving car returns to a slot a car entered at this event now
    /// occupies.
    ///
    /// Neither reading follows from the counts: the car may resume the slot it
    /// last held, or take the one this event vacates. Where its old slot is
    /// free, the ordinary pairing still holds, so a regular coming back from a
    /// substitution stays resolvable.
    fn contested(&self, entered: &BTreeSet<Car>, arriving: &[Car]) -> bool {
        arriving.iter().any(|car| {
            self.held.get(car).is_some_and(|slot| {
                self.occupants
                    .get(slot)
                    .is_some_and(|sitting| entered.contains(sitting))
            })
        })
    }

    /// Seat `car` in the lowest slot the team has never used.
    ///
    /// Slots are never released, so this never reuses one. A team entering more
    /// than [`Slot::MAX`] cars in a season leaves the car unseated rather than
    /// collide with a seat already taken.
    fn seat_in_new_slot(&mut self, car: Car) {
        if let Some(slot) = (Slot::MIN..=Slot::MAX).find(|slot| !self.occupants.contains_key(slot))
        {
            self.seat_in(slot, car);
        }
    }

    /// Seat `car` in `slot` and record the slot as the one that car last held.
    ///
    /// Every seating goes through here, so a car that leaves the team is still
    /// recognised when it comes back.
    fn seat_in(&mut self, slot: Slot, car: Car) {
        self.occupants.insert(slot, car);
        self.held.insert(car, slot);
    }

    /// The cars the lineage seats at this event, with their slots.
    fn seated(&self, entered: &BTreeSet<Car>) -> Vec<(Slot, Car)> {
        self.occupants
            .iter()
            .filter(|(_, car)| entered.contains(*car))
            .map(|(&slot, &car)| (slot, car))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Hand-built rosters drive the resolver. A swap and a substitution each
    //! transfer one seat; both occupants changing at once, and a car returning
    //! to a slot an entered car holds, are flagged and never guessed.

    use super::*;

    const SEASON: Season = 2026;
    const RED_BULL: &str = "Red Bull";
    const RACING_BULLS: &str = "Racing Bulls";
    const FERRARI: &str = "Ferrari";

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

    /// The grid the season opens with: three teams, two cars each.
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

    /// A 2026 window over three events.
    ///
    /// Rounds 1 and 2 run the opening grid. At round 3 Red Bull and Racing Bulls
    /// swap cars 30 and 22, and Ferrari substitutes car 43 for car 44. Every
    /// other entry holds.
    fn window() -> Vec<Roster> {
        vec![
            roster(1, opening_grid()),
            roster(2, opening_grid()),
            roster(
                3,
                vec![
                    entry(1, "Verstappen", RED_BULL),
                    entry(22, "Tsunoda", RED_BULL),
                    entry(6, "Hadjar", RACING_BULLS),
                    entry(30, "Lawson", RACING_BULLS),
                    entry(16, "Leclerc", FERRARI),
                    entry(43, "Colapinto", FERRARI),
                ],
            ),
        ]
    }

    /// Both of Red Bull's occupants change at round 2.
    fn double_change() -> Vec<Roster> {
        vec![
            roster(
                1,
                vec![
                    entry(1, "Verstappen", RED_BULL),
                    entry(30, "Lawson", RED_BULL),
                ],
            ),
            roster(
                2,
                vec![entry(22, "Tsunoda", RED_BULL), entry(6, "Hadjar", RED_BULL)],
            ),
            roster(
                3,
                vec![entry(22, "Tsunoda", RED_BULL), entry(6, "Hadjar", RED_BULL)],
            ),
        ]
    }

    /// Ferrari's second slot goes to substitute car 43 at round 2, and car 44
    /// takes it back at round 3.
    fn substitution_and_return() -> Vec<Roster> {
        vec![
            roster(
                1,
                vec![
                    entry(16, "Leclerc", FERRARI),
                    entry(44, "Hamilton", FERRARI),
                ],
            ),
            roster(
                2,
                vec![
                    entry(16, "Leclerc", FERRARI),
                    entry(43, "Colapinto", FERRARI),
                ],
            ),
            roster(
                3,
                vec![
                    entry(16, "Leclerc", FERRARI),
                    entry(44, "Hamilton", FERRARI),
                ],
            ),
        ]
    }

    /// The same substitution, but car 16 sits out at round 3, so cars 43 and 44
    /// both enter with one slot vacant: car 44 could resume the slot it held or
    /// take the one car 16 leaves.
    fn contested_return() -> Vec<Roster> {
        vec![
            roster(
                1,
                vec![
                    entry(16, "Leclerc", FERRARI),
                    entry(44, "Hamilton", FERRARI),
                ],
            ),
            roster(
                2,
                vec![
                    entry(16, "Leclerc", FERRARI),
                    entry(43, "Colapinto", FERRARI),
                ],
            ),
            roster(
                3,
                vec![
                    entry(43, "Colapinto", FERRARI),
                    entry(44, "Hamilton", FERRARI),
                ],
            ),
        ]
    }

    /// Red Bull enters car 1 alone at round 2, with nobody in car 30's slot.
    fn one_car_entry() -> Vec<Roster> {
        vec![
            roster(
                1,
                vec![
                    entry(1, "Verstappen", RED_BULL),
                    entry(30, "Lawson", RED_BULL),
                ],
            ),
            roster(2, vec![entry(1, "Verstappen", RED_BULL)]),
            roster(
                3,
                vec![
                    entry(1, "Verstappen", RED_BULL),
                    entry(30, "Lawson", RED_BULL),
                ],
            ),
        ]
    }

    #[test]
    fn a_teams_first_event_seats_its_lowest_car_number_in_the_first_slot() {
        assert_eq!(
            resolve_seats(&window()).seat(SEASON, 1, 1),
            Some(&seat(RED_BULL, 0))
        );
    }

    #[test]
    fn a_teams_first_event_seats_its_higher_car_number_in_the_next_slot() {
        assert_eq!(
            resolve_seats(&window()).seat(SEASON, 1, 30),
            Some(&seat(RED_BULL, 1))
        );
    }

    #[test]
    fn teams_slot_their_cars_independently() {
        assert_eq!(
            resolve_seats(&window()).seat(SEASON, 1, 6),
            Some(&seat(RACING_BULLS, 0))
        );
    }

    #[test]
    fn a_swap_seats_the_arriving_car_where_the_departing_car_sat() {
        let seats = resolve_seats(&window());
        let vacated = Some(&seat(RED_BULL, 1));

        assert_eq!(
            (seats.seat(SEASON, 2, 30), seats.seat(SEASON, 3, 22)),
            (vacated, vacated)
        );
    }

    #[test]
    fn the_car_moving_the_other_way_inherits_the_seat_it_arrives_in() {
        assert_eq!(
            resolve_seats(&window()).seat(SEASON, 3, 30),
            Some(&seat(RACING_BULLS, 1))
        );
    }

    #[test]
    fn a_substitute_from_outside_the_grid_inherits_the_seat_it_fills() {
        let seats = resolve_seats(&window());
        let vacated = Some(&seat(FERRARI, 1));

        assert_eq!(
            (seats.seat(SEASON, 2, 44), seats.seat(SEASON, 3, 43)),
            (vacated, vacated)
        );
    }

    #[test]
    fn a_car_that_never_changes_team_keeps_its_seat() {
        let seats = resolve_seats(&window());
        let held = Some(&seat(FERRARI, 0));

        assert_eq!(
            (seats.seat(SEASON, 1, 16), seats.seat(SEASON, 3, 16)),
            (held, held)
        );
    }

    #[test]
    fn a_regular_returning_from_a_substitution_returns_to_its_seat() {
        assert_eq!(
            resolve_seats(&substitution_and_return()).seat(SEASON, 3, 44),
            Some(&seat(FERRARI, 1))
        );
    }

    #[test]
    fn a_regular_returning_from_a_substitution_flags_nothing() {
        assert_eq!(resolve_seats(&substitution_and_return()).ambiguities(), &[]);
    }

    #[test]
    fn a_slot_keeps_its_occupant_while_its_car_sits_out_unreplaced() {
        assert_eq!(
            resolve_seats(&one_car_entry()).seat(SEASON, 3, 30),
            Some(&seat(RED_BULL, 1))
        );
    }

    #[test]
    fn a_car_returning_to_a_slot_an_entered_car_holds_is_flagged() {
        assert_eq!(
            resolve_seats(&contested_return()).ambiguities(),
            &[SeatAmbiguity {
                season: SEASON,
                round: 3,
                team: FERRARI.into(),
                vacated: vec![0],
                arriving: vec![44],
            }]
        );
    }

    #[test]
    fn a_car_returning_to_a_slot_an_entered_car_holds_takes_no_seat() {
        assert_eq!(resolve_seats(&contested_return()).seat(SEASON, 3, 44), None);
    }

    #[test]
    fn a_window_of_resolvable_diffs_flags_nothing() {
        assert_eq!(resolve_seats(&window()).ambiguities(), &[]);
    }

    #[test]
    fn both_occupants_changing_at_once_is_flagged() {
        assert_eq!(
            resolve_seats(&double_change()).ambiguities(),
            &[SeatAmbiguity {
                season: SEASON,
                round: 2,
                team: RED_BULL.into(),
                vacated: vec![0, 1],
                arriving: vec![6, 22],
            }]
        );
    }

    #[test]
    fn an_unpairable_diff_seats_neither_arriving_car() {
        assert_eq!(resolve_seats(&double_change()).seat(SEASON, 2, 22), None);
    }

    #[test]
    fn an_unpairable_diff_stops_the_team_at_every_later_event() {
        assert_eq!(resolve_seats(&double_change()).seat(SEASON, 3, 22), None);
    }

    #[test]
    fn a_stopped_team_is_flagged_once() {
        assert_eq!(resolve_seats(&double_change()).ambiguities().len(), 1);
    }

    #[test]
    fn a_car_the_window_never_enters_has_no_seat() {
        assert_eq!(resolve_seats(&window()).seat(SEASON, 1, 77), None);
    }

    #[test]
    fn seasons_resolve_independently() {
        let seats = resolve_seats(&[
            roster(1, vec![entry(1, "Verstappen", RED_BULL)]),
            Roster {
                season: SEASON + 1,
                round: 1,
                entries: vec![entry(30, "Lawson", RED_BULL)],
            },
        ]);

        assert_eq!(
            seats.seat(SEASON + 1, 1, 30),
            Some(&Seat {
                season: SEASON + 1,
                team: RED_BULL.into(),
                slot: 0,
            })
        );
    }

    #[test]
    fn a_car_repeated_within_an_event_keeps_its_first_entry() {
        let seats = resolve_seats(&[roster(
            1,
            vec![
                entry(1, "Verstappen", RED_BULL),
                entry(1, "Verstappen", RACING_BULLS),
            ],
        )]);

        assert_eq!(seats.seat(SEASON, 1, 1), Some(&seat(RED_BULL, 0)));
    }
}
