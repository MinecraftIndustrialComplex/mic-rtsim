//! Diagnostics for the faction sweep: events and pending kinds as days pass. Scaffolding.
use rtsim_conflict::synth;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::Tick;
fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1024);
    let (mut w, mut c) = synth::many(n, n, 1);
    c.run_until(&mut w, &mut NoInfrastructure, Tick::from_days(2));
    let t0 = std::time::Instant::now();
    let mut t = Tick::from_days(2);
    let end = Tick::from_days(30);
    let mut conflict_ns = 0u128;
    while t < end {
        let next = c.next_due().unwrap_or(end).clamp(t + 1, end);
        w.run_until(next);
        let a = std::time::Instant::now();
        c.run_until(&mut w, &mut NoInfrastructure, next);
        conflict_ns += a.elapsed().as_nanos();
        t = next;
    }
    println!(
        "{n}: events {} conflict {:.1} us per faction-day, wall {:?}",
        c.stats.events,
        conflict_ns as f64 / 1e3 / (n as f64 * 28.0),
        t0.elapsed()
    );
}
