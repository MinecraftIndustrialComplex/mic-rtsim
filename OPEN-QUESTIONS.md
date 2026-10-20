# mic-rtsim: open questions for Ben

**Updated 2026-10-20 after Ben's morning review.** Every question from the overnight sheet is answered
or deferred. The answers are in DECISIONS.md under "Answers to the open questions". What is left:

## Design reviews coming (Ben approves each design before its code)

- **The glass-box factory model (C28).** How the Create graph is compiled: kinetics and stress, machine
  cycle formulas, belts, funnels, arms, pipes, filters, sequenced assembly, heat and trains. How it is
  solved and checked against the black box, and how addon machines get models.
- **The oil gate (P13).** Dragons need oil to breathe fire, so they nest near deposits. They sense
  industrial activity near oil and target it. An underground threat stops players drilling to oil from
  below. The gate follows deposits from any source: Diesel, Destroy, Deep Time. It must prove at least
  as dangerous as In Control's desert dragons before that spawner is touched.
  - Ready for review: [OILGATE.md](OILGATE.md), proposals OG-1 to OG-13.
- **Surplus money buys real things.** Surplus treasury coin buys visible village upgrades, buildings or
  tribute, instead of leaving the world.
  - Ready for review: [SINK.md](SINK.md), proposals MS-1 to MS-11.
- **News carried separately from cargo:** messengers and bands carry news on their own schedule.
- **The player ship voyage plugin (I17, C30):** player Sable ships, such as a sailbot carrying vaults
  between ports, simulated while unloaded.
  - Ready for review: [VOYAGE.md](VOYAGE.md), proposals V-1 to V-13.

## Deferred by Ben

- **I1 (and I23 later):** damage and repair both scale with progression. Early on, enemies break a belt
  or jam a machine that a wrench fixes. Late game, a kaiju razes a base and a power-hungry rewind device
  undoes it. Repair schematics are built now.
- **I19, I20:** the threat order and how natural-mob pressure is measured.
- **C27:** Born in Chaos balance through the sim (occasional shows of force).
- **P8 ships:** which Aeronautics ship mod, if any (research/aeronautics-ship-mods-2026-10-20.md).
- **Caravan balance fixes** (research/caravan-balance-2026-10-20.md), left until the soak.
- **M3, M4, C22:** villages advancing, declining and being founded, and the sim taking over structure
  generation.
- **Kaiju bodies** (P27 sets the trigger; Alex's Caves is out because it adds too much world content).
- **P25 regions per province,** and the **P24 final performance numbers:** from benches, then brought to
  Ben.

## Open facts, settled by GameTest

1. **Do Create trains run in other dimensions?** INDUSTRY.md reads Create's source as yes; older notes
   said overworld only. A Nether train loop with nobody there settles it.
2. **Do random ticks happen under chunk loaders?** `pack/modDocs/create-power-loader.md` says crops need
   a nearby player; INDUSTRY.md reads NeoForge as random-ticking under a ticking ticket. Crops in a
   loader-held chunk with nobody within 128 blocks settle it.
3. **Industry plugin paths no GameTest has run yet:** spawn observation with a real player (the test
   starts it by hand), an assembled Create Radars turret, a real scheduled train, and Ender Transmission
   on a Sable vehicle.

## Overnight choices

Ben reads `docs/overnight-choices-*.md` and DECISIONS.md "Overnight choices" himself. They stand unless he
objects.
