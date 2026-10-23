// SPDX-License-Identifier: AGPL-3.0-or-later
//! A train's leg ends at a station outside every district (IND_TRAIN_UNWATCHED, choice SG50), and
//! a leg is never an edge from a station to itself (choice SG51): docs/stress-worlds-results.md
//! 10.9 found 21 to 31 such edges on the syn200 rtsim arms, each with a round trip's travel time,
//! from trains that called at stations the bridge did not watch (DS-12's deferred adopts).

use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, StationId, TrainId};
use rtsim_industry::obs::{
    CargoFrom, Obs, StationDecl, Timed, TrainCall, TrainDeparture, UnwatchedStop,
};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;

const IRON: Good = Good(1);
const WEST: StationId = StationId(10);
const EAST: StationId = StationId(11);
const OUTSIDE: StationId = StationId(12);
const T: TrainId = TrainId(7);
const D1: DistrictId = DistrictId(1);
const D2: DistrictId = DistrictId(2);

fn engine() -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    for (station, district, x) in [(WEST, D1, 0), (EAST, D2, 4000)] {
        send(
            &mut e,
            0,
            Obs::Station(StationDecl {
                station,
                district: Some(district),
                dim: 0,
                pos: BlockPos::new(x, 64, 0),
                name: 0,
            }),
        );
    }
    e
}

fn send(e: &mut Engine, at: u64, obs: Obs) {
    e.ingest(&Timed { at: Tick(at), obs });
}

fn arrive(e: &mut Engine, at: u64, station: StationId, district: DistrictId) {
    send(
        e,
        at,
        Obs::TrainArrived(TrainCall {
            train: T,
            station,
            district,
            simulated: false,
            cargo_from: CargoFrom::Approach,
            carriages: vec![],
        }),
    );
}

fn depart(e: &mut Engine, at: u64, station: StationId, district: DistrictId, to: StationId) {
    send(
        e,
        at,
        Obs::TrainDeparted(TrainDeparture {
            train: T,
            station,
            district,
            destination: Some(to),
            eta_ticks: 200,
            carriages: vec![],
        }),
    );
}

fn unwatched(e: &mut Engine, at: u64, station: StationId) {
    send(
        e,
        at,
        Obs::TrainUnwatched(UnwatchedStop { train: T, station }),
    );
}

fn trips(e: &Engine, a: StationId, b: StationId) -> u64 {
    e.lines().get(&(a, b)).map_or(0, |l| l.trips)
}

/// The syn200 shape: the train leaves the west station of a district, calls at a station the bridge
/// did not watch (the east one, its district still deferred), and comes back to the west station.
/// With the unwatched stop reported its leg ends at the east station; with none reported (an old
/// bridge, or a stop the bridge could not see) the engine still makes no edge from a station to itself.
#[test]
fn out_and_back_through_an_unwatched_station_makes_no_self_edge() {
    let mut e = engine();
    arrive(&mut e, 100, WEST, D1);
    depart(&mut e, 160, WEST, D1, EAST);
    unwatched(&mut e, 290, EAST);
    arrive(&mut e, 620, WEST, D1);
    assert_eq!(trips(&e, WEST, WEST), 0, "a station is not its own edge");
    assert!(
        e.lines().keys().all(|(a, b)| a != b),
        "self edges: {:?}",
        e.lines().keys().collect::<Vec<_>>()
    );
    assert!(
        e.lines().is_empty(),
        "the unwatched stop ended the leg: no edge at all"
    );
    assert_eq!(e.stats.unwatched_stops, 1);
    assert_eq!(e.stats.self_edges_refused, 0);
}

#[test]
fn the_engine_refuses_a_self_edge_when_no_unwatched_stop_came() {
    let mut e = engine();
    arrive(&mut e, 100, WEST, D1);
    depart(&mut e, 160, WEST, D1, EAST);
    arrive(&mut e, 621, WEST, D1);
    assert!(e.lines().is_empty(), "{:?}", e.lines());
    assert_eq!(e.stats.self_edges_refused, 1);
    // The leg is over: a second arrival with no departure between makes nothing.
    arrive(&mut e, 700, WEST, D1);
    assert!(e.lines().is_empty());
    assert_eq!(e.stats.self_edges_refused, 1);
}

/// The unwatched stop ends the leg it was reported in, not the next one.
#[test]
fn an_unwatched_stop_ends_one_leg_and_the_next_departure_makes_edges_again() {
    let mut e = engine();
    arrive(&mut e, 100, WEST, D1);
    depart(&mut e, 160, WEST, D1, EAST);
    unwatched(&mut e, 290, OUTSIDE);
    // The leg ended at the unwatched station: arriving at the east station is no edge from the west.
    arrive(&mut e, 400, EAST, D2);
    assert_eq!(trips(&e, WEST, EAST), 0);
    // The next leg is whole again.
    depart(&mut e, 460, EAST, D2, WEST);
    arrive(&mut e, 660, WEST, D1);
    assert_eq!(trips(&e, EAST, WEST), 1);
    assert_eq!(e.lines()[&(EAST, WEST)].travel_ticks, 200);
}

/// A control: stops the bridge watched all along still make their edges, as before.
#[test]
fn watched_legs_still_make_edges() {
    let mut e = engine();
    arrive(&mut e, 100, WEST, D1);
    depart(&mut e, 160, WEST, D1, EAST);
    arrive(&mut e, 360, EAST, D2);
    depart(&mut e, 420, EAST, D2, WEST);
    arrive(&mut e, 620, WEST, D1);
    assert_eq!(trips(&e, WEST, EAST), 1);
    assert_eq!(trips(&e, EAST, WEST), 1);
    assert_eq!(e.lines()[&(WEST, EAST)].travel_ticks, 200);
    assert_eq!(e.stats.unwatched_stops, 0);
    assert_eq!(e.stats.self_edges_refused, 0);
}

/// An unwatched stop for a train that has no leg (a new train, or one the engine just restarted
/// without) changes nothing but the count.
#[test]
fn an_unwatched_stop_with_no_leg_is_harmless() {
    let mut e = engine();
    unwatched(&mut e, 50, OUTSIDE);
    arrive(&mut e, 100, WEST, D1);
    assert!(e.lines().is_empty());
}

/// The two counters are the run's own (choice SG58): the saved pages are the same bytes with them
/// at 0 or not, so they add nothing to the page layout, and a restore starts them at 0.
#[test]
fn the_two_counters_are_not_in_the_saved_pages() {
    use rtsim_kernel::page::PageKey;

    let mut e = engine();
    arrive(&mut e, 100, WEST, D1);
    depart(&mut e, 160, WEST, D1, EAST);
    unwatched(&mut e, 290, EAST);
    arrive(&mut e, 620, WEST, D1);
    depart(&mut e, 700, WEST, D1, EAST);
    arrive(&mut e, 905, WEST, D1);
    assert_eq!(e.stats.unwatched_stops, 1);
    assert_eq!(e.stats.self_edges_refused, 1);
    let with: Vec<(PageKey, Vec<u8>)> = e.snapshot();
    e.stats.unwatched_stops = 0;
    e.stats.self_edges_refused = 0;
    assert_eq!(
        with,
        e.snapshot(),
        "the counters change no page byte (they were in the globals page once)"
    );
    let back = Engine::restore(with, Tunables::default()).expect("restore");
    assert_eq!(back.stats.unwatched_stops, 0);
    assert_eq!(back.stats.self_edges_refused, 0);
}
