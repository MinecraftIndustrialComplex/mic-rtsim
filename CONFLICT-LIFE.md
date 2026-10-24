# Conflict life: how factions decide to fight

**Status:** design approved by Ben (he has answered every question so far, CL-1 to CL-9, including
"Add alliances" and "All faction news in-world"), built only in part: alliances are designed and
measured in the model, not built in the game. CL-10, how a player hears gossip, is being answered as a
new TALK design. Design first drafted 2026-10-21; revised 2026-10-22 after an adversarial review and
MOWZIE.md's MZ-15, and again the same day for Ben's alliances (CL-5 (b), section 2.16). Branch
`plan/conflict-life`, worktree `mods/mic-rtsim-conflictlife`; the alliances on branch
`plan/umvuthi-alliances`, worktree `mods/mic-rtsim-conflict2`.

**Ben's answers (2026-10-22).** CL-1 to CL-4 take (a), as recommended. CL-5 takes (b): "Add
alliances". Camps that share an enemy can ally and send joint war parties; CL-D19 is reversed, and
section 2.16 is the alliance design. The questions the alliances raised are answered too: CL-6 and
CL-7 take (a) ("Rare, bases fair game"), CL-8 takes (a) ("Not in this build"), and CL-9 became a rule
for all conflict: "All faction news in-world". There is no chat notice for conflict at all. An envoy
still says why in person, and every other piece of news (a feud, a pact sworn or broken, a raid, a
death) travels as gossip with traders, caravans and villagers (section 2.17, CL-D56 and CL-D57). One
question is open: how a player hears that gossip (CL-10). Section 8 has the answers.

**What the revision changed.** The engine's abstract fights are deterministic: the stronger side wins
(`village_outcome`, `engine.rs:4350-4377`; `threat.rs`, `rtsim-industry 403-427`). The first draft's
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
  party and the memory behind it, so an envoy can say in person what it is owed and why, and the news
  of a feud can say who is angry and for what, told in the world (2.17).
- **Camps that share an enemy can swear against it** (CL-5 (b), 2.16). Two camps that would each do
  better or be safer side by side swear a pact against one foe: a camp, a village or a player. A
  sworn camp calls its ally instead of marching alone, and the ally comes or refuses; the two
  contingents fight together, by the engine's law and certain. Pacts lapse, break on refusals, and are
  betrayed mostly by hungry chiefs who never swore them. A pact is news, carried as gossip by traders,
  caravans and villagers; there is no chat notice of it (2.17).
  Between camps the engine's law is linear, so two parties together kill no more than the two apart,
  and pacts are rare: in the valley left alone, one about every seven years and a joint fight about
  every 28, and the fights on players do not change (4.5). As designed a player almost never meets
  one: a joint party came at the fortress about once in 4,450 server hours and at the hunter about
  once in 9,300, and never at the others (CL-7).

What the player sees, in the model's numbers (section 4):

| Player | Today's ladder | The gate (rejected) | This design |
|---|---|---|---|
| A walled base (defence 80) near three camps, never pays | 2.7 fights a year on him, every one lost, 39 attackers killed a year | 0.8 a year, 99% lost, 16 killed a year | 0.12 a year (one in eight years), 97% lost, 3 killed a year: a new neighbour's first test (in 3 worlds of 4 within two years), a feud after a chief fell at his walls, word that he was away; two camps sworn against him have beaten his walls while he was away, about once in 240 years (2.16, CL-6) |
| A weak base (defence 6) that pays when the price is no more than a raid would take | 14.7 wars a year in the last decade: in some camps today's price outgrows the raid, and one refusal is war for good; 138 tribute a year | no war; 329 tribute | 0.1 fights a year; 177 tribute and 30 loot a year, 43% of his income |
| A weak base that never pays | 23.7 wars a year | 10.4 | 3.4, which take all his income |
| A player who hunts a goblin camp and kills its envoys (defence 50) | 7.5 fights a year on him | 3.3 | 0.32: the attacks that find him away win (24 loot a year, 5% of his income); the rest break on his walls |
| A railway at 4 legs a day past a goblin camp and a Frostmaw den | no robbery | no robbery | 2.9 train ambushes a year: 2.5 by the goblins (one in seven lost to the guards the owner puts on) and 0.37 Frostmaw lurks; 0.7 caravan ambushes; at 12 legs, 1.8 lurks (section 3.3) |
| Nobody: the world left alone | 30 fights a year, 10 between camps and every one of those lost, every world a forever war | 12.6, 2.1 between camps, 75% of those lost | 7.4 a year, 1.2 between camps, 12% of those lost; wars that start and end; two goblin camps swear against the raider outpost about every seven years and march together about every 28 (2.16) |

---

## 1. Why the three rules felt formulaic

| Rule | What it does | Why it reads as a machine |
|---|---|---|
| Ladder (today, EQ-5 (c)) | grievance crosses a threshold, the feud climbs a rung after a fixed dwell; each climb sends its rung's party once and only Siege repeats at its dwell (the model's ladder mode repeats every war rung, CL-D59); a Poisson clock sends a plunder at the nearest hated node | the same state gives the same act on the same day (a refused envoy is followed by a raid 9 or 10 days later, every time); every goblin camp acts alike; a camp throws raid after losing raid at a fortress (8.8 a year in the duel, every one lost); the plunder clock sends a band of 8 at the same camp forever and loses every time, so every world has a forever war; nothing in it remembers why |
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
| Umvuthana (7) | none | 3 warriors at 0.4 and 1 raptor pack at 2.6: 3.8 | 6 warriors and 2 packs, 7.6, and the Umvuthi leading it, 5.0: 12.6 | 1.4 | 9.8, and the Umvuthi on its throne 5.0: 14.8 |
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
| Warn | a player, the railway (at most once in 16 days each), a camp the council is angry with | a goblin howl, a Frostmaw roar, the gong, a raider war band shown at the edge, a bandit lookout on the road who speaks the warning when a player talks to him (TALK.md §5.4; it was a written warning, TK12). These are sounds and sights in the world; none pushes a line of text | today's FACTION_NOTICE (0x8801, a sound cue only) and MOWZIE.md's NOTICE_SHOWN (§4.12) |
| Scout | anyone but a caravan | scouts at the base, or a raptor look; scouts seen by a defence stronger than them lose half as they run; toward the railway, a watch on the line (about 21 a year at 12 legs in seed 1003) that misses the guards half the time | today's Scouts party; toward the railway kept abstract, no party |
| Demand | a player | an envoy with guards asking the culture's tribute good (goblin emerald, Umvuthana gold) | today's Envoy party, ENVOY_MATERIALIZE (0x8802) and `Demand`; MOWZIE.md §4.5 |
| Band | a village, a camp, a player | a small party (today's plunder party) | today's Plunder party kind |
| Raid | a village, a camp, a player | today's Raid party | today's Raid |
| Invasion | a village, a camp, a player | today's Invasion party; a raider captain leads it and falls if it is destroyed. The Umvuthi leads its tribe's in person, only when its pride is wounded (W of 0.8 or more), and there is no Umvuthana invasion while the grove has no Umvuthi. Going with the band, its 5.0 is added: 12.6 for a full invasion (MZ-3 (b); MOWZIE.md §4.5 under MZ-15) | today's Invasion; the chief's stack (MOWZIE.md ledger 4) |
| Skirmish | a rival camp | a band falls on the rival's hunters in the field: it meets the share of the camp out hunting (at least 15%, all of a camp of 4 or fewer) and takes a quarter of a band's carry | a band party whose target is the rival's territory edge |
| Ambush | a train, a caravan | the Frostmaw's lurk by the track (MOWZIE.md §4.4); a goblin, bandit or raider band lying in wait by a track or a caravan road | a train: MOWZIE.md §4.4's lurk and AMBUSH_SPOT, given to every culture with an ambush weight, and `attack_train`; a caravan: new (2.15) |
| Truce | a rival camp | nothing seen; the two camps stop fighting for Exp(192) days, sworn by both chiefs | new feud fields (2.11) |
| Pact | a camp, against a foe both quarrel with (2.16) | nothing seen between camps; against a player, no notice: he hears of it as news (2.17) | new feud fields (2.16); a news report (2.17) |
| Call | a sworn ally, against the foe | a joint party: two columns from two camps, arriving together | two of today's parties, each with its BAND_MARCH, naming each other (2.16) |
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
  480 blocks a day), and a chief who led and won comes back with them. Until they are home the camp's
  defence and its next muster lack them, and a grove lacks the Umvuthi's 5.0.
- **Pressure.** Members killed by a player slow recruits (EQUILIBRIUM.md §2.4.5).
- **Hunger and seasons.** A fed camp values loot little: most fights come in winter and after bad
  years, and stop when the stores are full.
- **Truces.** A camp afraid or sociable enough offers a truce, and the other side accepts if its
  chief is sociable or afraid enough to outweigh its wounded pride, or one time in five; both chiefs
  swear it. In 64 worlds of 50 years, 1,511 truces between camps were sworn; the chiefs who swore them
  fought the other camp 45 times while they ran, later chiefs 300 times.
- **Peace cost and kin cost.** Making a new enemy, or fighting one's own culture, is dear.
- **Pacts fray** (2.16). A pact lapses after Exp(192) days; refusals, defeats and short contingents
  strain it until it breaks; falling on an ally ends it and brands the betrayer. A sworn ally makes a
  camp dearer to attack (deterrence), and an alliance pays only where one camp cannot win alone or
  fears the foe, so it does not snowball into a bloc: in 320 worlds of 50 years no camp held more than
  two pacts at once, and in 310 none held more than one.
- **Clashes keep it from going quiet.** Hunters of two hostile neighbouring camps meet about once in
  two years when both are fed, up to two or three times a year when both starve (rate 1 a year x
  (0.5 + hunger + hunger)); one side loses one or two members. Each clash is a provocation with a
  memory, and some become feuds. Sworn allies' hunters do not clash while the pact runs (2.16).
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
has the envoy say its reason in person (below); news goes through rtsim-knowledge (2.17). The parties, the raid director and the reconciler see the same party
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
| | memories (4) | 32 | 8 bytes each: kind u8, flags u8 (hostile, and so on), count u16 (saturating; a name index for a dead chief; for the six kinds that name a second party, a camp or the player a pact is sworn against, a u8 count and its feud index, 2.16), weight u16, day u16 |
| | away until, truce until, last warning, paid until, refused at | 20 | days |
| | the swearing chief | 4 | the chief's ordinal (`Faction::chiefs`) |
| | the defeat floor: strength and day | 8 | |
| | believed stock: amount and day | 8 | |
| | guards news | 4 | the day word came that the trains ride bare (a railway feud) |

About 45 bytes a faction and 96 a feud; for alliances (2.16), 16 more a faction, 40 on each feud toward
a camp and 4 on each toward a player. A memory is a code and its numbers, not text: the story is written
when it is read (2.14). The parties on the road home are today's `Party` records.

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
(ledger 16, MZ-1 (a)). This design adds no record kind and no record id on the boundary (2.12). One
record gains the reason, 16 bytes (reason u8, motive u8, memory kind u8, flags u8, count u16, day u16,
the other party u64: an actor, or a chief's name index):
- **An envoy speaks its reason, in person.** ENVOY_MATERIALIZE (0x8802, `boundary.rs:36`) grows from
  104 + 8m to 120 + 8m bytes; the envoy says one line to the player it addresses when they meet:
  "You killed Vorr, our chief, in the spring of year 3. Pay 106 emeralds." (CL-1, which Ben kept). It
  is the envoy's own speech, shown with the envoy, never a system notice.
- **A notice is a cue, not a line.** FACTION_NOTICE (0x8801, `boundary.rs:35`) stays 64 bytes and
  carries no reason and no text: the howl, roar or gong, and nothing in chat (Ben, "All faction news
  in-world"). Why a faction is angry reaches a player as news (2.17), or from its envoy.
- **Alliances (2.16)** add the ally's actor to ENVOY_MATERIALIZE, 8 bytes (128 + 8m), and the
  partner's agent and a joint flag to BAND_MARCH (72 bytes plus its counts and waypoints).
- **News (2.17)** is told through rtsim-knowledge's reports and gossip, inside the engine; none of it
  crosses the boundary as a notice. A player hears it by talking to the people who carry it (CL-10,
  answered by Ben's TALK design, TALK.md).
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
3. The reason field on ENVOY_MATERIALIZE and its GameTest, and the news kinds (2.17) with their tests.
4. With MOWZIE.md's lurk, AMBUSH_SPOT, RAIL_PASS, TRAIN_ROBBED and the sampler: the lurk for every
   culture with an ambush weight (test 12).
5. With CHIEF_STATE: a stolen crystal as a Theft report (test 14).
6. With BAND_MARCH and MZ-15's march: the walk-in and the quota's walk-in (2.12).
7. Caravan ambushes: the `rtsim-world` query and the robbery report.
8. Alliances (2.16): the pact fields and acts, the call and the joint fight on the harness first (tests
   21 to 25); then, with BAND_MARCH, the joint march, the shared quota, and the ConflictBridge's target
   listener (GameTests 5 and 6).

**UR-1.** The design adds no entity, block or item, no body and no world change. Its state lives in
the faction rows and the wheel, which rtsim saves and drops as today; removing rtsim leaves nothing,
and reinstalling starts every council fresh, with beliefs at their priors and no memories. Members
away on a march at an uninstall are lost and booked Other at a reinstall (EQUILIBRIUM.md §2.3), a
cost MZ-15 lengthens and this design does not change.

### 2.12 Marches, arrivals and the spawn quota (MZ-15)

MOWZIE.md's MZ-15 gives every party a visible march (§4.16). This design takes it as it is:

- **BAND_MARCH (0x880B) is the march record.** This design has no march or route record of its own and
  adds no record id. Its acts are today's party kinds, so each rides BAND_MARCH like any party: a
  skirmish's route ends at the rival's territory edge, a caravan ambush's on the road, a lurk's at its
  AMBUSH_SPOT. A joint party is two parties, each with its own BAND_MARCH naming the other (2.16). The
  id it touches, ENVOY_MATERIALIZE 0x8802, gains fields, and none clashes with
  MOWZIE.md's planned 0x080C, 0x0811, 0x0091, 0x8807, 0x8809 or 0x880B.
- **The 128-block rule**, worded once in EQUILIBRIUM.md §2.2 and used here as written:

  > **A war party on the march is not an arrival** (MZ-15, MOWZIE.md §4.16). An arrival adds a member
  > the site did not have; a party's members are its camp's own, out on a route that BAND_MARCH holds
  > and the far view draws from about a kilometre away. So the 128-block rule does not apply to it:
  > its bodies are placed where its march puts them as they come inside the entity-ticking area. For a
  > simulation distance of s chunks its edge is 16s to 16s + 16 blocks off along an axis and up to about
  > 1.4 times that on a diagonal (`DistanceManager.java:252-253`): 160 to 176 blocks at 10, vanilla's
  > default, and 128 to 144 at 8. The column fills in stack by stack (a row of two warriors, a raptor
  > pack, the chief alone; MZ-17, provisional (a)), each stack the tick its last member comes inside,
  > so a raptor pack's nearest body can be a few blocks inside the edge. No warrior or raptor is drawn
  > where it appears, at either distance: they walk into sight. The Umvuthi, the one body drawn beyond
  > 128 blocks (132), goes aloft at the rear of the column; at 8 it can be seen to appear 128 to
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
- **The Umvuthi goes with its band**, aloft at the rear of the column (MOWZIE.md §4.5, §4.16, MZ-16's
  second round, provisional): its 5.0 is in the band's strength, 12.6 for a full invasion, and missing
  from the grove's defence until it is home. If the band is destroyed, it falls with it, and the grove
  waits for an heir (2.10).
- **The spawn quota does not shrink a band.** MOWZIE.md leaves this to the conflict engine: a band
  bigger than the quota of 16 members in Minecraft at once (`tunables.rs:94`; a goblin invasion of 26,
  a raider one of 23, a bandit one of 20; a Mowzie band never exceeds it) keeps its whole strength in
  the fight's books. Its column is embodied up to the quota, and the members past it stay in the
  record and walk in along the route from the ticking edge as the quota frees, as `start_fight`'s
  later waves send only the members not yet in Minecraft (`spawn_wave`, `engine.rs:3378-3390`). The
  player sees the band the far view drew, arriving in two or three bunches. The two contingents of a
  joint party share one quota of 16 (2.16, CL-D49).

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
| bandit | 0.45 | 0.45 | 0.6 | 0.15 | 2 | 1.2 | 24 | 14 | 4 | 6 | ambush weight 1.6, ally weight 1.2 |
| raider | 0.6 | 0.6 | 0.35 | 0.15 | 2.5 | 0.9 | 96 | 20 | 4 | 2 | preempt 10, oath 10, pride_w 20, ally weight 0.8 |
| Umvuthana | 0.5 | 0.45 | 0.65 | 0.15 | 2.5 | 1.2 | 48 | 16 | 6 | 10 | guard share 1, invasion pride 0.8, chief strength 5, one by one, heir wait, hunts |
| Frostmaw | 0.5 | 0.5 | 0.1 | 0.25 | 3 | 1.0 | 96 | 30 | 14 | none | council of one, its life 60, pride_w 25, killer's grudge 80 and pride 2, ally weight 0 |

The ally weight (2.16) is 1.0 where the table names none; the `mind` block gains it, one line.

World numbers, shared: loot value 0.2 an item fed, 0.03 beyond the stores; boldness shift 1; battle
doubt 0.3; belief drift 1/384 a day, a quarter of it for a player's base; rumour error 0.5; speaker
weight 0.6; train notice 0.05; caravan attention 0.03; clash rate 1 a year a pair; guard share 0.25;
field share 0.15; peace cost 8; tributary cost 12; sworn multiplier 3; tribute truce 48 days.
Alliances (2.16): pact days 192; kin +4 and hated -10 to a pact's worth; effort of offering 3, of a
call no one answers 1; a refused call costs 1.0 of the oath's truce cost; strain breaks a pact above 2
and halves over 96 days; betrayal 10 beyond the oath; the guard up to 8 and deterrence up to 4 an ally.

### 2.14 The story

Each party and envoy, and each piece of news about them, carries its top motives and the memory behind them, as codes. Text is
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
| Envoy and tribute | today's Envoy, ENVOY_MATERIALIZE, `Demand`, tribute goods; MOWZIE.md §4.5 | the price (2.8); the reason field |
| Train ambushes | MOWZIE.md §4.4 lurk, AMBUSH_SPOT, `attack_train`, TRAIN_ROBBED, the track sampler (MZ-12); today's incidental ambush of a marching war party (`SYS_AMBUSH`) stays | the lurk opened to every culture with an ambush weight |
| Caravan ambushes | rtsim-world's caravans (`caravans.rs`, `caravan_place`) | a query for caravans passing a camp's reach through the conflict engine's world interface, and the robbery's loss of cargo reported to the economy |
| Chiefs, heirs, temperament | `Notable`, `Temperament::from_seed`, `succeed`, C24 grudges | an elder seed, standing, gamma tenure |
| Memories | `MemorySlot`'s 8-byte shape, report kinds (Killing, Robbery, Theft for a stolen crystal) | conflict memory kinds (lost at, drove us off, refused, killed envoy, raided us, border, paid, guards gone) |
| Notices (sound cues) | FACTION_NOTICE, NOTICE_SHOWN (MOWZIE.md §4.12) | none: no reason, no text (2.17) |
| News | rtsim-knowledge's reports, noticeboards and gossip packets, carried by travellers and messengers (PR6) | report kinds for a feud, a pact sworn, a pact broken, a raid and a chief's or champion's death (2.17) |
| Renewal and pressure | EQUILIBRIUM.md §2.4.5 and §2.4.7 (as designed there; `rtsim-kernel::renewal` is EQ-6 (a)'s design, not built) | none |
| Camp bodies, UR-1 | MOWZIE.md §4.9 reconciler, MZ-15's census | none |
| Decision | none fits | the council: drives, beliefs, Thompson draw, logit choice, truces, clashes |
| Alliances (2.16) | feud rows, the sitting's acts, `village_outcome` and `threat.rs` as the joint resolve's parts, today's parties and BAND_MARCH, NeoForge's `LivingChangeTargetEvent` (the core listens already, `industry/ThreatEvents.java:32`) | the pact fields and acts, the open-pact list, the call, the joint resolve (one fight on two parties at a camp, a village or a base) and its books, a partner on `Party` and BAND_MARCH, the partner-aware wait and the joint arrival, a two-party `start_fight` with one epoch, the shared quota, the target listener |

### 2.16 Alliances (CL-5 (b))

Ben's answer to CL-5 (2026-10-22): "Add alliances". Camps that share an enemy can ally and send joint
war parties. This section is that design. The model has it (`"alliances"` in `LIFE_DEFAULTS`), and
every number in sections 3 and 4 is measured with it on. Alliances draw from a random stream of their
own, so with them off the model is the world before them, draw for draw: `run.py`'s `*_noally`
scenarios are the same seeds with alliances off, and they reproduce the earlier results exactly. They
are the control for every comparison in section 4.5.

**What the fight allows.** Between camps the engine's fight is the linear law: the winner loses d / A
of its party. For members of one strength that is the same number of dead whatever the party's size,
and beside an ally each contingent loses the same share of its members. The take is what the joint
party left standing can carry, capped by what the target holds, and it is split by carry. So a camp
that can win alone gains less from an ally than it seems: it loses fewer, but where the target's
holdings cap the take, it carries home only its share of the same take. A joint party pays where one
camp cannot win alone, or where the dead it would lose alone are dear. At a player's base where
turrets outweigh guards the engine's fight is the square law (`threat.rs:461`), and a bigger party
loses fewer, so two camps gain more there. The model learned this the hard way: with pacts that bound
nobody, camps swore and then marched alone (CL-D44).

**Who can swear.** Camp f can offer camp y a pact against x when all of these hold:
- each reaches the other, and both reach x. x is a camp, a village or a player; never the railway or
  a caravan, which a band ambushes alone (CL-D24).
- both quarrel with x (a standing hate, a grievance of 30 or more, or wounded pride of 0.5 or more), or
  one does and the other is of its own culture.
- neither is sworn to the other already, neither is sworn to x, and neither is sworn against the
  other: a camp sworn against the Grey Outpost cannot swear beside it against a player.
- y's culture makes alliances. The Frostmaw never does: a council of one has no one to swear with
  (ally weight 0). Bandits swear most readily (1.2), raiders least (0.8), goblins and the Umvuthana at
  1.0.
- f is calm toward y (grievance under 30, wounded pride under 0.5), and y has not turned f down in the
  last 48 days.

A camp may hold several pacts, each with one camp against one foe, and at most four at once (the list
on its faction row, below; the model has no cap and never had a camp in more than two). A camp holding
four makes no offer and refuses any. While a pact
runs, the two camps' hunters share the border: no clash between them.

**What a pact is worth.** An offer is one of the acts a sitting weighs toward x (2.6), on the same
Thompson draw of x's strength. Its worth:
- **The gain:** the best joint war f could make beside y, over the best war it could make alone today
  (none is 0), floored at 0. The joint war counts y's help at the chance y comes: y's record of
  coming, Beta(ka, kb), 1 in 2 at first. If y does not come, the party goes alone, or turns home if
  going alone is worth no more than turning home (-1, the call's wasted effort). Each pact is valued
  as if it were the camp's first: a second pact against the same foe is measured against going alone,
  not against the joint war the first allows.
- **The guard:** what y at f's side is worth for safety against x, up to 8. It grows with the danger
  x poses and with y's strength against x's. The danger is the members x killed lately at f's camp or
  its border (a raid on it, a clash, a player's hunt), a tenth of the camp being full danger, and for a
  camp also how far its believed strength passes f's own defence. y's strength counts against x's only
  when x is a camp; against a player or a village the guard is the danger alone.
- The gain and the guard are scaled by the culture's ally weight and the council's sociability
  (0.5 + soc).
- **Who y is:** kin +4; a culture that hates f's, or that f's hates, -10 (raiders hate goblins and
  bandits, goblins hate bandits); less the grudge and wounded pride f bears y, as for any act.
- Less the effort of sending, 3.

A camp offers only when the gain and the guard together are above 0: as it sees x today, it would do
better or be safer beside y.

**The answer.** y weighs the same worth from its side, with its council's mean temper and its middle
guess of x. No speaker is drawn: a camp that is asked answers without calling a sitting. It refuses
outright if it bears f a grievance of 30 or more or wounded pride of 0.5 or more, or if the bars above
close the pact from its side. Otherwise its logit weighs the pact against refusing (0). A refusal is
remembered for 48 days.

**Swearing.** Both chiefs swear. The pact runs Exp(192) days (mean two game years) unless something ends
it first. Each camp tells the other what it knows of x: each belief takes the other's guess as an
observation (its uncertainty and 0.3 in the log, added in quadrature), and the stronger floor (2.4) is
shared. Each camp then sits on x within 1 + Exp(2) days (an "allied" occasion, sat unless the camp is
still busy with its last fight). While a camp is sworn against x, the peace cost toward x (2.9) is
waived: the oath has made x its enemy already.

**The oath binds** (CL-D44). A camp sworn against x does not march alone on x with a party its ally
could stand beside: it calls the ally. It still sends alone what no ally could join: a skirmish on x's
hunters, and a band where the ally can muster no party that small.

**The call.** The council weighs each of its war parties beside the ally's help as it believes it: the
ally's largest party of the same kind or a smaller kind (band, raid, invasion), from the members it
believes the ally has at home (its guess of the ally's defence). Kinds are not sizes: a party's size is
its culture's, so a goblin raid of 12 can join a grove's raid of 4. The grove's invasion is never
counted on: it waits on the Umvuthi's own pride (2.10), which no ally can know. When the council chooses
a joint party, an envoy goes to the ally. Between two camps nobody sees him, and the ally answers in the
same sitting:
- **Busy.** It cannot send: it is still busy with its last fight (two days in the model; in the engine,
  until that party is home), or it cannot muster a party of the caller's kind or a smaller one. No
  blame.
- **Join or refuse.** The caller's envoy tells what it knows of x, as at the swearing. The ally's
  council draws a guess of its own and weighs each party it could send, of the caller's kind or a
  smaller one, as its own war beside the caller's party: its own dead, its share of the take by carry,
  its own grudges and pride toward x, and the cost of x's allies (deterrence, below). It takes the best
  and weighs it against refusing. Refusing shames the oath: the culture's oath x (1.5 - aggression), the
  cost of breaking a truce, 3 times over for the chief who swore (CL-D8's multiplier) and once for his
  heir. A logit decides.

A refusal costs the ally the caller's trust (kb + 1), strains the pact (+1), and wounds the caller's
pride (+0.2), with a memory: "Mudhollow would not come against the Grey Outpost in the winter of year
3". The caller then goes alone if that is worth more than turning home, and its reason says so:
"Mudhollow would not come, so it went alone". A join adds to the ally's record (ka + 1).

**The joint fight.** The two contingents fight x together, by the engine's law and certain, as every
fight is (CL-D25). The engine fights one party at a time today, so the joint resolve is new (2.15),
built on the same calls:
- **At a camp or a village:** one `village_outcome` on A + AB against d, its `observed` list the union
  of both camps' observed fights (each shared its own at the swearing), which floors the attackers'
  surviving share as it does for one camp; 2.4's belief floor is another thing. Each contingent loses
  the same share of its members. A destroyed party destroys both contingents, and a chief who led either
  falls with it (CL-D41). The take is the joint carry left standing, capped by the target's loot share
  of each good, and split by carry.
- **At a player's base:** one `Threat` for each threat class, on both contingents' members of that
  class summed (count, strength, loads), resolved once by `threat.rs`. Each contingent loses
  `abstract_loss_share` of its own held-back members of the class (`engine.rs:2977-2980`), and the
  class's take is split by the carry of each contingent's survivors in it. Two contingents of
  different classes lose different shares, as two bands would. The model fights a player by the square
  law on the sum, which simplifies `threat.rs` (the square law only where turrets outweigh guards).

Each camp books its own:
- its dead, its fear, what it learned of x, and its floor if it lost;
- its memory: "we lost 9 at the Grey Outpost beside Mudhollow";
- at a player's base, its own pressure (EQUILIBRIUM.md §2.4.5: the warriors the player killed, over
  its own cap) and its own grievance for its dead.

The target meets one fight and blames both. A camp attacked bears each attacker the grievance of its
dead in proportion to that attacker's strength, +20 each, and wounded pride. It keeps a memory naming
both ("Ashpit and Mudhollow raided us in the spring of year 7") and sits on each. Its own sworn allies
are provoked at both attackers (+10 grievance, +0.2 pride, a memory, an occasion). Word of its losses
spreads as after any raid.

**After the fight.** A win draws the two together: the caller's record of coming rises in the ally's
book (ka + 1 on the ally's feud toward the caller; the join already raised the ally's in the caller's),
and the strain in the pact falls by 0.5 on each side. A defeat strains it, the ally most (+1 for the
ally, +0.5 for the caller): it followed and lost. An ally that sent less than the caller counted on
(under 0.8 of the strength it believed) is resented: +1 strain and a memory, "Mudhollow sent too few
against the Grey Outpost". Strain halves over 96 days, and the record of coming fades toward even over
192, as tribute trust does (2.4).

**How a pact ends.**
- **Lapsed:** its Exp(192) days ran out.
- **Strained:** the strain on either side passed 2, after defeats and short contingents.
- **Refused:** a refusal took the strain past 2.
- **Torn:** a camp sworn to both sides of a fight. An attack on x provokes x's allies; one that is also
  sworn to the attacker takes +1 strain on that pact.
- **Betrayed:** one camp fell on the other.

A camp beaten down to a few members keeps its pacts until they lapse: a call to it comes back busy,
with no blame, and the caller goes alone; it deters only as far as its neighbours still believe it
strong.

**Betrayal.** Falling on a sworn ally costs the oath (as breaking a truce does, 3 times over for the
chief who swore) and 10 more. A council that does it anyway ends the pact. The betrayed camp takes it as
the gravest insult: +1 pride, +40 grievance, and a memory of weight 3, "Ashpit broke its oath and fell
on us in the summer of year 12". Every camp that knows the betrayer trusts its word less (kb + 1): it
counts less on the betrayer's help, so a pact with it is worth less. The betrayer's reason names the
oath: "Gitz broke the oath sworn with Mudhollow against the Grey Outpost", or, for an heir, "Gitz was
not chief when Ashpit swore". The oath binds the chief who held the seat at the swearing, not whoever
spoke it: a war leader who speaks for his chief and takes the seat later is an heir to it (3.6).

**Deterrence.** Falling on a camp costs more for each of its sworn allies that can reach the attacker,
whoever the pact is against: up to 4 each, by that ally's strength against the attacker's own
defence, as the attacker believes it. Camps know their neighbours' pacts: envoys walk between camps,
and a pact is no secret among them (a player hears of one only as news, 2.17). This is a choice,
not a measurement of what camps could learn. The list of a camp's pacts is on its faction row, so a
sitting that weighs a fight on a camp reads that row (a page load when it is not resident; counted in
5.2), not its feuds. A camp that swears is safer for it, which is half of why camps swear (the guard).

**Joint demands** (CL-D48). An envoy may name a joint party as its threat: "Mudhollow would come with
us". The price is set on the joint take (2.8: between the war's worth to the camp and the believed
chance to win times what both would carry home). A payment is split by carry, and the ally is bound by
the same truce, sworn by its own chief, and remembers the payment. Only the caller's envoy walks: one
party, one line. A player who refuses may meet the joint party, called as any is, and the ally may still
refuse.

**Envoys and news** (CL-1 (a), CL-9, CL-D55, CL-D56).
- **The envoy names the ally, in person:** "You killed Vorr, our chief, in the spring of year 3. Ashpit
  and Mudhollow ask 140 emeralds." ENVOY_MATERIALIZE's reason gains the ally's actor, 8 bytes
  (128 + 8m).
- **A pact is news, never a notice.** When two camps swear, march together or one falls on the other,
  nothing is sent to a player's chat, whoever the pact is against (Ben, 2026-10-22: "All faction news
  in-world"). The caller's howl, gong or war band shown is the only cue, and it carries no line.
  Instead the pact becomes a report (2.17): "Ashpit and Mudhollow have sworn against you", "Ashpit and
  Mudhollow have sworn against the Grey Outpost", "Mudhollow broke its oath and fell on Ashpit". It
  spreads as gossip, at the speed of trade, to the traders, caravans and villagers a player meets. In
  the valley left alone they come about 0.15, 0.04 and 0.03 times a year; a player far from any
  trade route hears of none.
- **A joint party's reason names both camps:** "beside the Grey Outpost, sworn against player Ben".
  The reason is the story's (2.14) and the news's, not a line shown on arrival.

**What the player sees.**
- An envoy naming two camps, often first, and the news of the pact from the next trader.
- Two columns of two cultures marching on him from two camps and arriving together: a goblin band and
  a pillager war band side by side, not fighting each other (below; the goblins in bodies once P8
  picks their mod).
- After a fight he wins at his walls, two camps that hate him for their dead, each with its own memory.
  After one they win while he is away, each camp takes its part of his stock.

**Peaceful** (MOWZIE.md §4.12: no war party leaves; CL-D53). Councils sit and pacts may still be sworn:
a pact is words between camps. No call is made and no joint party leaves.

**UR-1.** A pact is fields on two feud rows (2.11), saved and dropped with them, and a joint attack is
two ordinary parties that name each other. Nothing is added to the world: no entity, block, item,
scoreboard team or saved tag. The Java side's joint key lives in memory only, built from BAND_MARCH
records, which the engine resends at every session start. An uninstall leaves nothing, and a reinstall
starts with no pacts, as every council starts fresh (2.11).

#### In the engine

**State.**

| Where | Field | Bytes | Notes |
|---|---|---|---|
| `Faction` | open pacts (up to 4) | 16 | each the ally's faction index (u16) and the foe's index in our feud list (u16), kept in step with the feud rows: deterrence and the bars on swearing read it |
| `Feud`, toward a camp | pact until, since | 8 | days; a pact is open while now is before its lapse, so nothing is booked for it |
| | the swearing chief | 4 | the chief's ordinal (`Faction::chiefs`), as for a truce |
| | the foe | 8 | its actor |
| | the ally's record of coming: ka, kb | 8 | fading toward even over 192 days, read at the feud's anchor day, as trust is |
| | strain | 4 | halving over 96 days, read at the feud's anchor day |
| | refused at | 4 | the day this camp last turned our offer down |
| `Feud`, toward a camp or a player | hurt | 4 | our members this neighbour killed lately at our camp or its border (danger), halving over 96 days, read at the feud's anchor day |
| `Party` | the partner party | 8 | a handle, set on both contingents of a joint attack |
| | going on alone | 1 bit | the sitting found going alone worth more than turning home (-1) |

About 16 bytes on each faction, 40 on each feud toward a camp and 4 on each toward a player. A feud
toward a village has no ally fields.

**Memories that name two parties.** Six memory kinds name a second party, a camp or the player a
pact is sworn against: "we lost 9 at the Grey Outpost
beside Mudhollow", "the Grey Outpost drove us and Mudhollow off", "Ashpit and Mudhollow raided us", "the
Grey Outpost raided Ashpit, our ally", "Mudhollow would not come against the Grey Outpost" and
"Mudhollow sent too few against the Grey Outpost". The 8-byte slot keeps its shape: for these kinds its
count is a u8 (the dead of one fight fit) and the other u8 is the second party's index in the camp's
own feud list. If that feud has been dropped when the memory is read, the line says "an ally" or
"another camp" for a camp, and "a player" for a player.

**Events and draws.** No new event kind. An offer and a call are answered inside the caller's sitting:
the answer reads the other camp's rows, a page load when they are not resident, as a fight on a camp
does. Every alliance draw comes from `StreamKey(seed, SYS_ALLY, key).slot(n)` of the camp it belongs
to, n that camp's own count of alliance draws, so one camp's alliances never shift another camp's
draws (CL-D21, CL-D54):
- the answering camp: its guess of x and its logit, at an offer or a call;
- the offerer: the pact's Exp(192) lapse, at the swearing;
- each camp: its "allied" occasion's delay, and after a joint fight its chief's fall (CL-D41);
- the target of a joint fight: its provoked delays and the news of its losses.

The model draws all of these from one stream for the whole world (`arng`), an approximation: there one
camp's answers shift the others' alliance draws, though never their council draws.

**Records.** No record carries two factions' members, and no record id is new:
- **BAND_MARCH (0x880B):** each contingent is its own party with its own record: one faction, one
  culture, its own route and phase. It gains the partner's agent (8 bytes) and a joint flag: 72 bytes
  plus 8 per body kind and 16 per waypoint.
- **ENVOY_MATERIALIZE (0x8802)** gains the ally's actor, 8 bytes. FACTION_NOTICE (0x8801) gains
  nothing: it is a sound cue and stays 64 bytes.

**The march** (MZ-15, CL-D51). Each contingent marches its own route from its own camp. The engine
ends the nearer contingent's muster later, so both arrive at the target on one tick. The mustering
phase already holds a band at its camp, its members standing in it (MOWZIE.md §4.9, §4.16). A
contingent that arrives first, because a route changed or a column slowed, waits at the target until
the other arrives or ends. That wait is new: `Phase::Waiting` today ends in `wait_over`, which sends a
war party home (`RECALL_DONE`), so a contingent's wait checks its partner first. If the other was
destroyed or recalled on its march, the survivor goes on alone only if its sitting found going alone
worth it (the party's flag); otherwise it turns home. The second arrival resolves both: unwatched,
the joint resolve above; watched, one fight for both (below). The census treats each contingent as any
party (CL-D42; EQUILIBRIUM.md §2.6 item 11): a camp that comes in while its contingent is out has that
contingent's bodies taken, and the march goes on.

**Edge loading and the quota** (CL-D35, MZ-17, CL-D49). Each column fills in stack by stack at the
entity-ticking edge, as any band's does (2.12). A joint attack shares one spawn quota (D1): 16 bodies in
Minecraft at once for both contingents (`tunables.rs:94` is per band today), split in proportion to
their bodies, at least one stack each. Today's quota counts members (`spawn_wave`: `room = spawn_quota -
in_mc` over the party's units), and D1 is about bodies, so the shared quota counts bodies: for goblin,
bandit and raider contingents that is the same, and a raptor pack (one member of 5 bodies, MOWZIE.md
§4.5) counts as 5. A joint attack then never puts more than 16 bodies in Minecraft, no more than one
band of 16. The members past a contingent's share walk in along its route as the quota frees, as
`spawn_wave` does for one band. Two invasions (a goblin 26 and a raider 23) show about 8 members of each
at first, and the rest in bunches.

**The watched fight.** At a watched target the raid director runs one fight for both contingents: a
`start_fight` on two parties, with one epoch and one end, whose waves draw from both within the shared
quota. Each contingent books its own dead from its own bodies' deaths.

**Allies do not fight each other** (CL-D50). Bodies of one joint attack must not turn on each other
when a stray arrow hits. A goal-based mob retaliates through `HurtByTargetGoal` and changes its target
through `Mob.setTarget`; a brain mob that attacks through `StartAttacking` goes through it too. Both
fire NeoForge's cancellable `LivingChangeTargetEvent` (`Mob.java:245-250`, `StartAttacking.java:33`).
The ConflictBridge listens and cancels a change of target to a body of the same joint attack, from
each body's party and BAND_MARCH's partner. It listens at a higher priority than the industry's own
listener (`industry/ThreatEvents.java:32`), which skips cancelled events. There is no mixin and no
scoreboard team: a team is saved with the world (UR-1), and the event already covers the targeting,
which is all that needs stopping.
- **Friendly fire still hurts**, since `Projectile.canHitEntity` has no ally check
  (`Projectile.java:257-264`); only the grudge is suppressed. A member killed by its partner's body is
  booked to its own camp's dead in the fight, with no grievance toward the partner and no strain.
- **Some brain mobs bypass the event:** the hoglin, the zoglin and the warden write their target
  straight into memory (`HoglinAi.java:219`, `Zoglin.java:218`, `Warden.java:543`). None is a camp's
  body.
- **The Mowzie bodies go through `setTarget`** (verified in the jar):
  `UmvuthanaHurtByTargetAI.canContinueToUse` ends in `this.mob.setTarget(livingentity)`; a follower
  raptor takes its leader's target with `setTarget(leader.getTarget())`
  (`EntityUmvuthanaFollowerToRaptor:48`); the Umvuthi hands its target on with
  `umvuthana.setTarget(entity.getTarget())` (`EntityUmvuthi:1093`). None overrides `isAlliedTo`.
- **The raider bodies** are vanilla pillagers, vindicators and, in an invasion, a ravager (PLAN.md's
  fallback; `data/cultures/raider.json`): goal-based `Raider` mobs that retarget through `setTarget`.
- **The goblin bodies** are abstract until P8. Whichever mod P8 picks for them must retarget through
  `setTarget` or `StartAttacking`: a gate on that choice, and GameTest 5 runs on it then.

**The far view.** Two columns, each drawn as any band from its own BAND_MARCH (DH-2: walking figures,
never names or counts), converging on one target. Nothing new is drawn.

**The Mowzie cultures.** The Frostmaw never swears. The grove may swear, against a player both it and a
goblin camp hate, and its raid can be called; its invasion goes only on the Umvuthi's own pride, with
the Umvuthi aloft at the rear of its own column (MOWZIE.md §4.16). No ally counts on it.

### 2.17 News in the world (CL-9, "All faction news in-world")

Ben's rule (2026-10-22): conflict sends no chat or action-bar notice, to anyone, for anything. A
feud, a pact, a raid and a death are told the way everything else in the valley is told: as news that
travellers carry. Two things stay as they are, because both are in the world: the cues (the howl, the
roar, the gong, a war band shown at the edge, which are sounds and sights and carry no text), and the
envoy, who says why in person (CL-1).

**What is news.** Each is a report in rtsim-knowledge (`ReportKind`, appended after `Encroachment`, the
way that kind was added), made by the conflict engine when the thing happens, with the day, the place
and the two actors:
- **A feud opens or climbs:** a camp's anger at a player reaches a rung (notice, envoy, scouts, a war
  band). `who` is the camp, `whom` the player; the severity byte holds the rung. No feud news names
  the settlers (TALK.md §9.4, TK26).
- **A pact is sworn, and a pact is broken or betrayed:** `who` the camp that swore or broke, `whom` its
  ally; the foe is a second row of the same kind (flag bit 1, "against") that travels in the same
  packet. A betrayal names the camp that fell on its ally.
- **A raid, an invasion or a skirmish fell on a target:** `who` the camp, `whom` the target (the
  village or haven it marched on, else the feud's player), the severity byte the outcome
  (`report::raid`: 1 held, a watched fight the band left bored or done; 2 beaten, a rout, or an
  unwatched raid that lost warriors and took nothing; 3 plundered, it took goods or blocks), joint
  or not by a flag (unused so far). A raid that found nothing is no news.
- **A chief or a champion fell:** `who` the camp, `whom` the fallen notable, the severity byte how
  (`report::fell`: 1 died of age, 2 killed in a fight). When a player killed the chief, the
  Killing report the engine already makes names them. No champion exists yet.

The reasons are the engine's motive and memory codes (2.14), so the text is written when it is read,
from them and from names.

**How it travels.** As every report does (PR6, rtsim-knowledge's packets): the holders that saw it
first-hand (the camp's own board; the target's village or base, if it was a raid) learn it at once,
and everyone else only from a packet a traveller carried, so news spreads at the speed of trade and a
village nobody trades with learns nothing. Messengers walk each road four times a day as flows;
caravans and bands carry gossip on their circuits. News has a lifetime by kind, like any report
(PLACEHOLDER: a feud 10 days, a pact 30 and a betrayal 30, a raid 20, a death 30), and the same
eviction when a board is full. News kinds change no holder's feeling: their sentiment delta is 0, and
they are never written into vanilla villager gossip (`vanilla_mapping` gives them nothing), because a
rumour of a feud between two camps is not a villager's grudge. News about a player does not make a
village like or dislike him either; his deeds still do, as today.

**How a player hears it** (CL-10, answered 2026-10-22 by Ben's own design, TALK.md): he crouches and
right-clicks a villager, a ribbit, a caravan merchant or a clan's member, and it says one line, in
chat, to him alone: fluff, how it feels, or what it has seen, the last from the reports its board
holds (TALK.md §5.3). Chat is used only because the player asked; nothing is pushed. The carrier is
a body that already exists, and a player who talks to no one hears nothing (D3).

**What this removes.** FACTION_NOTICE's reason and text, CL-D55's two notices (a pact against a player
and the pacts between camps), `a_pact_is_news_nearby`'s player filter ("every player both camps
reach"), and every plan of a line of chat. A player who is near both camps and trades with no one
hears nothing, which is the point.

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
never attacks them and never pays. Seed 1000: 6 fights on him in 50 years (median 6).

1. **Y1 d27 to d74.** The Sun Grove's masks and the Umvuthi beat the gong at him; the Grey Outpost
   shows him a war band. Nobody knows how strong this new base is: each camp's guess is 15, give or
   take a factor of three.
2. **Y1 d76.** The Grey Outpost invades with 23. Its war leader Captain Vask had the floor; its chief
   Captain Hesk leads, as raider captains do. The council's draw of a new base came out low and it
   believed the attack certain. It meets 80: all 23 die, and Hesk falls with them. Vask leads now.
   The reason it carries: "loot; and anger at player Ben's mines and smoke".
3. **Y1 d77.** Mudhollow, the next day, invades with 26 under its chief Vorr, for loot and its
   restless warriors: all 26 die. Each camp now knows a floor: this base beat its whole invasion (36,
   30.4).
4. **Y2 to Y6.** No fights: war bands shown at his border, a howl, the gong, and Mudhollow's scouts
   (97 looks in the 50 years).
5. **Y7 d2, Y7 d58 and Y8 d1.** Three Grey Outpost envoys, under three different speakers, ask 68,
   69 and 119, each "after: we lost 23 at player Ben in the winter of year 1". He refuses all three.
   Nobody attacks: the floor holds, and the camp's belief of the base blurs slowly.

Over the 50 years: 6 fights (2, 0, 3, 0 and 1 by decade), every one lost, 147 attackers killed at his
walls, 22 envoys, nothing paid; 71 war bands shown by the Grey Outpost, 67 gongs and 12 howls. In this
world the Sun Grove never sent a war party at him. Over all 64 fortress worlds, a fight on him every
eight years, 97% of them lost; the first test of the new base came within two years in 73% of worlds
(the median on day 62). The 3% the attackers won were joint parties of two camps sworn against him
that found him away (3.6, CL-6).

### 3.2 Farming the Frostmaw den

Ben (defence 30, pays envoys) kills the North Den's Frostmaw whenever it is back, and now and then
steals the crystal from the sleeping Frostmaw at the Pass Den. The Sun Grove is also in reach.
Seed 1002: 111 Frostmaw deaths in 50 years over the two dens (median 110).

1. **Y1 d2.** He kills the North Den's Frostmaw. Its heir arrives on d14, 12 days later
   (`min(192, 6 x 2^P)`, EQ-4 (b)); he kills it on d16.
2. **Y1 d21 and d39.** He steals the Pass Den's crystal. On d39 he comes back for another; the
   Frostmaw wakes, and in the fight he is at in person, he kills it.
3. **Y1 d39.** The same day the North Den's next heir arrives, after a wait doubled by the pressure.
   It knows who killed the last one, and its restless clock went stale while the den stood empty: it
   raids him that day, wins (41.7 against his walls of 30) and takes 60. "Honour: player Ben killed the
   Frostmaw before me in the spring of year 1." He kills it on d41.
4. **Y1 d55 and d80.** The Pass Den's heir, arrived on d51, raids him for the same reason and takes
   44; so does the North Den's next heir on d80, the day it arrives.
5. **Y1 d78.** The Sun Grove asks 14 gold. A grove raid on his walls (3.8), or an invasion with the
   Umvuthi (12.6), against 30 would take nothing, so his script refuses. The gong follows, about once a
   year.
6. **Y2.** He steals on d11, d58 and d75. The Pass Den raids him on d32, d61, d75 and d84, 21 days, 3
   days and the same day after a theft, and again: "honour: player Ben stole the crystal". Each raid
   wins and takes 44.
7. **Y4 d83.** The Frostmaw wakes as he reaches for the crystal and drives him off.

Over the 50 years: 94 raids on him, none lost (18, 18, 24, 21 and 13 by decade): 24 by the North Den,
the den he farms, and 70 by the Pass Den, the den he robs (64 thefts). The North Den's Frostmaw died
86 times at his hand, 1.73 a game year over all 64 worlds, MOWZIE.md's 1.8 for a farmed den; the Pass
Den's 25 times, when it woke. Over the 64 worlds the Pass Den raided him 1.41 times a year and the
North Den 0.50; he lost 93 items a year, 19% of his income. The Umvuthi led its grove at his walls 65
times in 39 of the 64 worlds, its pride wounded by refusals, and lost and fell every time.

### 3.3 A railway through goblin land

A line runs past Ashpit (goblins) and the Pass Den (a Frostmaw) at 12 legs a day. Its owner (defence
30) never pays; after three robberies in 30 days he puts guards on the trains for 48 to 144 days (his
script). Seed 1000: 217 train ambushes in 50 years (median 222).

1. **Y1 d6.** Ashpit ambushes a train with 8 goblins under its war leader Gitz and takes 49: "a train
   came through; loot".
2. **Y1 d11 to d47.** The Frostmaw roars at the line three times: the rail meter has made it angry,
   and a roar is cheap. On d47 it lurks by the track and robs a train of 62.
3. **Y1 d60 to d77.** Ashpit robs again (its elder Snag had the floor, "restless warriors"), and on
   d75 the Frostmaw: three robberies in a month, and the owner puts guards on. On d77 the next goblin
   ambush meets them: all 8 die.
4. **Y2.** Ashpit's envoy asks the owner 116 emeralds. Refused. The guards are still on: on d69 the
   Frostmaw beats them (41.7 against 20) and loses half its haul to them, 33 items; on d78 the next
   goblin ambush loses all 8 again.
5. **Y3 d70.** Ashpit's new war leader Skarl invades the owner's base with 26, "the new war leader must
   prove himself; and loot": it wins, loses 3 and takes 167.
6. **Y3 d80 to Y4 d56.** The guards are off. Ashpit robs three trains in 15 days, and on d95 the guards
   go back on; the Frostmaw robs twice through them in Y4, and the next goblin ambush, on Y4 d56, loses
   all 8: "revenge: we lost 8 at the railway in the winter of year 2". The pattern holds but not on a
   calendar: across the 64 worlds, the first robbery after the guards come off follows by a median of
   28 days, from 5 to 74 (p10 to p90).

Over the 50 years: 230 fights on the line and its owner (47, 49, 37, 49 and 48 by decade), 32 of them
lost; Ashpit ambushed 106 trains and invaded the owner 13 times, the Pass Den lurked 111 times and
roared 218 times, and the guards went on 32 times.

How traffic changes it (8 seeds x 20 years each):

| Legs a day | Rail meter grievance (MOWZIE.md §4.3) | Frostmaw lurks a year | Frostmaw roars a year | Goblin ambushes a year | Guards put on a year |
|---|---|---|---|---|---|
| 2 | 0 | 0.17 | 0.88 | 2.46 | 0.29 |
| 4 | 17.3 | 0.24 | 1.89 | 2.37 | 0.44 |
| 8 | 52.0 | 1.00 | 3.59 | 2.66 | 0.57 |
| 12 | 86.6 | 1.81 | 4.34 | 2.42 | 0.63 |
| 24 | 190.5 | 5.01 | 5.04 | 2.16 | 0.81 |

The Frostmaw's robbing grows smoothly with traffic. Today's ladder has none below the Raid rung (none at
4 legs) and at 12 legs a lurk every 10 days from day 14, to the day, in every seed (9.6 a year; CL-3).
The goblins rob for loot at any traffic; the guards cap them. CL-3's answer quotes the sweep as it stood
(0.35, 1.9 and 5.2 lurks a year at 4, 12 and 24 legs); with alliances in the world the seeds draw other
histories, and the sweep's 4-leg cell, 8 seeds of 20 years, is the noisiest: over 64 worlds of 50 years
the line at 4 legs sees 0.37 lurks a year, with alliances and without (0.36).

### 3.4 Multiplayer: one provokes, another suffers

Ash (defence 50) hunts Ashpit's goblins every few weeks and kills its envoys. Bea (defence 8) lives
near Ash, pays what she is asked when it is no more than a raid would take, and has done nothing.
Both are in reach of Ashpit and the Grey Outpost. A camp that cannot tell who killed its members
sometimes blames the other player near it (blame spill, 0.3 of the time; CL-2). Seed 1003: 7 fights
on Bea in 50 years (median 7).

1. **Y1 d10 to d22.** Ashpit's envoy asks Bea 46 and the Grey Outpost's 94. She pays both. Ash hunts
   Ashpit on d11.
2. **Y1 d39.** After two more hunts, Ashpit's elder Snag invades Ash with 26, "honour: player Ash
   killed 6 of ours at our camp in the summer of year 1": it wins, loses 11 and takes 109.
3. **Y1 d46.** The Grey Outpost invades Ash with 23 under its chief Talv, for loot "and anger at player
   Ash's mines and smoke": all 23 die, and Talv with them.
4. **Y1 d82.** Ashpit's envoy asks Bea 107, "after: player Bea killed 3 of ours at our camp in the
   autumn of year 1": Ash's hunt, blamed on her. She pays.
5. **Y2 to Y8.** Both camps' envoys ask Bea 92 to 164 and she pays every one, about three a year
   between them. Ash hunts Ashpit every few weeks, kills its chiefs Skarl (Y5 d29) and Lugg (Y6 d61),
   and kills three of its envoys. On Y8 d59 word reaches the Grey Outpost that Ash is away: it invades
   with 23, wins, loses 6 and takes 130.
6. **Y8 d90.** Ashpit invades Bea with 26 under its chief Vorr, who had taken her tribute: "winter
   hunger; and loot; Vorr broke his own truce". It takes 185.

Every fight on Bea in the 50 years was an invasion of 26 by Ashpit, a camp she was paying, and each
took 185 or 190:
- Y8 d90 (above).
- Y21 d66: "loot; and the new war leader must prove himself; Gitz had not sworn Nub's truce".
- Y23 d8: "empty stores; Gitz broke his own truce".
- Y29 d19: "word came that player Bea was away; loot; and the new war leader must prove himself; Vorr
  had not sworn Gitz's truce".
- Y34 d8: "loot; and honour: player Bea killed Skarl, our chief, in the spring of year 34; Grukk had
  not sworn Skarl's truce": Ash's kill, blamed on her.
- Y43 d57: "honour: player Bea killed Lugg, our chief, in the autumn of year 43; and the new war leader
  must prove himself".
- Y47 d19: "empty stores; and loot; Vorr had not sworn Skarl's truce".

The attackers win every one. Bea pays about 29 envoys a decade. Over the 64 worlds she pays 272 a year
and loses 25 to raids, 62% of her income; Ash takes 0.32 fights a year, kills 4.5 attackers a year,
and loses 24 items a year (5% of his income) to the attacks that find him away. Without blame spill
Bea's fights fall from 0.14 to 0.09 a year and her tribute from 272 to 266: the spill shows mostly in
the reasons her envoys give, and in the raids of chiefs who believe she killed theirs.

### 3.5 A world left alone for 50 years

No player. Seed 1001: 55 fights between camps in 50 years (median 55).

| Decade | Fights (camp, village, caravan) | Lost | Truces | Chiefs replaced | Dragons | Bloodiest camp fight |
|---|---|---|---|---|---|---|
| 1 to 10 | 65 (13, 47, 5) | 2 | 2 | 31 | 9 | Y3 d34 Mudhollow invades the Grey Outpost with 26 under its chief Gitz, its stores empty: wins, 22 dead, 32 taken |
| 11 to 20 | 65 (13, 49, 3) | 2 | 4 | 36 | 6 | Y12 d58 Mudhollow invades the Grey Outpost, its war leader Vorr speaking, empty stores: wins, 22 dead, 32 taken |
| 21 to 30 | 83 (12, 57, 14) | 1 | 5 | 31 | 9 | Y22 d7 Mudhollow invades the Grey Outpost under its chief Skarl, empty stores: wins, 22 dead, 32 taken |
| 31 to 40 | 56 (8, 43, 5) | 1 | 4 | 32 | 11 | Y35 d11 Ashpit invades the Grey Outpost under its chief Vorr, empty stores: wins, 22 dead, 32 taken |
| 41 to 50 | 53 (9, 38, 6) | 2 | 4 | 24 | 14 | Y44 d11 Mudhollow invades the Grey Outpost, its war leader Vorr speaking, empty stores: wins, 22 dead, 32 taken |

The bloodiest fight of every decade has the same shape: a starving goblin camp throws its whole
invasion (30.4) at the raider outpost's full guard (25.3) and wins a victory that costs it 22 of 26
for 32 items. The fight is deterministic, so the same two full camps always give the same result; what
varies is when hunger drives a council to it, and who speaks. Beside a sworn ally the same fight pays
better (3.6).

Fights between camps over the 50 years, attacker on target: the Grey Outpost on Mudhollow 11;
Mudhollow on the Grey Outpost 11; the Grey Outpost on Ashpit 9; the Grey Outpost on Crowsnest 6;
Mudhollow on Ashpit 5; Crowsnest on Ashpit 5; five more pairs, 1 to 3 each. The next seed on the same
map tells another history: Ashpit on the Grey Outpost 11 and on Crowsnest 9; the Grey Outpost on
Ashpit 9 and on Crowsnest 8; Mudhollow on the Grey Outpost 8.

The first years read like a chronicle: Y1 d31 a dragon strikes Ashpit; Y1 d51 Ashpit's chief Grukk is
killed in a quarrel; Y1 d63 the Grey Outpost's war leader Captain Mol ambushes a caravan; Y1 d81
Ashpit's hunters kill one of the Grey Outpost's at the border; Y1 d95 Mudhollow's war leader Brakka
takes 26 to Lowmarsh "to prove himself" and comes back with 120; Y2 d78 the Grey Outpost's band falls
on Mudhollow and dies; Y2 d80 Ashpit's chief Snag swears with Mudhollow against the Grey Outpost, a
pact that lapses twelve days later; Y3 d34 Mudhollow, its stores empty, invades the Grey Outpost; Y3
d88 Mudhollow, in winter hunger, invades Ashpit and takes 117.

Chiefs turn over fast: about eight a camp a decade, from today's `chief_lifespan_days` (goblin 120
days, bandit 90, raider 150) and from chiefs who fall leading attacks. Each change brings a new war
leader with a season or a year to prove himself and a council with another temper, which is much of
why no two decades of one world read alike. The figure is today's data, not this design's; a longer
tenure would give fewer, longer reigns.

### 3.6 Two camps swear against a third (2.16)

The valley of 3.5, no player. Seed 1000: 9 pacts sworn and 3 joint fights in 50 years (median 2 joint
fights over 16 seeds). Every pact is the same pair: Ashpit and Mudhollow, two goblin camps, kin,
against the Grey Outpost, which hates goblins and is the strongest camp in reach of both.

1. **Y3 d3.** Mudhollow's chief Lugg sends to Ashpit to swear against the Grey Outpost. Sworn; it
   lapses eleven days later. Pacts between the two come and go: sworn again in Y4 and Y5.
2. **Y7 d5.** Mudhollow's chief Ozzit, its stores empty, calls Ashpit, and it comes. Mudhollow invades
   the Grey Outpost with 26 beside Ashpit's band of 8: "empty stores; and loot; beside Ashpit, sworn
   against the Grey Outpost". They win, losing 17 and 5, and take 76. Alone, the same invasion loses 22
   of 26 for 32 items (3.5): the dead are about the same, but twelve carry home instead of four.
3. **Y10 d35 to Y12 d81.** Ashpit's chief Skarl swears again. In Y11 three calls, one from Ashpit and
   two from Mudhollow under two speakers, are each refused: each time the ally's council, weighing its
   own dead and its share of the take against the shame of refusing, said no. The pact lapses, and so
   does the next, sworn in Y14, unused.
4. **Y12 d5.** Crowsnest (bandits) asks the Grey Outpost to swear against Ashpit. Refused: raiders
   hate bandits.
5. **Y22 d8 to Y23 d75.** Mudhollow swears with Ashpit, its war leader Gitz speaking for its chief
   Ozzit; 17 days later Gitz is chief. 163 days after the swearing Mudhollow, in winter hunger, invades
   Ashpit: "winter hunger; Gitz had not sworn the oath sworn with Ashpit against the Grey Outpost". The
   oath binds the chief who held the seat when it was sworn, not the speaker, so Gitz weighs it as an
   heir does, once; the engine's line says so ("Gitz was not chief when Mudhollow swore"), where the
   model's says "had not sworn". Mudhollow wins, losing 18, and takes 60. The pact ends in betrayal:
   Ashpit takes it as the gravest insult, and every camp that knows Mudhollow trusts its word less.
6. **Y26 d31 to Y29 d55.** Ashpit's elder Snag swears with Mudhollow. Mudhollow's first call is
   refused; twice in Y27 it calls again and Ashpit comes: two joint invasions of the Grey Outpost, each
   won (17 and 5 dead for 83; 15 and 5 for 81). In Y29 Ashpit refuses twice, and the second refusal
   ends the pact.
7. **Y38 to Y42.** Two more pacts, each lapsing unused. On Y40 d48 Ashpit turns down Mudhollow's
   offer; two days later Ashpit's own chief offers the same pact, and Mudhollow swears: the camp that
   offers weighs the pact with its speaker's temper, the camp that answers with its council's mean, and
   the two can differ.

At the fortress (3.1), joint parties are rarer still: 3 in 16 worlds of 50 years.
- Seed 1002, Y4 d12: the Grey Outpost invades Ben with 23 under its chief Captain Mol beside
  Mudhollow's invasion of 26, "empty stores; beside Mudhollow, sworn against player Ben", and finds him
  away (40): won, 5 and 5 dead, 297 taken.
- Seed 1002, Y4 d65: the Grey Outpost again, with Mudhollow's band of 8: won, 14 and 5 dead, 94 taken.
- Seed 1014, Y11 d9: Mudhollow invades with 26 beside the Grey Outpost's 23, "newly sworn against
  player Ben; empty stores", and finds him home: all 49 die.

What Ben sees in seed 1002: a trader, some days after it, with the news that the Grey Outpost and
Mudhollow have sworn against him.
While he is away, two columns arrive together. He comes home to a base robbed of 297, and 53 days
later of 94 more. Over the 64 fortress worlds a joint party came at him about once in 140 years and
won about once in 250 (CL-6).

---

## 4. Numbers

64 seeds per scenario and mode, 50 game years each (`run.py`, `results/summary.md`). The definitions
are in `run.py`'s header: a fight is a war party that fights (raid, invasion, band, ambush, plunder,
skirmish); lost means the attackers lost; an endless war is one pair of camps fighting with no 96-day
gap for five years; a surge is a last decade with twice the fights of the second; extinct is a camp
held at one member or none for 96 days running. All three rules fight by the engine's deterministic
law; they differ only in how they decide.

The Ladder (c) columns overstate today's wars. The model's ladder sends a war rung's party again at
every dwell while the feud stays on it; the engine sends each climb's party once and repeats only
Siege (K2-8, CL-D59 (b)). A feud parked between Raid and Invasion raids once in the engine and again
at every dwell in the model (goblins 4 days), so these columns count more war-rung fights than
today's engine would send.

### 4.1 Stability and losses

The world left alone (no player):

| | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fights a world-year: mean [p5, p95] | 30.1 [23, 38] | 12.6 [9, 18] | 7.4 [3, 14] |
| Between camps a year | 10.2 | 2.1 | 1.2 |
| Share the attackers lost | 0.34 | 0.12 | 0.02 |
| Lost fights a year | 10.1 | 1.6 | 0.16 |
| Worlds with an endless war | 1.00 | 0 | 0 |
| Worlds with a surge | 0 | 0 | 0 |
| Worlds where a camp died out | 0 | 0 | 0 |
| Worlds with no camp fight in 50 years | 0 | 0 | 0 |
| Worlds with a decade of no camp fight | 0 | 0.02 | 0 |

The longest gap between two camp fights in any life world was 9.9 years. Unattended for 500 years
(8 worlds): fights a year by century stay between 6.4 and 8.7 in every world, fights between camps
between 0.88 and 1.55, with no trend.

Where the losses fall (the share of the attackers' fights they lost, `results/summary.md`):

| Target | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| A rival camp (alone) | 1.00 | 0.75 | 0.12 |
| A village (alone) | 0.00 | 0.00 | 0.00 |
| A train or caravan (railway) | (none) | (none) | 0.11 |
| The walled base, 80 (fortress) | 1.00 | 0.99 | 0.97 |
| The hunter and the bystander (multi) | 0.98 | 0.96 | 0.29 |
| The farmer, 30 | 0.00 | 0.00 | 0.01 |
| A weak base, 6 (duels) | 0.00 | 0.00 | 0.00 |

Losing raids are rare (one fight in fifty when the world is left alone or near the farmer, one in
seventeen to twenty-five near a fortress, a hunter or a busy line) and they come from wrong beliefs: a
new base guessed weak, a guard guessed thin, a player believed away. Under the ladder the plunder clock
keeps sending a band of 8 at a camp it cannot beat, and loses every time. The gate believes the square
law's odds, A² / (A² + d²), against an engine whose fight is certain, so it attacks camps it cannot beat
(75% lost) and, in the valley, the fortress (99% lost). Nearly every life fight on the fortress is lost
because the fortress is stronger than any one party in the valley; what keeps them rare is the floor and
the slow blur of a base's belief. The 3% the attackers won were joint parties of two camps sworn against
him that found him away (2.16, CL-6).

### 4.2 The players

| Scenario, player | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fortress, Ben: fights a year on him (attackers killed a year) | 2.74 (39.4) | 0.79 (16.0) | 0.12 (2.9) |
| Farmer, Ben: fights a year (loot taken a year, share of income) | 0.13 (6, 1%) | 0.18 (9, 2%) | 1.93 (93, 19%) |
| Multi, Ash: fights a year (attackers killed; loot) | 7.47 (79.7; 10) | 3.34 (48.0; 15) | 0.32 (4.5; 24) |
| Multi, Bea: fights a year (tribute and loot, share of income) | 0 (1, 0%) | 0 (0) | 0.14 (272 and 25, 62%) |
| Multi without blame spill, Bea | 0 (1) | 0 (0) | 0.09 (266 and 17, 59%) |
| Railway at 4 legs, the owner: invasions a year (loot) | 0 | 0 | 0.16 (19, 4%) |
| Railway at 4 legs: train ambushes a year | 0 | 0 | 2.9: goblins 2.16 won and 0.38 lost, the Frostmaw 0.37, every one won |
| Railway: caravan ambushes a year | 0 | 0 | 0.72 |

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
| Spread of days between one attacker's fights (CV, median; 1 is Poisson) | 0.83 | 0.83 | 0.85 |
| The same between one attacker's fights on one target | 0.83 | 1.30 | 0.77 |
| Same camp, different seeds: difference of act mix (Jensen-Shannon) | 0.000 | 0.004 | 0.012 |
| Same camp, different seeds: spread of fights a year (CV) | 0.04 | 0.10 | 0.16 |
| Two seeds of one world: share of the first ten years' acts that match (chance) | 0.52 (0.49) | 0.38 (0.36) | 0.09 (0.08) |
| A camp's years: variance beyond Poisson, median [p10, p90] | 0 [0, 0] | 0 [0, 1.06] | 0.32 [0, 0.61] |

The intervals alone do not tell the rules apart; what does is whether the next act can be called and
whether two seeds tell the same story. A life camp also has good years and bad years (variance beyond
Poisson): hunger, chiefs and feuds come and go. In a duel the gap is starker: against the stubborn
player the ladder's next act is called to the day 98% of the time, the gate's 14%, the life
council's 2%; two seeds share 99%, 65% and 15% of their acts.

**Can a watcher call the day after a refusal?** Days from a refused envoy to that camp's next fight on
the target, within a year:

| | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fortress: median [p10, p90] | 9 [9, 10] | 3 [3, 3] | 4 [1, 60] |
| Share within a day of the commonest lag | 1.00 | 0.99 | 0.40 |
| Share of refusals followed by a fight within a year | 1.00 | 0.99 | 0.30 |
| Multi: median [p10, p90] (share within a day of the commonest) | 9 [2, 37] (0.55) | 3 [3, 3] (0.98) | 9 [2, 66] (0.25) |

On the railway, the first robbery after the guards come off follows by a median of 28 days [5, 74]
(CV 0.79, 1,371 cases): word of the guards' going reaches the camps late, and the watch on the line
misses them half the time.

What started the life fights (alone): hunger was a driver of 63%, loot of 63%, glory of 13%, pride
of 2%; loot alone, with no other motive, 23%. Occasions: the restless clock 83%, caravans 14%,
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
- The ladder mode follows `engine.rs` in its rules, not in its code paths, except that it repeats every
  war rung where the engine repeats only Siege (CL-D59); the gate follows EQUILIBRIUM.md §2.5 as
  written, including its belief of the odds.
- Alliances (2.16): an offer and a call are answered inside the caller's sitting, and a joint fight
  resolves the day it is decided, so the two marches, the nearer contingent's longer muster and a
  first arrival's wait are not modelled; nor are the shared quota and the target listener. No player
  swears with a camp (CL-8), and no pact holds more than two camps.

### 4.5 Alliances

Every number in sections 3 and 4.1 to 4.3 is measured with alliances on. Here each scenario is set
against its control: the same 64 seeds and 50 years with alliances off (`run.py`'s `*_noally`). Pacts
draw from a stream of their own, so the two arms share every other draw and part only where an offer, a
pact or a call changes what a camp does. Each comparison is paired by seed (`extra/paired.py`,
`extra/base_an.py`).

**What they change** (on, then off; the paired difference a year with its sign test):

| Scenario | Measure | On | Off | Paired on less off (p) |
|---|---|---|---|---|
| Alone | fights a world-year; between camps | 7.36; 1.16 | 7.32; 1.16 | |
| Fortress | fights a year on Ben; share lost; loot a year | 0.125; 0.97; 1.0 | 0.125; 1.00; 0 | -0.0003 (0.18) |
| Farmer | fights a year on Ben; loot a year | 1.93; 93 | 1.92; 92 | +0.010 (0.70) |
| Farmer | Frostmaw deaths a year, North Den and Pass Den; their raids on Ben | 1.73, 0.49; 0.50, 1.41 | 1.73, 0.48; 0.51, 1.39 | |
| Multi | fights a year on Ash; loot a year | 0.32; 24 | 0.33; 25 | -0.012 (0.79) |
| Multi | fights a year on Bea; loot a year | 0.14; 25 | 0.14; 25 | +0.001 (0.59) |
| Railway at 4 legs | fights a year on the owner; loot a year | 0.16; 19 | 0.17; 20 | -0.006 (0.69) |
| Railway at 4 legs | Frostmaw lurks and roars a year; goblin train ambushes | 0.37, 2.18; 2.54 | 0.36, 2.19; 2.54 | |
| Railway at 4 legs | caravan ambushes a year by Ashpit, Crowsnest, the Grey Outpost | 0.16, 0.30, 0.26 | 0.15, 0.29, 0.23 | |

Nothing a player feels moves: no paired difference comes near significance, and the Frostmaw's rates
move by 0.02 a year at most. No camp died out and no war became endless in either arm. The one change
is the fortress's: with alliances off every fight on Ben is lost; with them on, two camps sworn against
him have beaten his walls while he was away, 13 times in 3,200 world-years (0.004 a year, about once
in 250 years), and taken about one item a year from him on average.

**The alliances themselves** (`results/summary.md`):

| Scenario | Pacts sworn a year | Calls a year (joined, refused) | Joint fights a year (on players; won there) | Joint demands a year | Betrayals a year | Who swore, against whom |
|---|---|---|---|---|---|---|
| Alone | 0.146 | 0.077 (47%, 48%) | 0.036 (0) | 0 | 0.026 | Ashpit and Mudhollow against the Grey Outpost, every pact |
| Farmer | 0.150 | 0.057 (52%, 45%) | 0.030 (0) | 0 | 0.022 | the same |
| Fortress | 0.169 | 0.092 (43%, 54%) | 0.039 (0.0072; 0.0041) | 0.023 | 0.027 | the same, 0.160; Mudhollow with the Grey Outpost against Ben, 0.005, and with the Sun Grove, 0.004 |
| Multi | 0.089 | 0.032 (50%, 48%) | 0.016 (0.0034 on Ash; 0.0025) | 0.007 | 0.012 | the goblin pair, 0.087; Ashpit with the Grey Outpost against Ash, 0.001 |
| Railway | 0.121 | 0.036 (32%, 54%) | 0.012 (0) | 0 | 0.021 | the goblin pair |

The rest of the calls found the ally busy. A pact runs a median of about 100 days (mean 134 to 154).
Four in five lapse; about one in six ends in betrayal (14% to 18% by scenario); a handful end in
refusals. Every betrayal in the valley left alone (82 in 64 worlds, `extra/betray.py`; 86 at the
fortress, 70 at the farmer's, 67 on the railway, 39 in multi) is a hungry goblin camp invading its
kin, and 59 of the 82 came under a chief who was not chief when his camp swore (about five of those
59 had spoken the oath as war leader or elder and took the seat later, as Gitz in 3.6). The joint
fights between camps are won: 99% in the valley.

Joint parties on players do better than single ones. At the fortress 13 of 23 won, every win while Ben
was away; of the 10 lost, 7 found him home and 3 found him away but were too weak for his walls. No
single party ever wins there: the strongest is 36 against his 40, even with him away. At Ash 27% of
joint parties were lost, against 41% of single ones.

**How often a player meets one.** A game year of 96 days is 32 hours of a running server:

| As designed | Once in (world-years) | About (server hours) |
|---|---|---|
| A pact sworn against Ben at the fortress | 119 | 3,800 |
| A joint party at the fortress | 139 | 4,450 |
| A joint win at the fortress | 246 | 7,900 |
| A joint party at Ash, the hunter | 291 | 9,300 |
| A joint fight between camps, in the valley left alone | 28 | 880 |

As designed, a player almost never meets an alliance. Bea, the farmer and the railway owner met none in
3,200 world-years each; the server would run about 3,800 hours before two camps swore against Ben, and
about 9,300 before a joint party came at Ash. Between camps they are part of the valley's history, which
a player sees only as far as he hears it (CL-9).

**No blocs.** In 310 of the 320 worlds (`extra/bloc.py`) no camp held more than one pact at a time. In
the other 10, all fortress worlds, Mudhollow held two: with Ashpit against the Grey Outpost, and with
the Sun Grove against Ben. No pact holds more than two camps, and the faction row's room for four open
pacts (5.1) was never more than half used.

**Why they are rare.** Two reasons are the model's structure; the rest are tunables, which CL-7
turns:
- **The law.** Between camps the fight is linear (2.16): a winning party loses d / A of its members, so
  two parties together lose about the dead one party of their joint strength would, each its share. A
  pact buys the carry of more survivors and the wins a camp could not take alone, and the ally pays for
  its part with its own dead. In 3.6 the joint invasion lost 22, as Mudhollow alone does, but brought 76
  home instead of 32; Ashpit's band lost 5 of its 8 for about a quarter of the take. Where the target's
  holdings cap the take, the extra carry buys nothing.
- **Who can swear.** Both must quarrel with the foe, reach it and each other. In this valley only the
  two goblin camps against the Grey Outpost pass often: they are kin (+4), and the raiders, the
  strongest camp in reach of both, hate goblins. Every other pair weighs a culture's hate (-10) or has
  no shared foe worth it. The structure is who quarrels with whom and who reaches whom; the weights,
  kin +4 and hate -10, are tunables.
- **Few offers pass.** A pact's worth to a camp is a joint war's gain and the guard, with who the other
  is (kin, hate), less the effort of sending to swear (`pact_effort` 3); the camp that answers weighs it
  the same way, and it must pass in both councils.
- **Most pacts lapse.** A pact runs Exp(`pact_days` 192) days, and four in five end so: the largest
  ender.
- **Half the calls are refused.** The ally's council weighs its own dead and its share against the shame
  of refusing (`call_honour` 1.0, tripled for the chief who swore), and about half the time the dead
  win.
- **Oaths do not outlive chiefs.** Chiefs turn over about eight times a camp a decade (3.5). The oath
  binds the chief who held the seat at the swearing three times over (the oath multiplier, 3); an heir
  is bound once, and 59 of the 82 betrayals in the valley are heirs'.

Between camps the structure sets the rate; against players the tunables do. The warm arm below turns
four of them and multiplies Ash's joint parties 13 times, joint demands in multi 22 times and offers 5
times.

**Warmer alliances.** Doubling a pact's run (`pact_days` 384), cutting the effort of sending to swear to
1, and making kin worth 6 and a hated culture cost 6 instead of 10 (`extra/warm_an.py`):

| Scenario | Warm | As designed |
|---|---|---|
| Alone | fights 7.10 a year, 1.08 between camps; 0.164 pacts sworn and 0.052 joint fights a year; 0.052 betrayals; pacts run a median 175 days | 7.36, 1.16; 0.146 and 0.036; 0.026; 105 |
| Fortress | Ben 0.127 fights a year, 97% lost, 1.0 loot; joint parties at him 0.011 a year, 0.0044 won; joint demands 0.044; 23 pacts of Mudhollow and the Sun Grove against him and 19 of Mudhollow and the Grey Outpost | 0.125, 97%, 1.0; 0.0072, 0.0041; 0.023; 12 and 15 |
| Multi | Ash 0.372 fights a year, 33 loot; joint parties at him 0.044 a year, 0.035 won; joint demands 0.156; 72 pacts of Ashpit and the Grey Outpost against him; Bea 0.129 | 0.322, 24; 0.0034, 0.0025; 0.007; 4; 0.141 |

Warm, the hunter is the one who feels it: the two camps he provokes swear against him, and against the
design as it stands his fights rise by 16% (paired +0.050 a year, sign p 0.0002) and the loot taken from
him by 41% (+9.6 a year, p 0.0001; `extra/knobs_an.py`). The fortress still loses almost nothing, and
Bea's fights do not rise. Fights between camps fall a little in the valley left alone (1.08 a world-year
against 1.16; sign test p 0.10, not significant). The cause is not measured; probably a sworn camp calls
before it marches, and a camp with a sworn ally is a costlier target. As designed, a player almost never
meets an alliance; warm, they are the hunter's standing problem. CL-7 asks which Ben wants.

**One knob at a time** (`extra/knobs_an.py`; the same 64 seeds, each arm paired with the design; the
paired difference a year and its sign test in brackets):

| Arm | Ash: fights a year; loot a year | Ash: joint parties a year; pacts against him | Bea: fights a year | Ben: fights a year; joint parties, won |
|---|---|---|---|---|
| As designed | 0.322; 23.6 | 0.0034; 4 | 0.141 | 0.125; 0.0072, 0.0041 |
| A pact's run doubled (`pact_days` 384) | 0.322 (0.000, p 0.87); 24.9 (+1.3, p 0.23) | 0.0025; 3 | 0.146 (+0.005, p 1) | 0.114 (-0.011, p 0.08); 0.0056, 0.0019 |
| Sending to swear costs 1 (`pact_effort`) | 0.327 (+0.005, p 0.69); 24.3 (+0.7, p 0.90) | 0.0069; 12 | 0.117 (-0.024, p 0.009) | 0.130; 0.0078, 0.0031 |
| Kin worth 6 (`kin_pact`) | 0.342 (+0.021, p 0.14); 25.0 (+1.4, p 0.40) | 0.0031; 6 | 0.140 | 0.124; 0.0047, 0.0025 |
| A hate costs 6 (`hate_pact`) | 0.334 (+0.013, p 0.29); 27.1 (+3.5, p 0.16) | 0.0144; 30 | 0.139 | 0.122; 0.0084, 0.0053 |
| All four (warm) | 0.372 (+0.050, p 0.0002); 33.2 (+9.6, p 0.0001) | 0.0444; 72 | 0.129 (-0.012, p 0.37) | 0.127; 0.0112, 0.0044 |

No single knob moves the hunter's fights or his loss significantly; the four compound. The hate weight
is the strongest alone: it lets the Grey Outpost swear with Ashpit against him (30 pacts in 3,200
world-years against 4) and makes joint parties at him four times as common, about once in 2,200 server
hours. Cheaper swearing lowered Bea's fights and her loss (p 0.009 and 0.004): with 24 comparisons in
the table one such result is not unlikely by chance, and the cause is not measured (probably camps
busier with each other). No knob moves the fortress.

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
| Pact fields (2.16): the open-pact list, the fields on each feud toward a camp, `hurt` on each toward a player | the paged faction row | 16 a faction, about 40 a camp feud, 4 a player feud |
| A joint attack's partner (2.16) | `Party`, resident while the party lives | 8 bytes and a bit a party |

The faction row (about 450 bytes encoded with two feuds, PM-C1) grows to about 1.6 to 1.9 KB with four
feuds toward nodes and the pact fields (about 180 bytes of it with four camp feuds); it is paged, 4
factions a page (PM-C12), so it costs disk, page loads and saves, not resident memory. Resident memory
grows by the clash event, about 100 bytes a faction: about 0.4 MB at 4,096 factions on top of PM-C13's
3.17 MB.

### 5.2 CPU

| What | How often | Cost |
|---|---|---|
| A sitting | 9.5 to 14.2 a faction-year in the valley scenarios (the most near a busy line), 16 to 18 in a duel; today's plunder clock books 8 to 16 (`plunder_mean_days` 12, 6 and 8). By culture, left alone: goblin 13.9, bandit 11.6, raider 10.0, Umvuthana 9.4, Frostmaw 3.9 | 3.9 to 5.0 neighbours weighed, each one normal draw and 4 to 7 scored acts of a few dozen operations: a few microseconds (inferred), plus the faction's page, which today's plunder event loads as well |
| A border clash | about 1 a year per hostile pair | one draw and a feud update |
| An occasion | a provocation, a refusal, news, a passing train or caravan | a booked event; most become one sitting |
| Belief, stores, fear, pride, trust | at each event touching the feud or faction | closed form, a few operations |
| Acts | 4.9 to 6.1 a faction-year | as today's parties |
| Rows touched | 0.11 to 0.19 a faction-day (two for a fight on a camp) | a page load each when not resident |
| An offer of a pact, a call (2.16) | at most 0.07 offers and 0.013 calls a faction-year in the valley scenarios (the fortress's) | inside the caller's sitting: the other camp's rows read (a page load when not resident) and its answer scored as one more act |
| Deterrence (2.16) | at each sitting, for each camp weighed as a target | the target's open-pact list, on its faction row: one row read, a page load when not resident (neighbours often share a page of 4) |

**Step CPU.** Sittings replace plunders about one for one, and a sitting's arithmetic is small beside a
page load (about 5 microseconds plus 0.45 decode a faction, PM-C12). What grows is the row: decoding a
row three to four times the size, against today's 52.3 microseconds a faction-day at 1,024 factions,
gives an estimated 52 to 60 (inferred).

**Save CPU.** A page of four rows of 1.6 to 1.9 KB is about the bytes of PM-C12's 16-a-page runs
(16 rows of 450). Those saved 44 times a day for 125 ms of CPU a day at 1,024 factions and 162 times
for 801 ms at 4,096, against 13 and 37 ms, and 47 and 276 ms, at today's 4 a page. So expect save CPU
about three times today's: about 120 to 140 ms a game day at 1,024 factions, about 900 at 4,096
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
| Java: the reason line on ENVOY_MATERIALIZE in the `ConflictBridge`; the way a player hears news (CL-10: TALK.md, built there) | about 120, the news path not counted |
| Java GameTests | about 300 |
| Rust: alliances (2.16: the pact fields and the open-pact list, offer and answer, the call, the joint resolve at a camp, a village and a base, the joint fight's books, strain, betrayal, deterrence, the guard, joint demands, the `SYS_ALLY` stream) | about 600 |
| Rust: the joint march (the longer muster, the partner-aware wait, the joint arrival, a two-party `start_fight`, the shared quota in bodies) and BAND_MARCH's partner | about 250 |
| Rust tests 21 to 25 | about 300 |
| Java: the target listener and the joint key from BAND_MARCH (2.16) | about 80 |
| Java GameTests 5 and 6 | about 150 |

The model is `model.py` (2,356 lines, about 680 of them alliances), `run.py` (615), `walk.py` (233) and
`summarize_md.py` (165), with eight measuring scripts in `extra/` (326 lines; their outputs are in
`results/alliances.txt`).

---

## 6. Tests

Property tests in `rtsim-conflict`, on the engine's scenario harness (`scenario.rs`), each over 64
seeds unless said. The model's value for each is in brackets.

1. `a_sitting_draws_from_its_own_stream`: give one faction an extra occasion; every other faction's
   draws are unchanged until its own state changes; two runs with one seed give one state hash.
2. `belief_converges`: after five fights or eight scouting trips, the believed defence is within 25%
   of the truth for 95% of draws; after a year with no look, a camp's variance is back above 0.2.
3. `a_strong_base_is_mostly_left_alone`: the fortress duel at most 0.05 fights a year over 50 years
   [0.01]; in the valley, at most 0.3 fights a year on the fortress [0.12], and at least one test of
   the new base in the first two years in at least half the seeds [73%].
4. `a_payer_is_asked_not_raided`: the weak duel, at most 0.5 fights a year, tribute at least 120 a
   year [0.2, 177].
5. `a_weak_refuser_is_raided`: the stubborn duel, between 1 and 6 fights a year [3.4].
6. `no_runaway_no_permanent_peace`: the world left alone, 50 years: no surge; endless wars in at most
   5% of worlds [0] and none longer than 10 years; no gap of 20 years without a camp fight in any
   world [the longest 9.9 years].
7. `no_camp_dies_by_fighting`: no camp held at one member or none for 96 days, 50 years; and 8 worlds
   of 500 years with fights a year by century within a factor of 1.5 of each other [at most 1.3].
8. `stories_differ`: two seeds of one world share less than 20% of their first ten years' acts [9%];
   the next act is called to the day under 5% of the time [2%].
9. `one_culture_many_minds`: the same camp over seeds has a spread of fights a year (CV) of at least
   0.1 [0.16].
10. `every_fight_has_a_reason`: every party's reason code names a motive; a revenge or honour motive
    names a memory that exists or a standing cause (industry, a culture's hatred, the rail meter).
11. `a_sworn_truce_mostly_holds`: fights by a chief on a camp he swore a running truce with number
    under 1 in 10 of the truces sworn [45 for 1,511].
12. `the_frostmaw_robs_more_on_a_busier_line`: lurks a year rise with legs a day over 2, 4, 8, 12 and
    24 [0.17 to 5.01].
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
    weight 3; its heir arrives with half and raids him before any other player [the farmed den 0.50 a
    year]; a camp whose chief a player killed bears him W + 1 and +40.
20. `a_beaten_party_takes_its_leader`: a destroyed raider invasion ends its captain's rule; a destroyed
    Umvuthana invasion ends the Umvuthi's, the grove then waits the later of its cowed time and
    `min(192, 6 x 2^P)` days and its count falls by one when a warrior takes up the mask.
21. `alliances_off_is_the_world_before` (2.16): with alliances off, the same event log and the same
    count of draws in every stream as the commit before them, and an equal hash over the fields that
    existed before them (the new fields make a whole-state hash differ) [the model's `*_noally` runs
    reproduce the earlier results exactly].
22. `a_pact_needs_a_shared_foe`: every pact sworn joins two camps that reach each other and the foe,
    both quarrelling with it or kin with one quarrelling; none by the Frostmaw; none against the
    railway or a caravan; a camp holding four pacts makes no offer and refuses one.
23. `the_joint_fight_is_the_engines`: at a camp or a village, a joint fight's outcome equals
    `village_outcome` on A + AB and d with both camps' observed fights, and each contingent loses the
    same share of its members; at a player's base, each class's outcome equals `threat.rs` on both
    contingents' members of that class, and each contingent loses `abstract_loss_share` of its own
    held-back members of the class; the take splits by carry; each camp's pressure counts its own dead
    only.
24. `the_oath_binds_and_frays`: a camp sworn against x sends no party alone on x that its ally could
    join, unless the ally refused or was busy at the call; a refusal adds 1 strain, and a pact past 2
    ends; a betrayal ends the pact and sets W + 1, +40 and a memory of weight 3 on the betrayed.
25. `alliances_keep_the_world_settled`: on the same seeds with alliances on and off, the world left
    alone keeps its fights a year within 5% [7.4 and 7.3], no endless war and no camp dying out [none
    in either], and the fortress at most 0.3 fights a year [0.12]; on Peaceful no call is made and no
    joint party leaves.
26. `a_pact_is_news_in_the_world` (CL-9): when two camps swear, march together, or one falls on the
    other, the engine records one report of that kind naming both, and no command is written for a
    player's chat; the report reaches a village or a caravan only along a road a traveller walks, and
    a holder no traveller reaches never learns it.

GameTests, only where Java is involved:

1. `anEnvoySpeaksItsReason`: an ENVOY_MATERIALIZE with a reason has the envoy say its line to the
   player it addresses, once, when they meet.
2. `aNoticeIsOnlyACue`: a FACTION_NOTICE plays its sound and sends no chat or action-bar message, and a
   notice for a player who is away waits for him (NOTICE_SHOWN). Faction news in the world reaches a
   player by gossip alone, and no conflict code sends a system message (CL-9).
3. `aGoblinBandAmbushesATrain`: a goblin band waits at an AMBUSH_SPOT and robs a passing synthetic
   train through `attack_train`; with guards on, the band is beaten and destroyed.
4. `aWatchedSkirmishIsABand`: a skirmish between two camps, and a caravan ambush, that a player meets
   are band parties on their BAND_MARCH routes, their bodies placed where the march puts them; a band
   past the spawn quota walks its remaining members in from the ticking edge as the quota frees.
5. `alliesDoNotFightEachOther` (2.16): a pillager band and an Umvuthana band (the pack's Mowzie
   bodies) of one joint attack at a target; an arrow from one hits the other, and neither turns on the
   other (`LivingChangeTargetEvent` cancelled), while both keep the player as their target; with the
   partner cleared, the hit one turns. The goblin bodies join it when P8 picks their mod.
6. `aJointPartyArrivesTogether`: two contingents on routes of different lengths arrive on one tick;
   the bodies in Minecraft never pass 16 for both, a raptor pack counting 5; the members past the
   shared quota walk in from the ticking edge as it frees; a contingent whose partner is late waits and
   is not sent home.

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
| CL-D19 | **Reversed by Ben's CL-5 (b), 2026-10-22.** Was: no alliances. Now: section 2.16, CL-D43 to CL-D55 | the alliance design replaces it |
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
| CL-D34 | BAND_MARCH (0x880B) is the march record; no new record id; the reason is a field on ENVOY_MATERIALIZE (0x8802) (FACTION_NOTICE (0x8801) carries none since CL-D56) | one march record for every party; no id to clash |
| CL-D35 | The spawn quota does not shrink a band: members past it walk in from the ticking edge as it frees | the band the far view drew is the band that fights; `start_fight`'s waves already work this way |
| CL-D36 | A defeat sets a floor under the guess (halving over 4 x pride_half), and a player's base blurs at a quarter of a camp's rate | without them a fortress was tested again as soon as the belief blurred |
| CL-D37 | Survivors walk home 1 to 3 days, a winning chief with them; the camp lacks them until then | MZ-15's march home has bodies; a camp whose band is out is weaker |
| CL-D38 | A camp plans its take against the stock it last saw plus the refill it knows | planning against the true stock was omniscience |
| CL-D39 | The Umvuthana's carry of 24 and 48 is the value of MZ-8's owed animals, 2 a pack at 12 items an animal | MOWZIE.md gives them a carry of 0 for goods; the council still needs what a raid is worth to the grove |
| CL-D40 | The Frostmaw dies in every abstract fight it loses; its killer gets +80 grievance and +2 pride; its heir arrives with half and a provoked occasion | a party that loses is destroyed, and the Frostmaw's party is itself; a den should hate its killer |
| CL-D41 | A chief who leads falls with a destroyed party, and with a chance of a tenth of his party's loss in a win | a beaten invasion is a story about its leader |
| CL-D42 | EQUILIBRIUM.md §2.6 item 11: the census does not recall a band's living members | MZ-15: the march goes on; a recall broke it at every visit to a camp |
| CL-D43 | A pact joins two camps against one foe both quarrel with (or kin, one quarrelling), offered only when a joint war's gain or the guard against a feared foe is above 0, sworn by both chiefs, lapsing after Exp(192) days | an alliance needs a reason both sides see; a lapse keeps pacts from lasting for ever |
| CL-D44 | The oath binds: a camp sworn against x calls its ally rather than march alone on a party the ally could join; refusing a call costs 1.0 of the oath's truce cost, 3 times over for the chief who swore; the oath binds the chief who held the seat at the swearing, not the speaker, so a war leader who spoke it and takes the seat later is an heir to it | under the linear law a camp that can win alone gains little beside an ally; with pacts that bound nobody, camps swore and then marched alone |
| CL-D45 | A joint fight is a new joint resolve on the engine's parts: one `village_outcome` on the sum at a camp or a village (each contingent the same share), one `threat.rs` resolve per class on the summed members at a base; the take splits by carry; each camp books its own dead, pressure and memories | the engine owns the truth (P3) and fights one party at a time today; pressure is per site (EQUILIBRIUM.md §2.4.5) |
| CL-D46 | Strain: +1 for a refusal, +1 for the ally and +0.5 for the caller after a lost joint fight, +1 for a contingent under 0.8 of what was counted on; a pact ends above 2; strain halves over 96 days | a pact should fray from what happens inside it, not on a clock |
| CL-D47 | Betrayal costs the oath and 10 more; the betrayed takes +1 pride, +40 grievance and a memory of weight 3; every camp that knows the betrayer counts less on its help | a broken oath is the gravest insult, and a reputation |
| CL-D48 | Joint demands: one envoy names the joint threat; the price is set on the joint take, the tribute split by carry, and the ally bound by the same truce | the price must match the threat the envoy names |
| CL-D49 | A joint attack shares one spawn quota of 16 bodies (a raptor pack counts as its 5), split by bodies, at least one stack each | D1: two bands at 16 each would double the bodies a weak PC carries, and D1 is about bodies |
| CL-D50 | Allies do not turn on each other: the ConflictBridge cancels a `LivingChangeTargetEvent` toward a body of the same joint attack; no team and no mixin; friendly fire still hurts, booked to the victim's own camp with no grievance | UR-1 (a team is saved with the world); the event covers every retargeting through `setTarget` or `StartAttacking`, which the Mowzie and pillager bodies use; the goblin bodies' mod is a P8 gate |
| CL-D51 | Each contingent marches its own route; the nearer musters longer so both arrive on one tick; one that arrives first waits; a survivor whose partner ended goes on only if its sitting found going alone worth it | MZ-15's march is per party; a joint attack is one fight |
| CL-D52 | Ally weight by culture: Frostmaw 0, raider 0.8, bandit 1.2, the others 1.0; no ally counts on the grove's invasion | a council of one swears with no one; the Umvuthi's pride is its own (2.10) |
| CL-D53 | Peaceful: pacts may be sworn; no call is made and no joint party leaves | MOWZIE.md §4.12 |
| CL-D54 | Alliances draw from their own stream (`SYS_ALLY` in the engine, `arng` in the model); with them off the world is the one before them, draw for draw | a clean same-seed control for every comparison (4.5) |
| CL-D55 | **Revised by CL-D56.** A joint envoy and a joint party name the ally. (Was also: a pact against a player is told by a notice naming both camps, and pacts between camps are told to the players both camps reach. Both are news now, with no chat line.) | CL-1 (a): a faction tells the player why, in person |
| CL-D56 | **No chat notice for conflict**: FACTION_NOTICE is a sound cue only (64 bytes, no reason, no text); a feud, a pact sworn or broken, a raid and a chief's or champion's death are news reports carried as gossip by traders, caravans and villagers (2.17); the envoy still says why in person | Ben, 2026-10-22: "All faction news in-world"; CL-9 widened to all conflict |
| CL-D57 | Only camps swear pacts; a player never swears with a camp in this build (CL-8 (a)). A pact against a player is allowed, rare, and his bases are fair game (CL-6 (a), CL-7 (a)) | Ben, 2026-10-22: "Not in this build"; "Rare, bases fair game (Recommended)" |
| CL-D58 | **A raid on a village or a rival camp takes from the surplus only** (built in today's engine, `raid_node`, 2026-10-23; it revises K2-14 in docs/overnight-choices-conflict.md). Each good's cap is the culture's `loot_share_milli` (goblins and raiders 150, bandits 200) of `max(stock - target, 0)`, the line's own target, coins included, in place of its share of the whole stock; the carry, the apportioning and `World::stock_to_held` stand. Before the fight the band must be able to carry off at least one whole item if nobody fell (its carry against the caps); otherwise it does not fight: nobody falls, no raid is counted, no robbery is recorded, a note says it found nothing worth taking, the raid's tape row says 0 items and 0 fell, and it goes home. The plunder drive still picks the nearest hated village or camp, not the nearest with something spare (a camp does not know a village's stock; CL-D38). District raids are unchanged: what a band takes from a player's base is the industry engine's (I23, `theft_cap_ppm` of the district's stock), a district has no target to have a surplus over, and the player's own machines refill it. Train ambushes are the industry engine's too. The share is a tuning knob, not a question for Ben. Measured by `raids_measured_1000_years` (crates/rtsim-conflict/tests/raid_surplus.rs, release): 1,000 years, seeds 1 to 12, on the census's goblin camp among 9 villages (`camp`) and on `synth::many(36, 3)` (a goblin, a bandit and a raider camp among 36 villages, each with two feuds), each against the same tree with `loot_share_milli` 0 (raids take nothing). Before is 7f55faf7, after is the same tree with the rule. `many`: villages broke (treasury under one coin) at the end 22 before, 0 after (control 0); the lowest treasury over the run 0.000 of its target before, 0.816 after (control 0.816); the mean village's lowest fill (each line's stock over its target, at most 1, averaged over its lines) 0.540 before, 0.566 after (control 0.566); villages ending 0.05 or more of fill under their own seed's control 65 of 432 (worst 0.349) before, 4 (worst 0.087, seed 12) after; plunders sent 28.80 a year either way, raids fought 28.80 before and 8.40 after, 22.9 items a raid before and 3.2 after, 3.38 and 2.78 warriors lost a raid; feuds 3.00 at the end and a peak of 4.00, 0.083 raid rungs a year, no siege and no chief killed, all the same before and after. `camp`: no village broke either way; the lowest fill over the run 0.363 before, 0.447 after (control 0.444 and 0.447); the worst village against its control 0.103 under before, 0.040 after; 6.27 raids a year either way (every plunder found something spare), 22.1 items a raid before and 4.9 after, 4.00 lost; no player, so no feud. The 100-year figures are the same to two places, so the drain is early and then lasting. `sim conflict` (seed 7) passes every check; its plunder at Ashby carries off 2 or 3 items instead of 22 | A line taken below its target needs imports or a mint to refill, and the quiet economy sends neither (docs/choices-ports.md PC73), so whole-stock raids drained the villages for good: in `many` one village in 20 ended broke. Over its target a line is what the village can spare. The ports found the same and fixed it the same way (PC72, PC78); their helper (`port_loot`) is the ships' own, with a hold's room and stack slots, so it does not fit a war band, and the rule is the one line it shares. A band that could take nothing should not pay a fight's dead for it (PC78) |
| CL-D59 | **(b), taken by the coordinator overnight under Ben's away rule; Ben may overrule.** Should today's ladder send a war rung's party again while the feud stays on it? (found 2026-10-23 by `sim conflict`'s check "the director's waves and the war chief" on seeds 1 to 24; under (b) the engine is unchanged, and sections 1, 4 and `model.py`'s ladder mode now say so.) Section 1 said today's ladder repeats a war rung at its dwell, and the model's ladder mode does (`model.py` `ladder_day`). The engine does not: each climb sends its rung's party once and only Siege repeats (K2-8 in docs/overnight-choices-conflict.md; MOWZIE.md §3.3, whose `repeat_days` (M2) is opt-in and absent for goblins, bandits and raiders). So a feud whose grievance stays between Raid's and Invasion's thresholds raids once and then waits for as long as the grievance holds. In `sim conflict` the three calmest of the 24 goblin chiefs (seeds 17, 21 and 23, Aggressive 5, 6 and 15 of 255) bend Invasion's 140 to 227.6, 226.5 and 216.7, above the 213.7 that the script's smog and logging can raise (seed 7's chief, at 247, climbs at 102.1). Their feud raids the refinery once while the player is offline and sends nothing while he is home, so no band meets the director. The other 21 seeds climb to Invasion. The check now accepts that parked feud and says why. Options: (a) goblins, bandits and raiders take M2's `repeat_days` at Raid's dwell (goblins 4 days), as section 1 and the model describe; (b) **(Recommended)** keep the engine as it is, correct section 1's row to "each climb sends its rung's party once; only Siege repeats", and read the Ladder (c) columns of section 4 as a ladder that repeats, so they overstate today's wars; (c) leave both as they are until CL-D4's council replaces the ladder's climb. | (b): the repeat is the fault section 1 names ("a camp throws raid after losing raid at a fortress"), the council (CL-D4) replaces the climb anyway, and MOWZIE.md's M2 keeps today's game unchanged; (a) would add more raids to today's game for a rule that is going away. |
| CL-D60 | **One rout, one rescue report** (built in today's engine, `rescue_news`, 2026-10-23; K2-25 in docs/overnight-choices-conflict.md says the villages witness "a rescue report"). When a player routs a band, the engine records one Rescue report (who: the player; whom: the node the band marched on, else the feud's actor unless that is the rescuer, else no one; place: where the band broke), and every village within `rescue_news_blocks` (2,000) of the camp whose culture the faction hates witnesses it; the others hear it only as news, at hearsay weight. It used to record one report per witness (whom: that village), and knowledge dedups per report, so every village counted one rout once per hating village: with six haters a human village took about +85 a rout (+25 first-hand and 5 x 12 hearsay) and a ribbit village about +72, and by day 96 every village sat at 122 to 127 toward the player. Measured with `sim conflict` on seeds 1 to 24 (the ignore and tribute runs with a rescue; each village's standing toward the player, mean (min..max)): day 24, human villages 87.9 (46..127) before and 8.3 (0..30) after, ribbit villages 67.4 (0..127) and 1.3 (0..14); day 96 (6 to 9 rescues a run), human 126.6 (123..127) and 7.9 (0..108), ribbit 126.2 (122..127) and 0.5 (0..12). The ribbits sit near 0 because the feeling fades, not because the news never comes: 54% of the 334 reports in the 96-day runs (70% of 57 in the 24-day runs) reach all three ribbit villages within their 10-day life, a median 3.2 days after the rout (p90 8.7), each at +12, and a feeling of 12 fades to 0 in about 1.6 days (`decay_base_ticks` 2,400, `decay_strong_factor` 99), a +25 in about 6. Stacked to 72 or more, a feeling loses a point only every 3 to 10 days, so it never faded between routs. The test is `a_rout_is_one_rescue_report_that_every_hater_in_range_witnesses` (`tests/mowzie_records.rs`). | K2-25 names one report, and the six copies were a bug of the loop, not a rule: the witness set is unchanged (whether it should be is CL-D61). The report names whom the band was after, as a caravan's rescue does, so a talker's "they saved us" (TALK) is said only by the village the band marched on. |
| CL-D61 | **OPEN, for Ben, no recommendation forced: who learns a rout's rescue first-hand?** (found 2026-10-23 with CL-D60; nothing is changed until Ben answers.) Options: (a) K2-25 as it is (docs/overnight-choices-conflict.md:38: "every village within 2,000 blocks (PLACEHOLDER) whose culture the faction hates witnesses a rescue report"; MOWZIE.md:942-943, "rescue reports within `rescue_news_blocks` 2,000"; MOWZIE.md:2194, "villages within 2,000 blocks hear of the rescue and think better of him"): every hating village in range witnesses the rout first-hand at the full +25, wherever the band broke and whoever saw it. This is built (CL-D60). (b) PLAN.md §8.7 (PLAN.md:921, a rescue is seen by "the caravan, and its owner once it gets home"; PLAN.md:928, "News spreads at the speed of trade") and section 2.17 of this document (lines 988 to 991: the holders that saw it first-hand learn it at once, "and everyone else only from a packet a traveller carried"): only the witnesses (the village or caravan the band was after, and anyone in view) get it first-hand, and every other village hears it by traveller news at hearsay weight (+12) or never. Not measured under (b). | Open: the docs disagree. K2-25's reason is PLAN.md's walkthrough (the village downriver that hates the goblins thinks better of the player), which (b) gives only when news reaches that village. |

---

## 8. Ben's answers and questions

**Answered** (Ben, 2026-10-22, in one line for the first five: CL-1 to CL-4 as recommended, "Add
alliances" for CL-5; then CL-6 to CL-9 the same day, below):

| Id | Question | Ben's answer |
|---|---|---|
| CL-1 | **Should a faction tell the player why?** | (a), as recommended (Ben, 2026-10-22): an envoy says one line in person when it arrives. **Revised the same day by CL-9:** the second half, a notice that says who is angry and for what, is news in the world instead (2.17); there is no chat notice |
| CL-2 | **Can a camp blame the wrong player?** | (a), as recommended (Ben, 2026-10-22): blame spill 3 times in 10 (CL-D18) |
| CL-3 | **How often does a Frostmaw rob a busy line?** | (a), as recommended (Ben, 2026-10-22): a lurk about 0.35 a year at 4 legs a day, 1.9 at 12, 5.2 at 24 (CL-D16) |
| CL-4 | **How often is a strong base tested?** | (a), as recommended (Ben, 2026-10-22): about one fight in eight years at a fortress, every one lost by the attackers |
| CL-5 | **Alliances between camps?** | **(b)**, not the recommendation (Ben, 2026-10-22): "Add alliances". Camps that share an enemy can ally and send joint war parties; CL-D19 is reversed |
| CL-6 | **May two sworn camps beat a strong base while its owner is away?** | **(a) Yes, rarely**, as recommended (Ben, 2026-10-22, "Rare, bases fair game (Recommended)", for CL-6 and CL-7 together): bases are fair game and a pact against a player is allowed (CL-D57) |
| CL-7 | **How common should alliances be?** | **(a) As designed**, rare, as recommended (the same answer): one pact about every seven years in the valley, a joint fight about every 28 |
| CL-8 | **Can a player swear with a camp?** | **(a) Not in this build**, as recommended (Ben, 2026-10-22, "Not in this build"): only camps swear pacts. Player pacts may come later as their own design |
| CL-9 | **Should players see pacts between camps?** | **Neither option: wider.** Ben, 2026-10-22: "Shouldn't be in chat notifs, but they should be able to learn about this stuff by travelling traders, etc.", and then "All faction news in-world". There are no chat notices for conflict at all; an envoy still says why in person (CL-1); traders, caravans and villagers pass on news of feuds, pacts, raids and deaths as gossip, carried by the existing news-by-messengers system (2.17, CL-D56) |

**The questions as they were asked** (recommended option first):

| Id | Question | Options | Why it is yours |
|---|---|---|---|
| CL-1 | **Should a faction tell the player why?** | (a) **Yes (Recommended):** an envoy says one line when it arrives ("You killed Vorr, our chief, in the spring of year 3. Pay 106 emeralds."), and a notice says who is angry and for what. (b) Only on request, through a command or a page. (c) Never: the player guesses. | It is what the player reads; the reasons are kept either way. |
| CL-2 | **Can a camp blame the wrong player?** | (a) **Yes, 3 times in 10 (Recommended)** when a player's kill had no witness and another player is in reach: a bystander may be asked to pay, or raided, for a neighbour's deeds (Bea, 3.4: 0.14 fights a year on her with it, 0.08 without). (b) Never: only the doer is blamed. | In multiplayer it punishes a player for what another did. |
| CL-3 | **How often does a Frostmaw rob a busy line?** | (a) **This design's rate (Recommended):** a lurk about 0.35 a year at 4 legs a day, 1.9 at 12, 5.2 at 24, roars between. (b) Today's ladder (MOWZIE.md §4.2): none below 70 grievance (none at 4 legs), then a lurk every 10 days, to the day (from day 14 at 12 legs, 9.6 a year). (c) This design with the Frostmaw's grudge weighing twice as much: 0.33 at 4 legs, 4.2 at 12, 12.5 at 24. | It sets how much the railway costs in Frostmaw country. |
| CL-4 | **How often is a strong base tested?** | (a) **Now and then (Recommended):** about one fight in eight years at a fortress, every one lost by the attackers, each with a reason (a new neighbour's first test, a feud after a chief fell at his walls, word he is away). (b) Never unless he provokes. (c) About one a season. | It decides whether walls end the game's danger. |
| CL-5 | **Alliances between camps?** | (a) **Not in this build (Recommended):** camps fight, truce and feud pairwise. (b) Camps that share an enemy can ally and send joint war parties. | Alliances are a new mechanic with Java-visible joint parties. |

**The questions the alliances raise** (answered above; as they were asked, recommended option first):

| Id | Question | Options | Why it is yours |
|---|---|---|---|
| CL-6 | **May two sworn camps beat a strong base while its owner is away?** | (a) **Yes, rarely (Recommended):** a pact against a player is told to him when it is sworn, and its joint party fights as any party does. At the fortress, two camps sworn against Ben found him away and won 13 times in 3,200 world-years, about once in 250 (about 7,900 server hours; 3.6, 4.5). A win takes about 297 items (10 of the 13; 94 or 61 in the rest); 97% of the fights on him are still lost, and he loses about one item a year on average. A pact is the only way his walls ever fall: no single party wins there, even when he is away. (b) No: a player is never the foe of a pact; pacts are sworn only against camps and villages, and every fight on a fortress is lost, as in CL-4. | It changes CL-4's answer, "every one lost by the attackers", and lets walls fail while their owner is away. |
| CL-7 | **How common should alliances be?** | (a) **As designed (Recommended):** rare. In the valley one pact about every seven years and a joint fight about every 28 (about 880 server hours), and a player almost never meets one: a joint party came at the fortress about once in 4,450 server hours, at the hunter about once in 9,300, and never at Bea, the farmer or the railway (4.5). Nothing a player feels changes. The weights are TUNABLE, like every number here, and can be warmed later without a format change. (b) Warm (4.5): pacts run twice as long (`pact_days` 384), sending costs 1 instead of 3, kin count 6 instead of 4, and a hated culture costs 6 instead of 10 to swear across. A hunter who provokes two camps meets them sworn together: 16% more fights on him and 41% more loot taken, joint parties at him 13 times as often (about once in 720 server hours) and joint envoys about twenty times as often; the fortress and Bea barely move. (c) One knob, the hate weight at 6, nothing else: pacts against the hunter 30 in 3,200 world-years instead of 4, joint parties at him four times as often (about once in 2,200 server hours), 4% more fights and 15% more loot taken (neither significant); the fortress and Bea barely move. No other single knob moves the hunter (4.5). | It decides whether an alliance is a rare story or a standing answer to a player who provokes. CL-6 (b) would bar pacts against players and take most of (b)'s and (c)'s effect on players with it, so CL-6 and CL-7 are best asked together. |
| CL-8 | **Can a player swear with a camp?** | (a) **Not in this build (Recommended):** pacts are between camps; a player deals with a camp by tribute, truce and feud, as now. (b) Yes: a player swears with a camp against a shared foe, is called to its fights and may refuse, with the strain and the shame a camp would take. It needs a call a player can answer, a place to show it, and rules for his refusals and his betrayals. | It is a new mechanic players use, with its own interface. |
| CL-9 | **Should players see pacts between camps?** | (a) **A notice to the players both camps reach (Recommended):** when they swear, when they march together and when one betrays the other (2.16, "Envoys and notices"); in the valley about 0.15, 0.04 and 0.03 times a year. Without it a player almost never sees an alliance (4.5). (b) Nothing, as designed so far: a player learns of a pact only when it is sworn against him or its parties reach him. | It decides whether the valley's politics is news a player reads or history he never sees. |

**Answered** (raised by the news-in-world build):

| Id | Question | Answer |
|---|---|---|
| CL-10 | **How does a player hear the news a traveller carries?** | **By talking** (Ben, 2026-10-22), none of the three options as offered: "Shift + Click to speak to a viller, have them output fluff lines, lines based on how they are feeling, and lines based on what they might have seen. Players should be able to do the same withi ribbits, and other clans as well. Talking with a villager should mildly increase a players 'relationship' with them". Closest to (c), with the trade screen left alone (a crouching click opens none) and the line coming only when the player asks. The design is TALK.md; the envoy speaks its demand the same way ("Speaks to you in chat", CL-1). The options offered were (a) a written book for one emerald from a caravan merchant, (b) a lectern's book in a village, (c) a spoken line. |
