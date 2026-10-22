# Conflict life: how factions decide to fight

**Status:** design for Ben's review, 2026-10-21; revised 2026-10-22 after an adversarial review and
MOWZIE.md's MZ-15. Nothing is built and no engine code changes here. Ben approves this design before
code. Branch `plan/conflict-life`, worktree `mods/mic-rtsim-conflictlife`.

**Ben's answers (2026-10-22).** CL-1 to CL-4 take (a), as recommended. CL-5 takes (b): "Add
alliances". Camps that share an enemy can ally and send joint war parties; CL-D19 is reversed, and
the alliance design is being added (section 8 has the answers).

**What the revision changed.** The engine's abstract fights are deterministic: the stronger side wins
(`village_outcome`, `engine.rs:4343-4365`; `threat.rs`, `rtsim-industry 403-427`). The first draft's
model rolled Lanchester odds as a chance to win, and so did EQUILIBRIUM.md's gate. The model now
fights as the engine does: a losing raid comes from a wrong belief, never from dice. Every number below
is from the corrected model. MZ-15 (MOWZIE.md on main) settles marches, and section 2.12 follows it.

**Ben's request.** He rejected all three ways EQUILIBRIUM.md offered for deciding when factions fight
(EQ-5: the gate, the gate plus a glory raid a season, and today's grievance ladder): "All of these
systems feel a bit too formulaic, come up with something that feels more life like." His scope is
"Only faction fighting": renewal and pressure stay as EQUILIBRIUM.md designs them.

**What it has to give.** The Nash equilibrium between tribes, animals and players that EQUILIBRIUM.md
asked for, in a world that lives: factions with wars of their own, and factions that come to hate a
player for what he did. The Frostmaw hates the railway and robs its trains (MOWZIE.md §4.3, §4.4).
The Umvuthana send an envoy to demand gold (MOWZIE.md §4.5). The Umvuthi leads invasions in person
(MZ-3 (b)). Nobody should be able to call a faction's next move to the day; two camps of one culture
should not behave alike; every fight should have a story. Over decades it stays stable: no runaway
wars and no permanent peace. Losing raids are rare but possible.

**Binding here:** D1 (late-game runtime cost on a weak PC: event-driven, closed forms, budgeted
memory and CPU), D3 (no bespoke entities, blocks or items), D4 (one generic engine; cultures are
data; the core needs no pack mod), P3 (the engine owns the truth), UR-1 (rtsim can be removed and
reinstalled safely; no backward compatibility before a release), the engine's seeded RNG only
(`StreamKey`), PLAN.md §10.3 (raids do not wait for the owner, C16 to C18), MOWZIE.md §4 (the envoy,
the rail meter, the lurk, the camp reconciler, and §4.16's march under MZ-15) and MZ-1 (a) (the Java
`ConflictBridge` in the core).

**Evidence:** a runnable model in `research/conflict-life-2026-10-21/` (`model.py` the world and the
three decision rules, `run.py` the measurements, `walk.py` the walkthroughs; `results/summary.md`,
`results/walkthroughs.md`, `results/stories.md`, `results/results.json`). It runs today's ladder
(EQ-5 (c)), EQUILIBRIUM.md §2.5's gate (EQ-5 (a)) and this design on the same world, 64 seeds of 50
game years each, plus 8 worlds left alone for 500 years. Section 4 has the numbers.

Calendar: a game day is 24,000 ticks; a game year is 96 days, four seasons of 24 (Serene Seasons).
"Y3 d14" is day 14 of year 3.

---

## 0. In one page

- **A camp is a few people, not a rule.** Each camp has a council of three: its chief, its war leader
  (the heir) and an elder. Each has a temperament drawn from the culture's (bold, aggressive,
  sociable) with a spread, and a standing in the camp. When the camp sits, one of them speaks, chosen
  by standing; his temperament carries the day and the other two temper it. A war leader whose raid
  succeeds gains standing; a lost raid hands the floor to the elder. Chiefs die, are deposed or fall
  leading a beaten attack, and the heir takes over with his own temper and a season to prove himself,
  a year if his chief was killed.
- **It wants things.** Food (each camp has stores that fill in summer and run down in winter), loot,
  revenge (the grievance today's engine already keeps), its honour (wounded pride after a refusal, an
  insult, a dead chief), glory (warriors grow restless; a new war leader must prove himself). It
  fears losses. Raiders also fear a neighbour grown too strong.
- **It knows things, roughly.** Each camp believes something about every neighbour's strength: a
  guess with an uncertainty, sharpened by its own fights, its scouts, its envoys and rumours, and
  blurring again over time. A base that destroyed one of its parties is at least that strong, and it
  remembers that for a while. It knows roughly what each target holds. It remembers up to four things
  about each neighbour ("player Bea killed Vorr, our chief, in the spring of year 3"), fading with
  time. It keeps a record of who pays.
- **It sits when something happens.** It was attacked, its envoy came back refused, word came that a
  neighbour is weak or a player is away, a caravan or a train passes. A hothead calls the council
  within a day or two of an insult; a cool chief lets it rest a week or more, and may let it pass.
  Otherwise it sits when its warriors grow restless, sooner when it is hungry, angry or proud.
- **It chooses like people do.** For each neighbour it weighs what it could do (wait, warn, send
  scouts, ask tribute, raid with a band, a raid or an invasion, fall on a rival camp's hunters, ambush
  a train or a caravan, offer a truce) against one guess drawn from its belief. Then it picks among
  staying home and its best idea for each neighbour by a softened best choice: usually the best,
  sometimes the second best, rarely something rash. A bold speaker guesses its enemies weaker.
- **The fight is the engine's, and certain.** The stronger side wins, as today. A beaten party is
  destroyed, and a chief who led it falls with it. A losing raid is a council that guessed wrong.
- **Terms first, when terms make sense.** An envoy asks for a price between what the war is worth to
  the camp and what the camp believes the target would lose. A target who pays buys a truce sworn by
  the chief who took the tribute; breaking it is costly to him and less to his heir. A camp that has
  been paid for years does not lightly kill the goose.
- **Every fight has a reason, and the reason is kept.** The engine keeps the motive that drove each
  party and the memory behind it, so an envoy can say what it is owed and why, and a notice can say
  who is angry and for what.

What the player sees, in the model's numbers (section 4):

| Player | Today's ladder | The gate (rejected) | This design |
|---|---|---|---|
| A walled base (defence 80) near three camps, never pays | 2.7 fights a year on him, every one lost, 39 attackers killed a year | 0.8 a year, 99% lost, 16 killed a year | 0.13 a year (one in eight years), every one lost, 3 killed a year: a new neighbour's first test (in 3 worlds of 4 within two years), a feud after a chief fell at his walls, word that he was away |
| A weak base (defence 6) that pays when the price is no more than a raid would take | 14.7 wars a year in the last decade: in some camps today's price outgrows the raid, and one refusal is war for good; 138 tribute a year | no war; 329 tribute | 0.1 fights a year; 177 tribute and 30 loot a year, 43% of his income |
| A weak base that never pays | 23.7 wars a year | 10.4 | 3.4, which take all his income |
| A player who hunts a goblin camp and kills its envoys (defence 50) | 7.5 fights a year on him | 3.3 | 0.33: the attacks that find him away win (25 loot a year, 5% of his income); the rest break on his walls |
| A railway at 4 legs a day past a goblin camp and a Frostmaw den | no robbery | no robbery | 2.9 train ambushes a year: 2.5 by the goblins (one in seven lost to the guards the owner puts on) and 0.36 Frostmaw lurks; 0.7 caravan ambushes; at 12 legs, 1.9 lurks (section 3.3) |
| Nobody: the world left alone | 30 fights a year, 10 between camps and every one of those lost, every world a forever war | 12.6, 2.1 between camps, 75% of those lost | 7.3 a year, 1.2 between camps, 12% of those lost; wars that start and end |

---

## 1. Why the three rules felt formulaic

| Rule | What it does | Why it reads as a machine |
|---|---|---|
| Ladder (today, EQ-5 (c)) | grievance crosses a threshold, the feud climbs a rung after a fixed dwell; a war rung repeats at its dwell; a Poisson clock sends a plunder at the nearest hated node | the same state gives the same act on the same day (a refused envoy is followed by a raid 9 or 10 days later, every time); every goblin camp acts alike; a camp throws raid after losing raid at a fortress (8.8 a year in the duel, every one lost); the plunder clock sends a band of 8 at the same camp forever and loses every time, so every world has a forever war; nothing in it remembers why |
| Gate (EQ-5 (a)) | sends a party only when it is worth sending against the faction's belief | its belief of the odds is the square law's, A² / (A² + d²), but the engine's fight is certain: it sends parties that cannot win (75% of its camp fights lost, 99% at the fortress) and leaves alone targets it would beat; a refused demand is followed by a raid 3 days later, every time |
| Gate plus glory (EQ-5 (b)) | the gate, plus one raid a season | a raid a season is a calendar |

The fix is not more randomness on a rule. It is a decision maker with state of its own: needs that
rise and fall, beliefs that can be wrong, a memory, and more than one voice. Behaviour then varies
because the state varies, and each act can name the state that caused it.

---

## 2. The model

The model is `research/conflict-life-2026-10-21/model.py` (class `World`, mode `life`). Numbers in
brackets are the goblin culture's; each culture's are in `Culture` there and become a `mind` block in
its JSON (section 2.13). Every number is TUNABLE.

### 2.1 What stays from today, and the fight

- **Grievance** in closed form, dG/dt = r - λG, with today's signals, weights, tolerances and
  half-lives (`grievance.rs`, `data/cultures/*.json`), and the rail meter (MOWZIE.md §4.3). It is no
  longer a ladder: it feeds revenge and wakes the council (2.4, 2.5).
- **Parties** as today: envoy, scouts, raid, invasion, plunder (here called a band), and the ambush
  (`SYS_AMBUSH`, `attack_train`), with today's sizes, strengths and carries, today's marches (with
  MZ-15's, section 2.12), the raid director when watched and the abstract fight when not (PLAN.md
  §10.3, C16 to C18).
- **Pressure and renewal** (EQUILIBRIUM.md §2.4.5, EQ-6 (a)): warriors killed by a player slow the
  camp's recruits by 2^P; a farmed camp breaks and refounds. Factions never break each other.
- **Chief and heir** as notables (`Faction::chief`, `heir`, `rtsim-knowledge::Notable`), the heir
  inheriting grudges (C24), the Mowzie chiefs with `chief_lifespan_days` 1,000,000 (MOWZIE.md §4.2).

**The fight is deterministic, as today.** An attack of strength A on a defence d:

| | Camps, villages, caravans, the railway (`village_outcome`, linear) | A player's base (`threat.rs`, square) |
|---|---|---|
| The attack wins when | A > d | A > d |
| Share of the attackers lost on a win | d / A | 1 - sqrt(1 - (d/A)²) |
| On a loss | the party is destroyed | the party is destroyed |
| Defenders lost when they hold (new) | A / d of those who met it | 1 - sqrt(1 - (A/d)²) |
| Defenders lost when they break (new) | min(1, A / 2d) of those who met it | (a player's defenders are not counted) |

Defender losses are new: today a raid on a haven or a village kills none of its defenders. Here a
camp that holds pays for it, and a camp that breaks loses up to all of the guard that met the attack.
The only fight that is drawn is one a player attends in person outside the raid director: waking the
Frostmaw while he steals its crystal is Minecraft's fight, and the model draws it at A² : d² odds.

**The strengths** (today's culture data and MOWZIE.md §4.5; a camp's defence is section 2.9's):

| Culture (members) | Band | Raid | Invasion | Home strength a member | A full camp's defence |
|---|---|---|---|---|---|
| goblin (60) | 8 at 7.6, carry 56 | 12 at 11.2, carry 80 | 26 at 30.4, carry 192 | 1.4 | 20.9 |
| raider (50) | 8 at 10.8, carry 64 | 12 at 16.2, carry 96 | 23 at 36.0, carry 180 | 2.0 | 25.3 |
| bandit (40) | 8 at 9.2, carry 76 | 10 at 11.2, carry 104 | 20 at 22.4, carry 208 | 1.65 | 16.5 |
| Umvuthana (7) | none | 3 warriors at 0.4 and 1 raptor pack at 2.6: 3.8 | 6 warriors and 2 packs, 7.6, and the Umvuthi riding with it, 5.0: 12.6 | 1.4 | 9.8, and the Umvuthi on its throne 5.0: 14.8 |
| Frostmaw (1) | | 41.7 for any war act, carry 64 | | | 50 at its den |

Targets: a village 4; a caravan 3 (carrying 60); a train 1 bare, 20 with guards (carrying 64); the
scripted players 80 (the fortress), 50 (the hunter), 30 (the farmer and the railway's owner), 8 (the
bystander), 6 and 12 (the duels); a player who is away has half his defence.

### 2.2 The council

Three voices: the **chief** (standing 1.0), the **war leader** (0.7), who is the heir, and the
**elder** (0.6). Each voice has a temperament drawn once from the culture's mean with the culture's
spread (goblin bold 0.55, aggressive 0.6, sociable 0.35, spread 0.2). The war leader is bolder and
more aggressive than the mean (+0.12, +0.15) and less sociable (-0.1); the elder the reverse (-0.15,
-0.15, +0.1). Two goblin camps in one world therefore have different councils, and one camp's council
changes when its chief changes.

**The speaker.** At each sitting one voice speaks, drawn with probability proportional to standing.
The council's temperament for that sitting is 0.6 of the speaker's and 0.4 of the council's
standing-weighted mean. So the same camp, in the same state, is bolder on the days the war leader
has the floor.

**Standing moves.** A won fight multiplies its speaker's standing by 1.25 (at most 3); a lost one by
0.7 (at least 0.2), and the elder gains 0.3. A hothead who wins keeps winning the argument until he
loses.

**Succession.** A chief's tenure is gamma-distributed (shape 3, so a chief seldom falls in his first
weeks) with today's `chief_lifespan_days` as its mean (goblin 120, bandit 90, raider 150); the end
is told as a fever, a quarrel, a hunting accident, a deposition, or old age for one who has led
longer than the mean. A chief who leads (a raider captain leads his outpost's invasions; the Umvuthi
leads its grove's) falls with a party that is destroyed, and in a win with a chance of a tenth of the
share his party lost: he is guarded. A player who hunts a camp kills its chief in about one hunt in
twelve. The war leader becomes chief and a new war leader is drawn; for a season (24 days), or a year
if the old chief was killed, the new leaders' glory counts double. The new chief keeps a share of
each grudge (0.5 + 0.5 x his aggression). A chief killed by someone adds a wound to the camp's pride
toward the killer (W + 1), grievance (+40) and a memory of weight 3. The Frostmaw is a council of
one, and its heir comes after its wait (EQ-4 (b)); the Umvuthi's after its own (MZ-14 (a), 2.10).
Chiefs turn over at about today's rates: goblin 0.79 a camp-year, bandit 1.05, raider 0.64, a few of
them fallen leading.

### 2.3 Drives

All of these are numbers the engine already has or can keep in closed form between events.

- **Hunger.** A camp has stores. They fill with the season (spring 1.3, summer 1.5, autumn 1.2,
  winter 0.3 times the camp's full-strength needs, times a year's luck drawn once a year, 0.5 to 1.5)
  and run down at the camp's eating rate; they hold at most 45 days. Hunger is 0 with 25 days of
  stores or more and rises to 1 at none. Hunger drives the council and nothing else: a camp out of
  stores loses no members and recruits as before (Ben's scope: renewal is EQUILIBRIUM.md's). A lost
  hunting ground (about once a decade, for 96 to 192 days) halves the income. Loot is worth
  0.2 + greed x hunger per item (greed 1.6) while it fits in the stores, 0.03 beyond.
- **Revenge.** rev_max x (0.5 + aggression) x g / (g + 100), from the grievance g (rev_max 10).
- **Pride.** Wounded pride W toward each neighbour, 0 to 2, halving over pride_half days (goblin 12,
  bandit 24, Umvuthana 48, raider and Frostmaw 96). It rises on a refused envoy (+0.5; raider 0.8,
  Umvuthana 0.9), a killed envoy (+0.8; raider 1.0, Umvuthana 1.2), a raid on the camp
  (+0.6 x (1 - W/2)), a skirmish or a border clash (+0.3 x (1 - W/2)), a player killing members
  (+0.2 to +0.5), a stolen crystal (+1.5), a dead chief (+1), a dead Frostmaw (+2, to its heir at half).
  Its pull is pride_w x (0.5 + aggression) x min(W, 1.5) (pride_w 12), weighted by the hope of
  answering it, 0.25 + 0.75 x the believed chance to win: a hopeless attack answers little.
- **Glory.** glory_w x restlessness x (2 while a new war leader proves himself) x (0.5 + boldness)
  (glory_w 5). Restlessness grows from 0 to 1 over restless_days (96) since the last fight, scaled by
  how full the camp is.
- **Fear.** Rises with the camp's own losses (twice the share of its cap lost, +0.2 for a lost
  fight) and a dragon strike (+0.5); halves over fear_half days (40). It scales every expected loss
  by 1 + fear_w x fear x (1.5 - boldness) (fear_w 2.5).
- **Preemption** (raiders only, preempt_w 10): a neighbour camp believed to have more than 1.2 times
  the outpost's members is a threat to strike first.
- **Costs.** warrior_value (3) per expected member lost, times the fear factor, where a party that
  loses is lost whole; 0.3 per member mustered; the culture's effort (2) per party.
- **Penalties.** A truce sworn by the sitting chief costs oath x (1.5 - aggression) x 3 to break
  (oath 20, raider 10), and a third of that for any later chief. Raiding a player who has paid more
  than once costs 12 x the share of demands he paid: it ends the tribute. Falling on a camp of one's
  own culture costs 25. Falling on a camp with no old hatred, no grievance and no wound costs up to 8:
  it makes an enemy.

### 2.4 Beliefs and memory

**Strength.** For each neighbour the camp holds a log-normal belief of its defence (a mean and a
variance of its logarithm). It starts near the truth for camps and villages it has always known
(an error of about 30%) and wide for a new player base (a guess of 15 with a variance of 1, so
anything from 5 to 40 is plausible). It sharpens by a Kalman step on each observation:

| Observation | Error (sd of the log) |
|---|---|
| A fight with the target | 0.15; against a player who was away, it guesses him twice what it met, at 0.3 |
| Scouts | 0.25; at the railway the guards ride in a closed car, and the watch misses them half the time |
| An envoy that walked to the base and came back | 0.3 |
| A rumour: a neighbour's losses, a dragon strike, the guards taken off the trains (arrives after Exp(6) days) | 0.5 |

It blurs again by 1/384 of variance a day toward a variance of 1, so a camp that has not looked at a
camp for a year or two is unsure of it again. A player's base blurs at a quarter of that rate: walls
do not fall down.

**What a defeat taught.** A target that destroyed a party of strength A is at least that strong (2A
if the player was away: his walls alone did it). The camp's guess is never below this floor, which
halves over 4 x pride_half days. Without it a camp tested the same fortress again as soon as its
belief blurred.

**What a target holds.** The camp keeps the stock it saw at its last look (a fight, scouts, an envoy)
and adds the refill it knows such places have: a village 1 item a day up to 150, a player 5 a day up
to 400; a camp's stores as last seen. It plans a raid's take against that, not against the truth.

**Away.** Word that a player has left his base (half of his trips are seen, arriving after Exp(4)
days) does not change what the camp believes of his walls; it marks him away for 3 to 10 days, during
which the camp guesses his defence and the floor at half (his walls are home, he is not). PLAN.md
§10.3: raids do not wait for the owner.

**Trust.** A Beta record of how often a player pays: +1 to one side per demand paid or refused, both
fading toward even over 192 days.

**Memories.** Up to four per neighbour, each with a weight that fades (half-life 4 x pride_half),
the strongest kept: "we lost 26 at player Ben", "player Bea killed Vorr, our chief", "player Ben
refused our envoy", "the Grey Outpost raided us", "Crowsnest's hunters killed 2 of ours at the
border", "player Ben stole the crystal", "player Ben killed the Frostmaw before me", "the railway
drove us off", "word came that the trains ride without guards", and the kind ones, "player Bea paid
87". A grievance says how angry; a memory says why.

### 2.5 When the council sits

| Occasion | When | Sat on after | Attended |
|---|---|---|---|
| Provoked | members killed by a player or a camp, a raid on the camp, a border clash; a stolen crystal (the Frostmaw finds it gone on waking: 1 + Exp(3) days); a new Frostmaw toward a player its predecessor hated (W > 0.3) | 1 + Exp(2 + 4 x (1 - the chief's aggression)) days | 0.5 + 0.5 x the chief's aggression |
| Refused | an envoy came back refused or did not come back | 2 + Exp(3 + 9 x (1 - the chief's aggression)) days | 0.3 + 0.6 x the chief's aggression |
| News | word of a neighbour's weakness, a player away, the guards gone | the day it comes | 7 in 10 |
| Caravan | a caravan passes the camps on the road (on a third of days) | at once | 3 in 100 |
| Train | a leg through a camp's or den's range that it notices (5 in 100 legs) | at once | always |
| Restless | its own clock: Exp with mean wake_days / (1 + 2 hunger + restlessness + 0.8 W_max + 0.5 g_max / (g_max + 50)) (wake_days 12) | | always |

An occasion puts one neighbour before the council; a restless sitting weighs all of them. A day with
an attended occasion has no restless sitting as well. A council whose party fought in the last two
days does not sit (in the engine: a feud whose party is still out is skipped, as today). An insult a
cool chief lets pass is not forgotten: its grievance and pride shorten the restless clock.

### 2.6 How it chooses

For each neighbour before it, the council draws one guess of its defence from the belief, shifted by
boldness, halved while a player is away, and never below the floor:

    D̂ = max(exp(μ + σ z - (boldness - 0.5)), floor)

This is Thompson sampling: an uncertain belief sometimes produces a guess low enough to try, which is
how a camp tests a new base and how it is sometimes badly wrong. The engine's fight is certain; the
council's count of the enemy is not. So for each war act of strength A it believes

    P(win) = Φ(ln(A / D̂) / 0.3)       (battle doubt 0.3)
    lost on a win = min(D̂ / A, 0.99) (linear), or 1 - sqrt(1 - q²) with q = min(D̂ / A, 0.99) (square)
    expected members lost = n x (P(win) x lost on a win + 1 - P(win))
    take = min(carry x (1 - lost on a win), the stock it believes the target holds)

and scores

    u(war act) = P(win) x value(take) - cost + revenge + pride x hope + glory + preemption - penalties

It keeps the best act for each neighbour. The sitting then chooses among staying home (u = 0) and
each neighbour's best act by a logit (quantal) choice with the culture's temperature τ (goblin 4,
bandit 2, raider 2.5, Umvuthana 2.5, Frostmaw 3), scaled by 0.75 + 0.5 x aggression: an aggressive
council is more erratic. With τ near 0 this is a gate that knows its own doubt; with τ large it is a
dice roll.

The model logs, for each act, every motive's share of the push: a fight's story names each motive
that was at least a quarter of it.

### 2.7 The acts

| Act | Toward | In the game | Carried by |
|---|---|---|---|
| Wait | | nothing | |
| Warn | a player, the railway (at most once in 16 days each), a camp the council is angry with | a goblin howl, a Frostmaw roar, the gong, a raider war band shown at the edge, a bandit's written warning | today's FACTION_NOTICE (0x8801) and MOWZIE.md's NOTICE_SHOWN (§4.12) |
| Scout | anyone but a caravan | scouts at the base, or a raptor look; scouts seen by a defence stronger than them lose half as they run; toward the railway, a watch on the line (about 21 a year at 12 legs in seed 1003) that misses the guards half the time | today's Scouts party; toward the railway kept abstract, no party |
| Demand | a player | an envoy with guards asking the culture's tribute good (goblin emerald, Umvuthana gold) | today's Envoy party, ENVOY_MATERIALIZE (0x8802) and `Demand`; MOWZIE.md §4.5 |
| Band | a village, a camp, a player | a small party (today's plunder party) | today's Plunder party kind |
| Raid | a village, a camp, a player | today's Raid party | today's Raid |
| Invasion | a village, a camp, a player | today's Invasion party; a raider captain leads it and falls if it is destroyed. The Umvuthi leads its tribe's in person, only when its pride is wounded (W of 0.8 or more), and there is no Umvuthana invasion while the grove has no Umvuthi. Riding with the band, its 5.0 is added: 12.6 for a full invasion (MZ-3 (b); MOWZIE.md §4.5 under MZ-15) | today's Invasion; the chief's stack (MOWZIE.md ledger 4) |
| Skirmish | a rival camp | a band falls on the rival's hunters in the field: it meets the share of the camp out hunting (at least 15%, all of a camp of 4 or fewer) and takes a quarter of a band's carry | a band party whose target is the rival's territory edge |
| Ambush | a train, a caravan | the Frostmaw's lurk by the track (MOWZIE.md §4.4); a goblin, bandit or raider band lying in wait by a track or a caravan road | a train: MOWZIE.md §4.4's lurk and AMBUSH_SPOT, given to every culture with an ambush weight, and `attack_train`; a caravan: new (2.15) |
| Truce | a rival camp | nothing seen; the two camps stop fighting for Exp(192) days, sworn by both chiefs | new feud fields (2.11) |
| Hunt | a village (Umvuthana only, when hungry) | raptor packs take a district's animals | MZ-8's owed hunts |

Siege stays in the culture data but is not chosen in this design (section 7, CL-D14).

### 2.8 Terms: the envoy

An envoy names a price only where its war threat is credible (a believed chance to win above 1 in 4)
and no tribute is running. The camp's lowest price, T_min, is what the war is materially worth to it
(the war act's score without revenge, pride, glory and preemption, in items at today's value of an
item). The highest is X = P(win) x take: the loot the camp believes it would carry home. A raid takes
goods and nothing else, so there is no damage term on top. If T_min is below X, the envoy asks
somewhere between, nearer the top the more aggressive or hungry the speaker:
T = T_min + (0.25 + 0.4 aggression + 0.15 hunger) x (X - T_min). This is the Nash bargaining range:
both sides are better off with the payment than with the war. The demand's own score is
trust x value(T) x (1 + demand bias / 10) + (1 - trust) x 0.9 x the war's score, less the envoy's
effort (demand bias: goblin 0, raider 2, bandit 6, Umvuthana 10), so a camp asks a payer and fights a
refuser, and a bandit or Umvuthana camp asks first more readily than a goblin one.

The envoy party walks to the base and back: the camp learns the base's defence and its stock (2.4)
whether or not he pays. A payment soothes the grievance (by the culture's relief, at least 0.3), the
pride (x 0.3), adds to trust, and buys a truce of 24 to 72 days (mean 48) sworn by the chief who took
it. A refusal adds to pride and grievance (+10) and a memory, and is an occasion; killing the envoy
adds more (+30) and costs the camp a member, a player's loss for its pressure.

The scripted players who pay (section 3) pay when the price is no more than what the threatened party
would truly take at their real defence, which is nothing if it would lose (`World.expected_loss`).
So a camp that overrates itself names a price the player refuses, and learns from the refusal.

### 2.9 What keeps the world stable

Every escalation has a brake that comes from the state, not a cap:

- **A camp's defence is its guard, not its whole people.** A camp attacked meets a quarter of the
  members at home (guard share 0.25: today's Haven defence, `pool_cap / 4` at `haven_defence_per_pop`
  1.0) at its home strength: a full goblin camp 20.9, a raider outpost 25.3, a bandit camp 16.5. A
  grove stands all its warriors around the throne and the Umvuthi with them: 14.8. Without the guard
  share every full camp beat every invasion, and camps never fought each other.
- **Losses.** A lost fight destroys the party, adds fear, hands the floor to the elder, and teaches
  the camp a floor: the target is at least that strong. A raid that wins on a camp costs the
  defenders (2.1), which feeds the defender's grievance and pride.
- **Wins.** A won fight halves the grievance and takes most of the wound away: vengeance sated.
- **The walk home.** Survivors are on the road home for 1 to 3 days (MOWZIE.md §4.16's pace, about
  480 blocks a day), and a chief who led and won rides back with them. Until they are home the camp's
  defence and its next muster lack them, and a grove lacks the Umvuthi's 5.0.
- **Pressure.** Members killed by a player slow recruits (EQUILIBRIUM.md §2.4.5).
- **Hunger and seasons.** A fed camp values loot little: most fights come in winter and after bad
  years, and stop when the stores are full.
- **Truces.** A camp afraid or sociable enough offers a truce, and the other side accepts if its
  chief is sociable or afraid enough to outweigh its wounded pride, or one time in five; both chiefs
  swear it. In 64 worlds of 50 years, 1,562 truces between camps were sworn; the chiefs who swore them
  fought the other camp 55 times while they ran, later chiefs 261 times.
- **Peace cost and kin cost.** Making a new enemy, or fighting one's own culture, is dear.
- **Clashes keep it from going quiet.** Hunters of two hostile neighbouring camps meet about once in
  two years when both are fed, up to two or three times a year when both starve (rate 1 a year x
  (0.5 + hunger + hunger)); one side loses one or two members. Each clash is a provocation with a
  memory, and some become feuds.
- **Dragons** (world physics in the model, not a decision): a strike about once in four years a camp
  kills 30 to 60% of it, burns half its stores and frightens it; word of its weakness reaches its
  neighbours about 6 days later.

### 2.10 The Mowzie cultures

- **The Frostmaw** is a council of one with no recruits: 41.7 when it goes out, 50 at its den. Its
  life is worth 60 items to it, at the risk of every lost fight ((1 - P(win)) x 60 x the fear factor,
  plus an effort of 14). What it can do: roar (warn), lie in wait by a track (ambush), raid a player
  who has wronged it. The rail meter's grievance (8.66 x (legs - 2) at equilibrium, MOWZIE.md §4.3)
  is its revenge; each train it notices is an occasion. A stolen crystal is an insult (+1.5 pride,
  +60 grievance, a memory of weight 3) and a provocation the morning it wakes. A thief who wakes it
  fights it in person. Its party is itself, so it dies in every abstract fight it loses. Its killer,
  whoever he is, gets +80 grievance, +2 pride and a memory of weight 3 on the den's record. Its heir
  comes after `min(192, 6 x 2^P)` days (EQ-4 (b)), inherits half of every grudge, and arrives with a
  provoked occasion toward any player it bears a wound above 0.3; its restless clock went stale while
  the den stood empty, so it often sits the day it arrives. The Frostmaw hates the man who killed its
  predecessor, and the farmed den now strikes back (3.2).
- **The Umvuthana** ask gold (MOWZIE.md §4.5), hunt villages' animals with raptors when hungry
  (MZ-8), and beat the gong. The grove is seven warriors in the model; its council is the Umvuthi and
  two masked warriors, named by their masks (the Fury mask, the Bliss mask: MOWZIE.md's masks). A raid
  is 3 warriors and a raptor pack (3.8); an invasion is 6 warriors and 2 packs led by the Umvuthi in
  person, 12.6 with its 5.0, and only when its pride is wounded (W of 0.8 or more; MZ-3 (b)). It does
  not die of age. Warriors refill one at a time, each after `min(96, 6 x 2^P)` days (EQUILIBRIUM.md
  §2.4.7). An Umvuthi that falls (with a destroyed invasion, or at a player's hand) leaves the grove
  without one for the later of its cowed time and `min(192, 6 x 2^P)` days; then a warrior takes up
  the mask and the grove's count falls by one (MZ-14 (a)). No Umvuthi, no invasion. MOWZIE.md gives
  the Umvuthana a carry of 0: they take no goods. The model's carry of 24 for a raid and 48 for an
  invasion is the value of the animals MZ-8's raptor packs are owed, 2 animals a pack at 12 items an
  animal (TUNABLE), so the council weighs a hunt's worth, and the engine books owed hunts, not cargo.

### 2.11 In the engine

The changes are mostly in `rtsim-conflict`, but not only: `rtsim-world` answers a query for caravans
passing a camp's reach and takes the robbery's loss of cargo into the economy, and the Java plugin
shows the reason lines (below). The parties, the raid director and the reconciler see the same party
kinds they see today.

**Feuds toward camps and villages.** Today a feud is a faction's grievance against an actor (a player
or the settlers), and camps and villages meet only the plunder clock. Under this design each camp
also keeps a feud with each camp and village in its plunder reach, keyed by that node's actor
(`Faction::actor` for a camp; the village's own actor). The feud folds into the faction's row as
today (PM-C2).

**New state.**

| Where | Field | Bytes | Notes |
|---|---|---|---|
| `Faction` | elder seed | 8 | chief and heir exist; temperaments come from seeds (`Temperament::from_seed`) |
| | standings (3) | 6 | u16 fixed point |
| | stores, fear, at an anchor day | 12 | closed form between events (season income is piecewise constant) |
| | last fight, heir until, ground until | 12 | days |
| | sittings | 4 | keys the sitting's random stream |
| | last three chiefs' name indexes | 3 | a dead chief's name does not come back at once |
| `Feud` | belief: mean and variance of the log | 8 | |
| | trust: paid and refused | 8 | fading toward even (192 days) |
| | wounded pride W | 4 | halves over pride_half, read at the grievance's anchor |
| | memories (4) | 32 | 8 bytes each: kind u8, flags u8 (hostile, and so on), count u16 (saturating; a name index for a dead chief), weight u16, day u16 |
| | away until, truce until, last warning, paid until, refused at | 20 | days |
| | the swearing chief | 4 | the chief's ordinal (`Faction::chiefs`) |
| | the defeat floor: strength and day | 8 | |
| | believed stock: amount and day | 8 | |
| | guards news | 4 | the day word came that the trains ride bare (a railway feud) |

About 45 bytes a faction and 96 a feud. A memory is a code and its numbers, not text: the story is
written when it is read (2.14). The parties on the road home are today's `Party` records.

**Events.** `Ev::Plunder` and its Poisson clock (`schedule_plunder`, `SYS_PLUNDER`) become
`Ev::Council { faction, stamp }`, booked from `StreamKey(seed, SYS_COUNCIL, key).slot(n)` at the
restless clock's mean, re-booked when hunger, pride or grievance move it. An occasion books an
`Ev::Council` with its kind and the neighbour at its delay (2.5). The ladder's `Ev::Feud` climbs
become sittings: a feud's grievance no longer schedules acts. Border clashes are one booked
`Ev::Clash { faction }` per camp with a hostile neighbour, at the sum of its pairs' rates, from
`SYS_CLASH`. The fight stays `village_outcome` and `threat.rs`; defender losses and a leading chief's
fall are new lines after it.

**Randomness.** Every draw in a sitting (the speaker, each neighbour's guess, the logit choice, the
price, a truce's length) comes from `StreamKey(seed, SYS_COUNCIL, faction key).slot(sitting)`, so one
faction's draws never shift because another faction sat more or less often. A sitting reads its
neighbours' state (a defence it fights, news it hears), so the world is deterministic in the wheel's
total order, as the engine is today, and replays depend on that order and nothing else. The model
uses one numpy generator for the whole world; the property tests check the engine's keyed streams
(section 6).

**What the Java side needs.** Nothing new to carry the acts: envoy, scouts, raid, invasion, plunder,
the lurk and notice are today's records or MOWZIE.md's planned ones, carried by the `ConflictBridge`
(ledger 16, MZ-1 (a)). This design adds no record kind and no record id (2.12). Two records gain the
reason, 16 bytes each (reason u8, motive u8, memory kind u8, flags u8, count u16, day u16, the other
party u64: an actor, or a chief's name index):
- **An envoy speaks its reason.** ENVOY_MATERIALIZE (0x8802, `boundary.rs:36`) grows from 104 + 8m
  to 120 + 8m bytes; the plugin shows one line of chat to the player it addresses when it arrives:
  "You killed Vorr, our chief, in the spring of year 3. Pay 106 emeralds." (CL-1).
- **A notice carries its reason.** FACTION_NOTICE (0x8801, `boundary.rs:35`) grows from 64 to 80
  bytes, shown with the howl, roar or gong.
No backward compatibility is needed before a release. A truce, a skirmish between camps, a border
clash and a caravan ambush are unwatched in nearly every case and need nothing from Java: a skirmish
or a caravan ambush a player meets is a band party on its march, as a plunder is today.

**What it waits for.** Several pieces this design uses are MOWZIE.md's and not built yet: the
`ConflictBridge` (MZ-1 (a)), the lurk (§4.4) and AMBUSH_SPOT (0x8807), TRAIN_ROBBED (0x8809),
RAIL_PASS (0x080C) and the track sampler (MZ-12), NOTICE_SHOWN (0x0811), CHIEF_STATE (0x0091, the
crystal held) and BAND_MARCH (0x880B, MZ-15). Build order:
1. The council on the engine's harness (`scenario.rs`), with no Java: feuds toward nodes, the
   events, drives, beliefs, the choice, the price, truces, succession, defender losses, story codes.
   It drives today's party kinds and raid director. Tests 1 to 11, 15, 16 and 18 to 20 run here.
2. Persistence of the new fields, and the paged sweep (test 17).
3. The reason fields on ENVOY_MATERIALIZE and FACTION_NOTICE, and their GameTests.
4. With MOWZIE.md's lurk, AMBUSH_SPOT, RAIL_PASS, TRAIN_ROBBED and the sampler: the lurk for every
   culture with an ambush weight (test 12).
5. With CHIEF_STATE: a stolen crystal as a Theft report (test 14).
6. With BAND_MARCH and MZ-15's march: the walk-in and the quota's walk-in (2.12).
7. Caravan ambushes: the `rtsim-world` query and the robbery report.

**UR-1.** The design adds no entity, block or item, no body and no world change. Its state lives in
the faction rows and the wheel, which rtsim saves and drops as today; removing rtsim leaves nothing,
and reinstalling starts every council fresh, with beliefs at their priors and no memories. Members
away on a march at an uninstall are lost and booked Other at a reinstall (EQUILIBRIUM.md §2.3), a
cost MZ-15 lengthens and this design does not change.

### 2.12 Marches, arrivals and the spawn quota (MZ-15)

MOWZIE.md's MZ-15 gives every party a visible march (§4.16). This design takes it as it is:

- **BAND_MARCH (0x880B) is the march record.** This design has no march or route record of its own
  and adds no record id. Its acts are today's party kinds, so each rides BAND_MARCH like any party: a
  skirmish's route ends at the rival's territory edge, a caravan ambush's on the road, a lurk's at its
  AMBUSH_SPOT. The ids it touches, FACTION_NOTICE 0x8801 and ENVOY_MATERIALIZE 0x8802, gain fields,
  and none clashes with MOWZIE.md's planned 0x080C, 0x0811, 0x0091, 0x8807, 0x8809 or 0x880B.
- **The 128-block rule**, worded once in EQUILIBRIUM.md §2.2 and used here as written:

  > **A war party on the march is not an arrival** (MZ-15, MOWZIE.md §4.16). An arrival adds a member
  > the site did not have; a party's members are its camp's own, out on a route that BAND_MARCH holds
  > and the far view draws from about a kilometre away. So the 128-block rule does not apply to it:
  > its bodies are placed where its march puts them as they come inside the entity-ticking area. For a
  > simulation distance of s chunks its edge is 16s to 16s + 16 blocks off along an axis and up to about
  > 1.4 times that on a diagonal (`DistanceManager.java:252-253`): 160 to 176 blocks at 10, vanilla's
  > default, and 128 to 144 at 8. The column fills in stack by stack (a row of two warriors, a raptor
  > pack, the chief and its horse; MZ-17, provisional (a)), each stack the tick its last member comes
  > inside, so a raptor pack's nearest body can be a few blocks inside the edge. No warrior, raptor or
  > horse is drawn where it appears, at either distance: they walk into sight. The Umvuthi, the one body
  > drawn beyond 128 blocks (132), rides at the rear of the column; at 8 it can be seen to appear 128 to
  > 132 blocks off along an axis, never nearer, and at 10 it appears beyond its draw distance. Nothing
  > appears that was not already walking toward the player. A party that reaches a watched target with
  > no bodies yet (a player who logs in or teleports there as it arrives) comes as a WalkIn along its
  > own route: placed at the farthest ticking point on that route back from the target, it walks the
  > rest, instead of today's waves on the arc 112 blocks out. That point is at least 160 blocks off
  > along an axis at a simulation distance of 10, and at least 128 at 8.

  The rule's other half stands: an arrival (a recruit, a new Frostmaw, a new Umvuthi) is never placed
  within 128 blocks of a player.
- **The census never ends a march.** A camp that comes in while its party is out has that party's
  bodies taken, and the party marches on; a living member away is counted with its party and not
  recalled; a party ends only at its own home or at its last death (EQUILIBRIUM.md §2.6 item 11, now
  rewritten to MZ-15). The model's walk home (2.9) is that march home.
- **The Umvuthi rides with its band**, at the rear of the column (MOWZIE.md §4.5, §4.16): its 5.0 is in the band's strength, 12.6 for a
  full invasion, and missing from the grove's defence until it is home. If the band is destroyed, it
  falls with it, and the grove waits for an heir (2.10).
- **The spawn quota does not shrink a band.** MOWZIE.md leaves this to the conflict engine: a band
  bigger than the quota of 16 members in Minecraft at once (`tunables.rs:94`; a goblin invasion of 26,
  a raider one of 23, a bandit one of 20; a Mowzie band never exceeds it) keeps its whole strength in
  the fight's books. Its column is embodied up to the quota, and the members past it stay in the
  record and walk in along the route from the ticking edge as the quota frees, as `start_fight`'s
  later waves send only the members not yet in Minecraft (`spawn_wave`, `engine.rs:3364-3378`). The
  player sees the band the far view drew, arriving in two or three bunches.

### 2.13 Cultures as data

Each `data/cultures/*.json` gains a `mind` block with the culture's numbers: temperament means and
spread, τ, greed, warrior value, rev_max, pride weight and half-life, refusal and insult pride, fear
weight and half-life, glory weight, restless and wake days, effort, demand bias, scout and warn
weights, oath, kin cost, preemption, guard share, invasion pride and chief strength (Umvuthana), the
one-by-one refill and the heir's wait (Umvuthana), the killer's grudge and pride (Frostmaw), ambush
weight. About 25 lines a culture. The engine has no culture name in it (D4): the Frostmaw is a culture
with a pool of one, the Umvuthana one with a small pool, an invasion gate and raptor hunts.

| Culture | bold | aggr | soc | spread | τ | greed | pride_half | wake_days | effort | demand bias | other |
|---|---|---|---|---|---|---|---|---|---|---|---|
| goblin | 0.55 | 0.6 | 0.35 | 0.2 | 4 | 1.6 | 12 | 12 | 2 | 0 | ambush weight 1.4 |
| bandit | 0.45 | 0.45 | 0.6 | 0.15 | 2 | 1.2 | 24 | 14 | 4 | 6 | ambush weight 1.6 |
| raider | 0.6 | 0.6 | 0.35 | 0.15 | 2.5 | 0.9 | 96 | 20 | 4 | 2 | preempt 10, oath 10, pride_w 20 |
| Umvuthana | 0.5 | 0.45 | 0.65 | 0.15 | 2.5 | 1.2 | 48 | 16 | 6 | 10 | guard share 1, invasion pride 0.8, chief strength 5, one by one, heir wait, hunts |
| Frostmaw | 0.5 | 0.5 | 0.1 | 0.25 | 3 | 1.0 | 96 | 30 | 14 | none | council of one, its life 60, pride_w 25, killer's grudge 80 and pride 2 |

World numbers, shared: loot value 0.2 an item fed, 0.03 beyond the stores; boldness shift 1; battle
doubt 0.3; belief drift 1/384 a day, a quarter of it for a player's base; rumour error 0.5; speaker
weight 0.6; train notice 0.05; caravan attention 0.03; clash rate 1 a year a pair; guard share 0.25;
field share 0.15; peace cost 8; tributary cost 12; sworn multiplier 3; tribute truce 48 days.

### 2.14 The story

Each party, envoy and notice carries its top motives and the memory behind them, as codes. Text is
written when a player, a page or a log reads it (from the model's runs):

- "Y8 d22 the Grey Outpost: invasion on player Ben by 23 (Dorn, chief). Why: honour: we lost 23 at
  player Ben in the winter of year 1; and loot."
- "Y5 d93 Ashpit: invasion on player Bea by 26 (Brakka, chief). Why: loot; Brakka had not sworn
  Nub's truce."
- "Y1 d44 Ashpit: ambush on the railway by 8 (Brakka, chief). Why: a train came through; the new war
  leader must prove himself; and old hatred of the railway."
- "Y1 d39 the North Den: raid on player Ben by 1. Why: honour: player Ben killed the Frostmaw before
  me in the spring of year 1."

### 2.15 Reuse and custom

| Need | Reused | Custom |
|---|---|---|
| Grievance, signals, rail meter | `grievance.rs`, culture signals, MOWZIE.md §4.3 | none |
| Parties, marches, waves, abstract fights | today's party kinds, raid director, `village_outcome` and `threat.rs`; BAND_MARCH and MZ-15's march | a skirmish target (a camp's territory edge); defender losses; a leading chief's fall |
| Envoy and tribute | today's Envoy, ENVOY_MATERIALIZE, `Demand`, tribute goods; MOWZIE.md §4.5 | the price (2.8); the reason fields |
| Train ambushes | MOWZIE.md §4.4 lurk, AMBUSH_SPOT, `attack_train`, TRAIN_ROBBED, the track sampler (MZ-12); today's incidental ambush of a marching war party (`SYS_AMBUSH`) stays | the lurk opened to every culture with an ambush weight |
| Caravan ambushes | rtsim-world's caravans (`caravans.rs`, `caravan_place`) | a query for caravans passing a camp's reach through the conflict engine's world interface, and the robbery's loss of cargo reported to the economy |
| Chiefs, heirs, temperament | `Notable`, `Temperament::from_seed`, `succeed`, C24 grudges | an elder seed, standing, gamma tenure |
| Memories | `MemorySlot`'s 8-byte shape, report kinds (Killing, Robbery, Theft for a stolen crystal) | conflict memory kinds (lost at, drove us off, refused, killed envoy, raided us, border, paid, guards gone) |
| Notices | FACTION_NOTICE, NOTICE_SHOWN (MOWZIE.md §4.12) | the reason fields |
| Renewal and pressure | EQUILIBRIUM.md §2.4.5 and §2.4.7 (as designed there; `rtsim-kernel::renewal` is EQ-6 (a)'s design, not built) | none |
| Camp bodies, UR-1 | MOWZIE.md §4.9 reconciler, MZ-15's census | none |
| Decision | none fits | the council: drives, beliefs, Thompson draw, logit choice, truces, clashes |

---

## 3. Walkthroughs

Each is the model's own run (`walk.py`, `results/walkthroughs.md`): of 16 seeds, the one whose headline
count is nearest the median, so the story is a typical one, not a picked one. Players are scripts. A
player who "pays" pays when the price is no more than what the threatened party would truly take at
his real defence, which is nothing if it would lose (2.8). What the player sees is told from the
acts: an envoy is a party walking up to him; a war party he is online for is met on its march, its
column walking in from the ticking edge (2.12); one he is away for meets his walls abstractly, and he
finds the damage on his return (PLAN.md §10.3).

### 3.1 A strong base that leaves the Umvuthana alone

Ben's walled base (defence 80) is in reach of the Sun Grove (Umvuthana), Mudhollow (goblins) and the
Grey Outpost (pillagers). His industry makes 3 grievance a day toward each camp that hates humans. He
never attacks them and never pays. Seed 1000: 7 fights on him in 50 years (median 7).

1. **Y1 d27 to d74.** The Sun Grove's masks beat the gong at him; the Grey Outpost shows him a war
   band. Nobody knows how strong this new base is: each camp's guess is 15, give or take a factor of
   three.
2. **Y1 d76.** The Grey Outpost invades with 23. Its war leader Captain Vask had the floor; its chief
   Captain Hesk leads, as raider captains do. The council's draw of a new base came out low and it
   believed the attack certain. It meets 80: all 23 die, and Hesk falls with them. Vask leads now.
   The reason it carries: "loot; and anger at player Ben's mines and smoke".
3. **Y1 d77.** Mudhollow, the next day, invades with 26 under its chief Vorr, for loot and its
   restless warriors: all 26 die. Each camp now knows a floor: this base beat its whole invasion (36,
   30.4).
4. **Y2 to Y4.** No fights: war bands shown at his border, howls, the gong, and Mudhollow's scouts
   (100 looks in the 50 years). Y4 d67: Mudhollow's elder Brakka has the floor and sends an envoy
   asking 115, "after: we lost 26 at player Ben in the winter of year 1". Refused.
5. **Y5 d96.** Word reaches Mudhollow that Ben is away, in a hungry winter. Its chief Mott invades with
   26, guessing his walls at half. Half is 40, and 30.4 is short of it: all 26 die.
6. **Y7 d54.** The Sun Grove asks 9 gold, its only envoy to him in 50 years. Refused. A grove of 12.6
   at its strongest has no hope against 80, and the Umvuthi never leads it there.
7. **Y8 d12 to d22.** The Grey Outpost's envoy asks 94, reminding him of the 23 lost in year 1.
   Refused. Ten days later its chief Dorn leads an invasion of 23 out of honour, and falls with it.
8. **The rest.** Mudhollow tries once more before the decade is out; then one invasion in each of
   decades 3 and 4, and nothing in decades 2 and 5.

Over the 50 years: 7 fights, every one lost, 173 attackers killed at his walls, 12 envoys, nothing
paid; 57 war bands shown by the Grey Outpost, 66 gongs and 13 howls. Over all 64 fortress worlds, a
fight on him every eight years, every one lost; the first test of the new base came within two years
in 73% of worlds (the median on day 62). In this world the Sun Grove never sent a party at him.

### 3.2 Farming the Frostmaw den

Ben (defence 30, pays envoys) kills the North Den's Frostmaw whenever it is back, and now and then
steals the crystal from the sleeping Frostmaw at the Pass Den. The Sun Grove is also in reach.
Seed 1002: 109 Frostmaw deaths in 50 years over the two dens (median 110).

1. **Y1 d2.** He kills the North Den's Frostmaw. Its heir arrives on d14, 12 days later
   (`min(192, 6 x 2^P)`, EQ-4 (b)); he kills it on d16.
2. **Y1 d21 and d39.** He steals the Pass Den's crystal. On d39 he comes back for another; the
   Frostmaw wakes, and in the fight he is at in person, he kills it.
3. **Y1 d39.** The same day the North Den's next heir arrives, after a wait doubled by the pressure.
   It knows who killed the last one, and its restless clock went stale while the den stood empty: it
   raids him that day, wins (41.7 against his walls of 30) and takes 60. "Honour: player Ben killed the
   Frostmaw before me in the spring of year 1." He kills it on d41.
4. **Y1 d55.** The Pass Den's heir, arrived on d51, raids him for the same reason, and takes 44.
5. **Y1 d78.** The Sun Grove asks 14 gold. A grove raid on his walls (3.8, or 12.6 with the Umvuthi,
   against 30) would take nothing, so his script refuses. The gong follows, about once a year.
6. **Y2.** He steals on d11, d58 and d75. The Pass Den answers 21 days, 3 days and the same day later,
   and again on d84: "honour: player Ben stole the crystal". Each raid wins and takes 44.
7. **Y4 d89 to Y5 d96.** Four times the Frostmaw wakes as he reaches for the crystal and drives him off.

Over the 50 years: 79 raids on him, none lost (17, 17, 17, 16 and 12 by decade): 24 by the North Den,
the den he farms, and 55 by the Pass Den, the den he robs (68 thefts). The North Den's Frostmaw died 87
times at his hand, 1.73 a game year over all 64 worlds, MOWZIE.md's 1.8 for a farmed den; the Pass
Den's 22 times, when it woke. Over the 64 worlds the Pass Den raided him 1.39 times a year and the North
Den 0.51; he lost 92 items a year, 19% of his income. The Umvuthi led its grove at his walls 58 times in
40 of the 64 worlds, its pride wounded by refusals, and lost and fell every time.

### 3.3 A railway through goblin land

A line runs past Ashpit (goblins) and the Pass Den (a Frostmaw) at 12 legs a day. Its owner (defence
30) never pays; after three robberies in 30 days he puts guards on the trains for 48 to 144 days (his
script). Seed 1003: 231 fights on the line in 50 years (median 226).

1. **Y1 d3.** The Frostmaw roars at the line: the rail meter has made it angry, and a roar is cheap.
2. **Y1 d7 to d73.** Ashpit ambushes five trains, 8 goblins each, 49 items a train, under its chiefs
   Gitz and then Brakka: "a train came through; loot; and old hatred of the railway", once "the new
   war leader must prove himself". On d73 the owner puts guards on. On d77 the next ambush (the elder
   Snag had the floor) meets them: all 8 die.
3. **Y1 d95 to Y2 d9.** The Frostmaw lurks by the track three times and robs a train each time. It
   beats the guards (41.7 against 20) and loses half its haul to them: 33 items.
4. **Y2 d13 to d48.** Ashpit's envoy asks the owner 122 emeralds. Refused. On d48 word comes that he is
   away: Ashpit invades his base with 26, wins, loses 3 and takes 167.
5. **Y2 d63 to d86.** The guards are off again. The Frostmaw robs twice and Ashpit once in 15 days,
   and on d77 the guards go back on; the next goblin ambush, on d86, loses all 8. The pattern holds but not on a calendar: across the 64 worlds, the first robbery
   after the guards come off follows by a median of 27 days, from 5 to 74 (p10 to p90).
6. **The Frostmaw** robs a train a few times a year and roars at the line every few weeks (232 roars,
   119 lurks in the 50 years).

Over the 50 years: 238 fights on the line and its owner (50, 51, 44, 45 and 48 by decade), 25 of them
lost; Ashpit ambushed 112 trains and invaded the owner 7 times, the Pass Den lurked 119 times, and the
guards went on 34 times.

How traffic changes it (8 seeds x 20 years each):

| Legs a day | Rail meter grievance (MOWZIE.md §4.3) | Frostmaw lurks a year | Frostmaw roars a year | Goblin ambushes a year | Guards put on a year |
|---|---|---|---|---|---|
| 2 | 0 | 0.17 | 0.88 | 2.33 | 0.31 |
| 4 | 17.3 | 0.35 | 1.97 | 2.42 | 0.43 |
| 8 | 52.0 | 0.91 | 3.58 | 2.63 | 0.55 |
| 12 | 86.6 | 1.89 | 4.29 | 2.38 | 0.62 |
| 24 | 190.5 | 5.19 | 5.02 | 2.06 | 0.83 |

The Frostmaw's robbing grows smoothly with traffic. Today's ladder has none below the Raid rung (none
at 4 legs) and at 12 legs a lurk every 10 days from day 14, to the day, in every seed (9.6 a year; CL-3).
The goblins rob for loot at any traffic; the guards cap them.

### 3.4 Multiplayer: one provokes, another suffers

Ash (defence 50) hunts Ashpit's goblins every few weeks and kills its envoys. Bea (defence 8) lives
near Ash, pays what she is asked when it is no more than a raid would take, and has done nothing.
Both are in reach of Ashpit and the Grey Outpost. A camp that cannot tell who killed its members
sometimes blames the other player near it (blame spill, 0.3 of the time; CL-2). Seed 1000: 7 fights
on Bea in 50 years (median 6).

1. **Y1 d11 to d13.** Ash hunts Ashpit. Two days later Ashpit's envoy finds Bea and asks 81, "after:
   player Bea killed 4 of ours at our camp": Ash's hunt, blamed on her. She pays.
2. **Y1 d33.** Word reaches Ashpit that Ash is away; it sends a band of 8 at him. His walls at half are
   25: all 8 die. On d68 the Grey Outpost's band meets the same end.
3. **Y1 d83.** The Grey Outpost's band falls on Bea instead, before anyone has asked her for anything:
   it wins, loses 3 and takes 43. "Loot."
4. **Y1 d94 to Y4.** Both camps' envoys ask Bea 79 to 114 and she pays every one, about three a year
   between them. Ash kills Ashpit's chiefs Nub (Y2 d38) and Gitz (Y2 d68); a week later Ashpit's
   envoy asks Bea 87, "after: player Bea killed Nub, our chief". On Y4 d4 Ash kills Vorr, its chief
   by then.
5. **Y3 d32.** The Grey Outpost invades Ash with 23 while he is away, for its restless warriors and
   "revenge: we lost 8 at player Ash in the autumn of year 1": it wins, loses 6, takes 130.
6. **Y4 d14.** Ashpit sends an envoy to Ash, "after: player Ash killed Vorr, our chief". He kills it.

Every fight on Bea in the 50 years, each by a camp she was paying:
- Y1 d83, the Grey Outpost's band (above).
- Y5 d93, Ashpit invades with 26: "loot; Brakka had not sworn Nub's truce". It takes 185.
- Y11 d25, Ashpit: "the new war leader must prove himself".
- Y22 d61, Ashpit: "empty stores".
- Y24 d91, the Grey Outpost: "winter hunger; Captain Hesk broke his own truce".
- Y27 d81, the Grey Outpost: "word came that player Bea was away; loot; Talv broke his own truce".
- Y34 d65, the Grey Outpost: "loot; and the new war leader must prove himself; Talv had not sworn
  Captain Mol's truce".

The attackers win every one. Bea pays about 27 envoys a decade. Over the 64 worlds she pays 272 a year
and loses 25 to raids, 62% of her income; Ash takes 0.33 fights a year, kills 4.6 attackers a year,
and loses 25 items a year (5% of his income) to the attacks that find him away. Without blame spill
Bea's fights fall from 0.14 to 0.08 a year and her tribute from 272 to 268: the spill shows mostly in
the reasons her envoys give, and in the raids of chiefs who believe she killed theirs.

### 3.5 A world left alone for 50 years

No player. Seed 1001: 54 fights between camps in 50 years (median 54).

| Decade | Fights (camp, village, caravan) | Lost | Truces | Chiefs replaced | Dragons | Bloodiest camp fight |
|---|---|---|---|---|---|---|
| 1 to 10 | 65 (12, 39, 14) | 4 | 4 | 30 | 11 | Y7 d96 Mudhollow invades the Grey Outpost with 26 under its chief Snag, in winter hunger: wins, 22 dead, 32 taken |
| 11 to 20 | 52 (6, 35, 11) | 1 | 4 | 36 | 8 | Y13 d3 Ashpit invades the Grey Outpost with 26, its stores empty: wins, 22 dead, 32 taken |
| 21 to 30 | 69 (15, 41, 13) | 3 | 7 | 35 | 17 | Y24 d67 Ashpit invades the Grey Outpost, empty stores and loot: wins, 22 dead, 32 taken |
| 31 to 40 | 61 (10, 42, 9) | 1 | 5 | 33 | 9 | Y31 d26 Mudhollow invades the Grey Outpost, empty stores: wins, 22 dead, 32 taken |
| 41 to 50 | 72 (11, 49, 12) | 3 | 4 | 33 | 13 | Y50 d77 Mudhollow invades the Grey Outpost, winter hunger: wins, 22 dead, 32 taken |

The bloodiest fight of every decade has the same shape: a starving goblin camp throws its whole
invasion (30.4) at the raider outpost's full guard (25.3) and wins a victory that costs it 22 of 26
for 32 items. The fight is deterministic, so the same two full camps always give the same result; what
varies is when hunger drives a council to it, and who speaks.

Fights between camps over the 50 years, attacker on target: the Grey Outpost on Crowsnest 9; Ashpit on
Crowsnest 8; the Grey Outpost on Mudhollow 7; Mudhollow on the Grey Outpost 7; Mudhollow on Ashpit 7;
the Grey Outpost on Ashpit 5; Ashpit on the Grey Outpost 5; three more pairs, 2 each. The next seed on
the same map tells another history: the Grey Outpost on Ashpit 10, on Mudhollow 8, and on Crowsnest
only 3; Mudhollow on Ashpit 7 and on the Grey Outpost 7; Ashpit on Crowsnest 5.

The first years read like a chronicle: Y1 d31 a dragon strikes Ashpit; Y1 d51 Ashpit's chief Grukk is
killed in a quarrel; Y1 d63 the Grey Outpost's war leader Captain Mol ambushes a caravan; Y1 d81
Ashpit's hunters kill one of the Grey Outpost's at the border; Y1 d95 Mudhollow's war leader Brakka
takes 26 to Lowmarsh "to prove himself" and comes back with 120; Y2 d78 the Grey Outpost's band falls
on Mudhollow and dies; Y3 d44 Mudhollow, its stores empty, invades the dragon-struck Grey Outpost;
four days later the Grey Outpost, its hunting ground lost, invades Ashpit; Y3 d86 the Sun Grove's
raptors, hungry, take Lowmarsh's animals.

Chiefs turn over fast: about three a camp a decade, from today's `chief_lifespan_days` (goblin 120
days, bandit 90, raider 150) and from chiefs who fall leading attacks. Each change brings a new war
leader with a season or a year to prove himself and a council with another temper, which is much of
why no two decades of one world read alike. The figure is today's data, not this design's; a longer
tenure would give fewer, longer reigns.

---

## 4. Numbers

64 seeds per scenario and mode, 50 game years each (`run.py`, `results/summary.md`). The definitions
are in `run.py`'s header: a fight is a war party that fights (raid, invasion, band, ambush, plunder,
skirmish); lost means the attackers lost; an endless war is one pair of camps fighting with no 96-day
gap for five years; a surge is a last decade with twice the fights of the second; extinct is a camp
held at one member or none for 96 days running. All three rules fight by the engine's deterministic
law; they differ only in how they decide.

### 4.1 Stability and losses

The world left alone (no player):

| | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fights a world-year: mean [p5, p95] | 30.1 [23, 38] | 12.6 [9, 18] | 7.3 [3, 14] |
| Between camps a year | 10.2 | 2.1 | 1.2 |
| Share the attackers lost | 0.34 | 0.12 | 0.02 |
| Lost fights a year | 10.1 | 1.6 | 0.17 |
| Worlds with an endless war | 1.00 | 0 | 0 |
| Worlds with a surge | 0 | 0 | 0 |
| Worlds where a camp died out | 0 | 0 | 0 |
| Worlds with no camp fight in 50 years | 0 | 0 | 0 |
| Worlds with a decade of no camp fight | 0 | 0.02 | 0.02 |

The longest gap between two camp fights in any life world was 10.9 years. Unattended for 500 years
(8 worlds): fights a year by century stay between 6.1 and 8.4 in every world, fights between camps
between 0.87 and 1.56, with no trend.

Where the losses fall (the share of the attackers' fights they lost, `results/summary.md`):

| Target | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| A rival camp (alone) | 1.00 | 0.75 | 0.12 |
| A village (alone) | 0.00 | 0.00 | 0.01 |
| A train or caravan (railway) | (none) | (none) | 0.11 |
| The walled base, 80 (fortress) | 1.00 | 0.99 | 1.00 |
| The hunter and the bystander (multi) | 0.98 | 0.96 | 0.28 |
| The farmer, 30 | 0.00 | 0.00 | 0.01 |
| A weak base, 6 (duels) | 0.00 | 0.00 | 0.00 |

Losing raids are rare (one fight in fifty when the world is left alone or near the farmer, one in
seventeen to twenty-five near a fortress, a hunter or a busy line) and they come from wrong beliefs: a new base guessed weak, a guard guessed thin, a
player believed away. Under the ladder the plunder clock keeps sending a band of 8 at a camp it cannot
beat, and loses every time. The gate believes the square law's odds, A² / (A² + d²), against an engine
whose fight is certain, so it attacks camps it cannot beat (75% lost) and, in the valley, the fortress
(99% lost). Every life fight on the fortress is lost because the fortress is stronger than any party
in the valley; what keeps them rare is the floor and the slow blur of a base's belief.

### 4.2 The players

| Scenario, player | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fortress, Ben: fights a year on him (attackers killed a year) | 2.74 (39.4) | 0.79 (16.0) | 0.13 (2.9) |
| Farmer, Ben: fights a year (loot taken a year, share of income) | 0.13 (6, 1%) | 0.18 (9, 2%) | 1.92 (92, 19%) |
| Multi, Ash: fights a year (attackers killed; loot) | 7.47 (79.7; 10) | 3.34 (48.0; 15) | 0.33 (4.6; 25) |
| Multi, Bea: fights a year (tribute and loot, share of income) | 0 (1, 0%) | 0 (0) | 0.14 (272 and 25, 62%) |
| Multi without blame spill, Bea | 0 (1) | 0 (0) | 0.08 (268 and 15, 59%) |
| Railway at 4 legs, the owner: invasions a year (loot) | 0 | 0 | 0.17 (20, 4%) |
| Railway at 4 legs: train ambushes a year | 0 | 0 | 2.9: goblins 2.15 won and 0.38 lost, the Frostmaw 0.36, every one won |
| Railway: caravan ambushes a year | 0 | 0 | 0.68 |

The duels (`game.py`'s setting: one goblin camp, one base, grievance 9 a day, no trips), last decade;
the player's income is 480 a year:

| Player | Ladder (c): wars a year, killed a year, tribute | Gate (a) | Life | Life: what it costs him a year |
|---|---|---|---|---|
| Fortress (80, never pays) | 9.6, 65, 0 | 0, 0, 0 | 0, 0, 0 (0.01 a year over 50 years) | nothing |
| Weak (6, pays) | 14.7, 28, 138 | 0, 0, 329 | 0.1, 0, 177 | 177 tribute and 30 loot: 43% |
| Stubborn (6, never pays) | 23.7, 45, 0 | 10.4, 20, 0 | 3.4, 3, 0 | 481 loot: all of it |
| Adaptive (12, builds after each loss, pays) | 9.6, 65, 0 | 2.2, 23, 1 | 0, 0, 158 | 160 tribute and 9 loot: 35% |

**Best replies.** For a weak base, refusing costs everything, paying costs 43%, building a little and
paying (the adaptive script, defence 12) costs 35%, and walls of 80 cost nothing a year. So the
player's best reply is to pay or to build, and the camp's best reply to a payer is to ask and not
raid (0.1 fights a year on him, against 3.4 on the refuser). Toward a fortress the camp's best reply
is to leave it alone, and it nearly does: one test in eight years in the valley, after which it knows
better for years. Each side plays close to its best reply with some noise: a logit (quantal response)
equilibrium, and the noise is what makes the world feel alive. Under the ladder the camp plays its
worst reply against strength (9.6 lost wars a year at the fortress, 65 dead), and under the gate it
does in the valley (0.8 a year on the fortress, 99% lost).

**The ladder against a payer.** Today's `escalate` prices tribute at `tribute_per_grievance` x g,
bent by the aggression trait. In a high-bend camp that price passes what a raid would take; a payer
who pays only up to the raid refuses, the feud climbs, and the war rung repeats for good. So the
ladder's weak payer is bimodal: in 42% of years no war, in the rest about 24. EQUILIBRIUM.md §3.5's
`game.py` reported 21.6 wars a year for a payer who always paid at the Envoy rung; neither is today's
engine exactly (`settle` resets a paid feud to Notice), and both show the ladder failing a weak base.

### 4.3 How formulaic

| Measure (world left alone) | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Next act called to the day from the last two (kind, target, day within 1) | 0.18 | 0.17 | 0.02 |
| Spread of days between one attacker's fights (CV, median; 1 is Poisson) | 0.83 | 0.83 | 0.84 |
| The same between one attacker's fights on one target | 0.83 | 1.30 | 0.78 |
| Same camp, different seeds: difference of act mix (Jensen-Shannon) | 0.000 | 0.004 | 0.011 |
| Same camp, different seeds: spread of fights a year (CV) | 0.04 | 0.10 | 0.15 |
| Two seeds of one world: share of the first ten years' acts that match (chance) | 0.52 (0.49) | 0.38 (0.36) | 0.09 (0.07) |
| A camp's years: variance beyond Poisson, median [p10, p90] | 0 [0, 0] | 0 [0, 1.06] | 0.31 [0, 0.61] |

The intervals alone do not tell the rules apart; what does is whether the next act can be called and
whether two seeds tell the same story. A life camp also has good years and bad years (variance beyond
Poisson): hunger, chiefs and feuds come and go. In a duel the gap is starker: against the stubborn
player the ladder's next act is called to the day 98% of the time, the gate's 14%, the life
council's 2%; two seeds share 99%, 65% and 15% of their acts.

**Can a watcher call the day after a refusal?** Days from a refused envoy to that camp's next fight on
the target, within a year:

| | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fortress: median [p10, p90] | 9 [9, 10] | 3 [3, 3] | 4 [1, 65] |
| Share within a day of the commonest lag | 1.00 | 0.99 | 0.41 |
| Share of refusals followed by a fight within a year | 1.00 | 0.99 | 0.30 |
| Multi: median [p10, p90] (share within a day of the commonest) | 9 [2, 37] (0.55) | 3 [3, 3] (0.98) | 8 [2, 67] (0.26) |

On the railway, the first robbery after the guards come off follows by a median of 27 days [5, 74]
(CV 0.81, 1,354 cases): word of the guards' going reaches the camps late, and the watch on the line
misses them half the time.

What started the life fights (alone): hunger was a driver of 62%, loot of 64%, glory of 13%, pride
of 2%; loot alone, with no other motive, 24%. Occasions: the restless clock 83%, caravans 14%,
provocations 2%, news 1%. With players near, provocations, refusals and news start more of them, and
revenge and pride drive more: at the farmer pride is a driver of 22% (the Frostmaw's honour); on the
railway a passing train is the occasion of 35%.

### 4.4 What the model leaves out

- The march out: a fight resolves the day it is decided; the engine's marches add days before it. The
  walk home is modelled (survivors and a winning chief away 1 to 3 days).
- The watched fight: every fight is the abstract one, except the Frostmaw woken at its den; the raid
  director's waves, the spawn quota and the walk-in are not modelled.
- Players are scripts: a fortress, a payer, a hunter, a thief, a railway owner. Real players change.
  A payer pays only up to what a raid would truly take (2.8).
- One valley of seven factions, two villages, a road and one line.
- Dragons strike a camp 0.25 times a year; a hunting ground is lost about once a decade. Both are the
  model's stand-ins for the world, not part of the design.
- The ladder mode follows `engine.rs` in its rules, not in its code paths; the gate follows
  EQUILIBRIUM.md §2.5 as written, including its belief of the odds.

---

## 5. Cost

### 5.1 Memory

| What | Where | Bytes |
|---|---|---|
| New faction fields (2.11) | the paged faction row | about 45 a faction |
| New fields on each feud | the paged faction row | about 96 a feud |
| Feuds toward camps and villages in reach | the paged faction row | about 4 a camp in the model's valley: about 4 x (today's feud plus 96) |
| The council clock | the wheel | replaces the plunder clock's event: no change |
| Border clashes | the wheel | one event a camp with a hostile neighbour: about 94 bytes a faction (PM-C13's wheel cost of an event) |
| Occasions waiting their delay | the wheel | about 0.1 a faction at any time |

The faction row (about 450 bytes encoded with two feuds, PM-C1) grows to about 1.4 to 1.7 KB with four
feuds toward nodes; it is paged, 4 factions a page (PM-C12), so it costs disk, page loads and saves,
not resident memory. Resident memory grows by the clash event, about 100 bytes a faction: about 0.4 MB
at 4,096 factions on top of PM-C13's 3.17 MB.

### 5.2 CPU

| What | How often | Cost |
|---|---|---|
| A sitting | 9.4 to 14.2 a faction-year in the valley scenarios (the most near a busy line), 16 to 18 in a duel; today's plunder clock books 8 to 16 (`plunder_mean_days` 12, 6 and 8). By culture, left alone: goblin 13.7, bandit 11.5, raider 9.9, Umvuthana 9.5, Frostmaw 3.9 | 3.9 to 5.0 neighbours weighed, each one normal draw and 4 to 7 scored acts of a few dozen operations: a few microseconds (inferred), plus the faction's page, which today's plunder event loads as well |
| A border clash | about 1 a year per hostile pair | one draw and a feud update |
| An occasion | a provocation, a refusal, news, a passing train or caravan | a booked event; most become one sitting |
| Belief, stores, fear, pride, trust | at each event touching the feud or faction | closed form, a few operations |
| Acts | 4.9 to 6.1 a faction-year | as today's parties |
| Rows touched | 0.11 to 0.19 a faction-day (two for a fight on a camp) | a page load each when not resident |

**Step CPU.** Sittings replace plunders about one for one, and a sitting's arithmetic is small beside
a page load (about 5 microseconds plus 0.45 decode a faction, PM-C12). What grows is the row: decoding
a row three times the size, against today's 52.3 microseconds a faction-day at 1,024 factions, gives
an estimated 52 to 60 (inferred).

**Save CPU.** A page of four rows of 1.4 to 1.7 KB is about the bytes of PM-C12's 16-a-page runs
(16 rows of 450). Those saved 44 times a day for 125 ms of CPU a day at 1,024 factions and 162 times
for 801 ms at 4,096, against 13 and 37 ms, and 47 and 276 ms, at today's 4 a page. So expect save CPU
about three times today's: about 110 to 125 ms a game day at 1,024 factions, about 800 at 4,096
(inferred; the paged sweep measures it, test 17). If that is too much, the feuds toward nodes can
live in a page of their own, loaded only by sittings.

Nothing scans per tick; nothing grows with time or with dead history (memories are four slots, beliefs
two numbers, the floor and the stock one pair each).

### 5.3 Lines

| Part | Lines (estimate) |
|---|---|
| Rust: the council (voices, drives, beliefs, the floor and stock, the sitting, prices, truces, story codes) in `rtsim-conflict/src/council.rs` | about 750 |
| Rust: engine changes (feuds toward nodes, occasions, `Ev::Council`, `Ev::Clash`, skirmish target, defender losses, a leading chief's fall, hunts hook, chief tenure, the lurk for every culture, the quota's walk-in) | about 500 |
| Rust: caravan ambush (the caravan query in the world interface, the robbery report, `rtsim-world` side) | about 150 |
| Rust: persistence of the new fields (`persist.rs`, `tables.rs`) | about 170 |
| Rust: the `mind` block in `culture.rs` | about 120 |
| JSON: five cultures' `mind` blocks | about 125 |
| Rust tests (section 6) | about 1,000 |
| Java: reason lines on ENVOY_MATERIALIZE and FACTION_NOTICE in the `ConflictBridge` | about 120 |
| Java GameTests | about 300 |

The model is `model.py` (1,676 lines), `run.py` (542), `walk.py` (194) and `summarize_md.py` (144).

---

## 6. Tests

Property tests in `rtsim-conflict`, on the engine's scenario harness (`scenario.rs`), each over 64
seeds unless said. The model's value for each is in brackets.

1. `a_sitting_draws_from_its_own_stream`: give one faction an extra occasion; every other faction's
   draws are unchanged until its own state changes; two runs with one seed give one state hash.
2. `belief_converges`: after five fights or eight scouting trips, the believed defence is within 25%
   of the truth for 95% of draws; after a year with no look, a camp's variance is back above 0.2.
3. `a_strong_base_is_mostly_left_alone`: the fortress duel at most 0.05 fights a year over 50 years
   [0.01]; in the valley, at most 0.3 fights a year on the fortress [0.13], and at least one test of
   the new base in the first two years in at least half the seeds [73%].
4. `a_payer_is_asked_not_raided`: the weak duel, at most 0.5 fights a year, tribute at least 120 a
   year [0.2, 177].
5. `a_weak_refuser_is_raided`: the stubborn duel, between 1 and 6 fights a year [3.4].
6. `no_runaway_no_permanent_peace`: the world left alone, 50 years: no surge; endless wars in at most
   5% of worlds [0] and none longer than 10 years; no gap of 20 years without a camp fight in any
   world [the longest 10.9 years].
7. `no_camp_dies_by_fighting`: no camp held at one member or none for 96 days, 50 years; and 8 worlds
   of 500 years with fights a year by century within a factor of 1.5 of each other [at most 1.3].
8. `stories_differ`: two seeds of one world share less than 20% of their first ten years' acts [9%];
   the next act is called to the day under 5% of the time [2%].
9. `one_culture_many_minds`: the same camp over seeds has a spread of fights a year (CV) of at least
   0.1 [0.15].
10. `every_fight_has_a_reason`: every party's reason code names a motive; a revenge or honour motive
    names a memory that exists or a standing cause (industry, a culture's hatred, the rail meter).
11. `a_sworn_truce_mostly_holds`: fights by a chief on a camp he swore a running truce with number
    under 1 in 10 of the truces sworn [55 for 1,562].
12. `the_frostmaw_robs_more_on_a_busier_line`: lurks a year rise with legs a day over 2, 4, 8, 12 and
    24 [0.17 to 5.19].
13. `the_umvuthi_leads_only_when_wounded`: every Umvuthana invasion was chosen with W at least 0.8,
    with an Umvuthi in the grove, carries the chief flag, and fights at the band's strength plus 5.0
    (12.6 at full strength); no Umvuthana invasion while the grove waits for an heir.
14. `a_thief_is_hated_and_a_stranger_is_not`: a Frostmaw raids the player who stole its crystal and
    not one who has not.
15. `matches_the_model`: the engine on the five duel and valley scenarios, with the march out made
    instant and the walk home 1 to 3 days as in the model, within 25% of `model.py` on fights a year
    and tribute, and within 0.05 on the lost share.
16. `uninstall_reinstall` (UR-1): drop the conflict state and load again: no entity, block or item
    was added by the council; every council starts at its priors; members away on a march at the drop
    are booked Other, as EQUILIBRIUM.md §2.3 has it.
17. The paged sweep (`docs/choices-paging-modules.md`) gains the sittings and the bigger rows: step
    and save CPU per faction-day at 1,024 and 4,096 factions against today's, same state hash across
    two runs.
18. `the_fight_is_the_engines`: every abstract fight's outcome and losses equal `village_outcome` or
    `threat.rs` on its A and d; the council's believed chance never enters it; a camp that holds loses
    A / d of the guard that met it.
19. `a_killer_is_hated`: the Frostmaw's killer is on the den's record at +80 and +2 with a memory of
    weight 3; its heir arrives with half and raids him before any other player [the farmed den 0.51 a
    year]; a camp whose chief a player killed bears him W + 1 and +40.
20. `a_beaten_party_takes_its_leader`: a destroyed raider invasion ends its captain's rule; a destroyed
    Umvuthana invasion ends the Umvuthi's, the grove then waits the later of its cowed time and
    `min(192, 6 x 2^P)` days and its count falls by one when a warrior takes up the mask.

GameTests, only where Java is involved:

1. `anEnvoySpeaksItsReason`: an ENVOY_MATERIALIZE with a reason shows its line to the player it
   addresses, once, on arrival.
2. `aNoticeCarriesItsReason`: a FACTION_NOTICE with a reason plays its sound and shows its line, and a
   notice for a player who is away waits for him (NOTICE_SHOWN).
3. `aGoblinBandAmbushesATrain`: a goblin band waits at an AMBUSH_SPOT and robs a passing synthetic
   train through `attack_train`; with guards on, the band is beaten and destroyed.
4. `aWatchedSkirmishIsABand`: a skirmish between two camps, and a caravan ambush, that a player meets
   are band parties on their BAND_MARCH routes, their bodies placed where the march puts them; a band
   past the spawn quota walks its remaining members in from the ticking edge as the quota frees.

---

## 7. Decisions taken here (each can be undone)

| Id | Decision | Reason |
|---|---|---|
| CL-D1 | A council of three voices (chief, war leader as heir, elder), the speaker drawn by standing and weighted 0.6 | one voice is one rule again; three give a camp moods and politics with three seeds of state |
| CL-D2 | A log-normal belief of each neighbour's defence, Kalman-updated, drifting back to uncertain | a camp can be wrong and learn; testing a new base is the Thompson draw, not a probe clock |
| CL-D3 | A logit choice among waiting and each neighbour's best act, culture temperature τ | the softened best reply is a quantal response equilibrium: near rational, never predictable |
| CL-D4 | Occasions and a restless clock replace the ladder's climb and the plunder clock | fights follow events; grievance still matters, as revenge and as how soon a camp sits |
| CL-D5 | Feuds toward camps and villages in plunder reach | camps need beliefs, pride and memories about each other to have their own wars |
| CL-D6 | Camp stores with seasons drive hunger | winter hunger and bad years give fights a reason and a rhythm that is not a calendar |
| CL-D7 | Border clashes between hostile neighbours, 0.5 a year a pair when both are fed, up to 2.5 when both starve | without them camps at strength drift into permanent peace |
| CL-D8 | A paid tribute buys a truce of 24 to 72 days sworn by the chief; breaking it costs 3 times more for him than for his heir | a payer must be safe enough that paying is his best reply; an heir breaking his father's word is a story |
| CL-D9 | Raiding a player with a record of paying costs up to 12 | without it a weak payer was raided by glory and revenge |
| CL-D10 | An envoy learns the base's defence and stock | otherwise a refused new base was invaded on the prior's optimism year after year |
| CL-D11 | Word that a player is away halves the guess and the floor for 3 to 10 days, and changes no belief | treating it as an observation made camps believe him half as strong for good |
| CL-D12 | Pride is weighted by the hope of winning | otherwise pride sent hopeless raids |
| CL-D13 | A player or the railway is warned at most once in 16 days; camps without the limit | warnings twice in four days are noise; limiting camps lengthened border wars |
| CL-D14 | No siege in the choice | the model has no walls to break; the siege party stays in the data |
| CL-D15 | Chief tenure gamma, shape 3, mean today's `chief_lifespan_days`; the Mowzie chiefs keep 1,000,000 | an exponential tenure killed new chiefs within days; MOWZIE.md §4.2 |
| CL-D16 | The Frostmaw is a council of one whose rail grievance is revenge, not a threshold | its robbing rises smoothly with traffic and each lurk is a decision (CL-3) |
| CL-D17 | The Umvuthi leads an invasion in person only with wounded pride of 0.8 or more, and the grove has no invasion without it | MZ-3 (b) says it leads; a pride gate makes it rare and always provoked |
| CL-D18 | Blame spill 0.3 | multiplayer stories where a camp cannot tell two players apart (CL-2) |
| CL-D19 | **Reversed by Ben's CL-5 (b), 2026-10-22.** Was: no alliances | the alliance design replaces it |
| CL-D20 | Reasons as codes, text written when read | 8-byte memories; no text in saves |
| CL-D21 | Every draw from `StreamKey(seed, SYS_COUNCIL or SYS_CLASH, faction key)` slotted by the faction's own counter; the world deterministic in the wheel's order | one faction's draws do not shift with another's sittings |
| CL-D22 | A stolen crystal is a Theft report naming the player at the steal | MOWZIE.md reports the steal in its CHIEF_STATE record (crystal held, §4.2); this makes it a grievance and a memory |
| CL-D23 | Renewal, pressure, break and refound unchanged | Ben's scope: only faction fighting |
| CL-D24 | Camps ambush caravans on the road near them (3 in 100 passing caravans are weighed) | a caravan is the natural prey of a bandit camp, and with trains it gives camps a target that is neither a village nor a player |
| CL-D25 | The abstract fight stays deterministic (`village_outcome`, `threat.rs`); the council's doubt (0.3 in the log) is its belief, not the fight's; only a fight a player attends in person is drawn | the engine owns the truth (P3); a losing raid then always has a story (a wrong guess), never dice |
| CL-D26 | A camp attacked meets a quarter of its members at home (guard share 0.25, today's Haven defence); a grove all of them, and the Umvuthi's 5.0 when it is home | with the whole camp defending, no full camp was ever beaten and camps never fought each other |
| CL-D27 | Defenders lose: A / d of the guard when it holds (linear), min(1, A / 2d) when it breaks | a camp that is raided pays for it, which is what makes a raid a grievance |
| CL-D28 | Occasion delays and attendance from the chief's aggression (2.5) | a fixed delay is a calendar; a hothead answers in a day or two, a cool chief in a week or not at all |
| CL-D29 | A new war leader's glory doubles for a season, a year if his chief was killed | a year after every natural succession made heirs too warlike |
| CL-D30 | The railway's guards stay on 48 to 144 days; word they are gone reaches the camps after Exp(6) days; the watch on the line misses the guards half the time | with fixed guard spells and instant news, robberies resumed on a known day |
| CL-D31 | No starvation: a camp out of stores is hungry, nothing more | renewal is EQUILIBRIUM.md's (Ben's scope); hunger only drives the council |
| CL-D32 | The Umvuthi's heir waits the later of its cowed time and `min(192, 6 x 2^P)` days, and a warrior takes up the mask (the grove's count falls by one); warriors refill one at a time | MZ-14 (a) and EQUILIBRIUM.md §2.4.7 |
| CL-D33 | The 128-block rule as EQUILIBRIUM.md §2.2 words it under MZ-15: a war party on the march is not an arrival; a bodiless party at a watched target walks in from the ticking edge | MZ-15 wins where it and this design disagree; the column appears stack by stack at the entity-ticking edge, 128 to 144 blocks off along an axis at a simulation distance of 8 and 160 to 176 at 10, having walked from a kilometre in the far view; only the Umvuthi, at the rear, can be seen to appear, 128 to 132 blocks off at 8 |
| CL-D34 | BAND_MARCH (0x880B) is the march record; no new record id; the reasons are fields on FACTION_NOTICE (0x8801) and ENVOY_MATERIALIZE (0x8802) | one march record for every party; no id to clash |
| CL-D35 | The spawn quota does not shrink a band: members past it walk in from the ticking edge as it frees | the band the far view drew is the band that fights; `start_fight`'s waves already work this way |
| CL-D36 | A defeat sets a floor under the guess (halving over 4 x pride_half), and a player's base blurs at a quarter of a camp's rate | without them a fortress was tested again as soon as the belief blurred |
| CL-D37 | Survivors walk home 1 to 3 days, a winning chief with them; the camp lacks them until then | MZ-15's march home has bodies; a camp whose band is out is weaker |
| CL-D38 | A camp plans its take against the stock it last saw plus the refill it knows | planning against the true stock was omniscience |
| CL-D39 | The Umvuthana's carry of 24 and 48 is the value of MZ-8's owed animals, 2 a pack at 12 items an animal | MOWZIE.md gives them a carry of 0 for goods; the council still needs what a raid is worth to the grove |
| CL-D40 | The Frostmaw dies in every abstract fight it loses; its killer gets +80 grievance and +2 pride; its heir arrives with half and a provoked occasion | a party that loses is destroyed, and the Frostmaw's party is itself; a den should hate its killer |
| CL-D41 | A chief who leads falls with a destroyed party, and with a chance of a tenth of his party's loss in a win | a beaten invasion is a story about its leader |
| CL-D42 | EQUILIBRIUM.md §2.6 item 11: the census does not recall a band's living members | MZ-15: the march goes on; a recall broke it at every visit to a camp |

---

## 8. Ben's answers and questions

**Answered** (Ben, 2026-10-22, in one line for all five: CL-1 to CL-4 as recommended, "Add alliances" for CL-5):

| Id | Question | Ben's answer |
|---|---|---|
| CL-1 | **Should a faction tell the player why?** | (a), as recommended (Ben, 2026-10-22): an envoy says one line when it arrives, and a notice says who is angry and for what |
| CL-2 | **Can a camp blame the wrong player?** | (a), as recommended (Ben, 2026-10-22): blame spill 3 times in 10 (CL-D18) |
| CL-3 | **How often does a Frostmaw rob a busy line?** | (a), as recommended (Ben, 2026-10-22): a lurk about 0.35 a year at 4 legs a day, 1.9 at 12, 5.2 at 24 (CL-D16) |
| CL-4 | **How often is a strong base tested?** | (a), as recommended (Ben, 2026-10-22): about one fight in eight years at a fortress, every one lost by the attackers |
| CL-5 | **Alliances between camps?** | **(b)**, not the recommendation (Ben, 2026-10-22): "Add alliances". Camps that share an enemy can ally and send joint war parties; CL-D19 is reversed |

**The questions as they were asked** (recommended option first):

| Id | Question | Options | Why it is yours |
|---|---|---|---|
| CL-1 | **Should a faction tell the player why?** | (a) **Yes (Recommended):** an envoy says one line when it arrives ("You killed Vorr, our chief, in the spring of year 3. Pay 106 emeralds."), and a notice says who is angry and for what. (b) Only on request, through a command or a page. (c) Never: the player guesses. | It is what the player reads; the reasons are kept either way. |
| CL-2 | **Can a camp blame the wrong player?** | (a) **Yes, 3 times in 10 (Recommended)** when a player's kill had no witness and another player is in reach: a bystander may be asked to pay, or raided, for a neighbour's deeds (Bea, 3.4: 0.14 fights a year on her with it, 0.08 without). (b) Never: only the doer is blamed. | In multiplayer it punishes a player for what another did. |
| CL-3 | **How often does a Frostmaw rob a busy line?** | (a) **This design's rate (Recommended):** a lurk about 0.35 a year at 4 legs a day, 1.9 at 12, 5.2 at 24, roars between. (b) Today's ladder (MOWZIE.md §4.2): none below 70 grievance (none at 4 legs), then a lurk every 10 days, to the day (from day 14 at 12 legs, 9.6 a year). (c) This design with the Frostmaw's grudge weighing twice as much: 0.33 at 4 legs, 4.2 at 12, 12.5 at 24. | It sets how much the railway costs in Frostmaw country. |
| CL-4 | **How often is a strong base tested?** | (a) **Now and then (Recommended):** about one fight in eight years at a fortress, every one lost by the attackers, each with a reason (a new neighbour's first test, a feud after a chief fell at his walls, word he is away). (b) Never unless he provokes. (c) About one a season. | It decides whether walls end the game's danger. |
| CL-5 | **Alliances between camps?** | (a) **Not in this build (Recommended):** camps fight, truce and feud pairwise. (b) Camps that share an enemy can ally and send joint war parties. | Alliances are a new mechanic with Java-visible joint parties. |
