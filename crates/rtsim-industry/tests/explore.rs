use rtsim_industry::engine::Mode;
use rtsim_industry::scenario::{IronLine, iron_line, run_iron_line};
use rtsim_industry::tunables::Tunables;

#[test]
fn explore_iron_line() {
    let cfg = IronLine { seed: 7, days: 10, threats: true, control: false, mode: Mode::Apply };
    let mut w = iron_line(cfg, Tunables::default());
    run_iron_line(&mut w, &cfg);
    for e in &w.engine.log {
        println!("{:>8.3} d{} {}", e.tick as f64 / 24000.0, e.district, e.what);
    }
    for e in &w.log {
        println!("{:>8.3} world {}", e.day, e.what);
    }
    println!("stats {:?}", w.engine.stats);
    println!("books {:?}", w.engine.books);
    println!("world {:?}", w.books);
    println!("totals {:?}", w.totals());
    println!("conservation {:?}", w.conservation());
    w.engine.check_books().unwrap();
}

#[test]
fn explore_exchange_rules() {
    let cfg = IronLine { seed: 7, days: 3, threats: false, control: false, mode: Mode::Apply };
    let mut w = iron_line(cfg, Tunables::default());
    w.run_until(rtsim_kernel::clock::Tick(36_000));
    for d in w.engine.districts() {
        println!("district {} exchange {:?} stations {:?}", d.id.0, d.exchange, d.stations);
    }
    for t in &w.trains { println!("train {} {:?} {:?}", t.id.0, t.state, t.carriages.iter().map(|c| c.cargo.clone()).collect::<Vec<_>>()); }
    for d in &w.districts { println!("{} {:?}", d.name, d.inventories.iter().map(|i| i.contents.clone()).collect::<Vec<_>>()); }
}

#[test]
fn explore_train2() {
    let cfg = IronLine { seed: 7, days: 4, threats: false, control: false, mode: Mode::Apply };
    let mut w = iron_line(cfg, Tunables::default());
    w.run_until(rtsim_kernel::clock::Tick(60_000));
    for e in w.engine.log.iter().filter(|e| e.tick >= 30_000) {
        println!("{:>8.3} d{} {}", e.tick as f64 / 24000.0, e.district, e.what);
    }
    println!("sim stock smelter {:?}", w.engine.sim_stock(rtsim_industry::ids::DistrictId(2)));
    println!("sim run smelter {:?}", w.engine.sim_run(rtsim_industry::ids::DistrictId(2)));
}
