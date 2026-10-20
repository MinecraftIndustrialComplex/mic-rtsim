# Choices: the kernel review bugs, and the modules on paging (PM)

Reversible choices made on 2026-10-20 by the paging-modules agent (branch `feat/paging-modules`),
for Ben's review (D10). Each one can be undone.

## The two open kernel review bugs

| # | Choice | Why |
|---|---|---|
| PM1 | **Two saves in flight.** A page now records the number of the latest snapshot that holds it (`Page::saving`, 0 for none), and each `SaveTicket` carries its snapshot's number. `saved` unpins a page only when its ticket is that latest snapshot; `save_failed` always marks its pages dirty again and unpins only its own. Test: `paging::tests::two_saves_in_flight_keep_the_later_change` (failed before the fix: the first ticket back unpinned a page the second snapshot still held, the budget evicted it, and the second save's failure lost its change). | The review's proposed fix. A later snapshot holds the newer bytes, so only its ticket says the page's state is on disk. Marking dirty on any failure is conservative: at worst a page is written twice. |
