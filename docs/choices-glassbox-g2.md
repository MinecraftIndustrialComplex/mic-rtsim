# Glass box G2 choices (glass-box replay)

Every reversible choice made while building phase G2 (GLASSBOX.md §11.3), with ids G2-1,
G2-2, ... Anything that is Ben's call is in "For Ben" at the end, recommended option first.
G1's choices are in docs/choices-glassbox-g1.md (GB1 to GB111).

## Choices

- **G2-1 Report header 41.** The glass report (docs/industry-boundary.md §9) grows from 33 to
  41 header slots: 33 replay model (0 none, 1 black box, 2 glass), 34 since (tick, -1 never),
  35 regimes, 36 breakpoints, 37 oscillation fallbacks, 38 cut edges, 39 eligible (0/1), 40
  learned sources. The good rows follow at 41. Java's `GlassReport.HEADER` matches.
- **G2-2 Eligibility (G-B4).** A compiled district replays on the glass model once one window
  of its current compile has passed; before that, and after a recompile until the new compile
  passes, it freezes (no replay, the books stay as the last reconcile left them). A district
  that never compiled (not Create, or refused by the compiler) keeps the black box. A derate
  or a probe after a pass keeps eligibility; a demotion keeps eligibility and the solve uses
  the demoted model.
- **G2-3 Per-store pools.** A glass replay books into `PoolKey::Store(district, inventory,
  good)`, one pool per store and good (§4.1). Starting a replay reads each store's level from
  the inventory's stock; stopping carries each pool's change into that inventory's owed, so a
  reconcile lands it per store. A change that cannot be placed in its own store goes to the
  district's owed (reconcile::plan spreads it as today).
- **G2-4 One regime process per district.** A district's glass replay is one cluster member
  with one process per regime: the regime's net rate per (store, good), as Box terms for the
  district's sources and sinks and exact moves between stores, all-or-nothing. When a
  breakpoint stops it, the engine re-reads the regime from the pool levels, re-solves (the
  regime cache answers most of these) and replaces the member's processes in place. No slot
  period (C28).
- **G2-5 Held and room in replay.** In replay a store holds a good iff its pool is above 0
  (the window check's holding threshold is a live-world reading and has no replay meaning);
  room is the store's Space constraint (item slots weighted by stack size) and, for fluids, an
  Upper bound at its capacity.
- **G2-6 Restart latency.** A regime that raises any output waits the path latency (§4.4) of
  the path that changed before its raised rates apply, and runs the lower of the two regimes
  meanwhile (never above the control). A regime that only lowers rates switches at once.
- **G2-7 Breakpoints.** A breakpoint is a store at a bound (empty, full), a heater's phase
  timer, a controller's threshold on a replayed pool, a season boundary, or a periodic edge's
  arrival. More than `breakpointsPerEventMax` (64) breakpoints between two outside events marks
  the regime oscillating: the district falls back to the black box for that regime until the
  next compile or event (§4.5), and the report counts the fallback.
- **G2-8 Residual node.** The residual (G-B13, §5.1) is measured minus predicted per good over
  each passing window: exact goods 0, bounded goods clamp(measured, lo, hi) minus predicted,
  unbounded goods measured minus predicted. It is split over stores by each store's own
  window slope and replayed as a source (positive) or sink (negative) at that store. Positive
  residual counts as a learned source (I9, §5.3: measured above U goes to the residual).
- **G2-9 Paging.** The glass state needed for replay (graph records, eligibility, derates,
  demotions, residual, controller states, heater phases) pages with its district, since a
  compile only arrives while the district is loaded. A page format change: delete
  run-gametest/world in every project.

## For Ben

(Filled in as the work goes.)

## UNVERIFIED

(Filled in as the work goes.)
