# TALK: speaking with the valley's people

Status (2026-10-22): designed; slices 1 to 4 being built in this order (§12). Choices are TK# in
`docs/choices-talk.md`; Ben answered TQ-1 to TQ-3 the same day (§13): talk wins over Carry On, no
hint (players find talk themselves), and both names and descriptions of places.

Ben's decision (2026-10-22), answering CL-10 (CONFLICT-LIFE.md §8, how a player hears gossip):

> "Shift + Click to speak to a viller, have them output fluff lines, lines based on how they are
> feeling, and lines based on what they might have seen. Players should be able to do the same withi
> ribbits, and other clans as well. Talking with a villager should mildly increase a players
> 'relationship' with them"

And for the envoy (CL-1), "Speaks to you in chat": one line, to that player only, in person, when they
talk to it. Talk is the one place rtsim puts text in chat, and only because the player asked for it:
nothing is ever pushed into chat (CL-9, "All faction news in-world").

**The rules this design keeps:** D1 (on demand and cheap: nothing runs until a player talks), D3 (no
entity, block, item or screen of our own: the talker is a body that already exists, the line is a
chat message), D4 (each mod's talkers live in that mod's plugin), UR-1 (removing rtsim leaves only
vanilla gossip, which vanilla decays), and in multiplayer only the player who talks sees the line.

---

## 1. A player's walkthrough

Ben is in Oakford, an adopted plains village. It is raining, mid-morning. Alex stands beside him.

1. **Fluff.** Ben crouches and right-clicks a farmer with an empty hand. No trade screen opens. The
   farmer turns to look at him and makes its villager sound, and Ben alone reads in chat:
   `Oakford Farmer: "Rain on the barley again. Good for the barley. Bad for my knees."`
   Alex hears the villager's murmur and sees nothing in chat.
2. **No repeats, no spam.** Ben clicks again at once: nothing (the farmer spoke under two seconds
   ago). A moment later he gets a different line, never the one he just heard:
   `Oakford Farmer: "You're soaked through. There's a dry bench under the bell, if you want it."`
3. **Mood.** Two days ago goblins from Mudhollow plundered Oakford's market. The baker's bread is
   scarce. The butcher says:
   `Oakford Butcher: "Mudhollow's lot took half the market two days back. Bread's dear as iron now."`
4. **What they have seen.** The cartographer has news a caravan brought from the east:
   `Oakford Cartographer: "A caravan from Highmoor says the port at Saltmere has closed. Pirates
   hold every lane into it. That's a day's ride east of here."`
   And the weaponsmith, who saw it himself:
   `Oakford Weaponsmith: "I saw you run that wolf pack off Tom's sheep yesterday. Good arm."`
5. **Relationship.** Ben talks to the cleric each day. Each first talk of the day adds 3 minor
   positive gossip about Ben to that cleric; vanilla takes 1 back each day, so a daily chat nets 2
   a day, up to vanilla's own cap (§7). It never makes Ben a hero; it does soften the memory of the
   time he hit a villager by mistake.
6. **The wandering trader** at the well is a caravan's merchant (C8a). Crouch-clicking it talks and
   opens no trades: `Trader: "Came up the Highmoor road. Nothing moves by sea at Saltmere now. Good
   for me, bad for them."`
7. **A Ribbit village**, two days later. A gardener ribbit: `Ribbit Gardener: "Brrup! The toadstools
   love this rain. So do I."` A merchant ribbit, who knows Ben paid fairly last week, is warmer than
   before; talking adds a little to the village's feeling for him (§7).
8. **A bandit on the road.** Ben crouch-clicks a crossbowman of the Crow Road gang, whose camp has
   warned him once: `Crow Road Bandit: "This road's ours. Walk it again with a full purse and we'll
   lighten it."` It is a warning, not a chat; the bandit does not like him and nothing he says
   changes that.
9. **The envoy** from Ashpit, standing at the edge of Ben's base, says when he talks to it:
   `Ashpit Envoy: "You killed Vorr, our chief, in the spring of year 3. Ashpit and Mudhollow ask
   140 emeralds, by dusk two days from now."` The same words whenever he asks again, until the
   demand is settled.
10. **Nobody here.** In a village rtsim never adopted, or with the sim switched off, villagers still
    talk: fluff and the mood they can see for themselves (rain, night, a raid on now). They just
    know no news.

---

## 2. The gesture

**Crouch (Shift) and right-click a talker with an empty main hand.** The off hand may hold anything
(a shield, a torch, food, a pearl): it is not used in that click. Nothing opens; the talker says one
line, in chat, to that player only.

Facts (vanilla 1.21.1 and NeoForge 21.1, read from the sources; the audit is in §3):
- **Villager:** `Villager.mobInteract` returns `super.mobInteract` (PASS) when
  `player.isSecondaryUseActive()`: a crouching click on a villager does nothing in vanilla today. The
  gesture is free.
- **Wandering trader:** `WanderingTrader.mobInteract` has no crouch check, so a crouching click opens
  its trades; and it opens them for either hand. Talk cancels both hands on a talker.
- **The crouch flag is the client's:** `ServerGamePacketListenerImpl.handleInteract` sets the server
  player's shift state from the packet before the interaction, so `isSecondaryUseActive()` on the
  server is what the player held when they clicked.
- **Order:** for each hand, `EntityInteractSpecific`, then `EntityInteract`
  (`Player.interactOn` calls `CommonHooks.onInteractEntity` first; a cancelled event returns its
  result and `entity.interact` never runs), then the item's own use on the mob. So a cancel at
  `EntityInteract` stops every `mobInteract`.

**How Java handles it** (`talk/TalkEvents`, core):
- `PlayerInteractEvent.EntityInteract` at **HIGHEST** priority, both sides. When the player is
  crouching, their main hand is empty and the target is a talker (§4): cancel with SUCCESS (one arm
  swing). On the server, the main hand speaks; the off hand's event is cancelled silently, so no
  mod's `mobInteract` runs for it either.
- On the client the same check runs for entity types a plugin registered as always talking
  (villager, wandering trader, ribbit, Umvuthana): the client cancels too, so it neither swings twice
  nor sends the off hand's packet. Talkers decided on the server only (a pillager that is a conflict
  body) send the off hand's packets: the server cancels the off hand's interact, and also its item
  use (`RightClickItem`, off hand, from that player in the same tick), so an off-hand pearl, food or
  bow does not fire with the talk.
- **Not a talker, or not now:** the event goes on untouched; so it does for a fake player (a Create deployer, any machine) and a frozen or stunned player (§3). A talker that is asleep, mid-trade with
  another player, a baby, or in a fight still answers (§5.4); a dead or removed one does not.
- `BridgeEvents.onEntityInteract` (the phase 1 repricing before a trade) runs at normal priority and
  never sees a talk: no trade opens, so nothing needs repricing.

---

## 3. What else in the pack uses the gesture

The audit (a Sonnet sub-agent over every jar in `pack/mods`, 2026-10-22, decompiled with CFR;
output in `/tmp/talk-audit/`) found one real conflict and a handful of mobs whose screens open even
when crouching. Talk at HIGHEST, empty main hand only, resolves all of them:

| Mod (version) | What it does on a crouching right-click | Effect on talk | Resolution |
|---|---|---|---|
| **Carry On 2.2.6.13** | `CommonEvents.onEntityRightClick` (EntityInteract, HIGH, server): with both hands empty, within 2.5 blocks, picks the entity up and cancels with SUCCESS. The pack leaves the carry key unbound (`pack/options.txt` line 200), so Shift is the carry key. Its `forbiddenEntities` default lacks villagers, wandering traders and ribbits; it never picks up MONSTER mobs (`pickupHostileMobs=false`) | **The one conflict**: the same gesture on the same mobs | TQ-1, answered (a): talk at HIGHEST wins on talkers, so Carry On no longer picks up villagers, wandering traders or ribbits; it still carries every other mob and every block, as now. Illagers and Umvuthana are MONSTER, which Carry On never takes |
| vanilla Villager | crouching: PASS, nothing | none | talk takes the free gesture |
| vanilla WanderingTrader | crouching still opens trades (any hand) | trade screen | talk cancels both hands |
| Ribbits 4.1.6 | crouch + amethyst shard sets a ribbit's home; otherwise no crouch check, so a merchant ribbit opens its trades | trade screen | empty main hand only: the shard keeps working, the trade does not open |
| Mowzie's Mobs 1.8.2 | Umvuthana and Umvuthi open their trade to a masked player, with no crouch check; the Sculptor opens its screen to anyone | trade screen | talkers cancel first. The Sculptor is no talker (one quest NPC, not a people), so it keeps its screen |
| Mowzie's Mobs 1.8.2, L_Ender's Cataclysm 3.33, Sable Player Ragdoll 0.7.5 | cancel any interaction while the player is frozen, stunned or ragdolled | none | they cancel at NORMAL, after talk's HIGHEST, so talk checks for itself: a player with Mowzie's `frozen` or Cataclysm's `stun` effect does not talk and the click goes on to them untouched. Sable's ragdoll has no public state to read; a ragdolled player who manages a click may hear a line, which is harmless. Mowzie also blocks interaction while a player's ability runs (`AbilityData.interactingPrevented`); talk does not read it, so a player mid-ability may talk, which is harmless too |
| Create 6.0.10 | `StockTickerInteractionHandler` (EntityInteractSpecific, client too): a seated mob beside a Stock Ticker opens the shop; Create cancels on the client, so no packet reaches the server | a seated shopkeeper never talks | accepted: a shopkeeper is Create's. TK8 |
| Create 6.0.10 | `ScheduleItemEntityInteraction`: a conductor on a scheduled train | none in practice | conductors are not talkers |
| Ice and Fire CE 2.1-beta.1 | tamed dragons and other pets: crouch + empty hand opens the inventory (owner only) | none | pets are not talkers; Ice and Fire has no villager-like people |
| rctmod 0.19.0-beta, Cobblemon 1.8.1 | trainers start a battle, `trainer_association` extends WanderingTrader, Cobblemon NPCs open their dialogue, with no crouch check | none | not talkers (D4: Cobblemon is leaving the pack). rctmod's own chat replies are a precedent for NPC lines in chat |
| Sophisticated Backpacks 3.26.3 | crouch + a backpack with a Mob Catcher in hand captures | none | item in hand, so talk never fires |
| Accessories, Petrolpark, Born in Chaos, Destroy, L2 Complements, MutantMonsters, Ice and Fire's chain and horn | item-gated or a few mob types | none | an item in hand, or not a talker |
| KubeJS (pack scripts) | no entity-interact script (only `disable_villager_trading.js`, MoreJS trade removal) | none | |
| mic-rtsim's own `BridgeEvents`, `HerdEvents`, `mic_rtsim_carryon` | repricing before a trade, shears on herds, a puppet released from the sim when Carry On picks it up | none | unchanged |

Not read one by one: the `mobInteract` bodies of the jars with no interact hook at all (Artifacts,
Comforts, Copycats, Create Aeronautics and the rest the audit lists). Talk only fires on its own
talkers, so their mobs are untouched whatever they do.

---

## 4. Who talks

A **talker** is an entity a plugin registered with `RtsimApi.registerTalker(type, Talker)`. The
`Talker` answers, for one entity and one player: its **voice** (which lines, §8), its **name** as a
speaker, its **holder** (whose noticeboard and market it speaks from, if any), its **attitude**
toward the player (friendly, wary, hostile) and the **credit** a talk earns (§7). Each mod's talkers
live in that mod's plugin (D4).

| People | Entity types | Plugin | Voice | Holder (news, mood) | Attitude | Credit (§7) |
|---|---|---|---|---|---|---|
| Villagers | `minecraft:villager` (adults by profession; babies) | core | `villager.<profession>`, `child` | the adopted village it is a resident of (`Residents.villageOf`); none outside one | friendly; wary when vanilla reputation for the player is at or below -25 | vanilla gossip |
| Caravan merchants | `minecraft:wandering_trader` (C8a: while trade is on, every one is a caravan's puppet) | core | `trader` | the caravan's own board (HolderKind Caravan); a stray non-puppet trader has none | friendly; wary after the player robbed a caravan | none (TK6) |
| Ribbits | `ribbits:ribbit` (gardener, fisherman, merchant, sorcerer, nitwit; nitwit musicians in a band) | `mic_rtsim_ribbits` | `ribbit.<profession>` | its ribbit village; a merchant caravan's or band's board when it is a puppet | by the village's sentiment: wary at or below -25 (the sorcerers' threshold), warm at or above 25 | the village's sentiment |
| Conflict bands: bandits, raiders, goblins | the conflict bodies, whatever stands in for them (`BodyKinds.standIn`: pillager, vindicator, evoker and others in M3) | core (M3's ConflictBridge) | `bandit`, `raider`, `goblin`, by the party's culture; beasts (ravagers, wolves) are not talkers | the camp's board (its Haven node) | hostile in a raid or when the camp feuds with the player; wary otherwise | none (TK6) |
| Envoys | the envoy band's body | core (M3) | `envoy.<culture>` | the camp | wary | none |
| Umvuthana and the Umvuthi | `mowziesmobs:umvuthana`, `umvuthana_raptor`, `umvuthana_crane`, `umvuthi`, and the player-led followers `umvuthana_follower_player`, `umvuthana_crane_player`, `umvuthana_follower_raptor` | `mic_rtsim_mowzie` | `umvuthana`, `umvuthi`; followers `umvuthana.follower` | the grove's camp | by mask: friendly to a masked player, hostile to a bare face (Mowzie's own rule for trade) | none |
| Not talkers | the Frostmaw, the Sculptor, golems, animals, pets, trader llamas, Create's seated shopkeepers, rctmod and Cobblemon NPCs | | | | | |

Talkers whose type always talks (villager, wandering trader, ribbit, the Umvuthana types) are
registered by type, so the client can cancel too (§2). Conflict bodies are talkers only while they
are a band's puppet (the conflict tag), decided on the server.

Ships' crews and ports (the ports agent's PORTS work) can add a `sailor` and `pirate` voice in the
ships plugin later through the same API; the ports news kinds (§9.4) need no talker of their own,
since villagers in a port already speak them.

---

## 5. The lines

Three kinds, as Ben asked, plus the hostile and envoy variants.

### 5.1 Fluff: who they are and the day they are having

Picked from the talker's voice and what the talker can see for itself, all on the Java side, no
engine needed:
- **Who:** profession (or role), and whether it is a child.
- **Time:** dawn (game day time 23,000 to 1,000), day, dusk (11,500 to 13,000), night.
- **Weather at the talker:** clear, rain, thunder, snow (rain in a cold biome).
- **Biome**, by the common biome tags (`c:is_hot`, `c:is_cold`, `c:is_wet`, `c:is_dry`,
  `minecraft:is_ocean`, `c:is_mountain`, `minecraft:is_forest`, `c:is_plains`, `c:is_swamp`,
  `c:is_desert`, `c:is_snowy`, `minecraft:is_jungle`, `c:is_savanna`, `minecraft:is_taiga`,
  `minecraft:is_badlands`): Terralith's and Deep Time's biomes carry them.
- **Asleep, busy, a child, hurt:** a sleeping villager mumbles, one trading with another player says
  to wait, a child says child things, a talker under half health says so.

### 5.2 Mood: how they feel

- **What the talker sees** (Java): a vanilla raid on the village now (`ServerLevel.getRaidAt`), night
  with monsters near (any `Enemy` within 16 blocks), and the player's vanilla reputation with this
  villager (`Villager.getPlayerReputation`): liked at 25 and up, disliked at -25 and below, and
  "hated" at -100 and below, where vanilla's golems turn on them.
- **What the engine says** (the TALK_FACTS answer, §9): the holder's sentiment toward the player (for
  the sim's own peoples), the market's scarcest and most glutted good (stock per target), a port
  closed or blockaded, and the news (§5.3), from which Java reads:
  - **safety:** an assault, killing, robbery, raid or port raid on the talker's own settlement within
    10 days is "uneasy", within 2 days "afraid";
  - **feud:** a camp's feud with the talker's settlement (a Feud report naming it) is "we are hated by
    Mudhollow";
  - **dragon:** a dragon's assault on the settlement, or one seen near it, within 10 days;
  - **hunger:** the scarce good is a food (it has `DataComponents.FOOD`) below a third of its target;
  - **prices:** a good under half its target is dear, over twice its target is cheap.
- **A camp's own mood** (slice 4, with M3): its pool hungry or fed, a new chief, cowed after a lost
  fight, and its anger at the player (the feud rung).

### 5.3 News: what they might have seen

The engine sends up to four reports from the holder's noticeboard, ranked (§9.3). Each line names who
did what to whom, when and where, from the talker's side:
- **Perspective**, picked by the actors: `you` (the player did it), `to_you` (it was done to the
  player), `us` (done to the talker's own settlement or camp), `by_us` (the talker's own camp did it),
  `other`.
- **Seen or heard:** a report whose place is within 96 blocks of the holder is told as seen ("I saw"),
  the rest as heard ("a caravan says", "they say").
- **When:** today, yesterday, two or three days ago, last week, a while back.
- **Where:** a compass direction and a distance band from the talker (close by, a short walk, a
  day's ride, far off), with the nearest settlement ("near Lowmarsh") when it is one the talker names
  (§6, TK29).
- **Named or described** (TQ-3, both): "Mudhollow raided Lowmarsh, a day's ride east" one time, "the
  goblins in the hills raided a village a day's ride east" another (§6).

Every report kind has lines (§9.4), so this is the path CL-9 and CL-10 need: the five conflict kinds
(feuds, pacts, betrayals, raids, deaths), the ports kinds (raids on ports, blockades, closures,
reopenings, havens broken), and the kinds rtsim already keeps (assaults, killings, robberies, thefts,
fair trades, rescues, a dragon's attacks and deaths). News reaches a talker only the way rtsim's news
already travels (messengers, caravans, bands; CONFLICT-LIFE.md §2.17): a village nobody trades with
knows nothing, and a trader's news is from the road he walked.

### 5.4 Hostile clans: insults and warnings

A hostile talker never chats. It **warns** (the camp is at Warn or above toward the player, or the
player stands in its territory) or **insults** (a raid is on, or it is losing a fight). A warned
player hears the reason when the camp has one ("You've been cutting the hill oaks."). A bandit's
"written warning" (CONFLICT-LIFE.md §2.7, the Warn row) is this: spoken by a bandit when he is talked
to, not written down (TK12).

### 5.5 The envoy

The envoy's one line is its demand and its reason, the CL-1 line, as CONFLICT-LIFE.md §2.8 and
CL-D34 shape it: who was wronged, how, when, who stands with the caller (CL-D48), the price and the
deadline. It is the same line every time they ask until the demand is settled; then it says the
settlement ("Paid. Go in peace, for now."). The fields come from ENVOY_MATERIALIZE (the faction, the
player, the tribute, the deadline, and CL-D34's reason with the ally) and the faction block of
TALK_FACTS (§9.2); slice 4 builds it with M3.

### 5.6 Picking one line

1. **Gather candidates**, each a (topic, arguments, weight):
   - fluff: 1.0 for the voice's own lines, 0.5 for each condition met (time, weather, biome);
   - mood: 2.0 when uneasy, 4.0 when afraid, 1.5 for prices or hunger, 1.5 for liked or disliked;
   - news: 3.0 × freshness (1 today, falling to 0.25 at the end of the report's life) × 2 when it is
     about the player × 1.5 when it is about us.
   All weights TUNABLE (TK4).
2. **Pick a topic** by weight, then **a variant** of it at random.
3. **Never the same line back to back:** each player keeps their last 8 line keys (in memory); a
   variant among them is skipped while another is left, and a topic whose every variant is among them
   loses half its weight.
4. **Rate limits** (TK3): a talker answers the same player once per 40 ticks (a click inside that is
   still cancelled, silently), and anyone once per 10 ticks. In memory, never saved.
5. **Speak:** `ServerPlayer.sendSystemMessage` to the talking player only; the talker looks at them
   for 40 ticks (a villager's `LOOK_TARGET` memory; any other mob's look control, set again each
   tick, so a running look goal wins only its own tick) and plays its ambient sound, which everyone near
   hears, as vanilla's own murmur. Talking to a villager or a wandering trader counts in vanilla's
   "Talked to Villagers" statistic.

---

## 6. Names and places

rtsim keeps no names today: villages, camps and chiefs are keys (CL-D20: codes, text written when
read). Ben answered TQ-3 with both styles ("Generated names and directions, both are cool ideas!"),
so a line either **names** a place or people, or **describes** it by where it is (TK29):
- **Names** (`talk/Names`, TK13): deterministic from the key, so every player and every session hears
  the same name, and the envoy and the villagers agree. Two parts from the people's own list
  (`mic_rtsim.talk.name.<people>.a.<n>` and `.b.<n>` in the lang file, 32 to 48 each), put together by
  `mic_rtsim.talk.name.<people>.format`: villages "Oak" + "ford", goblin camps "Mud" + "hollow",
  bandit gangs "the Crow Road gang", raider outposts "the Grey Tower", havens "Blackwater"; chiefs one
  name from `mic_rtsim.talk.name.chief.<culture>.<n>` (Vorr, Gitz, Lugg). The server composes the
  name from its own (English) lang and sends it as plain text, so a client in another language sees
  the same name.
- **Descriptions**: the people or kind of place and a locator, `mic_rtsim.talk.desc.<kind>.at`
  ("the goblins %1$s") with the locator from the biome at its place (`mic_rtsim.talk.loc.<terrain>`,
  "in the hills", by the same tags as §5.1, read with `getBiome`, which never loads a chunk) and the
  compass or a distance band ("in the hills to the east", "a day's ride east"). A described ref that
  stands at the report's own place drops its locator ("a village"), since the line's where phrase
  already says where: "the goblins in the hills raided a village a day's ride east".
- **Which, each time** (TK29): the listener is "you", other players are named, and the talker's own
  settlement or camp, anything it saw (SEEN) and anything within a short walk (under 600 blocks) are
  named; within a day's ride (under 3,000) a ref is named half the time, farther a quarter of the
  time, and otherwise described. Each ref is decided on its own, per line, from the talker's random, so
  the same news is sometimes told with names and sometimes by place.
- **The village a talker lives in** is named in its own lines ("here in Oakford"), which is how a
  player learns the names: by visiting.
- **Players** are named by their name, or "you" when it is the listener.
- **Dragons** by species ("a fire dragon"), from the engine's body name.
- **Places** as §5.3 says: compass and distance from the talker, and the nearest settlement. On
  Toroidal World's wrapped map the engine computes the shortest way round (its topology), so
  "east" is the short way.

---

## 7. Relationship

**Vanilla villagers: vanilla gossip.** The first talk of each game day (24,000 ticks of game time, so
`/time set` and sleeping do not reset it) between a player and a villager adds **MINOR_POSITIVE 3**
about the player to that villager (`GossipContainer.add`; TK1). Vanilla's rules then carry it:

| Gossip | Weight | Max | Decay a day | Lost passing to another villager |
|---|---|---|---|---|
| MINOR_POSITIVE | +1 | 25 | 1 | 5 |

- Values below 2 are dropped, on add and on decay. So +3 is the least that survives a day; +2 would
  vanish at the first decay (TK1).
- Decay runs once each 24,000 ticks per villager (`Villager.maybeDecayGossip`). A player who talks
  every day nets +2 a day: 3, 2, 5, 4, 7, ... up to 25, which is reputation +25 with that villager
  (MINOR_POSITIVE weighs 1). That is all talk can ever reach, a fifth of what curing a zombie
  villager gives (MAJOR_POSITIVE 20, weight 5).
- Villagers pass gossip on every 1,200 ticks, losing 5 a pass: a value of 25 reaches a neighbour as
  20. Vanilla's own spread; talk does nothing to it.
- What it changes in MIC: rtsim's prices ignore vanilla's reputation discount (G31) and standing only
  adds penalty levels from -25 down, so positive talk gossip only offsets a player's slights (and
  keeps golems calm). The engine sees it in VILLAGE_GOSSIP like any other gossip.
- **The cap** lives on the villager: a saved attachment `mic_rtsim:talk_days` (player UUID to the last
  game day credited, at most 16 players, oldest dropped), as `mic_rtsim:gossip_ops` does. Babies,
  sleeping and busy villagers give no credit.

**Ribbits: the sim's sentiment.** Ribbits have no gossip of their own; their village's feeling is
rtsim-knowledge's sentiment (prices, the sorcerers' buffs). The first talk of the day between a player
and a ribbit sends TALKED (§9.2), journaled, and the engine adjusts the ribbit village's sentiment
toward them by **+3** (TK2) through the phase 1 world's input log (so a crash replays it). The cap is
the same attachment on the ribbit. rtsim's sentiment fades weak feelings in hours (a +3 is gone in
about 7,500 ticks), so talk alone never makes a friend (25): it lifts a player hovering under the
sorcerers' -25 back over it for the day, which is "mildly".

**Everyone else: nothing.** Talking to a caravan merchant, a bandit, a raider, a goblin, an envoy or
an Umvuthana changes no feeling (TK6): a camp's anger is its memories and grievances (CONFLICT-LIFE),
and chatting must not talk a war down.

**UR-1.** Removed, rtsim leaves the MINOR_POSITIVE gossip it wrote, vanilla data that vanilla decays
by 1 a day and that only ever helps the player; NeoForge drops the unknown `mic_rtsim:talk_days`
attachment on load. The ribbit credit is in rtsim's own state, reset with it. Nothing else is
written.

---

## 8. The text

All lines are in lang files as keyed templates, en_us only for now. The core's lines are in
`neoforge/core/src/main/resources/assets/mic_rtsim/lang/en_us.json`; each plugin keeps its people's
lines in its own (`assets/mic_rtsim_ribbits/lang/en_us.json`, `assets/mic_rtsim_mowzie/...`).

**Keys:** `mic_rtsim.talk.<voice>.<topic>.<n>`, `n` from 0. The server counts each key's variants
once, at start, from its own language table (NeoForge loads every mod's en_us on a dedicated server
too), so adding a line is adding a key. A **voice** has a chain of levels: `villager.farmer`, then
`villager`; `ribbit.sorcerer`, then `ribbit`. A topic draws from every level that has lines, the most
specific at the topic's weight and each parent at 0.6 of the level below, so a farmer's fluff weighs
1.6 in all; `any` speaks only when no level has lines (TK31). A plugin's voice names its own chain (the
envoy's, slice 4).

| Topic | When |
|---|---|
| `fluff`, `fluff.dawn`, `.day`, `.dusk`, `.night`, `.rain`, `.thunder`, `.snow`, `.clear`, `.hot`, `.cold`, `.wet`, `.dry`, `.ocean`, `.mountain`, `.forest`, `.plains`, `.swamp`, `.desert`, `.snowy`, `.jungle`, `.savanna`, `.taiga`, `.badlands` | §5.1 |
| `state.asleep`, `state.busy`, `state.hurt` | §5.1 |
| `mood.raid`, `mood.afraid`, `mood.uneasy`, `mood.monsters`, `mood.liked`, `mood.disliked`, `mood.hated`, `mood.dear` (%1$s good), `mood.cheap` (%1$s good), `mood.hungry`, `mood.feud` (%1$s camp), `mood.dragon` (%1$s dragon), `mood.port_closed`, `mood.blockade` | §5.2 |
| `news.<kind>.<perspective>` (args %1$s who, %2$s whom, %3$s when, %4$s where, %5$s source) | §5.3; kinds in §9.4 |
| `hostile.warn`, `hostile.warn.<reason>`, `hostile.insult` | §5.4 |
| `envoy.demand`, `envoy.reason.<motive>`, `envoy.settled` | §5.5 |

Shared phrase keys: `mic_rtsim.talk.say` (`%1$s: "%2$s"`), `mic_rtsim.talk.speaker.<voice>`
(`%1$s %2$s`: the place, then vanilla's own profession name, `entity.minecraft.villager.farmer`),
`mic_rtsim.talk.when.<band>`, `mic_rtsim.talk.where.<band>` and `.dir.<compass>`,
`mic_rtsim.talk.heard.<source>`.

**Sent as** `Component.translatableWithFallback(key, serverText, args)`: a client with the lang file
renders it in its own language; one without it (a server-side install the client never had) shows
the server's English. Names and places go in as plain text (§6).

**Voices** (each line short, one or two sentences, no em dashes):
- **Villagers** speak plainly, a little nosy, proud of their village. Each profession has its
  concerns: the farmer weather and crops, the librarian words and a bit of airs, the cleric omens,
  the smiths iron and quality, the butcher meat and prices, the cartographer roads and far places
  (the best one to ask for news), the fisherman water and ports, the shepherd the hills, the mason
  stone, the nitwit nonsense, the unemployed looking for work; children play.
  - "Flax won't weed itself. Neither will I, at this rate."
  - "Another cart came up from Lowmarsh. Cheese, mostly. I'd trade a book for cheese."
  - "The wind's from the east. My grandmother said that means visitors. She also said a lot of things."
- **Caravan merchants** are worldly, terse and a bit mercenary; their news is the road's.
  - "Roads are roads. This one has fewer bandits than the last."
  - "Highmoor wants copper. Everyone wants copper. I want a dry bed."
- **Ribbits** are frog villagers: cheerful, damp, fond of rain, mushrooms and music, with a croak in
  most lines. The sorcerer is mystical, the merchant counts amethyst, the gardener loves toadstools,
  the fisherman lily pads and ponds, the nitwit hums.
  - "Brrup! Rain is just the sky saying hello."
  - "Amethyst, amethyst. Shiny and sings when you tap it. Ribbit."
  - "The moon whispers to the pond. The pond whispers to me. Today it said: buy more mushrooms."
- **Bandits** are wry and menacing; they talk tolls.
  - "Nice boots. Shame if they walked off."
- **Raiders** (illagers) are cold and contemptuous; they despise villages and those who trade there.
  - "Hrmm. Run home, merchant's pet."
- **Goblins** are crude, gleeful and scrappy.
  - "Shiny thing! Give shiny thing! No? Then go away before Gitz sees you."
- **Umvuthana** are formal, devoted to the sun and the Umvuthi; they call a masked player "masked one"
  and a bare face something worse.
- **Envoys** are formal and exact: they speak for the camp and name the price.

---

## 9. The boundary

### 9.1 Kind ranges

TALK takes the next free family range (boundary.md §12.1, DHT26 left 0x00C0, 0x0A00 and 0x8A00
free): **0x00C0 to 0x00CF** latest-wins observations, **0x0A00 to 0x0AFF** journaled observations,
**0x8A00 to 0x8AFF** commands (TK10). 0x0B00 to 0x0BFF and 0x8B00 to 0x8BFF stay free.

### 9.2 Records

**TALK_ASK (0x00C0, latest-wins, aux = n).** Java's open questions, sent each tick until answered
(dropped and sent again when a buffer is full, as every latest-wins kind is):

| Offset in each ask (56 bytes, from 8) | Type | Field |
|---|---|---|
| 0 | u64 | askId (Java's, per session, never 0) |
| 8, 16 | u64 | playerMost, playerLeast |
| 24 | u32 | holder kind: 1 village (Java's village id), 2 caravan (a puppet's agent), 3 faction (the camp's key), 4 ship (an agent, later) |
| 28 | u32 | dimension (dictionary 1) |
| 32 | u64 | holder |
| 40, 44 | i32 | the talker's x, z |
| 48 | u32 | flags: 1 news, 2 faction facts |
| 52 | u32 | reserved |

**TALK_FACTS (0x8A00, aux = news n, at most 4; per session, no op id, never kept).** The answer, one
tick or more later:

| Offset | Type | Field |
|---|---|---|
| 8 | u64 | cmdSeq |
| 16 | u64 | askId |
| 24 | u32 | flags: 1 HOLDER (the engine knows the holder), 2 PORT_CLOSED, 4 BLOCKADED, 8 FACTION (a faction block follows, slice 4) |
| 28 | i32 | the holder's sentiment toward the player (rtsim-knowledge's scale) |
| 32 | u32 | scarcest good (dictionary 2; 0 none) |
| 36 | u32 | its stock per target, thousandths |
| 40 | u32 | most glutted good (0 none) |
| 44 | u32 | its stock per target, thousandths |
| 48 | n × 120 | news |

Each news item: 0 u16 report kind, 2 u8 severity, 3 u8 the report's flags, 4 u32 talk flags (1 YOU_DID,
2 TO_YOU, 4 TO_US, 8 BY_US, 16 SEEN), 8 u32 ticks ago (saturating), 12 i32 dx, 16 i32 dz (the
place from the talker, the short way round), 20 reserved, 24 ref who, 56 ref whom, 88 ref near (the
settlement nearest the place); 120 bytes.

A **ref** (32 bytes) is 0 u8 class, 1 u8 flags (1 dead, 2 POS), 2 reserved, 4 u32 what (dictionary 5
culture for settlements, camps and chiefs; dictionary 3 body for dragons and ships), 8 u64 a,
16 u64 b, 24 i32 dx, 28 i32 dz (where it is from the talker, the short way round, when POS: what a
description's locator reads, §6). Classes: 0 unknown, 1 player (a, b the UUID), 2 village (a Java's village id), 3 player
base, 4 caravan (a its seed, b its home village), 5 camp (a the faction key), 6 chief or notable
(a its seed, b its camp), 7 dragon, 8 lair, 9 haven, 10 ship.

The faction block (slice 4, built with M3 and the conflict engine): the camp's culture, its anger rung
at the player, the open demand (tribute, deadline), the reason (motive and memory codes, CL-D20,
with the ally of CL-D48), its chief's seed, its pool and whether it is cowed.

**TALKED (0x0A00, journaled):** 24 u64 opId (Java's), 32 u64 village, 40 u64 playerMost,
48 u64 playerLeast, 56 u32 the talker's body (dictionary 3), 60 reserved; 64 bytes. Sent only for a
credited talk with a ribbit (§7); the engine applies it once by op id, as the phase 1 input
`Talked`, which adjusts the village's sentiment.

### 9.3 The engine side (`rtsim-jni/src/engine/talk.rs`)

- **Holder:** village to its phase 1 node and actor; caravan agent to its caravan row's holder;
  faction to its camp (slice 4). Unknown: an answer without HOLDER, and Java says fluff and its own
  mood.
- **News:** the holder's board (at most 256 reports), each scored `kind weight × severity ×
  freshness × (3 about the player, 2 about us)`, the top four sent. Kind weights TUNABLE; fair trades
  weigh least.
- **Refs:** each actor is described by the module that owns it: phase 1 (villages, bases, caravans,
  players), ecology (dragons, lairs), conflict (camps, chiefs), ships (havens, ships). One lookup
  each; the nearest settlement by the phase 1 market cells.
- **Port state:** PORT_CLOSED when the latest of PortClosed and PortReopened about the holder is a
  closure; BLOCKADED when a Blockade about it is newer than both.
- **When phase 1 is off** (U10), the engine answers at once without HOLDER, so Java speaks fluff and
  its own mood straight away (TK32). When the engine is not running, nothing answers: Java waits 20
  ticks (TK5), then speaks without facts.

### 9.4 Report kinds (rtsim-knowledge `ReportKind`)

The ids TALK owns from 10 up (allocated 2026-10-22; the ports agent's rows as it sent them, commit
7001c090 on `ports/lifecycle`). `from_u16` becomes a match and the per-kind tables are sized by the
highest id, since the ids have gaps. Every new kind has sentiment delta 0 and no vanilla_mapping:
news is not a grudge (CONFLICT-LIFE.md §2.17).

| Id | Kind | who | whom | Severity | Lifetime (PLACEHOLDER) | Lines |
|---|---|---|---|---|---|---|
| 1 to 9 | Assault, Killing, Robbery, Theft, Trade, Rescue, ResidentAssault, ResidentKilling, Encroachment | (today's) | | | (today's) | `news.assault` and so on |
| 10 | Feud | the camp | its target (player, camp, village) | the rung | 10 days | `news.feud` |
| 11 | PactSworn | the camp that swore | its ally; a second row with flag "against" names the foe | | 30 days | `news.pact_sworn` |
| 12 | PactBroken | the camp that broke it | its ally; a betrayal is a flag | | 30 days | `news.pact_broken` |
| 13 | Raid | the camp | the target | the outcome (held, beaten, plundered); joint by a flag | 20 days | `news.raid` |
| 14 | Fell | the camp | the notable (the killer's actor for a killing, the camp for old age or a fever) | | 30 days | `news.fell` |
| 15 | spare, conflict | | | | | |
| 16 | PortRaided | the haven | the port village | 1 | as Robbery, 6 days | `news.port_raided` |
| 17 | Blockade | the haven holding most of the dock's lanes | the port | 1 | 6 days | `news.blockade` |
| 18 | PortClosed | that haven | the port | 2 | 6 days | `news.port_closed`, mood `port_closed` |
| 19 | PortReopened | the haven whose blockade lifted | the port | 1 | 6 days | `news.port_reopened` |
| 20 | HavenFell | the navy's home port | the haven | 2 | 6 days | `news.haven_fell` |
| 21 to 23 | spare, ports | | | | | |
| 24 to 31 | reserved, TALK's own | | | | | |
| 32 and up | free; ask the TALK owner first | | | | | |

A dragon's news needs no kind of its own: it is today's Assault (a dragon on a village), Killing (a
dragon slain), Robbery (a hoard robbed) and Rescue, told as dragon news because `who` or `whom` is a
dragon (ref class 7). A closed port's villagers grumble about empty markets and idle ships
(`mood.port_closed`): its ships in view keep running with empty holds until out of sight, then lay
up (the ports agent).

**Who records them** (main, 2026-10-22: no CONFLICT-LIFE builder is running). Slice 2 adds the
kinds, their lines and the engine side, and also records them at the conflict engine's sites that
exist today (TK26): Feud where FACTION_NOTICE fires today (a feud reaching a rung), Raid at a raid's
and a skirmish's resolution, Fell at a chief's or champion's death. PactSworn and PactBroken (11, 12)
stay unrecorded until alliances are built in the game (they are designed and measured in the model
only). The ports agent records 16 to 20.

---

## 10. Cost (D1)

Nothing runs until a player talks. A talk is one 64-byte ask, a board scan of at most 256 reports and
a sort, and an answer of at most 432 bytes; the Java side is a few map lookups and one chat message.
Per player, at most one line each 10 ticks. The variant counts are read once at start. No tick
handler, no saved state except the villager's day cap and the ribbit credit.

---

## 11. Reuse vs custom

| Need | Reused | Custom (and why) |
|---|---|---|
| The gesture | vanilla's crouch flag and NeoForge's `EntityInteract`, the free crouching click on villagers | the HIGHEST-priority listener that cancels on talkers |
| Who talks | existing bodies: vanilla villagers and traders, Ribbits, Mowzie's Umvuthana, the conflict stand-ins | `RtsimApi.registerTalker` (each plugin names its own, D4) |
| Showing a line | vanilla system chat (`sendSystemMessage`), `translatableWithFallback`, the lang system, vanilla's profession names, the talker's own ambient sound and look memory, vanilla's "Talked to Villagers" stat | none: no screen, entity, block or item (D3) |
| Mood | vanilla raids, weather, biome tags, reputation; rtsim's markets, sentiment and boards | the mood rules (§5.2) |
| News | rtsim-knowledge's reports, boards and the messengers that carry them | TALK_ASK, TALK_FACTS, ReportKinds 10 to 20, the ranking |
| Names | the keys rtsim already has | `talk/Names`, two-part names from lang lists (no names existed) |
| Relationship | vanilla gossip and its decay (villagers); rtsim's sentiment (ribbits); NeoForge attachments for the day cap | TALKED, the phase 1 input `Talked` |
| Warnings and envoys | conflict's Warn act and ENVOY_MATERIALIZE | the spoken warning (instead of a written one) |

---

## 12. Build slices and tests

Each slice is built by one Sonnet implementer, reviewed adversarially by an Opus sub-agent before it
lands, and landed on main.

1. **The talk path and vanilla villagers** (Java only, no boundary change): `talk/` in the core (the
   API, the listener on both sides, the picker, rate limits, no repeats, the lang counts, names),
   villagers and wandering traders, fluff and Java-side mood, the gossip credit with its cap, the core
   lang file (no hint: TQ-2, TK27). GameTests:
   - `talk_line_reaches_only_the_talker`: two mock players by a villager; one crouch-clicks; only they
     receive a line, and it is a `mic_rtsim.talk.` key;
   - `talk_opens_no_trade_screen`: a crouching click on a villager with trades and on a wandering
     trader leaves the player's menu the inventory;
   - `talk_gossip_capped_per_day`: two talks in one day add MINOR_POSITIVE 3 once; after 24,000
     ticks, a talk adds 3 again;
   - `talk_needs_an_empty_hand`: holding an item, the crouching click is not a talk.
2. **News** (the boundary): TALK_ASK, TALK_FACTS, `engine/talk.rs`, the ReportKinds, news and
   engine-side mood lines. GameTest `talk_tells_the_village_news`: a report put on the village's board
   (a test order) shows in a villager's line, with its names; Rust tests for the ranking, refs, the
   short way round, and the kind table with gaps.
3. **Ribbits:** the plugin's talker and lines, TALKED and the `Talked` input. GameTest on the Ribbits
   jar: a talk adds the sentiment credit once a day, and a merchant ribbit's trade screen does not
   open.
4. **Clans:** conflict bodies and envoys (with M3's ConflictBridge), the faction block, hostile lines,
   the spoken warning; the Mowzie plugin's Umvuthana and Umvuthi (with the Mowzie agents, through
   main). GameTests: a hostile body warns or insults, an envoy speaks its demand.

---

## 13. Ben's questions (answered 2026-10-22)

- **TQ-1. Carry On and talk share Shift + right-click with empty hands.** Ben: "Talk wins on talkers
  (Recommended)". Talk wins on talkers: Carry On can no longer pick up villagers, wandering traders and
  ribbits, and still carries every other mob and every block. (Not chosen: Carry On on another key;
  talk only with something in the off hand.)
- **TQ-2. How players find out they can talk.** Ben: "Nothing". No hint and no guide line: players
  find talk themselves (TK27). (Not chosen: a one-time hint and a guide line; the guide only.)
- **TQ-3. Names.** Ben: "Generated names and directions, both are cool ideas!". Lines use both:
  generated two-part names, the same for everyone, and descriptions by place, mixed as §6 says
  (TK29).
