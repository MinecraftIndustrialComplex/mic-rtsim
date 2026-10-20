// SPDX-License-Identifier: AGPL-3.0-or-later
//! The walkthrough end to end: a goblin tribe, a polluting refinery and a logged forest.

use rtsim_conflict::scenario::{self, Options, Variant};

#[test]
fn debug_print_ignore() {
    let v = match std::env::var("VARIANT").as_deref() {
        Ok("tribute") => Variant::Tribute,
        Ok("converter") => Variant::Converter,
        _ => Variant::Ignore,
    };
    let o = scenario::run(&Options {
        seed: 7,
        days: 24,
        variant: v,
        restart_day: None,
    });
    for l in &o.conflict.log {
        println!("{:7.2} {:<10} {:<10} {}", l.day, l.faction, l.kind, l.what);
    }
    for r in &o.rungs {
        println!("rung {:.2} {} {:?}", r.day, r.actor, r.step);
    }
    for a in &o.attacks {
        println!("attack {:?}", a);
    }
    println!("blocks broken {} reloaded {:?}", o.blocks_broken, o.reloaded_day);
    for (t, c) in o.plugin.commands.iter().take(60) {
        println!("cmd {:.2} {c}", *t as f64 / 24000.0);
    }
    println!("{:?}", o.conflict.stats);
    println!("check {:?}", o.conflict.check(&o.world));
}
