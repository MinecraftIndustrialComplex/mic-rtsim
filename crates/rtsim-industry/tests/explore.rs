use rtsim_industry::engine::Mode;
use rtsim_industry::scenario::{IronLine, iron_line, run_iron_line};
use rtsim_industry::tunables::Tunables;

#[test]
fn explore_iron_line() {
    let cfg = IronLine { seed: 7, days: 10, threats: true, control: false, mode: Mode::Apply, tick_first: true };
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
    let cfg = IronLine { seed: 7, days: 3, threats: false, control: false, mode: Mode::Apply, tick_first: true };
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
    let cfg = IronLine { seed: 7, days: 4, threats: false, control: false, mode: Mode::Apply, tick_first: true };
    let mut w = iron_line(cfg, Tunables::default());
    w.run_until(rtsim_kernel::clock::Tick(60_000));
    for e in w.engine.log.iter().filter(|e| e.tick >= 30_000) {
        println!("{:>8.3} d{} {}", e.tick as f64 / 24000.0, e.district, e.what);
    }
    println!("sim stock smelter {:?}", w.engine.sim_stock(rtsim_industry::ids::DistrictId(2)));
    println!("sim run smelter {:?}", w.engine.sim_run(rtsim_industry::ids::DistrictId(2)));
}

#[test]
fn explore_smelter_start() {
    let cfg = IronLine { seed: 7, days: 10, threats: false, control: false, mode: Mode::ReadOnly, tick_first: true };
    let mut w = iron_line(cfg, Tunables::default());
    w.run_until(rtsim_kernel::clock::Tick(36_010));
    println!("run {:?}", w.engine.sim_run(rtsim_industry::ids::DistrictId(2)));
    println!("stock {:?}", w.engine.sim_stock(rtsim_industry::ids::DistrictId(2)));
    let d = w.engine.district(rtsim_industry::ids::DistrictId(2)).unwrap();
    println!("power {:?} model {:?}", d.power_source, d.models.values().next().map(|m| &m.learned.rates));
    for e in w.engine.log.iter().filter(|e| e.tick >= 35_000) { println!("{} d{} {}", e.tick, e.district, e.what); }
    for (i, inv) in &d.inventories { println!("engine inv {i} known {:?} at {:?} pending {:?}", inv.known, inv.known_at, inv.pending); }
    for inv in &w.districts[1].inventories { println!("world inv {} {:?}", inv.index, inv.contents); }
    for t in &w.trains { println!("train {} {:?} {:?}", t.id.0, t.state, t.carriages.iter().map(|c| c.cargo.clone()).collect::<Vec<_>>()); }
}

#[test]
fn explore_ib_split() {
    use rtsim_kernel::clock::Tick;
    for control in [false, true] {
        let cfg = IronLine { seed: 7, days: 10, threats: false, control, mode: Mode::Apply, tick_first: false };
        let mut w = iron_line(cfg, Tunables::default());
        w.run_until(Tick(36_000));
        let a = w.books.minted.get(rtsim_industry::scenario::INGOT);
        w.run_until(Tick(121_000));
        let r = w.engine.sim_stock(rtsim_industry::ids::DistrictId(2));
        println!("  at 5.04 sim stock {:?} engine minted so far {:?}", r, w.engine.books.minted);
        w.run_until(Tick(192_100));
        println!("  at 8.004 world minted {:?} engine minted {:?} trimmed {:?} shortfall {:?}", w.books.minted, w.engine.books.minted, w.engine.books.trimmed, w.engine.books.shortfall);
        for e in w.engine.log.iter().filter(|e| e.tick > 100_000 && !e.what.contains("exchange") && !e.what.contains("Natural")) { println!("    {} d{} {}", e.tick, e.district, e.what); }
        let b = w.books.minted.get(rtsim_industry::scenario::INGOT) + w.engine.books.minted.get(rtsim_industry::scenario::INGOT);
        let sim = w.engine.sim_stock(rtsim_industry::ids::DistrictId(2));
        println!("control={control} ingots by 1.5: {a}; by 5.04 (world only): {}; batches {}; sim stock {:?}", w.books.minted.get(rtsim_industry::scenario::INGOT), w.books.batches, sim);
        let _ = b;
    }
}
