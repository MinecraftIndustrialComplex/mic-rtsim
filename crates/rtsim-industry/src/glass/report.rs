// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the glass box reports per district (G1, read only): the solve's bounds per good and the
//! checksum's verdict against one window of live sweeps (GLASSBOX.md §3.5, §5.3; the meaning of each
//! number is docs/industry-boundary.md §9.3). Operators read it through `/rtsim district` and the
//! server log, GameTests through the JNI inspection call; nothing in the world shows it (I18).

/// How a good's prediction is judged (docs/industry-boundary.md §9.3).
pub mod class {
    /// Every rule on its paths is exact: judged against `in_l - out_l`.
    pub const EXACT: u32 = 0;
    /// A bounded rule, node or unconverged round on its paths: judged against [L, U].
    pub const BOUNDED: u32 = 1;
    /// A black box or a port on its paths: reported, never judged.
    pub const UNBOUNDED: u32 = 2;
}

/// A window's verdict for one good or for the whole district.
pub mod verdict {
    /// No completed window yet (or the good was not judged).
    pub const NONE: u32 = 0;
    pub const PASS: u32 = 1;
    pub const FAIL: u32 = 2;
}

/// One good's bounds and its checksum. Rates are per tick (items in units, fluids in mB).
#[derive(Clone, Debug, PartialEq)]
pub struct GoodReport {
    /// The good's id, as the district's records carry it.
    pub good: u32,
    /// [`class`].
    pub class: u32,
    /// Flow into stores: L (the fixed point) and U (the LP maximum). `f64::INFINITY` when unbounded.
    pub in_l: f64,
    pub in_u: f64,
    /// Flow out of stores, likewise.
    pub out_l: f64,
    pub out_u: f64,
    /// The predicted net change of the district's sampled stock, averaged over the window's regimes
    /// (controllers change regime between sweeps): `in_l - out_l` in a single regime.
    pub predicted: f64,
    /// The measured slope of the sampled stock over the window; `f64::NAN` before any window.
    pub measured: f64,
    /// The slope's standard error, including a stochastic source's model variance.
    pub sigma: f64,
    /// The batch quantization over the window, `b / W`.
    pub q: f64,
    /// [`verdict`].
    pub verdict: u32,
    /// Sweeps in the window.
    pub samples: u32,
}

/// A district's glass state for operators and tests.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Report {
    /// The compile the solve used.
    pub compile: u32,
    /// Coupling rounds the solve took (§3.2 step 4), and whether it converged before
    /// `glass.solveRoundsMax`.
    pub rounds: u32,
    pub converged: bool,
    pub nodes: u32,
    pub edges: u32,
    /// Wall time of the last solve, in microseconds (bench and operator use only; never in state).
    pub solve_micros: f64,
    /// The window the goods below come from: the last completed one if any, else the one in progress.
    pub window_start: u64,
    pub window_ticks_done: u64,
    pub window_ticks_target: u64,
    pub windows_completed: u32,
    /// The last completed window's district verdict ([`verdict`]): PASS only if every judged good passed.
    pub verdict: u32,
    /// Failed windows in a row (demotion after `glass.mismatchStrikes`).
    pub strikes: u32,
    /// Solves run for this compile (a regime change solves again, unless its regime was cached).
    pub solves: u32,
    /// Node kinds (model codes) demoted to measured in this district (G-B14).
    pub demoted: u32,
    /// The compile's longest path latency in ticks (part of the warm-up before a window).
    pub latency: u64,
    pub goods: Vec<GoodReport>,
}

/// Values per good in [`Report::to_f64s`].
pub const GOOD_STRIDE: usize = 12;
/// Values before the first good in [`Report::to_f64s`].
pub const HEADER: usize = 16;

impl Report {
    /// The flat form the JNI inspection call returns (`NativeEngine.glassReport`):
    ///
    /// - header (16): 0 compile, 1 rounds, 2 converged (0 or 1), 3 nodes, 4 edges, 5 solve µs,
    ///   6 window start tick, 7 window ticks done, 8 window ticks target, 9 windows completed,
    ///   10 verdict, 11 strikes, 12 goods, 13 solves, 14 demoted kinds, 15 latency ticks (13 to 15
    ///   were reserved 0 before the solver landed: choice GB29);
    /// - then per good (12): 0 good, 1 class, 2 in L, 3 in U, 4 out L, 5 out U, 6 predicted,
    ///   7 measured, 8 sigma, 9 q, 10 verdict, 11 samples.
    pub fn to_f64s(&self) -> Vec<f64> {
        let mut v = Vec::with_capacity(HEADER + GOOD_STRIDE * self.goods.len());
        v.extend_from_slice(&[
            f64::from(self.compile),
            f64::from(self.rounds),
            if self.converged { 1.0 } else { 0.0 },
            f64::from(self.nodes),
            f64::from(self.edges),
            self.solve_micros,
            self.window_start as f64,
            self.window_ticks_done as f64,
            self.window_ticks_target as f64,
            f64::from(self.windows_completed),
            f64::from(self.verdict),
            f64::from(self.strikes),
            self.goods.len() as f64,
            f64::from(self.solves),
            f64::from(self.demoted),
            self.latency as f64,
        ]);
        for g in &self.goods {
            v.extend_from_slice(&[
                f64::from(g.good),
                f64::from(g.class),
                g.in_l,
                g.in_u,
                g.out_l,
                g.out_u,
                g.predicted,
                g.measured,
                g.sigma,
                g.q,
                f64::from(g.verdict),
                f64::from(g.samples),
            ]);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flat_form_has_the_documented_layout() {
        let r = Report {
            compile: 3,
            rounds: 2,
            converged: true,
            solves: 4,
            demoted: 1,
            latency: 40,
            goods: vec![GoodReport {
                good: 7,
                class: class::BOUNDED,
                in_l: 0.0625,
                in_u: 0.1175,
                out_l: 0.0,
                out_u: 0.0,
                predicted: 0.0625,
                measured: 0.07,
                sigma: 0.001,
                q: 0.0005,
                verdict: verdict::PASS,
                samples: 120,
            }],
            ..Report::default()
        };
        let v = r.to_f64s();
        assert_eq!(v.len(), HEADER + GOOD_STRIDE);
        assert_eq!(v[0], 3.0);
        assert_eq!(v[2], 1.0);
        assert_eq!(v[12], 1.0);
        assert_eq!(&v[13..16], &[4.0, 1.0, 40.0]);
        assert_eq!(v[HEADER], 7.0);
        assert_eq!(v[HEADER + 1], 1.0);
        assert_eq!(v[HEADER + 3], 0.1175);
        assert_eq!(v[HEADER + 11], 120.0);
    }
}
