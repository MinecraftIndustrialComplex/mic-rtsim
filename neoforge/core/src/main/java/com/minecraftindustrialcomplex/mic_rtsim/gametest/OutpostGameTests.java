package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeEvents;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.SiteTestHooks;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Outposts;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.sites.SiteLedger;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Registry;
import net.minecraft.core.SectionPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.monster.Pillager;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The vanilla pillager outposts as raider camps (EQUILIBRIUM.md §4, slice S7, EQC237 to EQC254; Ben's FB-5 (b)), on a real
 * {@code minecraft:pillager_outpost} start the structure itself generates, handed to {@code StructureManager.setStartForStructure}
 * as {@code ChunkGenerator.tryGenerateStructure} does, and a chunk load posted as the game posts it ({@link Outposts#onChunkLoad}):
 * an outpost is found once and reported as a raider camp, follows the conflict module's switch, joins late in a world that
 * had it before the mod, takes a player's kills of its raiders for the killing signal and no others, sends its envoy to a
 * provoker, breaks when farmed and is refounded, leaves vanilla's own raiders alone, and leaves the world as vanilla after an
 * uninstall. Each test generates its outpost in its own far chunk and is its own batch. The engine tests skip on the stub.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class OutpostGameTests {
    private OutpostGameTests() {}

    private static final String OUTPOST = "minecraft:pillager_outpost";
    private static final long DAY = 24_000L;

    /** An outpost generated at a far chunk of its own: what the structure made, and where. */
    private record Placed(ServerLevel level, ChunkPos cp, ChunkAccess chunk, StructureStart start, long faction) {
        /** A block inside the start's box, at the ground. */
        BlockPos inside() {
            BoundingBox b = start.getBoundingBox();
            int x = (b.minX() + b.maxX()) / 2, z = (b.minZ() + b.maxZ()) / 2;
            return new BlockPos(x, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z), z);
        }
    }

    /** Generates the outpost's start at a chunk of a far base, sets it on the chunk, and returns it (the chunk is loaded, no load posted). */
    private static Placed generate(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
        Structure outpost = reg.get(ResourceLocation.parse(OUTPOST));
        check("vanilla has the pillager outpost", outpost != null);
        int[] base = ConflictKit.farBase();
        ChunkPos cp = new ChunkPos(base[0] >> 4, base[1] >> 4);
        StructureStart start = outpost.generate(level.registryAccess(), level.getChunkSource().getGenerator(),
                level.getChunkSource().getGenerator().getBiomeSource(), level.getChunkSource().randomState(), level.getStructureManager(),
                level.getSeed(), cp, 0, level, b -> true);
        check("the outpost start is valid", start.isValid());
        ChunkAccess chunk = level.getChunk(cp.x, cp.z);
        level.structureManager().setStartForStructure(SectionPos.bottomOf(chunk), outpost, start, chunk);
        long key = Obs.villageId(RtsimTests.session().dimId(level.dimension()), cp.x, cp.z);
        return new Placed(level, cp, chunk, start, key);
    }

    private static void load(Placed p, boolean isNew) {
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(new ChunkEvent.Load(p.chunk(), isNew));
    }

    private static List<ScriptedEngine.Received> found(long faction) {
        return ConflictKit.factionsFound(faction);
    }

    private static SiteLedger.Row row(ServerLevel level, long faction) {
        return SiteTestHooks.row(level.getServer(), SiteLedger.CAMP, faction);
    }

    private static boolean nativeOnly(GameTestHelper helper, String test) {
        if (RtsimTests.engine() instanceof NativeEngine)
            return true;
        record(test, "skipped", "the camps are the engine's only on the native engine");
        helper.succeed();
        return false;
    }

    // ---- 1: found once ---------------------------------------------------------------------------

    /**
     * An outpost is found once when its chunk loads (EQC238): FACTION_FOUND for the start chunk's key, under the structure's id,
     * not late, with the raider culture's territory back (TERRITORY, 300 blocks); the same chunk loading again changes nothing.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "mic_rtsim_outposts_find")
    public static void anOutpostIsFoundOnceWhenItsChunkLoads(GameTestHelper helper) {
        String test = "anOutpostIsFoundOnceWhenItsChunkLoads";
        if (!nativeOnly(helper, test))
            return;
        Placed p = generate(helper);
        long[] before = {Outposts.found};
        load(p, true);
        helper.startSequence()
                .thenWaitUntil(() -> check("FACTION_FOUND reached the engine", !found(p.faction()).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received r = found(p.faction()).get(0);
                    check("not late (aux " + r.aux() + ")", (r.aux() & ConflictRecords.FactionFound.FLAG_LATE) == 0);
                    String structure = RtsimTests.engine().javaDictName(r.view().getInt(ConflictRecords.FactionFound.STRUCTURE));
                    check("under the structure's id: " + structure, OUTPOST.equals(structure));
                    check("one outpost found (" + (Outposts.found - before[0]) + ")", Outposts.found == before[0] + 1);
                    load(p, true);
                    load(p, false);
                })
                .thenWaitUntil(() -> check("the engine gave the territory back", bridge().territories().get(p.faction()) != null))
                .thenExecute(() -> {
                    var t = bridge().territories().get(p.faction());
                    check("the raiders' radius, 300 (" + t.radius() + ")", t.radius() == 300);
                    check("it minds killing", t.minds(ConflictRecords.SIGNAL_KILLING));
                    record(test, "territory", "radius " + t.radius() + ", " + Outposts.known() + " known");
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("loading again found nothing more (" + (Outposts.found - before[0]) + ")", Outposts.found == before[0] + 1);
                    check("one FACTION_FOUND (" + found(p.faction()).size() + ")", found(p.faction()).size() == 1);
                })
                .thenSucceed();
    }

    // ---- 2: the conflict switch -------------------------------------------------------------------

    /**
     * An outpost follows the conflict module's switch (EQC243, U-e): found while the module is off, it is known and journals
     * nothing; the module switched on, FACTION_FOUND goes within a tick.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "mic_rtsim_outposts_switch")
    public static void anOutpostFollowsTheConflictSwitch(GameTestHelper helper) {
        String test = "anOutpostFollowsTheConflictSwitch";
        if (!nativeOnly(helper, test))
            return;
        MinecraftServer server = helper.getLevel().getServer();
        Placed p = generate(helper);
        ConflictKit.onEnd(helper, () -> {
            RtsimConfig.Test.setModule(Module.CONFLICT, null);
            BridgeEvents.moduleSwitchesChanged(server);
        });
        RtsimConfig.Test.setModule(Module.CONFLICT, false);
        BridgeEvents.moduleSwitchesChanged(server);
        load(p, true);
        helper.startSequence()
                .thenIdle(40)
                .thenExecute(() -> {
                    check("known while the module is off", Outposts.known() >= 1);
                    check("nothing journaled while off (" + found(p.faction()).size() + ")", found(p.faction()).isEmpty());
                    RtsimConfig.Test.setModule(Module.CONFLICT, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(() -> check("FACTION_FOUND once the module is on", !found(p.faction()).isEmpty()))
                .thenExecute(() -> record(test, "switched on", found(p.faction()).size() + " FACTION_FOUND"))
                .thenSucceed();
    }

    // ---- 3: an existing world --------------------------------------------------------------------

    /**
     * A world that had its outposts before the mod (EQC244, FB-9 (a)): an outpost found in a chunk that was not new joins flagged
     * late; with {@code outpostsJoinExisting} off it is known and not reported.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "mic_rtsim_outposts_late")
    public static void existingWorldsOutpostsJoinLate(GameTestHelper helper) {
        String test = "existingWorldsOutpostsJoinLate";
        if (!nativeOnly(helper, test))
            return;
        Placed off = generate(helper);
        Placed on = generate(helper);
        ConflictKit.onEnd(helper, () -> RtsimConfig.Test.setOutpostsJoinExisting(null));
        RtsimConfig.Test.setOutpostsJoinExisting(false);
        load(off, false);
        helper.startSequence()
                .thenIdle(40)
                .thenExecute(() -> {
                    check("an existing outpost is known and not reported with the setting off (" + found(off.faction()).size() + ")",
                            found(off.faction()).isEmpty());
                    RtsimConfig.Test.setOutpostsJoinExisting(null);
                    load(on, false);
                })
                .thenWaitUntil(() -> check("the existing outpost joined", !found(on.faction()).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received r = found(on.faction()).get(0);
                    check("flagged late (aux " + r.aux() + ")", (r.aux() & ConflictRecords.FactionFound.FLAG_LATE) != 0);
                    record(test, "late", "FACTION_FOUND aux " + r.aux());
                })
                .thenSucceed();
    }

    // ---- 4: what counts as clearing an outpost ----------------------------------------------------

    /**
     * Clearing an outpost is grievance and a village's raid is not (EQC245, FB-6 (a)): a player's kill of a pillager inside the
     * outpost's box widened by the margin is the camp's killing signal; a witch there, and a pillager 200 blocks off (a village
     * raid's, a patrol's), are not.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mic_rtsim_outposts_kin")
    public static void clearingAnOutpostIsGrievanceAndAVillageRaidIsNot(GameTestHelper helper) {
        String test = "clearingAnOutpostIsGrievanceAndAVillageRaidIsNot";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Placed p = generate(helper);
        ServerLevel level = p.level();
        List<Long> forced = new ArrayList<>();
        for (int dx = -1; dx <= 1; dx++)
            for (int dz = -1; dz <= 1; dz++)
                if (ConflictKit.force(level, p.cp().x + dx, p.cp().z + dz, true))
                    forced.add(ChunkPos.asLong(p.cp().x + dx, p.cp().z + dz));
        ServerPlayer[] player = new ServerPlayer[1];
        long[] mark = new long[1];
        ConflictKit.onEnd(helper, () -> {
            for (long c : forced)
                ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
            if (player[0] != null)
                FakePlayers.remove(player[0]);
        });
        load(p, true);
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> {
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            check("the forced chunks tick entities", RtsimSession.entitiesTickAt(level,
                                    new BlockPos((p.cp().x + dx) * 16 + 8, 64, (p.cp().z + dz) * 16 + 8)));
                }))
                .thenWaitUntil(waits.logic(() -> check("the engine gave the territory back", bridge().territories().get(p.faction()) != null)))
                .thenExecute(() -> {
                    BlockPos in = p.inside();
                    player[0] = FakePlayers.add(level, new Vec3(in.getX() + 0.5, in.getY(), in.getZ() + 0.5), "outpost_kin");
                    player[0].setGameMode(GameType.CREATIVE);
                    mark[0] = bridge().signalsCounted;
                    // A witch inside the box: a raider, but not the camp's kin.
                    kill(level, player[0], EntityType.WITCH.create(level), in);
                    check("a witch is no kin (" + (bridge().signalsCounted - mark[0]) + ")", bridge().signalsCounted == mark[0]);
                    // A pillager far off, as a village raid's: outside the box and its margin.
                    kill(level, player[0], EntityType.PILLAGER.create(level), in.offset(200, 0, 0));
                    check("a pillager 200 blocks off is no kin (" + (bridge().signalsCounted - mark[0]) + ")", bridge().signalsCounted == mark[0]);
                    // Three pillagers and a vindicator inside: kin.
                    for (int i = 0; i < 3; i++)
                        kill(level, player[0], EntityType.PILLAGER.create(level), in);
                    kill(level, player[0], EntityType.VINDICATOR.create(level), in);
                    check("each kin killed counts (" + (bridge().signalsCounted - mark[0]) + ")", bridge().signalsCounted == mark[0] + 4);
                })
                .thenWaitUntil(waits.logic(() -> {
                    ConflictKit.reportPlayer(player[0]);
                    var rs = ConflictKit.signals(ConflictRecords.SIGNAL_KILLING, player[0].getUUID());
                    int n = 0;
                    for (ScriptedEngine.Received r : rs)
                        if (r.view().getLong(ConflictRecords.TerritorySignal.FACTION) == p.faction())
                            n += r.view().getInt(ConflictRecords.TerritorySignal.COUNT);
                    check("TERRITORY_SIGNAL for the four, none for the others (" + n + ")", n == 4);
                }))
                .thenSucceed();
    }

    private static void kill(ServerLevel level, ServerPlayer by, Mob mob, BlockPos at) {
        mob.moveTo(at.getX() + 0.5, at.getY(), at.getZ() + 0.5);
        level.addFreshEntity(mob);
        mob.hurt(level.damageSources().playerAttack(by), 1000f);
    }

    // ---- 5: a provoked outpost sends its envoy -----------------------------------------------------

    /**
     * A provoked outpost sends its envoy (EQC246): a player who kills thirty-two of its pillagers gets a herald with two
     * pillagers asking emeralds or the killing stopped, as any camp's envoy comes. The world's clock moves on 1,200 ticks to an ask
     * with the player reporting, as the ladder test has it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mic_rtsim_outposts_envoy")
    public static void aProvokedOutpostSendsItsEnvoy(GameTestHelper helper) {
        String test = "aProvokedOutpostSendsItsEnvoy";
        if (!(RtsimTests.engine() instanceof NativeEngine e)) {
            record(test, "skipped", "the stub engine decides nothing");
            helper.succeed();
            return;
        }
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Placed p = generate(helper);
        ServerLevel level = p.level();
        List<Long> forced = new ArrayList<>();
        for (int dx = -2; dx <= 2; dx++)
            for (int dz = -2; dz <= 2; dz++)
                if (ConflictKit.force(level, p.cp().x + dx, p.cp().z + dz, true))
                    forced.add(ChunkPos.asLong(p.cp().x + dx, p.cp().z + dz));
        ServerPlayer[] player = new ServerPlayer[1];
        long[] mark = new long[2];
        int[] asks = {0, 0};
        ConflictBridge.Party[] envoy = new ConflictBridge.Party[1];
        ConflictKit.onEnd(helper, () -> {
            for (long c : forced)
                ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
            if (player[0] != null)
                FakePlayers.remove(player[0]);
        });
        load(p, true);
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> {
                    for (int dx = -2; dx <= 2; dx++)
                        for (int dz = -2; dz <= 2; dz++)
                            check("the forced chunks tick entities", RtsimSession.entitiesTickAt(level,
                                    new BlockPos((p.cp().x + dx) * 16 + 8, 64, (p.cp().z + dz) * 16 + 8)));
                }))
                .thenExecute(() -> {
                    // A reused world's clock runs ahead of game time by the last run's asks: one ask of the gap catches it up.
                    mark[0] = e.phase1Stats()[NativeEngine.P1_WORLD_TICK];
                    long gap = mark[0] - level.getGameTime();
                    if (gap > 0)
                        e.phase1Advance(gap);
                })
                .thenWaitUntil(waits.logic(() -> check("the world's clock moves", e.phase1Stats()[NativeEngine.P1_WORLD_TICK] > mark[0])))
                .thenWaitUntil(waits.logic(() -> check("the engine gave the territory back", bridge().territories().get(p.faction()) != null)))
                .thenExecute(() -> {
                    BlockPos in = p.inside();
                    player[0] = FakePlayers.add(level, new Vec3(in.getX() + 0.5, in.getY(), in.getZ() + 0.5), "outpost_provoker");
                    player[0].setGameMode(GameType.CREATIVE);
                    for (int i = 0; i < 32; i++)
                        kill(level, player[0], EntityType.PILLAGER.create(level), in);
                })
                .thenWaitUntil(waits.logic(() -> {
                    ConflictKit.reportPlayer(player[0]);
                    for (ConflictBridge.Party q : bridge().parties())
                        if (q.faction == p.faction() && q.envoy)
                            return;
                    check("an envoy within 300 asks of 1200 ticks (asks " + asks[1] + ")", asks[1] < 300);
                    if (asks[0]++ % 4 == 0) {
                        e.phase1Advance(1_200);
                        asks[1]++;
                    }
                    check("the envoy comes", false);
                }))
                .thenWaitUntil(waits.logic(() -> {
                    ConflictKit.reportPlayer(player[0]);
                    for (ConflictBridge.Party q : bridge().parties())
                        if (q.faction == p.faction() && q.envoy)
                            envoy[0] = q;
                    check("the envoy is placed", envoy[0] != null && envoy[0].stacks().get(0).state() == ConflictBridge.StackState.PLACED);
                }))
                .thenExecute(() -> {
                    ConflictBridge.Demand d = envoy[0].demand();
                    check("a demand", d != null);
                    boolean pillagers = false;
                    for (ConflictBridge.Stack s : envoy[0].stacks())
                        for (ConflictBridge.Slot sl : s.slots())
                            pillagers |= sl.entity() instanceof Pillager;
                    check("the envoy comes as pillagers", pillagers);
                    record(test, "envoy", "after " + asks[1] + " asks; demand " + d.itemName);
                })
                .thenSucceed();
    }

    // ---- 6: a farmed outpost -----------------------------------------------------------------------

    /**
     * A farmed outpost breaks and the new band refounds it (EQC247): the camp is killed to the last warrior with pressure to
     * break it (P 2.0: a wait of 192 days), its ledger row says broken and waiting; the reinstall guard's emulation of 250 days
     * without rtsim resets the engine, the finder reports the outpost to the new session by itself, and the camp comes up refounded,
     * a quarter of its cap (12 of 50), with the pressure aged by the ticks between the rows' stamps.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mic_rtsim_outposts_farmed", skyAccess = true)
    public static void aFarmedOutpostBreaksAndTheNewBandRefounds(GameTestHelper helper) {
        String test = "aFarmedOutpostBreaksAndTheNewBandRefounds";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Placed p = generate(helper);
        ServerLevel level = p.level();
        MinecraftServer server = level.getServer();
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        long[] upTo = new long[1], at = new long[1];
        SiteLedger.Row[] broken = new SiteLedger.Row[1], after = new SiteLedger.Row[1];
        boolean forced = ConflictKit.force(level, p.cp().x, p.cp().z, true);
        ConflictKit.onEnd(helper, () -> {
            if (forced)
                ConflictKit.force(level, p.cp().x, p.cp().z, false);
        });
        load(p, true);
        helper.startSequence()
                .thenWaitUntil(waits.engine(() -> check("the engine gave the territory back", bridge().territories().get(p.faction()) != null)))
                .thenExecute(() -> RtsimTests.slayCamp(p.faction(), -1, 1.0f))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, p.faction());
                    check("the outpost broke (empty, broken, waiting): " + r, r != null && r.broken() && r.waiting() && r.count() == 0);
                    broken[0] = r;
                }))
                .thenExecute(() -> {
                    check("P is 2.0 and the wait 192 days: " + broken[0], Math.abs(broken[0].p() - 2.0f) < 0.05f
                            && Math.abs(broken[0].waitLeft() - 192 * DAY) < 2_000);
                    upTo[0] = SiteTestHooks.journalEnd();
                    SiteTestHooks.requestSave();
                })
                .thenWaitUntil(waits.engine(() -> check("an engine save covers the journal through " + upTo[0], SiteTestHooks.savedThrough(upTo[0]))))
                .thenExecute(() -> {
                    SiteTestHooks.gapRestart(server, 250 * DAY);
                    check("a session runs after the reinstall: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                })
                // The restart moved the game clock on by the gap: the budget of the logic waits starts again (the tick after the
                // restart shows the jump, then a chunk wait marks it; SiteLedgerGameTests.clockMoved).
                .thenExecute(() -> at[0] = helper.getTick())
                .thenWaitUntil(() -> check("the tick after the restart", helper.getTick() > at[0]))
                .thenWaitUntil(waits.chunks(() -> check("the outpost's chunk stands through the restart",
                        level.getChunkSource().getChunkNow(p.cp().x, p.cp().z) != null)))
                .thenWaitUntil(waits.engine(() -> check("the finder reported the outpost to the new session",
                        !found(p.faction()).isEmpty())))
                .thenWaitUntil(waits.engine(() -> {
                    SiteLedger.Row r = row(level, p.faction());
                    check("the refounded camp's row: " + r, r != null && !r.broken() && r.count() >= 12 && r.stamp() > broken[0].stamp());
                    after[0] = r;
                }))
                .thenExecute(() -> {
                    check("a new band of a quarter of the cap, a few recruited since: " + after[0], after[0].count() < 20 && !after[0].waiting());
                    check("the pressure kept, aged (" + after[0].p() + ")", after[0].p() > 0.3f && after[0].p() < 1.5f);
                    record(test, "refounded", after[0].toString());
                })
                .thenSucceed();
    }

    // ---- 7: vanilla's own raiders are left alone --------------------------------------------------

    /**
     * Vanilla's own raiders stay vanilla's (EQC248, FB-7 (a)): the outpost's pillager (its structure spawn override) and a patrol are
     * nobody's body, whether or not the outpost is a camp: no rtsim tag, no camp-member mark, not removed.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mic_rtsim_outposts_vanilla")
    public static void aVanillaPatrolAndTheOutpostsOwnPillagersAreLeftAlone(GameTestHelper helper) {
        String test = "aVanillaPatrolAndTheOutpostsOwnPillagersAreLeftAlone";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Placed p = generate(helper);
        ServerLevel level = p.level();
        List<Long> forced = new ArrayList<>();
        for (int dx = -1; dx <= 1; dx++)
            for (int dz = -1; dz <= 1; dz++)
                if (ConflictKit.force(level, p.cp().x + dx, p.cp().z + dz, true))
                    forced.add(ChunkPos.asLong(p.cp().x + dx, p.cp().z + dz));
        Pillager[] pillagers = new Pillager[2];
        ConflictKit.onEnd(helper, () -> {
            for (Pillager x : pillagers)
                if (x != null)
                    x.discard();
            for (long c : forced)
                ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
        });
        load(p, true);
        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> check("the outpost's chunk ticks entities", RtsimSession.entitiesTickAt(level,
                        new BlockPos(p.cp().x * 16 + 8, 64, p.cp().z * 16 + 8)))))
                .thenWaitUntil(waits.engine(() -> check("the outpost joined", !found(p.faction()).isEmpty())))
                .thenExecute(() -> {
                    BlockPos in = p.inside();
                    for (int i = 0; i < 2; i++) {
                        pillagers[i] = EntityType.PILLAGER.create(level);
                        pillagers[i].moveTo(in.getX() + 0.5 + i, in.getY(), in.getZ() + 0.5);
                        pillagers[i].setPersistenceRequired();
                        pillagers[i].setNoAi(true);
                        level.addFreshEntity(pillagers[i]);
                    }
                })
                .thenIdle(200)
                .thenExecute(() -> {
                    for (Pillager x : pillagers) {
                        check("still here, vanilla's", x.isAlive() && !x.isRemoved());
                        check("no rtsim tag", RtsimAttachments.tag(x) == null);
                        check("no camp-member mark", RtsimAttachments.campMember(x) == null);
                    }
                    check("no party of the outpost's", bridge().parties().stream().noneMatch(q -> q.faction == p.faction()));
                    record(test, "left alone", "two pillagers in the box, untagged after 200 ticks");
                })
                .thenSucceed();
    }

    // ---- 8: uninstall ---------------------------------------------------------------------------------

    /**
     * Uninstall leaves the outpost as vanilla made it (EQC249, UR-1): the reporter writes nothing to the world. With the outpost a
     * camp and its camp farmed, the session stops (an uninstall): the chunk's structure start is the one the structure made, no
     * entity in the box carries a mark, and nothing runs until the reinstall.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mic_rtsim_outposts_uninstall", skyAccess = true)
    public static void uninstallLeavesTheOutpostAsVanilla(GameTestHelper helper) {
        String test = "uninstallLeavesTheOutpostAsVanilla";
        if (!nativeOnly(helper, test))
            return;
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Placed p = generate(helper);
        ServerLevel level = p.level();
        MinecraftServer server = level.getServer();
        ConflictKit.onEnd(helper, () -> SiteTestHooks.cleanUp(server));
        long[] upTo = new long[1];
        int[] starts = new int[1];
        load(p, true);
        helper.startSequence()
                .thenWaitUntil(waits.engine(() -> check("the outpost joined", !found(p.faction()).isEmpty())))
                .thenExecute(() -> {
                    RtsimTests.slayCamp(p.faction(), 20, 0.5f);
                    starts[0] = p.chunk().getAllStarts().size();
                    upTo[0] = SiteTestHooks.journalEnd();
                    SiteTestHooks.requestSave();
                })
                .thenWaitUntil(waits.engine(() -> check("an engine save covers the journal through " + upTo[0], SiteTestHooks.savedThrough(upTo[0]))))
                .thenExecute(() -> {
                    SiteTestHooks.stop(server);
                    check("no session runs after the uninstall", RtsimSession.current() == null);
                    check("the chunk keeps the one start the structure made (" + p.chunk().getAllStarts().size() + ")",
                            p.chunk().getAllStarts().size() == starts[0]);
                    BoundingBox b = p.start().getBoundingBox();
                    int marked = level.getEntitiesOfClass(Mob.class, new net.minecraft.world.phys.AABB(b.minX(), b.minY(), b.minZ(), b.maxX(), b.maxY(), b.maxZ()))
                            .stream().filter(m -> RtsimAttachments.tag(m) != null || RtsimAttachments.campMember(m) != null).mapToInt(m -> 1).sum();
                    check("no mob in the box carries a mark (" + marked + ")", marked == 0);
                    SiteTestHooks.gapRestart(server, 5 * DAY);
                })
                .thenWaitUntil(() -> check("a session runs again", RtsimSession.current() != null))
                .thenSucceed();
    }
}
