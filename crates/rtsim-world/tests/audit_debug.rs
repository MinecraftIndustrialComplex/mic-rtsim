use rtsim_kernel::clock::Tick;
use rtsim_world::{WorldConfig, synth};

#[test]
fn audit_debug() {
    let mut cfg = WorldConfig::new(42);
    cfg.keep_history = true;
    cfg.genesis = rtsim_trade::build::Genesis::Share(500);
    let (mut w, _ids) = synth::build(cfg, true);
    for t in (1000..=2_400_000).step_by(1000) {
        w.run_until(Tick(t));
        let a = w.audit(Tick(t));
        if !a.is_empty() || w.check_conserved(Tick(t)).is_err() {
            eprintln!("t={t} conserved={:?}", w.check_conserved(Tick(t)));
            for x in a { eprintln!("  {:?}", x); }
            panic!("mismatch");
        }
    }
}
