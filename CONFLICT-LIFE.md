# Conflict life: how factions decide to fight

**Status:** design for Ben's review, 2026-10-21. Nothing is built and no engine code changes here.
Ben approves this design before code. Branch `plan/conflict-life`, worktree
`mods/mic-rtsim-conflictlife`.

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
the rail meter, the lurk, the camp reconciler) and MZ-1 (a) (the Java `ConflictBridge` in the core).

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
  in battle, and the heir takes over with his own temper and a year to prove himself.
- **It wants things.** Food (each camp has stores that fill in summer and run down in winter), loot,
  revenge (the grievance today's engine already keeps), its honour (wounded pride after a refusal, an
  insult, a dead chief), glory (warriors grow restless; a new war leader must prove himself). It
  fears losses. Raiders also fear a neighbour grown too strong.
- **It knows things, roughly.** Each camp believes something about every neighbour's strength: a
  guess with an uncertainty, sharpened by its own fights, its scouts, its envoys and rumours, and
  blurring again over time. It remembers up to four things about each neighbour ("player Bea killed
  Vorr, our chief, in the spring of year 3"), fading with time. It keeps a record of who pays.
- **It sits when something happens.** It was attacked, its envoy came back refused, word came that a
  neighbour is weak or a player is away, a caravan or a train passes. Otherwise it sits when its
  warriors grow restless, sooner when it is hungry, angry or proud.
- **It chooses like people do.** For each neighbour it weighs what it could do (wait, warn, send
  scouts, ask tribute, raid with a band, a raid or an invasion, fall on a rival camp's hunters, ambush
  a train or a caravan, offer a truce) against one guess drawn from its belief. Then it picks among
  staying home and its best idea for each neighbour by a softened best choice: usually the best,
  sometimes the second best, rarely something rash. A bold speaker guesses its enemies weaker.
- **Terms first, when terms make sense.** An envoy asks for a price between what the war is worth to
  the camp and what the war would cost the target. A target who pays buys a truce sworn by the chief
  who took the tribute; breaking it is costly to him and less to his heir. A camp that has been paid
  for years does not lightly kill the goose.
- **Every fight has a reason, and the reason is kept.** The engine keeps the motive that drove each
  party and the memory behind it, so an envoy can say what it is owed and why, and a notice can say
  who is angry and for what.

What the player sees, in the model's numbers (section 4):

| Player | Today's ladder | The gate (rejected) | This design |
|---|---|---|---|
| A walled base (defence 80) near three camps, never pays | 3.0 fights a year on him, 39 attackers killed a year | 0.07 a year | 0.34 a year, most of them lost, 5 attackers killed a year: the first test of a new neighbour, a camp starving in winter, a feud after he killed a chief, word that he was away |
| A weak base that pays what it is asked | no war; 406 tribute a year | no war; 362 | 0.1 fights a year; 255 tribute |
| A weak base that never pays | 21 wars a year | 12.5 | 3.1 |
| A player who hunts a goblin camp every few weeks and kills its envoys (defence 50) | 8.9 fights a year on him | 4.7 | 0.4: the camp he hunts is too thin to strike back and sends envoys he kills; the raider outpost nearby answers its killed envoys with invasions that break on his walls |
| A railway at 4 legs a day past a goblin camp and a Frostmaw den | no robbery: at 4 legs the Frostmaw's grievance (17) is below its Raid rung (70) | 22 Frostmaw robberies a year | 3.5 robberies a year: 3.25 goblin ambushes (one in five lost to the guards the owner puts on) and 0.26 Frostmaw lurks; at 12 legs, 3.3 and 1.8 (section 3.3) |
| Nobody: the world left alone | 30 fights a year, 10 between camps, every world a forever war | 11.5, never between camps | 10 a year, 2.3 between camps, wars that start and end |

---

## 1. Why the three rules felt formulaic

| Rule | What it does | Why it reads as a machine |
|---|---|---|
| Ladder (today, EQ-5 (c)) | grievance crosses a threshold, the feud climbs a rung after a fixed dwell; a war rung repeats at its dwell; a Poisson clock sends a plunder at the nearest hated node | the same state gives the same act on the same day; every goblin camp acts alike; a camp throws raid after losing raid at a fortress (in the model 9 wars a year at a defence of 80, 99% lost); the plunder clock fights the same target forever, so every world has a forever war; nothing in it remembers why |
| Gate (EQ-5 (a)) | sends a party only when it is worth sending against the faction's belief | perfectly rational: the strong are never touched, camps at strength never fight each other (no camp fight in any world in 50 years), and the timing of what does happen is set by the probe clock |
| Gate plus glory (EQ-5 (b)) | the gate, plus one raid a season | a raid a season is a calendar |

The fix is not more randomness on a rule. It is a decision maker with state of its own: needs that
rise and fall, beliefs that can be wrong, a memory, and more than one voice. Behaviour then varies
because the state varies, and each act can name the state that caused it.

---

## 2. The model

The model is `research/conflict-life-2026-10-21/model.py` (class `World`, mode `life`). Numbers in
brackets are the goblin culture's; each culture's are in `Culture` there and become a `mind` block in
its JSON (section 2.12). Every number is TUNABLE.

### 2.1 What stays from today

- **Grievance** in closed form, dG/dt = r - λG, with today's signals, weights, tolerances and
  half-lives (`grievance.rs`, `data/cultures/*.json`), and the rail meter (MOWZIE.md §4.3). It is no
  longer a ladder: it feeds revenge and wakes the council (2.4, 2.5).
- **Parties** as today: envoy, scouts, raid, invasion, plunder (here called a band), and the ambush
  (`SYS_AMBUSH`, `attack_train`), with today's sizes, strengths and carries, today's marches, the raid
  director's waves when watched and the abstract fight when not (PLAN.md §10.3, C16 to C18).
- **Battles** by Lanchester's square law (`threat.rs`): an attack of strength A on a defence d wins
  with probability A² / (A² + d²).
- **Pressure and renewal** (EQUILIBRIUM.md §2.4.5, EQ-6 (a)): warriors killed by a player slow the
  camp's recruits by 2^P; a farmed camp breaks and refounds. Factions never break each other.
- **Chief and heir** as notables (`Faction::chief`, `heir`, `rtsim-knowledge::Notable`), the heir
  inheriting grudges (C24), the Mowzie chiefs with `chief_lifespan_days` 1,000,000 (MOWZIE.md §4.2).

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
longer than the mean. A chief who leads an invasion can fall (3 in 10 when it is lost, 3 in 100 when
it is won); a player who hunts a camp can kill its chief. The war leader becomes chief, a new war
leader is drawn, and for a year the new leaders' glory counts double. The new chief keeps a share of
each grudge (0.5 + 0.5 x his aggression). A chief killed by someone adds a wound to the camp's pride
toward the killer (W + 1), grievance (+40) and a memory of weight 3. The Frostmaw is a council of
one, and its heir comes after its wait (EQ-4 (b)); the Umvuthi's after its own (MZ-14 (a)).

### 2.3 Drives

All of these are numbers the engine already has or can keep in closed form between events.

- **Hunger.** A camp has stores. They fill with the season (spring 1.3, summer 1.5, autumn 1.2,
  winter 0.3 times the camp's full-strength needs, times a year's luck drawn once a year, 0.5 to 1.5)
  and run down at the camp's eating rate; they hold at most 45 days. Hunger is 0 with 25 days of
  stores or more and rises to 1 at none. A camp out of stores loses 2% of its members a day and
  recruits at a quarter of its rate. A lost
  hunting ground (about once a decade, for 96 to 192 days) halves the income. Loot is worth
  0.2 + greed x hunger per item (greed 1.6) while it fits in the stores, 0.03 beyond.
- **Revenge.** rev_max x (0.5 + aggression) x g / (g + 100), from the grievance g (rev_max 10).
- **Pride.** Wounded pride W toward each neighbour, 0 to 2, halving over pride_half days (goblin 12,
  bandit 24, Umvuthana 48, raider and Frostmaw 96). It rises on a refused envoy (+0.5), a killed
  envoy (+0.8), a raid on the camp (+0.6 x (1 - W/2)), a skirmish or a border clash (+0.3 x (1 - W/2)),
  a player killing members (+0.2 to +0.5), a stolen crystal (+1.5), a dead chief (+1). Its pull is
  pride_w x (0.5 + aggression) x min(W, 1.5) (pride_w 12), weighted by the hope of answering it,
  0.25 + 0.75 x the believed chance to win: a hopeless attack answers little.
- **Glory.** glory_w x restlessness x (2 for a year after a succession) x (0.5 + boldness)
  (glory_w 5). Restlessness grows from 0 to 1 over restless_days (96) since the last fight, scaled by
  how full the camp is.
- **Fear.** Rises with the camp's own losses (twice the share of its cap lost, +0.2 for a lost
  fight) and a dragon strike (+0.5); halves over fear_half days (40). It scales every expected loss
  by 1 + fear_w x fear x (1.5 - boldness) (fear_w 2.5).
- **Preemption** (raiders only, preempt_w 10): a neighbour camp with more than 1.2 times the
  outpost's members is a threat to strike first.
- **Costs.** warrior_value (3) per expected member lost, times the fear factor; 0.3 per member
  mustered; the culture's effort (2) per party.
- **Penalties.** A truce sworn by the sitting chief costs oath x (1.5 - aggression) x 3 to break
  (oath 20), and a third of that for any later chief. Raiding a player who has paid more than once
  costs 12 x the share of demands he paid: it ends the tribute. Falling on a camp of one's own culture
  costs 25. Falling on a camp with no old hatred, no grievance and no wound costs up to 8: it makes an
  enemy.

### 2.4 Beliefs and memory

**Strength.** For each neighbour the camp holds a log-normal belief of its defence (a mean and a
variance of its logarithm). It starts near the truth for camps and villages it has always known
(an error of about 30%) and wide for a new player base (a guess of 15 with a variance of 1, so
anything from 5 to 40 is plausible). It sharpens by a Kalman step on each observation:

| Observation | Error (sd of the log) |
|---|---|
| A fight with the target | 0.15; against a player who was away, it guesses him twice what it met, at 0.3 |
| Scouts | 0.25 |
| An envoy that walked to the base and came back | 0.3 |
| A rumour: a neighbour's losses, a dragon strike (arrives after about 6 days) | 0.5 |

It blurs again by 1/384 of variance a day, so a camp that has not looked at a base for a year or two
is unsure of it again.

**Away.** Word that a player has left his base (half of his trips are seen, arriving after about 4
days) does not change what the camp believes of his walls; it marks him away for 3 to 10 days, during
which the camp guesses his defence at half (his walls are home, he is not). PLAN.md §10.3: raids do
not wait for the owner.

**Trust.** A Beta record of how often a player pays: +1 to one side per demand paid or refused, both
fading toward even over 192 days.

**Memories.** Up to four per neighbour, each with a weight that fades (half-life 4 x pride_half),
the strongest kept: "we lost 13 at player Ben", "player Bea killed Vorr, our chief", "player Ben
refused our envoy", "the Grey Outpost raided us", "Crowsnest's hunters killed 2 of ours at the
border", "player Ben stole the crystal", "the railway drove us off", and the kind ones, "player Bea
paid 147". A grievance says how angry; a memory says why.

### 2.5 When the council sits

| Occasion | When | Attended |
|---|---|---|
| Provoked | members killed by a player or a camp, a border clash, a stolen crystal; after 1 + Exp(3) days, when the party has gathered or the Frostmaw has woken | always |
| Refused | an envoy came back refused or did not come back | always |
| News | word of a neighbour's weakness, or a player away | 7 in 10 |
| Caravan | a caravan passes the camps on the road (on a third of days) | 3 in 100 |
| Train | each leg a day through a camp's or den's range | 5 in 100 |
| Restless | its own clock: Exp with mean wake_days / (1 + 2 hunger + restlessness + 0.8 W_max + 0.5 g_max / (g_max + 50)) (wake_days 12) | always |

An occasion puts one neighbour before the council; a restless sitting weighs all of them. A day with
an attended occasion has no restless sitting as well. (In the model a fight resolves the day it is
chosen; in the engine a feud whose party is still out is skipped, as today.)

### 2.6 How it chooses

For each neighbour before it, the council draws one guess of its defence from the belief, shifted by
boldness (a bold speaker guesses lower: -1 log unit per unit of boldness above 0.5), halved while a
player is away. This is Thompson sampling: an uncertain belief sometimes produces a guess low enough
to try, which is how a camp tests a new base and how it is sometimes badly wrong.

Against that guess it scores each act it could take toward that neighbour:

    u(war act) = P(win) x value(loot) - cost + revenge + pride x hope + glory + preemption - penalties

and keeps the best act for that neighbour. The sitting then chooses among staying home (u = 0) and
each neighbour's best act by a logit (quantal) choice with the culture's temperature τ (goblin 4,
bandit 2, raider 2.5, Umvuthana 2.5, Frostmaw 3), scaled by 0.75 + 0.5 x aggression: an aggressive
council is more erratic. With τ near 0 this is the gate; with τ large it is a dice roll.

The model logs, for each act, every motive's share of the push: a fight's story names each motive
that was at least a quarter of it.

### 2.7 The acts

| Act | Toward | In the game | Carried by |
|---|---|---|---|
| Wait | | nothing | |
| Warn | a player, the railway (at most once in 16 days each), a camp the council is angry with | a goblin howl, a Frostmaw roar, the Umvuthi's gong, a raider war band shown at the edge, a bandit's written warning | today's Notice and NOTICE_SHOWN (MOWZIE.md §4.12) |
| Scout | anyone but a caravan | scouts at the base, or a raptor look; toward the railway, a watch on the line for guards (about 21 a year at 12 legs) | today's Scouts party; toward the railway kept abstract, no party |
| Demand | a player or a village | an envoy with guards asking the culture's tribute good (goblin emerald, Umvuthana gold) | today's Envoy party and `Demand`; MOWZIE.md §4.5 |
| Band | a village, a camp, a player | a small party (today's plunder party) | today's Plunder party kind |
| Raid | a village, a camp, a player | today's Raid party | today's Raid |
| Invasion | a village, a camp, a player | today's Invasion party; a raider captain leads it; the Umvuthi leads its tribe's in person, only when its pride is wounded (W of 0.8 or more), adding its own strength (MZ-3 (b)); how it travels with the band and hands over is MOWZIE.md's rework under MZ-15 (DH-5), and this design decides only when | today's Invasion |
| Skirmish | a rival camp | a band falls on the rival's hunters in the field: it meets a share of the camp's strength (at least 15%, all of a camp of 4 or fewer) and takes a quarter of a band's carry | a band party whose target is the rival's territory edge |
| Ambush | a train, a caravan | the Frostmaw's lurk by the track (MOWZIE.md §4.4); a goblin, bandit or raider band lying in wait by a track or a caravan road | a train: MOWZIE.md §4.4's lurk, given to every culture with an ambush weight, and `attack_train`; a caravan: new (2.14) |
| Truce | a rival camp | nothing seen; the two camps stop fighting for Exp(192) days, sworn by both chiefs | new feud fields (2.11) |
| Hunt | a village (Umvuthana only, when hungry) | raptor packs take a district's animals | MZ-8's owed hunts |

Siege stays in the culture data but is not chosen in this design (section 7, CL-D14).

### 2.8 Terms: the envoy

An envoy names a price only where its war threat is credible (a believed chance to win above 1 in 4)
and no tribute is running. The camp's lowest price, T_min, is what the war is materially worth to it
(the war act's score without revenge, pride, glory and preemption, in items at today's value of an
item). The target's highest is what he stands to lose, X(1 + 0.5): the loot the camp believes it
would take, plus the damage a raid does beyond the loot. If T_min is below that, the envoy asks
somewhere between, nearer the top the more aggressive or hungry the speaker:
T = T_min + (0.25 + 0.4 aggression + 0.15 hunger) x (top - T_min). This is the Nash bargaining
range: both sides are better off with the payment than with the war. The demand's own score is
trust x value(T) + (1 - trust) x 0.9 x the war's score, so a camp asks a payer and fights a refuser.

The envoy party walks to the base and back: the camp learns the base's defence (2.4) whether or not
he pays. A payment soothes the grievance (by the culture's relief, 0.7), the pride (x 0.3), adds to
trust, and buys a truce of 24 to 72 days (mean 48) sworn by the chief who took it. A refusal adds to
pride (+0.5) and grievance (+10) and a memory; killing the envoy adds more (+0.8, +30) and costs the
camp a member.

### 2.9 What keeps the world stable

Every escalation has a brake that comes from the state, not a cap:

- **Losses.** A lost fight costs members, adds fear, hands the floor to the elder, and makes the
  camp's belief of the target sharper (it now knows it is strong).
- **Wins.** A won fight halves the grievance and takes most of the wound away: vengeance sated.
- **Pressure.** Members killed by a player slow recruits (EQUILIBRIUM.md §2.4.5).
- **Hunger and seasons.** A fed camp values loot little: most fights come in winter and after bad
  years, and stop when the stores are full.
- **Truces.** A camp afraid or sociable enough offers a truce, and the other side accepts if its
  chief is sociable or afraid enough to outweigh its wounded pride, or one time in five; both chiefs
  swear it, and the oath then holds most of them.
- **Peace cost and kin cost.** Making a new enemy, or fighting one's own culture, is dear.
- **Clashes keep it from going quiet.** Hunters of two hostile neighbouring camps meet about once in
  two years when both are fed, up to two or three times a year when both starve (rate 1 a year x
  (0.5 + hunger + hunger)); one side loses one or two members. Each clash is a provocation with a
  memory, and some become feuds.
- **Dragons** (world physics in the model, not a decision): a strike about once in four years a camp
  kills 30 to 60% of it, burns half its stores and frightens it; word of its weakness reaches its
  neighbours about 6 days later.

### 2.10 The Mowzie cultures

- **The Frostmaw** is a council of one with no recruits. Its life is worth 60 items to it, at the
  risk of half of a lost fight; its effort is 14. What it can do: roar (warn), lie in wait by a track
  (ambush), raid a player who has wronged it. The rail meter's grievance (8.66 x (legs - 2) at
  equilibrium, MOWZIE.md §4.3) is its revenge; each train through its range is an occasion; a stolen
  crystal is an insult (+1.5 pride, +60 grievance, a memory of weight 3) and a provocation the
  morning it wakes. It dies in half of the lost fights against a player; its heir comes after
  `min(192, 6 x 2^P)` days (EQ-4 (b)) and inherits half of every grudge.
- **The Umvuthana** ask gold (MOWZIE.md §4.5), hunt villages' animals with raptors when hungry
  (MZ-8), and beat the gong. Their Umvuthi leads an invasion in person, and only when its pride is
  wounded (MZ-3 (b)); it does not die of age. Their grove is small (seven warriors in the model), so
  their envoy's price is small and their wars are rare.

### 2.11 In the engine

The engine changes are confined to `rtsim-conflict`; the parties, the raid director, the reconciler
and the Java side see the same party kinds they see today.

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
| | memories (4) | 32 | 8 bytes each: kind, count, weight, flags (hostile), day |
| | away until, truce until, last warning, paid until, refused at | 20 | days |
| | the swearing chief | 4 | the chief's ordinal (`Faction::chiefs`) |

About 45 bytes a faction and 76 a feud. A memory is a code and its numbers, not text: the story is
written when it is read (2.13).

**Events.** `Ev::Plunder` and its Poisson clock (`schedule_plunder`, `SYS_PLUNDER`) become
`Ev::Council { faction, stamp }`, booked from `StreamKey(seed, SYS_COUNCIL, key).slot(n)` at the
restless clock's mean, re-booked when hunger, pride or grievance move it. An occasion books an
`Ev::Council` with its kind and the neighbour at its delay (the provoked delay, or now). The ladder's
`Ev::Feud` climbs become sittings: a feud's grievance no longer schedules acts. Border clashes are
one booked `Ev::Clash { faction }` per camp with a hostile neighbour, at the sum of its pairs' rates,
from `SYS_CLASH`.

**Randomness.** Every draw in a sitting (the speaker, each neighbour's guess, the logit choice, the
price, a truce's length) comes from `StreamKey(seed, SYS_COUNCIL, faction key).slot(sitting)`, so a
sitting's result depends on its faction's state only, never on the order of other events. The model
uses one numpy generator for the whole world; the engine's keyed streams are stricter, and the
property tests check it (section 6).

**What the Java side needs.** Nothing new to carry the acts: envoy, scouts, raid, invasion, plunder,
the lurk and notice are all carried by MOWZIE.md's `ConflictBridge` (ledger 16, MZ-1 (a)), which is
planned and not yet built; this design adds no record kind to it. Two small things to show the
story, both in the `ConflictBridge`:
- **An envoy speaks its reason.** ENVOY gains a reason code and its numbers (a memory kind, a count,
  a day, the other party's name); the plugin shows one line of chat to the player it addresses when
  it arrives: "You killed Vorr, our chief, in the spring of year 3. Pay 106 emeralds." (CL-1).
- **A notice carries its reason.** The same code on NOTICE, shown with the howl, roar or gong.
A truce, a skirmish between camps, a border clash and a caravan ambush are unwatched in nearly every
case and need nothing from Java: a skirmish or a caravan ambush a player watches is a band party at
the place, as a plunder is today.

**UR-1.** The design adds no entity, block or item, no body and no world change. Its state lives in
the faction rows and the wheel, which rtsim saves and drops as today; removing rtsim leaves nothing,
and reinstalling starts every council fresh, with beliefs at their priors and no memories.

### 2.12 Cultures as data

Each `data/cultures/*.json` gains a `mind` block with the culture's numbers: temperament means and
spread, τ, greed, warrior value, rev_max, pride weight and half-life, refusal and insult pride, fear
weight and half-life, glory weight, restless and wake days, effort, demand bias, scout and warn
weights, oath, kin cost, preemption, invasion pride and chief strength (Umvuthana), ambush weight.
About 25 lines a culture. The engine has no culture name in it (D4): the Frostmaw is a culture with a
pool of one, the Umvuthana one with a small pool, an invasion gate and raptor hunts.

| Culture | bold | aggr | soc | spread | τ | greed | pride_half | wake_days | effort | demand bias | other |
|---|---|---|---|---|---|---|---|---|---|---|---|
| goblin | 0.55 | 0.6 | 0.35 | 0.2 | 4 | 1.6 | 12 | 12 | 2 | 0 | ambush weight 1.4 |
| bandit | 0.45 | 0.45 | 0.6 | 0.15 | 2 | 1.2 | 24 | 14 | 4 | 6 | ambush weight 1.6 |
| raider | 0.6 | 0.6 | 0.35 | 0.15 | 2.5 | 0.9 | 96 | 20 | 4 | 2 | preempt 10, oath 10, pride_w 20 |
| Umvuthana | 0.5 | 0.45 | 0.65 | 0.15 | 2.5 | 1.2 | 48 | 16 | 6 | 10 | invasion pride 0.8, chief strength 5, hunts |
| Frostmaw | 0.5 | 0.5 | 0.1 | 0.25 | 3 | 1.0 | 96 | 30 | 14 | none | council of one, its life 60, pride_w 25 |

World numbers, shared: loot value 0.2 an item fed, 0.03 beyond the stores; boldness shift 1; belief
drift 1/384 a day; rumour error 0.5; speaker weight 0.6; train notice 0.05; caravan attention 0.03;
clash rate 1 a year a pair; raid damage 0.5; field share 0.15; peace cost 8; tributary cost 12;
sworn multiplier 3; tribute truce 48 days.

### 2.13 The story

Each party, envoy and notice carries its top motives and the memory behind them, as codes. Text is
written when a player, a page or a log reads it:

- "Y7 d22 the Grey Outpost: invasion on player Ben by 23 (Captain Hesk, war leader). Why: honour:
  player Ben killed Talv, our chief, in the winter of year 6; and loot."
- "Y29 d59 Mudhollow: invasion on the Grey Outpost by 26. Why: empty stores; Nub had not sworn
  Snag's truce."
- "Y2 d26 Ashpit: ambush on the railway by 12. Why: a train came through; the new war leader must
  prove himself; and revenge: we lost 8 at the railway in the spring of year 1."

### 2.14 Reuse and custom

| Need | Reused | Custom |
|---|---|---|
| Grievance, signals, rail meter | `grievance.rs`, culture signals, MOWZIE.md §4.3 | none |
| Parties, marches, waves, abstract fights | today's party kinds, raid director, Lanchester (`threat.rs`) | a skirmish target (a camp's territory edge) |
| Envoy and tribute | today's Envoy, `Demand`, tribute goods; MOWZIE.md §4.5 | the price (2.8) |
| Train ambushes | MOWZIE.md §4.4 lurk, AMBUSH_SPOT, `attack_train`, the track sampler (MZ-12); today's incidental ambush of a marching war party (`SYS_AMBUSH`) stays | the lurk opened to every culture with an ambush weight |
| Caravan ambushes | rtsim-world's caravans (`caravans.rs`, `caravan_place`) | a query for caravans passing a camp's reach through the conflict engine's world interface, and the robbery's loss of cargo reported to the economy |
| Chiefs, heirs, temperament | `Notable`, `Temperament::from_seed`, `succeed`, C24 grudges | an elder seed, standing, gamma tenure |
| Memories | `MemorySlot`'s 8-byte shape, report kinds (Killing, Robbery, Theft for a stolen crystal) | conflict memory kinds (lost at, drove us off, refused, killed envoy, raided us, border, paid) |
| Notices | NOTICE, NOTICE_SHOWN (MOWZIE.md §4.12) | a reason code |
| Renewal and pressure | EQUILIBRIUM.md §2.4.5, `rtsim-kernel::renewal` (EQ-6 (a)) | none |
| Camp bodies, UR-1 | MOWZIE.md §4.9 reconciler | none |
| Decision | none fits | the council: drives, beliefs, Thompson draw, logit choice, truces, clashes |

---

## 3. Walkthroughs

Each is the model's own run (`walk.py`, `results/walkthroughs.md`): of 16 seeds, the one whose headline
count is nearest the median, so the story is a typical one, not a picked one. Players are scripts. A
player who "pays" pays a price below what a raid would truly cost him: the loot a raid at his real
defence would take, plus half again for damage (`World.demand`). What the player sees
is told from the acts: an envoy is a party walking up to him; a war party he is online for comes in
waves at the edge of his loaded area; one he is away for meets his walls abstractly, and he finds
the damage on his return (PLAN.md §10.3).

### 3.1 A strong base that leaves the Umvuthana alone

Ben's walled base (defence 80) is in reach of the Sun Grove (Umvuthana), Mudhollow (goblins) and the
Grey Outpost (pillagers). His industry makes 3 grievance a day toward each camp that hates humans. He
never attacks them and never pays. Seed 1000: 18 fights on him in 50 years (median 18).

1. **Y1 d23.** A goblin envoy from Mudhollow walks up and asks 144 emeralds. Ben refuses. The envoy
   saw his walls: Mudhollow now believes him strong.
2. **Y1 d35 to 36.** The Grey Outpost's envoy asks 153; refused. The next day, stung by the refusal
   and wanting his goods, the outpost invades with 23. Its elder Dorn had the floor that day, and
   even an elder's draw can come out wrong: 13 pillagers die at Ben's walls. The reason it carries:
   "loot; and honour: player Ben refused our envoy in the summer of year 1".
3. **Y1 d91.** The Umvuthi's envoy asks 13 gold. Refused. From then on the Sun Grove beats the gong at
   him about once a year (53 times in the 50 years) and asks again now and then (11 envoys), but
   never fights in this world: a grove of seven cannot hope to win, and its pride is never wounded
   enough for the Umvuthi to lead an invasion in person. Over all 64 fortress worlds the Umvuthana
   fought the fortress 10 times, in 8 of them.
4. **Y2 d93 to 95.** A bad winter at Mudhollow: its stores are empty. War leader Ozzit leads 26 goblins
   against Ben and loses 17; two days later chief Vorr goes again and wins, losing 13, taking 96.
   Hunger, not hatred: the reason says "winter hunger".
5. **Y3 to Y6.** No fights: envoys (refused), war bands shown at his border (the outpost's warnings),
   the gong, and Mudhollow's scouts (about twice a year).
6. **Y6 d72 to 73.** Word reaches the Grey Outpost that Ben is away, in a hungry winter. Its envoy is
   refused, and the outpost invades the next day: it wins, takes 90, loses 12 to his walls, and its
   chief Talv falls leading the attack.
7. **Y7 d21 to 96.** The outpost now has a dead chief to avenge. It invades three more times that
   year: once won while Ben is away again (chief Captain Vask falls), twice lost (13 and 23 dead,
   chief Captain Hesk falls). Each dead chief is a fresh wound; each lost invasion gives the elder
   more say.
8. **Y8.** Mudhollow's new chief Snag asks tribute again, reminding him of the 17 it lost in year 2.
   The outpost shows war bands. No fights.

Over the 50 years: 7, 3, 2, 1 and 5 fights by decade, 11 of the 18 lost, 280 attackers killed at his
walls, 59 envoys, nothing paid. All 18 were invasions: 10 by the Grey Outpost (5 won), 8 by Mudhollow
(2 won). A strong base is tested by a new neighbour, then mostly when a camp starves, when word comes
he is away, or when a feud is hot (a chief who fell at his walls). The Umvuthana leave him alone.

### 3.2 Farming the Frostmaw den

Ben (defence 30, pays envoys) kills the North Den's Frostmaw whenever it is back, and now and then
steals the crystal from the sleeping Frostmaw at the Pass Den. The Sun Grove is also in reach.
Seed 1004: 121 Frostmaw deaths in 50 years over the two dens (median 122).

1. **Y1 d2.** He kills the North Den's Frostmaw. Its heir arrives on d14, 12 days later
   (`min(192, 6 x 2^P)`, EQ-4 (b)); he kills it on d16, the next comes on d39, and after the kill on
   d41, on d78. The North Den's Frostmaw dies 91 times in the 50 years, 1.82 a game year: MOWZIE.md's
   1.8 for a farmed den.
2. **Y1 d21.** At the Pass Den the sleeping Frostmaw wakes as he reaches for its crystal and drives
   him off. On Y1 d80 he tries again; it wakes, and this time he kills it.
3. **Y2 d14.** He steals the new Pass Den Frostmaw's crystal. It wakes the same day, finds it gone,
   and comes for him: it wins at his base and takes 44. Nine days later he steals again; this time
   it only roars. Over the next years the pattern holds but never to the day: a theft is followed by
   a roar, a raid, or nothing; a raid comes the next day, two or three days later, a week later, or a
   month later (Y2 d68, when word came he was away).
4. **Y1 d88.** The Umvuthi asks 28 gold. Ben's script pays a price below what a raid would truly cost
   him (the loot a raid at his real defence would take, plus half again for damage), here about 11:
   the grove overrates its chance, and he refuses. The gong follows, about once a year. No fight.

Over the 50 years: 54 fights on him (8, 8, 14, 5, 19 by decade), every one a raid by the Frostmaw of
the Pass Den, the den he robs; 9 lost. The farmed North Den never strikes back: each heir is killed
within days of arriving, before it can act on the grudge it inherits. Over all 64 farmer worlds the
North Den attacked Ben not once, and the Pass Den 2,805 times. The Frostmaw hates him for what he
did to it, and only once it lives long enough to answer.

### 3.3 A railway through goblin land

A line runs past Ashpit (goblins) and the Pass Den (a Frostmaw) at 12 legs a day. Its owner (defence
30) never pays; after three robberies in 30 days he puts guards on the trains for a year (his
script). Seed 1003: 231 fights on the line in 50 years (the median of 16 seeds).

1. **Y1 d7.** The Frostmaw roars at the line: the rail meter has made it angry, and a roar is cheap.
2. **Y1 d11 to 16.** Ashpit ambushes three trains in five days, 56 items each, twice under its chief
   Skarl and once under its war leader Nub. The owner puts guards on. Y1 d20: the next ambush meets
   the guards and loses 8.
3. **Y1 d28.** The Frostmaw lurks by the track and robs a train (MOWZIE.md §4.4).
4. **Y1 d85.** Ashpit's envoy asks the owner 133 emeralds for safe passage. Refused.
5. **Y2 d26 to 41.** The guards are gone. Ashpit's new war leader Snag must prove himself, and he has
   a grudge: "we lost 8 at the railway in the spring of year 1". Three ambushes, guards, a lost
   fourth. A run of robberies comes back each year, but not on a calendar: runs begin on Y3 d32,
   Y4 d61, Y5 d75 and Y6 d87, driven in different years by restless warriors and revenge, by loot,
   by winter hunger; the guards end each one, usually with a lost ambush.
6. **The Frostmaw** robs a train about twice a year (Y1 d28, Y2 d52, Y4 d30, Y5 d8, Y6 d8, Y7 d14,
   Y8 d30) and roars at the line every few weeks. Once (Y7 d35) the guards drive it off; its next lurk
   carries "revenge: the railway drove us off".

How traffic changes it (8 seeds x 20 years each):

| Legs a day | Rail meter grievance (MOWZIE.md §4.3) | Frostmaw lurks a year | Frostmaw roars a year | Goblin ambushes a year | Guards put on a year |
|---|---|---|---|---|---|
| 2 | 0 | 0.17 | 0.83 | 2.96 | 0.52 |
| 4 | 17.3 | 0.31 | 2.48 | 3.23 | 0.70 |
| 8 | 52.0 | 0.86 | 4.03 | 3.29 | 0.81 |
| 12 | 86.6 | 1.84 | 4.64 | 3.28 | 0.87 |
| 24 | 190.5 | 4.87 | 5.27 | 2.72 | 0.94 |

The Frostmaw's robbing grows smoothly with traffic; MOWZIE.md's ladder had none below the Raid rung
(none at 4 legs) and a lurk about every week from day 17 at 12 legs (CL-3). The goblins rob for loot
at any traffic; the guards cap them.

### 3.4 Multiplayer: one provokes, another suffers

Ash (defence 50) hunts Ashpit's goblins every few weeks and kills its envoys. Bea (defence 8) lives
near Ash, pays what she is asked and has done nothing. Both are in reach of Ashpit and the Grey
Outpost. A camp that cannot tell who killed its members sometimes blames the other player near it
(blame spill, 0.3 of the time; CL-2). Seed 1003: 5 fights on Bea in 50 years (median 5).

1. **Y1 d11.** Ash hunts Ashpit and kills its chief Skarl. Nub takes over and howls at Ash.
2. **Y1 d32 to 37.** Both camps' envoys find Bea and ask 147 and 154. She pays both. They believe her
   weak, and she is.
3. **Y1 d53.** The Grey Outpost invades Ash on word that he is away, angry at his mines and smoke. It
   loses 12 at his walls.
4. **Y3 d14.** Ash hunts Ashpit again and kills its chief Vorr; no witness lives. Ashpit blames Bea.
   Its new chief Grukk howls at her, and on Y3 d25 its envoy asks her 106, "after: player Bea killed
   Vorr, our chief". She pays.
5. **Y3 d56 to Y5 d2.** Ashpit sends envoys to Ash three times; he kills each. Ashpit is too thin to
   fight him (he keeps hunting it), so it does what it can: it howls, and it takes from Bea.
6. **Y4 d17 to 18, Y6 d6 to 7.** Ash kills the Grey Outpost's envoys too. Each time the outpost
   invades the next day out of honour, and each time loses (16, then 12 and its chief Dorn).
7. **Y1 to Y8.** Bea pays about three times a year between the two camps, 96 to 182 a time. Some of
   Ashpit's envoys ask "after: player Bea killed Gitz, our chief" or "killed 5 of ours": Ash's
   hunts, blamed on her.
8. **Y10 d33.** Ashpit's new chief Mott never swore the truce Skarl, an earlier chief, took her
   tribute for, and he believes she killed Ozzit, his chief (Ash did). He invades: 26 goblins against
   her walls of 8. They win, lose 1, take 185.

Bea is attacked five times in the 50 years, every time by a chief who had not sworn the truce she
paid for: Y10 d33 (above), Y21 d54 and Y23 d7 (the Grey Outpost, empty stores and loot), Y35 d49
and Y45 d32 (Ashpit, empty stores and loot). The attackers win every one. She pays about 370 a year.
Ash takes about 0.4 fights a year and kills about 5 attackers a year. Without blame spill Bea's
fights fall from 0.09 to 0.07 a year and her tribute from 369 to 363: the spill shows mostly in the
reasons her envoys give, not in her losses.

### 3.5 A world left alone for 50 years

No player. Seed 1004: 103 fights between camps in 50 years (median 104).

| Decade | Fights (camp, village, caravan) | Lost | Truces | Chiefs replaced | Dragons | Bloodiest camp fight |
|---|---|---|---|---|---|---|
| 1 to 10 | 85 (18, 52, 15) | 15 | 4 | 31 | 7 | Y6 d4 Ashpit invades Crowsnest with 26, out of empty stores; loses all 26 |
| 11 to 20 | 112 (26, 70, 16) | 24 | 10 | 33 | 12 | Y16 d18 the Grey Outpost invades Ashpit with 23; loses all 23 |
| 21 to 30 | 94 (14, 68, 12) | 14 | 11 | 32 | 11 | Y29 d59 Mudhollow invades the Grey Outpost with 26, starving; its chief Nub had not sworn the truce Snag, an earlier chief, made; loses 26 |
| 31 to 40 | 89 (17, 60, 12) | 18 | 7 | 34 | 12 | Y35 d36 the Grey Outpost invades Mudhollow for loot; loses 23 |
| 41 to 50 | 122 (28, 72, 22) | 22 | 8 | 28 | 15 | Y50 d38 Mudhollow invades Ashpit, starving; loses 26 |

Fights between camps over the 50 years, attacker on target: the Grey Outpost on Crowsnest 36, on
Mudhollow 24, on Ashpit 24; Mudhollow on the Sun Grove 9; Crowsnest on the Grey Outpost 5; Ashpit on
Crowsnest 3; two more. The raider outpost, which hates everyone and fears any camp growing strong, is
the valley's bully. The next seed on the same map tells another history: the Grey Outpost on Ashpit
49 and on Mudhollow 42, but on Crowsnest only 11, and Mudhollow on the Sun Grove 15.

The first years read like a chronicle: Y1 d6 Crowsnest raids Lowmarsh and loses 5; Y1 d19 the Grey
Outpost's hunters kill one of Ashpit's at the border; Y1 d80 Mudhollow's elder Skarl takes 26 to
Lowmarsh and comes back with 150; Y1 d89 the Grey Outpost deposes Dorn; Y2 d5 the Sun Grove's
raptors, hungry, take Lowmarsh's animals; Y2 d60 the Grey Outpost's new war leader Isk falls on
Mudhollow's hunters to prove himself; Y3 d14 Captain Mol dies in a hunting accident.

Chiefs turn over fast: about three a camp a decade, from today's `chief_lifespan_days` (goblin 120
days, bandit 90, raider 150) and from chiefs who fall leading attacks. Each change brings a new war
leader with a year to prove himself and a council with another temper, which is much of why no two
decades of one world read alike. The figure is today's data, not this design's; a longer tenure
would give fewer, longer reigns.

---

## 4. Numbers

64 seeds per scenario and mode, 50 game years each (`run.py`, `results/summary.md`). The definitions
are in `run.py`'s header: a fight is a war party that fights (raid, invasion, band, ambush, plunder,
skirmish); lost means the attackers lost; an endless war is one pair of camps fighting with no 96-day
gap for five years; a surge is a last decade with twice the fights of the second; extinct is a camp
held at one member or none for 96 days running.

### 4.1 Stability and losses

The world left alone (no player):

| | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fights a world-year: mean [p5, p95] | 29.9 [22, 37] | 11.5 [9, 15] | 10.2 [5, 17] |
| Between camps a year | 10.1 | 0 | 2.3 |
| Share the attackers lost | 0.45 | 0.17 | 0.18 |
| Lost fights a year | 13.5 | 2.0 | 1.8 |
| Worlds with an endless war | 1.00 | 0 | 0.06 |
| Worlds with a surge | 0 | 0 | 0 |
| Worlds where a camp died out | 0 | 0 | 0 |
| Worlds with no camp fight in 50 years | 0 | 1.00 | 0 |
| Worlds with a decade of no camp fight | 0 | 1.00 | 0 |

Unattended for 500 years (8 worlds): fights a year by century stay between 8.7 and 11.3 in every
world, fights between camps between 1.4 and 3.1, with no trend. The endless wars of the life worlds
are 4 border wars in 64 worlds, each 5.1 to 6.0 years long and 12 to 18 fights, every one between the
raider outpost and a goblin camp, mostly skirmishes, and every one ends.

Losing raids: about one fight in six is lost, as under the gate, against nearly one in two under the
ladder. Where the losses fall (the share of the attackers' fights they lost, world left alone and
the player scenarios, `results/summary.md`):

| Target | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| A rival camp | 0.98 | (none fought) | 0.52 |
| A village | 0.18 | 0.17 | 0.09 |
| A train or caravan (railway) | (none) | 0.18 | 0.17 |
| The walled base (fortress) | 0.89 | 0.72 | 0.66 |
| The farmer, defence 30 | 0.26 | 0.32 | 0.30 |
| A weak payer (duel) | (none fought) | (none fought) | 0.04 |

So a raid on a village or a weak base is rarely lost, a raid on a strong base usually is, and a war
between camps is an even match. At the walled base 11 of 18 fights in 50 years were lost, about one
lost attack every four or five years. Under the ladder, camps attacking camps lose 98% of the time:
the plunder clock keeps sending parties at a target it cannot beat.

### 4.2 The players

| Scenario, player | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Fortress, Ben: fights a year on him (attackers killed a year) | 2.97 (39.3) | 0.07 (1.2) | 0.34 (5.4) |
| Farmer, Ben: fights a year (tribute a year) | 0.10 (0) | 0.15 (0) | 0.94 (5) |
| Multi, Ash: fights a year (attackers killed) | 8.85 (83.8) | 4.67 (50.6) | 0.41 (5.0) |
| Multi, Bea: fights a year (tribute a year) | 0 (76) | 0 (0) | 0.09 (369) |
| Multi without blame spill, Bea | 0 (76) | 0 (0) | 0.07 (363) |
| Railway at 4 legs a day: train robberies a year | 0 | 22.1 (all Frostmaw) | 3.5 (3.25 goblin, 0.26 Frostmaw) |
| Railway: caravan ambushes a year | 0 | 0 | 1.3 |

The duels (`game.py`'s setting: one goblin camp, one base, grievance 9 a day, no trips), last
decade:

| Player | Ladder (c): wars a year, killed a year, tribute | Gate (a) | Life |
|---|---|---|---|
| Fortress (80, never pays) | 9.6, 65, 0 | 0, 0, 0 | 0.1, 3, 0 |
| Weak (6, pays) | 0, 0, 406 | 0, 0, 362 | 0.1, 0, 255 |
| Stubborn (6, never pays) | 21.3, 60, 0 | 12.5, 35, 0 | 3.1, 4, 0 |
| Adaptive (12, builds after each loss, pays) | 0, 0, 406 | 0, 0, 195 | 0, 0, 228 |

The best responses line up: a camp asks a payer and leaves him alone, mostly leaves a strong base
alone, and raids a weak refuser a few times a year (his own best response is to pay or build). That
is a logit (quantal response) equilibrium: each side plays its best reply with some noise, and the
noise is what makes a world feel alive.

**A difference from EQUILIBRIUM.md §3.5.** `game.py`'s ladder keeps a paying player's feud at the
Envoy rung and climbs on, so it reports 21.6 wars a year against a weak payer. Today's engine resets a
paid feud to Notice (`settle`), and this model follows the engine: under today's ladder a payer is
not raided. The ladder's real failures are the fortress (9.6 lost wars a year), the weak refuser
(21) and the forever wars between camps.

### 4.3 How formulaic

| Measure (world left alone) | Ladder (c) | Gate (a) | Life |
|---|---|---|---|
| Next act called to the day from the last two (kind, target, day within 1) | 0.18 | 0.15 | 0.02 |
| Spread of days between one attacker's fights on one target (CV; 1 is Poisson) | 0.83 | 0.76 | 1.04 |
| Same camp, different seeds: difference of act mix (Jensen-Shannon) | 0.000 | 0.000 | 0.017 |
| Same camp, different seeds: spread of fights a year (CV) | 0.04 | 0.05 | 0.15 |
| Two seeds of one world: share of the first ten years' acts that match | 0.50 | 0.44 | 0.07 |

In a duel the gap is starker: against the stubborn player the ladder's next act is called to the day
65% of the time and two seeds share 96% of their acts; the life council 1% and 16%.

What started the life fights (alone): hunger was a driver of 49%, loot of 51%, glory of 24%, pride
of 11%, revenge and preemption 1% each; loot alone, with no other motive, 19%. Occasions: the
restless clock 80%, caravans 16%, provocations 4%. With players near, provocations, refusals and
news start more of them, and revenge and pride drive more.

### 4.4 What the model leaves out

- Bodies and marches: a fight resolves the day it is decided; the engine's marches add days.
- Players are scripts: a fortress, a payer, a hunter, a thief, a railway owner. Real players change.
  A payer pays a fair price only (section 3's preamble).
- One valley of seven factions, two villages, a road and one line.
- Dragons strike a camp 0.25 times a year; a hunting ground is lost about once a decade. Both are the
  model's stand-ins for the world, not part of the design.
- The ladder mode follows `engine.rs` in its rules, not in its code paths; the gate follows
  EQUILIBRIUM.md §2.5.

---

## 5. Cost

### 5.1 Memory

| What | Where | Bytes |
|---|---|---|
| New faction fields (2.11) | the paged faction row | about 45 a faction |
| New fields on each feud | the paged faction row | about 76 a feud |
| Feuds toward camps and villages in reach | the paged faction row | about 4 a camp in the model's valley: about 4 x (today's feud plus 76) |
| The council clock | the wheel | replaces the plunder clock's event: no change |
| Border clashes | the wheel | one event a camp with a hostile neighbour: about 94 bytes a faction (PM-C13's wheel cost of an event) |
| Occasions waiting their delay | the wheel | about 0.1 a faction at any time |

The faction row (about 450 bytes encoded with two feuds, PM-C1) grows to about 1.3 to 1.6 KB with four
feuds toward nodes; it is paged, 4 factions a page (PM-C12), so it costs disk and page loads, not
resident memory. Resident memory grows by the clash event, about 100 bytes a faction: about 0.4 MB at
4,096 factions on top of PM-C13's 3.17 MB.

### 5.2 CPU

| What | How often | Cost |
|---|---|---|
| A sitting | 9.5 to 14 a faction-year in the valley scenarios (the most near a busy line), up to 18 in a duel; today's plunder clock books 8 to 16 (`plunder_mean_days` 12, 6 and 8) | about 4 to 5 neighbours weighed, each one normal draw and 4 to 7 scored acts of a few dozen operations: a few microseconds (inferred), plus the faction's page, which today's plunder event loads as well |
| A border clash | about 1 a year per hostile pair | one draw and a feud update |
| An occasion | a provocation, a refusal, news, a passing train or caravan | a booked event; most become one sitting |
| Belief, stores, fear, pride, trust | at each event touching the feud or faction | closed form, a few operations |
| Acts | 5 to 6 a faction-year | as today's parties |

The work per faction-year is close to today's plunder clock's, since sittings replace plunders one
for one in count and a sitting's arithmetic is small beside a page load (about 5 microseconds plus
0.45 decode, PM-C12). Against today's 52.3 microseconds a faction-day at 1,024 factions, the
sittings add an estimated 1 to 3% (inferred; the paged sweep measures it, section 6). Nothing scans
per tick; nothing grows with time or with dead history (memories are four slots, beliefs two numbers).

### 5.3 Lines

| Part | Lines (estimate) |
|---|---|
| Rust: the council (voices, drives, beliefs, the sitting, prices, truces, story codes) in `rtsim-conflict/src/council.rs` | about 700 |
| Rust: engine changes (feuds toward nodes, occasions, `Ev::Council`, `Ev::Clash`, skirmish target, hunts hook, chief tenure, the lurk for every culture) | about 450 |
| Rust: caravan ambush (the caravan query in the world interface, the robbery report, `rtsim-world` side) | about 150 |
| Rust: persistence of the new fields (`persist.rs`, `tables.rs`) | about 150 |
| Rust: the `mind` block in `culture.rs` | about 120 |
| JSON: five cultures' `mind` blocks | about 125 |
| Rust tests (section 6) | about 900 |
| Java: reason lines on ENVOY and NOTICE in the `ConflictBridge` | about 120 |
| Java GameTests | about 150 |

The model is `model.py` (1,440 lines), `run.py` (469), `walk.py` (194) and `summarize_md.py` (119).

---

## 6. Tests

Property tests in `rtsim-conflict`, on the engine's scenario harness (`scenario.rs`), each over 64
seeds unless said:

1. `a_sitting_depends_only_on_its_faction`: two worlds with the same seed whose events between
   factions are interleaved differently produce the same acts for every faction.
2. `belief_converges`: after five fights or eight scouting trips, the believed defence is within 25%
   of the truth for 95% of draws; after a year with no look, the variance is back above 0.2.
3. `a_strong_base_is_mostly_left_alone`: the fortress duel, at most 0.3 fights a year over 50 years;
   at least one test of a new base in the first two years in at least half the seeds.
4. `a_payer_is_asked_not_raided`: the weak duel, at most 0.3 fights a year, tribute at least 150.
5. `a_weak_refuser_is_raided`: the stubborn duel, between 1 and 6 fights a year.
6. `no_runaway_no_permanent_peace`: the world left alone, 50 years: no surge; endless wars in at most
   15% of worlds and none longer than 10 years; every world with a camp fight in each 20 years.
7. `no_camp_dies_by_fighting`: no camp held at one member or none for 96 days, 50 years; and 8 worlds
   of 500 years with fights a year by century within a factor of 1.5 of each other.
8. `stories_differ`: two seeds of one world share less than 20% of their first ten years' acts; the
   next act is called to the day under 5% of the time.
9. `one_culture_many_minds`: the same camp over seeds has a spread of fights a year (CV) of at least
   0.1.
10. `every_fight_has_a_reason`: every party's reason code names a motive, and a revenge or honour
    motive names a memory that exists.
11. `a_sworn_truce_mostly_holds`: under 1 in 10 of the truces between camps are broken by the chief
    who swore them.
12. `the_frostmaw_robs_more_on_a_busier_line`: lurks a year rise with legs a day over 2, 4, 8, 12 and
    24.
13. `the_umvuthi_leads_only_when_wounded`: every Umvuthana invasion was chosen with W at least 0.8
    and carries the chief flag.
14. `a_thief_is_hated_and_a_stranger_is_not`: a Frostmaw raids the player who stole its crystal and
    not one who has not.
15. `matches_the_model`: the engine on the five duel and valley scenarios, with marches disabled,
    within 25% of `model.py` on fights a year, lost share and tribute.
16. `uninstall_reinstall` (UR-1): drop the conflict state and load again: no entity, block or item
    was added by the council, and every council starts at its priors.
17. The paged sweep (`docs/choices-paging-modules.md`) gains the sittings: CPU per faction-day at
    1,024 and 4,096 factions against today's, same state hash across two runs.

GameTests, only where Java is involved:

1. `anEnvoySpeaksItsReason`: an ENVOY with a reason code shows its line to the player it addresses,
   once, on arrival.
2. `aNoticeCarriesItsReason`: a NOTICE with a reason plays its sound and shows its line, and a notice
   for a player who is away waits for him (NOTICE_SHOWN).

---

## 7. Decisions taken here (each can be undone)

| Id | Decision | Reason |
|---|---|---|
| CL-D1 | A council of three voices (chief, war leader as heir, elder), the speaker drawn by standing and weighted 0.6 | one voice is one rule again; three give a camp moods and politics with three seeds of state |
| CL-D2 | A log-normal belief of each neighbour's defence, Kalman-updated, drifting back to uncertain | a camp can be wrong and learn; testing a new base is the Thompson draw, not a probe clock |
| CL-D3 | A logit choice among waiting and each neighbour's best act, culture temperature τ | the softened best reply is a quantal response equilibrium: near the gate's rationality, never predictable |
| CL-D4 | Occasions and a restless clock replace the ladder's climb and the plunder clock | fights follow events; grievance still matters, as revenge and as how soon a camp sits |
| CL-D5 | Feuds toward camps and villages in plunder reach | camps need beliefs, pride and memories about each other to have their own wars |
| CL-D6 | Camp stores with seasons drive hunger | winter hunger and bad years give fights a reason and a rhythm that is not a calendar |
| CL-D7 | Border clashes between hostile neighbours, 0.5 a year a pair when both are fed, up to 2.5 when both starve | without them camps at strength drift into permanent peace (the gate's 0 camp fights) |
| CL-D8 | A paid tribute buys a truce of 24 to 72 days sworn by the chief; breaking it costs 3 times more for him than for his heir | a payer must be safe enough that paying is his best reply; an heir breaking his father's word is a story |
| CL-D9 | Raiding a player with a record of paying costs up to 12 | without it a weak payer was raided 0.9 times a year by glory and revenge |
| CL-D10 | An envoy learns the base's defence | otherwise a refused new base was invaded on the prior's optimism year after year |
| CL-D11 | Word that a player is away halves the believed defence for 3 to 10 days, and changes no belief | treating it as an observation made camps believe him half as strong for good, and they lost invasion after invasion |
| CL-D12 | Pride is weighted by the hope of winning | otherwise pride sent hopeless raids (an Umvuthana raid at a believed 2%) |
| CL-D13 | A player or the railway is warned at most once in 16 days; camps without the limit | warnings twice in four days are noise; limiting camps lengthened border wars |
| CL-D14 | No siege in the choice | the model has no walls to break; the siege party stays in the data |
| CL-D15 | Chief tenure gamma, shape 3, mean today's `chief_lifespan_days`; the Mowzie chiefs keep 1,000,000 | an exponential tenure killed new chiefs within days; MOWZIE.md §4.2 |
| CL-D16 | The Frostmaw is a council of one whose rail grievance is revenge, not a threshold | its robbing rises smoothly with traffic and each lurk is a decision (CL-3) |
| CL-D17 | The Umvuthi leads an invasion in person only with wounded pride of 0.8 or more | MZ-3 (b) says it leads; a pride gate makes it rare and always provoked |
| CL-D18 | Blame spill 0.3 | multiplayer stories where a camp cannot tell two players apart (CL-2) |
| CL-D19 | No alliances | a coalition needs a model of its own and joint parties (CL-5) |
| CL-D20 | Reasons as codes, text written when read | 8-byte memories; no text in saves |
| CL-D21 | Every draw from `StreamKey(seed, SYS_COUNCIL or SYS_CLASH, faction key)` slotted by the faction's own counter | a sitting does not depend on the order of other events |
| CL-D22 | A stolen crystal is a Theft report naming the player at the steal | MOWZIE.md reports the steal in its CHIEF_STATE record (crystal held, §4.2); this makes it a grievance and a memory |
| CL-D23 | Renewal, pressure, break and refound unchanged | Ben's scope: only faction fighting |
| CL-D24 | Camps ambush caravans on the road near them (3 in 100 passing caravans are weighed) | a caravan is the natural prey of a bandit camp, and with trains it gives camps a target that is neither a village nor a player; the conflict engine has no caravan hook today (2.14) |

---

## 8. Questions for Ben

| Id | Question | Options | Why it is yours |
|---|---|---|---|
| CL-1 | **Should a faction tell the player why?** | (a) **Yes (Recommended):** an envoy says one line when it arrives ("You killed Vorr, our chief, in the spring of year 3. Pay 106 emeralds."), and a notice says who is angry and for what. (b) Only on request, through a command or a page. (c) Never: the player guesses. | It is what the player reads; the reasons are kept either way. |
| CL-2 | **Can a camp blame the wrong player?** | (a) **Yes, 3 times in 10 (Recommended)** when a player's kill had no witness and another player is in reach: a bystander may be asked to pay for a neighbour's deeds (Bea, 3.4). (b) Never: only the doer is blamed. | In multiplayer it punishes a player for what another did. |
| CL-3 | **How often does a Frostmaw rob a busy line?** | (a) **This design's rate (Recommended):** a lurk about 0.3 a year at 4 legs a day, 1.8 at 12, 4.9 at 24, roars between. (b) MOWZIE.md §4.2's ladder: none below 70 grievance (none at 4 legs), then a lurk about every week (from day 17 at 12 legs, day 6 at 24). (c) This design with the Frostmaw twice as angry (about 4 a year at 12 legs). | It sets how much the railway costs in Frostmaw country. |
| CL-4 | **How often is a strong base tested?** | (a) **Now and then (Recommended):** about one fight in three years, most lost by the attackers, each with a reason (a new neighbour, a starving camp, a feud, word he is away). (b) Never unless he provokes. (c) About one a season. | It decides whether walls end the game's danger. |
| CL-5 | **Alliances between camps?** | (a) **Not in this build (Recommended):** camps fight, truce and feud pairwise. (b) Camps that share an enemy can ally and send joint war parties. | Alliances are a new mechanic with Java-visible joint parties. |
