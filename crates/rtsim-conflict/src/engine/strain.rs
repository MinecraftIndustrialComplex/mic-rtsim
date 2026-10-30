// SPDX-License-Identifier: AGPL-3.0-or-later
//! A pool camp's strain (EQUILIBRIUM.md §2.4.5, slice S7, EQC200 to EQC212): what a player's
//! kills do to a camp whose warriors are a number, and the camp that breaks of them.
//!
//! - **Pressure** ([`Conflict::pool_bump`], [`Conflict::recruit_rate`]) slows the recruits.
//! - **A break** ([`Conflict::strain_check`], [`Conflict::break_camp`]): a camp that is thin
//!   (under `break_share` of its cap, home and away), under `break_pressure` or more of
//!   pressure, and idle (no party in any phase, no open demand) breaks. Its warriors and its
//!   recruits are gone, it sends nothing and is nothing to plunder, until the wait the shared
//!   rule gives (`min(refound_cap, refound_base x 2^P)` days) is over. The node, the market and
//!   the grievances stay as they were: a camp that broke is remembered by those who hate it.
//! - **A refounding** ([`Conflict::refound`]): a new band, a new chief, a quarter of a pool.
//!
//! A booked [`Ev::Strain`] is the strain's one wake: the recheck of a camp that was busy, or the
//! end of a broken camp's wait; its stamp voids any older one.

use super::*;

/// What `break_share x cap` is read with: a pool exactly at the share is not thin (strict), whatever
/// the share's float rounding.
const THIN_EPS: f64 = 1e-6;

impl Conflict {
    /// Whether the camp acts: false while it is broken (EQC209). A faction with no strain always
    /// does.
    pub fn camp_active(&self, f: u32) -> bool {
        self.factions[f as usize]
            .strain
            .as_ref()
            .is_none_or(|s| s.broken.is_none())
    }

    /// Whether the faction has nothing under way: no party in any phase (an envoy and a plunder
    /// band count), and no feud with an open demand (EQC208).
    fn camp_idle(&self, f: u32) -> bool {
        self.parties.iter().all(|(_, x)| x.faction != f)
            && self.feuds_of(f).iter().all(|e| e.1.demand.is_none())
    }

    /// Books the strain's wake at `t`, in place of any older one.
    fn book_strain(&mut self, f: u32, t: Tick) {
        let s = self.factions[f as usize]
            .strain
            .as_mut()
            .expect("a faction with a strain");
        s.stamp = s.stamp.wrapping_add(1);
        let stamp = s.stamp;
        self.schedule(t, Ev::Strain { faction: f, stamp });
    }

    /// Looks at whether a pool camp breaks now (EQC207, EQC208): it does when it is thin, under
    /// enough pressure and idle. Thin and pressured but busy, the look is booked a game day on
    /// (`strain_recheck_ticks`), once; otherwise whatever was booked is voided. Called after every
    /// pool bump, at the end of every band's homecoming and at the booked wake.
    pub(super) fn strain_check(&mut self, f: u32, at: Tick) {
        let Some(pr) = self.culture(f).pool_renewal else {
            return;
        };
        let Some(s) = self.factions[f as usize].strain.as_ref() else {
            return;
        };
        if s.broken.is_some() {
            return;
        }
        let x = &self.factions[f as usize];
        let have = f64::from(self.pool_at(f, at).floor() as u32 + x.away);
        let thin = have < f64::from(pr.def.break_share) * f64::from(x.cap) - THIN_EPS;
        if !thin || self.pressure_now(f, at) < pr.def.break_pressure {
            // Not looking any more: a booked recheck is void.
            let s = self.factions[f as usize].strain.as_mut().expect("checked");
            if s.check_at.take().is_some() {
                s.stamp = s.stamp.wrapping_add(1);
                self.site_touch(f);
            }
            return;
        }
        if self.camp_idle(f) {
            self.break_camp(f, at);
            return;
        }
        self.stats.break_deferrals += 1;
        let s = self.factions[f as usize].strain.as_mut().expect("checked");
        if s.check_at.is_some_and(|t| t > at) {
            return;
        }
        let t = at + self.tun.strain_recheck_ticks.max(1);
        self.factions[f as usize]
            .strain
            .as_mut()
            .expect("checked")
            .check_at = Some(t);
        self.book_strain(f, t);
        self.site_touch(f);
    }

    /// The camp breaks at `at` (EQC207): no warriors, no recruits, nothing sent, nothing booked
    /// for its feuds or its plunder, until the wait is over. The node, its market, the grievances
    /// and the pressure are as they were; the chief it had is not remembered (the refounding
    /// names a new one) and no news is made of it (FB-4).
    pub(super) fn break_camp(&mut self, f: u32, at: Tick) {
        let Some(pr) = self.culture(f).pool_renewal else {
            return;
        };
        let day = (at.0 / TICKS_PER_DAY) as u32;
        let pressure = self.factions[f as usize]
            .strain
            .as_ref()
            .expect("a pool camp")
            .pressure;
        let wait = pr.refound.delay_ticks(pressure, day).max(1);
        let due = at + wait;
        {
            let x = &mut self.factions[f as usize];
            (x.pool, x.pool_at) = (0.0, at);
            x.chief_out = false;
            x.cowed_until = Tick::ZERO;
            x.plunder_stamp = x.plunder_stamp.wrapping_add(1);
            let s = x.strain.as_mut().expect("a pool camp");
            s.broken = Some(Broken { since: at, due });
            s.check_at = None;
        }
        // Every booked wake of a feud is void, and a wait only the pool could end is over.
        for a in self.feud_actors(f) {
            if let Some(feud) = self.feud_mut(f, a) {
                feud.stamp = feud.stamp.wrapping_add(1);
                if feud.wait_until >= BLOCKED {
                    feud.wait_until = Tick::ZERO;
                }
            }
        }
        self.book_strain(f, due);
        self.site_touch(f);
        self.stats.breaks += 1;
        let (fid, camp) = (self.fid(f), self.factions[f as usize].pos);
        let p = f64::from(pressure.now(day, pr.def.half_life_days));
        self.tape
            .event(
                kinds::CONFLICT_BROKEN,
                spot(at, camp),
                fid,
                0,
                (wait / TICKS_PER_DAY) as i64,
            )
            .text(|| {
                format!(
                    "the camp breaks at P {p:.2}: back in {} days",
                    wait / TICKS_PER_DAY
                )
            });
        self.note(
            at,
            Some(f),
            "broken",
            format!(
                "the camp breaks at P {p:.2}: nothing sent for {:.1} days",
                wait as f64 / TICKS_PER_DAY as f64
            ),
        );
    }

    /// The strain's wake (`Ev::Strain`): a recheck of a busy camp, or the end of a broken camp's
    /// wait.
    pub(super) fn strain_event(&mut self, _w: &mut World, f: u32, stamp: u32, at: Tick) {
        let Some(s) = self.factions[f as usize].strain.as_mut() else {
            self.stats.stale += 1;
            return;
        };
        if s.stamp != stamp {
            self.stats.stale += 1;
            return;
        }
        if let Some(b) = s.broken {
            if at >= b.due {
                self.refound(f, at);
            }
            return;
        }
        s.check_at = None;
        self.site_touch(f);
        self.strain_check(f, at);
    }

    /// The broken camp's wait is over (EQC210). The refounding is the next step's.
    fn refound(&mut self, _f: u32, _at: Tick) {}
}
