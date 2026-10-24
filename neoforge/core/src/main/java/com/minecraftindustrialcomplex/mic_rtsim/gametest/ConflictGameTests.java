package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictNews;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictTalker;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Territories;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.industry.ThreatConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.talk.RateLimit;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talkers;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Difficulty;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.monster.Endermite;
import net.minecraft.world.entity.monster.Skeleton;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.entity.projectile.Arrow;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.UncheckedIOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.ARCHER;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.EMISSARY;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.WARRIOR;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.about;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.party;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The conflict bridge (MOWZIE.md slice M3, docs/boundary.md §9) on the server's real session, native
 * engine or stub. The tests script conflict commands as the engine's conflict would send them
 * ({@link ConflictKit}) for test agents and factions the engine does not know (it ignores what Java
 * reports about them), and read back what the bridge journaled. Each test has its own batch: parties of
 * two tests never meet, and no other test's player stands near a party. Every test logs what it compared
 * on a {@code [gametest]} line.
 *
 * <p>The arena tests have sky access: without it the framework roofs the test with barriers, which are
 * motion-blocking, so a body would stand on the roof.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class ConflictGameTests {
    private ConflictGameTests() {}

    private static long chunk(int cx, int cz) {
        return ChunkPos.asLong(cx, cz);
    }

    private static String chunks(Set<Long> cs) {
        List<String> out = new ArrayList<>();
        for (long c : cs)
            out.add(ConflictKit.chunk(c));
        return out.toString();
    }

    /** Holds a stack's bodies where they stand (no AI, so no stroll crosses a chunk edge; MZB-119). */
    private static void pin(ConflictBridge.Stack st) {
        for (Mob m : st.bodies())
            m.setNoAi(true);
    }

    private static int count(List<ScriptedEngine.Received> rs, int aux) {
        int n = 0;
        for (ScriptedEngine.Received r : rs)
            if (r.aux() == aux)
                n++;
        return n;
    }

    private static ScriptedEngine.Received first(List<ScriptedEngine.Received> rs, int aux) {
        for (ScriptedEngine.Received r : rs)
            if (r.aux() == aux)
                return r;
        return null;
    }

    private static String name(Mob m) {
        return m.getCustomName() == null ? null : m.getCustomName().getString();
    }

    // ---- ledger 25: a band stack by stack --------------------------------------------------------

    /**
     * Test 33 without its column (MOWZIE.md §8, ledger 25), at a place far from every earlier run's with
     * forced chunks standing in for the simulation distance: a band of two stacks whose places are in
     * chunks that do not tick entities is answered BAND_MATERIALIZED with none placed and never
     * rejected; each stack is pending and is tested only when its place moves into other chunks or a
     * chunk it waits on changes (counted: idle ticks test nothing). Each stack straddles a chunk edge,
     * and is placed the tick its last chunk ticks entities, not before; the other stack, whose chunks did
     * not change, is not tested. A member moved out of the ticking area while its place still ticks is
     * moved back, the same body, and its stack stays placed (MZB-125); a stack whose chunk stops ticking
     * goes back to pending, and the other stays; once its chunk ticks again it comes back, but not before
     * {@link ConflictBridge#REQUEUE_COOLDOWN} ticks. A recall reports every living member, placed or
     * pending, as a survivor. Chunks an earlier failed run left forced in the far area are released first.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 3000, batch = "conflict_stacks")
    public static void aBandAppearsStackByStackAndAPendingStackIsTestedOnlyOnChange(GameTestHelper helper) {
        String test = "aBandAppearsStackByStackAndAPendingStackIsTestedOnlyOnChange";
        ServerLevel level = helper.getLevel();
        String dim = RtsimTests.dimension(helper);
        int[] base = ConflictKit.farBase();
        int bx = base[0], bz = base[1];
        int k = bx >> 4, z0 = bz >> 4;
        long agent = ConflictKit.newAgent();
        ConflictKit.Band band = ConflictKit.band(agent, ConflictKit.newFaction(), dim);
        // Radius 0: both stacks at the arc's centre, 4 blocks apart across the heading (+x), so stack 0
        // stands in chunk row z0 and stack 1 in z0 + 1; the front rows 16 blocks west of bx, the rear
        // rows 1.5 behind.
        band.arcX = bx - 15.5;
        band.arcZ = bz + 16;
        band.targetX = bx + 1000;
        band.targetZ = bz + 16;
        band.speed = 0f;
        int released = ConflictKit.releaseFarChunks(level);
        List<Long> forced = new ArrayList<>();
        Runnable unforce = () -> {
            for (long c : forced)
                ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
            forced.clear();
        };
        ConflictKit.onEnd(helper, unforce);
        java.util.function.BiConsumer<Integer, Integer> force = (cx, cz) -> {
            if (ConflictKit.force(level, cx, cz, true))
                forced.add(chunk(cx, cz));
        };
        int[] tests = new int[2], moves = new int[2];
        long[] arrive = {0};
        long[] requeued = {0}, strays = {0}, pendingAt = {0};
        ConflictBridge.Stack[] st = new ConflictBridge.Stack[2];
        Mob[] out = new Mob[1];
        record(test, "released", released + " chunks an earlier run left forced");

        helper.startSequence()
                .thenExecute(() -> ConflictKit.materialize(band, List.of(
                        new ConflictKit.StackSpec(WARRIOR, 4, false), new ConflictKit.StackSpec(ARCHER, 4, false))))
                .thenWaitUntil(() -> check("BAND_MATERIALIZED", !about(ConflictRecords.BAND_MATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received m = about(ConflictRecords.BAND_MATERIALIZED, agent).get(0);
                    check("none placed now (" + m.aux() + ")", m.aux() == 0);
                    check("never rejected", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    ConflictBridge.Party p = party(agent);
                    check("two stacks", p.stacks().size() == 2);
                    for (int i = 0; i < 2; i++) {
                        st[i] = p.stacks().get(i);
                        check("stack " + i + " pending", st[i].state() == ConflictBridge.StackState.PENDING);
                        Set<Long> want = Set.of(chunk(k - 1, z0 + i), chunk(k - 2, z0 + i));
                        Set<Long> got = st[i].chunksNow(level.getGameTime());
                        check("stack " + i + " straddles " + chunks(want) + ": " + chunks(got), got.equals(want));
                        tests[i] = st[i].tests();
                        moves[i] = st[i].moves();
                    }
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++)
                        check("stack " + i + " was not tested while nothing changed (" + st[i].tests() + " vs " + tests[i] + ")",
                                st[i].tests() == tests[i] && st[i].moves() == moves[i]);
                    // Walk 16 blocks east: the front rows into chunk k, the rear rows into k - 1.
                    arrive[0] = level.getGameTime() + 100;
                    ConflictKit.steer(agent, 1, 0.8f, bx + 0.5, bz + 16, arrive[0], false);
                })
                .thenWaitUntil(() -> {
                    long now = level.getGameTime();
                    check("arrived", now > arrive[0] && bridge().steered > 0);
                    for (int i = 0; i < 2; i++) {
                        Set<Long> want = Set.of(chunk(k - 1, z0 + i), chunk(k, z0 + i));
                        check("stack " + i + " in " + chunks(want), st[i].chunksNow(now).equals(want));
                    }
                })
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        int dm = st[i].moves() - moves[i], dt = st[i].tests() - tests[i];
                        record(test, "stack " + i + " walking", dm + " moves into other chunks, " + dt + " tests");
                        check("stack " + i + " moved chunks twice (" + dm + ")", dm == 2);
                        check("and was tested once per move (" + dt + ")", dt == dm);
                        check("still pending", st[i].state() == ConflictBridge.StackState.PENDING);
                        tests[i] = st[i].tests();
                    }
                    force.accept(k, z0);
                    force.accept(k, z0 + 1);
                })
                .thenWaitUntil(io(() -> check("chunk k ticks entities in both rows",
                        RtsimSession.entitiesTickAt(level, new BlockPos(bx + 8, 64, bz + 8))
                                && RtsimSession.entitiesTickAt(level, new BlockPos(bx + 8, 64, bz + 24)))))
                .thenWaitUntil(io(() -> check("chunks k - 1 are loaded", level.hasChunk(k - 1, z0) && level.hasChunk(k - 1, z0 + 1))))
                .thenWaitUntil(() -> {
                    for (int i = 0; i < 2; i++)
                        check("stack " + i + " tested on its chunk's change", st[i].tests() == tests[i] + 1);
                })
                .thenIdle(10)
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++) {
                        check("stack " + i + " tested once (" + (st[i].tests() - tests[i]) + ")", st[i].tests() == tests[i] + 1);
                        check("and still pending: its rear rows do not tick", st[i].state() == ConflictBridge.StackState.PENDING);
                        tests[i] = st[i].tests();
                    }
                    check("never rejected", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    platform(level, bx - 4, bx + 3, bz + 11, bz + 21);
                    force.accept(k - 1, z0);
                })
                .thenWaitUntil(io(() -> {
                    check("stack 0 placed", st[0].state() == ConflictBridge.StackState.PLACED && st[0].bodies().size() == 4);
                    pin(st[0]);
                }))
                .thenExecute(() -> {
                    check("stack 1 still pending", st[1].state() == ConflictBridge.StackState.PENDING);
                    check("stack 1 not tested: none of its chunks changed (" + (st[1].tests() - tests[1]) + ")", st[1].tests() == tests[1]);
                    check("stack 0 placed once", st[0].placings() == 1);
                    for (Mob m : st[0].bodies())
                        check("a warrior is a vindicator", m.getType() == EntityType.VINDICATOR && RtsimApi.isPuppet(m));
                    force.accept(k - 1, z0 + 1);
                })
                .thenWaitUntil(io(() -> {
                    check("stack 1 placed", st[1].state() == ConflictBridge.StackState.PLACED && st[1].bodies().size() == 4);
                    pin(st[1]);
                }))
                .thenExecute(() -> {
                    for (Mob m : st[1].bodies())
                        check("an archer is a pillager", m.getType() == EntityType.PILLAGER);
                    check("never rejected", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    record(test, "placed", "stack 0 when chunk (" + (k - 1) + ", " + z0 + ") ticked, stack 1 when ("
                            + (k - 1) + ", " + (z0 + 1) + ") did; tests " + st[0].tests() + ", " + st[1].tests());
                    // A member strays (moved into chunk k - 2, loaded but not ticking entities) while its place ticks.
                    requeued[0] = bridge().requeued;
                    strays[0] = bridge().strays;
                    out[0] = st[0].bodies().get(0);
                    out[0].moveTo(bx - 19.5, out[0].getY(), out[0].getZ());
                })
                .thenWaitUntil(() -> check("the stray moved back", bridge().strays > strays[0]))
                .thenExecute(() -> {
                    record(test, "stray", "moved back to (" + out[0].getX() + ", " + out[0].getZ() + ")");
                    check("once (" + (bridge().strays - strays[0]) + ")", bridge().strays == strays[0] + 1);
                    check("into the ticking chunks", out[0].getX() > bx - 16 && !out[0].isRemoved());
                    check("the same body", st[0].bodies().contains(out[0]) && st[0].bodies().size() == 4);
                    check("stack 0 stayed placed, not requeued", st[0].state() == ConflictBridge.StackState.PLACED
                            && st[0].placings() == 1 && bridge().requeued == requeued[0]);
                    check("stack 1 stayed in", st[1].state() == ConflictBridge.StackState.PLACED && st[1].placings() == 1);
                    // Its rear chunk stops ticking entities: stack 1 goes back to pending.
                    ConflictKit.force(level, k - 1, z0 + 1, false);
                    forced.remove(chunk(k - 1, z0 + 1));
                })
                .thenWaitUntil(io(() -> check("stack 1 pending again", st[1].state() == ConflictBridge.StackState.PENDING)))
                .thenExecute(() -> {
                    pendingAt[0] = level.getGameTime();
                    check("requeued once", bridge().requeued == requeued[0] + 1);
                    check("its bodies are gone", st[1].bodies().isEmpty());
                    check("stack 0 stayed in", st[0].state() == ConflictBridge.StackState.PLACED && st[0].placings() == 1);
                    // The chunk ticks again at once; the stack waits out its cooldown.
                    force.accept(k - 1, z0 + 1);
                })
                .thenWaitUntil(io(() -> check("its chunk ticks again", RtsimSession.entitiesTickAt(level, new BlockPos(bx - 8, 64, bz + 24)))))
                .thenIdle(ConflictBridge.REQUEUE_COOLDOWN / 2)
                .thenExecute(() -> check("not back within the cooldown", st[1].state() == ConflictBridge.StackState.PENDING
                        && st[1].placings() == 1))
                .thenWaitUntil(() -> check("stack 1 placed again", st[1].state() == ConflictBridge.StackState.PLACED))
                .thenExecute(() -> {
                    long after = level.getGameTime() - pendingAt[0];
                    pin(st[1]);
                    record(test, "requeued", "stack 1 back " + after + " ticks after it went (cooldown " + ConflictBridge.REQUEUE_COOLDOWN + ")");
                    check("after the cooldown (" + after + ")", after >= ConflictBridge.REQUEUE_COOLDOWN - 1);
                    check("placed twice, all of it", st[1].placings() == 2 && st[1].bodies().size() == 4);
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 0, bx - 200, bz + 16);
                })
                .thenWaitUntil(() -> check("BAND_DEMATERIALIZED", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = about(ConflictRecords.BAND_DEMATERIALIZED, agent).get(0);
                    Map<String, Integer> s = ConflictKit.survivors(d);
                    record(test, "recalled", "survivors " + s);
                    check("reason: the command", d.aux() == ConflictRecords.Dematerialized.COMMAND);
                    check("every living member, placed or pending, survives: " + s, s.equals(Map.of(WARRIOR, 4, ARCHER, 4)));
                    check("the party is gone", bridge().party(agent) == null);
                    check("and its bodies", st[0].bodies().isEmpty() && st[1].bodies().isEmpty());
                    check("never rejected", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    unforce.run();
                })
                .thenSucceed();
    }

    /** A stone floor over the area at its highest surface, three blocks of air above (no water, no trees under a body). */
    private static void platform(ServerLevel level, int x0, int x1, int z0, int z1) {
        int top = level.getMinBuildHeight();
        for (int x = x0; x <= x1; x++)
            for (int z = z0; z <= z1; z++)
                top = Math.max(top, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z));
        for (int x = x0; x <= x1; x++)
            for (int z = z0; z <= z1; z++) {
                level.setBlock(new BlockPos(x, top, z), Blocks.STONE.defaultBlockState(), 2);
                for (int y = 1; y <= 3; y++)
                    level.setBlock(new BlockPos(x, top + y, z), Blocks.AIR.defaultBlockState(), 2);
            }
    }

    // ---- a raid party: steer, losses, loot, recall ---------------------------------------------

    /**
     * A raid band of two stacks in the arena (ledger 25 with every chunk ticking): each stack is placed
     * at once, the war chief as an extra body at the rear of its stack (MZB-109), every body a tagged,
     * persistent stand-in. BAND_STEER moves every stack's place; a member far from its place walks back
     * to it (MZB-106). Each loss is journaled with who did it: a player's kill (BAND_CASUALTIES kind 1,
     * the player, the body), a death with no killer (0), a mob's kill (2), a removal by something else
     * (3, no killer), and the chief killed by a player (CHIEF_KILLED kind 1 with a Java op id); the drops
     * carry their record. BAND_LOOTED has a Java op id. BAND_STATE counts the bodies present. A recall
     * keeps the bodies while a player can see them, and ends the party (survivors) once none can.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 1500, batch = "conflict_raid", skyAccess = true)
    public static void aRaidPartyIsSteeredLosesMembersAndIsRecalled(GameTestHelper helper) {
        String test = "aRaidPartyIsSteeredLosesMembersAndIsRecalled";
        ServerLevel level = helper.getLevel();
        Vec3 c = RtsimTests.centre(helper);
        long agent = ConflictKit.newAgent();
        ConflictKit.Band band = ConflictKit.band(agent, ConflictKit.newFaction(), RtsimTests.dimension(helper));
        band.arcX = c.x + 0.5;
        band.arcZ = c.z;
        band.targetX = c.x + 100;
        band.targetY = c.y;
        band.targetZ = c.z;
        band.flags = ConflictRecords.Materialize.FLAG_HOSTILE | ConflictRecords.Materialize.FLAG_CHIEF;
        ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(WARRIOR, 3, true), new ConflictKit.StackSpec(ARCHER, 3, false)));
        ServerPlayer[] player = new ServerPlayer[1];
        ConflictKit.onEnd(helper, () -> {
            if (player[0] != null)
                FakePlayers.remove(player[0]);
        });
        ConflictBridge.Stack[] st = new ConflictBridge.Stack[2];
        Mob[] strayed = new Mob[1];
        long[] arrive = {0};
        long[] statesBefore = {0};
        long[] recalledAt = {0};

        helper.startSequence()
                .thenWaitUntil(() -> {
                    ConflictBridge.Party p = party(agent);
                    check("two stacks", p.stacks().size() == 2);
                    for (ConflictBridge.Stack s : p.stacks())
                        check("stack " + s.index + " placed", s.state() == ConflictBridge.StackState.PLACED);
                    // Journaled at the placing; it reaches received() at the next hand-over.
                    check("BAND_MATERIALIZED", !about(ConflictRecords.BAND_MATERIALIZED, agent).isEmpty());
                })
                .thenExecute(() -> {
                    ConflictBridge.Party p = party(agent);
                    st[0] = p.stacks().get(0);
                    st[1] = p.stacks().get(1);
                    ScriptedEngine.Received m = about(ConflictRecords.BAND_MATERIALIZED, agent).get(0);
                    check("BAND_MATERIALIZED counts every body placed, the chief too (" + m.aux() + ")", m.aux() == 7);
                    check("4 and 3 bodies", st[0].bodies().size() == 4 && st[1].bodies().size() == 3);
                    int chiefs = 0;
                    for (ConflictBridge.Slot s : st[0].slots())
                        if (s.chief) {
                            chiefs++;
                            for (ConflictBridge.Slot o : st[0].slots())
                                check("the chief walks at the rear", o.chief || o.entity().getX() > s.entity().getX());
                        }
                    check("one chief, in stack 0", chiefs == 1);
                    for (ConflictBridge.Slot s : st[1].slots())
                        check("no chief in stack 1", !s.chief);
                    for (ConflictBridge.Stack s : st)
                        for (ConflictBridge.Slot sl : s.slots()) {
                            Mob b = sl.entity();
                            check("a tagged puppet", RtsimApi.isPuppet(b) && b.isPersistenceRequired());
                            check("at its place (" + sl.lag() + ")", sl.lag() < 1.0);
                        }
                    player[0] = FakePlayers.add(level, c.add(0, 0, 0), "conflict_raid_watcher");
                    player[0].setGameMode(GameType.CREATIVE);
                    arrive[0] = level.getGameTime() + 40;
                    ConflictKit.steer(agent, 1, 0.1f, c.x + 2.5, c.z, arrive[0], false);
                })
                .thenWaitUntil(() -> check("steered", st[0].target()[0] == c.x + 2.5 && st[1].target()[0] == c.x + 2.5))
                .thenExecute(() -> {
                    check("each stack keeps its side", st[0].target()[1] == c.z - 2.0 && st[1].target()[1] == c.z + 2.0);
                    // A warrior pushed to the far corner, more than LEASH blocks from its place.
                    strayed[0] = st[0].slots().get(0).entity();
                    strayed[0].moveTo(c.x - 4.0, strayed[0].getY(), c.z + 4.0);
                })
                .thenWaitUntil(() -> {
                    check("arrived", level.getGameTime() > arrive[0]);
                    for (ConflictBridge.Stack s : st)
                        for (ConflictBridge.Slot sl : s.slots())
                            check("a member near its place (" + sl.lag() + ")", sl.lag() <= ConflictBridge.LEASH);
                })
                .thenExecute(() -> {
                    record(test, "steered", "the strayed warrior walked back to " + String.format("%.1f", st[0].slots().get(0).lag())
                            + " blocks from its place");
                    check("the strayed warrior is still placed, not requeued", st[0].placings() == 1 && st[0].slots().get(0).entity() == strayed[0]);
                    // Every kind of loss.
                    Mob archerByPlayer = st[1].slots().get(0).entity();
                    Mob archerRemoved = st[1].slots().get(1).entity();
                    Mob warriorNone = st[0].slots().get(0).entity();
                    Mob warriorByMob = st[0].slots().get(1).entity();
                    Mob chief = null;
                    for (ConflictBridge.Slot sl : st[0].slots())
                        if (sl.chief)
                            chief = sl.entity();
                    archerByPlayer.setDropChance(EquipmentSlot.MAINHAND, 2.0f); // always drops its crossbow
                    archerByPlayer.hurt(level.damageSources().playerAttack(player[0]), 1000f);
                    archerRemoved.discard();
                    warriorNone.hurt(level.damageSources().generic(), 1000f);
                    IronGolem golem = EntityType.IRON_GOLEM.create(level);
                    warriorByMob.hurt(level.damageSources().mobAttack(golem), 1000f);
                    golem.discard();
                    chief.hurt(level.damageSources().playerAttack(player[0]), 1000f);
                })
                .thenWaitUntil(() -> check("four casualties and the chief", about(ConflictRecords.BAND_CASUALTIES, agent).size() == 4
                        && about(ConflictRecords.CHIEF_KILLED, agent).size() == 1))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> cs = about(ConflictRecords.BAND_CASUALTIES, agent);
                    int archer = ConflictKit.bodyId(ARCHER), warrior = ConflictKit.bodyId(WARRIOR);
                    ScriptedEngine.Received byPlayer = first(cs, ConflictRecords.BY_PLAYER), byNone = first(cs, ConflictRecords.BY_NONE),
                            byMob = first(cs, ConflictRecords.BY_MOB), byOther = first(cs, ConflictRecords.BY_OTHER);
                    check("one of each killer kind", byPlayer != null && byNone != null && byMob != null && byOther != null);
                    check("a player's kill names the player", player[0].getUUID().equals(ConflictKit.uuidAt(byPlayer, ConflictRecords.Casualties.KILLER_MOST)));
                    check("of an archer", byPlayer.view().getInt(ConflictRecords.Casualties.BODY) == archer
                            && byPlayer.view().getInt(ConflictRecords.Casualties.COUNT) == 1);
                    check("a removal names no one, an archer", ConflictKit.uuidAt(byOther, ConflictRecords.Casualties.KILLER_MOST) == null
                            && byOther.view().getInt(ConflictRecords.Casualties.BODY) == archer);
                    check("a death with no killer names no one, a warrior", ConflictKit.uuidAt(byNone, ConflictRecords.Casualties.KILLER_MOST) == null
                            && byNone.view().getInt(ConflictRecords.Casualties.BODY) == warrior);
                    check("a mob's kill names no one, a warrior", ConflictKit.uuidAt(byMob, ConflictRecords.Casualties.KILLER_MOST) == null
                            && byMob.view().getInt(ConflictRecords.Casualties.BODY) == warrior);
                    ScriptedEngine.Received ck = about(ConflictRecords.CHIEF_KILLED, agent).get(0);
                    check("the chief killed by the player", ck.aux() == ConflictRecords.BY_PLAYER
                            && player[0].getUUID().equals(ConflictKit.uuidAt(ck, ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("with a Java op id", (ck.view().getLong(ConflictRecords.ChiefKilled.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    int carrying = 0;
                    for (ItemEntity drop : level.getEntitiesOfClass(ItemEntity.class, new AABB(BlockPos.containing(c)).inflate(12)))
                        if (drop.hasData(RtsimAttachments.PENDING))
                            carrying++;
                    check("the drops carry their record (" + carrying + ")", carrying >= 1);
                    check("one body left in each stack", st[0].bodies().size() == 1 && st[1].bodies().size() == 1);
                    check("both still placed", st[0].state() == ConflictBridge.StackState.PLACED && st[1].state() == ConflictBridge.StackState.PLACED);
                    record(test, "losses", "kinds 1, 0, 2, 3 and the chief by " + player[0].getUUID() + "; " + carrying + " drops carry their record");
                    // Loot: what a member carried off.
                    Mob looter = st[0].bodies().get(0);
                    check("loot from a member", bridge().looted(looter, List.of(new ItemStack(Items.BREAD, 3), ItemStack.EMPTY)));
                    check("none from anything else", !bridge().looted(player[0], List.of(new ItemStack(Items.BREAD, 3))));
                    statesBefore[0] = ConflictKit.states(agent).size();
                })
                .thenWaitUntil(() -> check("BAND_LOOTED", about(ConflictRecords.BAND_LOOTED, agent).size() == 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received l = about(ConflictRecords.BAND_LOOTED, agent).get(0);
                    check("one item", l.aux() == 1);
                    check("3 bread", l.view().getInt(ConflictRecords.Looted.ITEMS + ConflictRecords.I_ITEM) == RtsimTests.session().itemId(Items.BREAD)
                            && l.view().getInt(ConflictRecords.Looted.ITEMS + ConflictRecords.I_COUNT) == 3);
                    check("with a Java op id", (l.view().getLong(ConflictRecords.Looted.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                })
                .thenWaitUntil(() -> check("a later BAND_STATE", ConflictKit.states(agent).size() > statesBefore[0]))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> s = ConflictKit.states(agent);
                    ScriptedEngine.Received last = s.get(s.size() - 1);
                    check("two present (" + last.view().getInt(ConflictRecords.BandState.PRESENT) + ")",
                            last.view().getInt(ConflictRecords.BandState.PRESENT) == 2);
                    check("ticking", (last.aux() & ConflictRecords.BandState.FLAG_TICKING) != 0);
                    recalledAt[0] = level.getGameTime();
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_ROUTED, 400, c.x - 100, c.z);
                })
                .thenWaitUntil(() -> check("recalling", party(agent).recalling()))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("a player sees them: the bodies stay", bridge().party(agent) != null && bridge().bodies(agent).size() == 2);
                    check("not ended yet", about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty());
                    FakePlayers.remove(player[0]); // no one sees them now
                })
                .thenWaitUntil(() -> check("BAND_DEMATERIALIZED", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = about(ConflictRecords.BAND_DEMATERIALIZED, agent).get(0);
                    Map<String, Integer> s = ConflictKit.survivors(d);
                    record(test, "recalled", "after " + (RtsimTests.gameTimeOf(d) - recalledAt[0]) + " ticks, survivors " + s);
                    check("before the timeout", RtsimTests.gameTimeOf(d) - recalledAt[0] < 400);
                    check("the two left, never the chief: " + s, s.equals(Map.of(WARRIOR, 1, ARCHER, 1)));
                    check("their bodies are gone", st[0].bodies().isEmpty() && st[1].bodies().isEmpty() && bridge().party(agent) == null);
                })
                .thenSucceed();
    }

    /**
     * A chief-flagged stack with no units left (the engine's band lost them, its war chief still walks) is
     * the chief alone: a stack of one body, placed and counted in BAND_MATERIALIZED, where the chief once
     * got no body at all. The stack after it keeps its own index and members, and a recall reports them
     * as survivors, never the chief.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "conflict_chief", skyAccess = true)
    public static void aChiefWhoseStackHasNoUnitsLeftStillComes(GameTestHelper helper) {
        String test = "aChiefWhoseStackHasNoUnitsLeftStillComes";
        Vec3 c = RtsimTests.centre(helper);
        long agent = ConflictKit.newAgent();
        ConflictKit.Band band = ConflictKit.band(agent, ConflictKit.newFaction(), RtsimTests.dimension(helper));
        band.arcX = c.x + 0.5;
        band.arcZ = c.z;
        band.targetX = c.x + 100;
        band.targetY = c.y;
        band.targetZ = c.z;
        band.speed = 0f;
        band.flags = ConflictRecords.Materialize.FLAG_HOSTILE | ConflictRecords.Materialize.FLAG_CHIEF;
        ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(ARCHER, 2, false), new ConflictKit.StackSpec(WARRIOR, 0, true)));
        helper.startSequence()
                .thenWaitUntil(() -> check("BAND_MATERIALIZED", !about(ConflictRecords.BAND_MATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    ConflictBridge.Party p = party(agent);
                    ScriptedEngine.Received m = about(ConflictRecords.BAND_MATERIALIZED, agent).get(0);
                    record(test, "materialized", m.aux() + " bodies in " + p.stacks().size() + " stacks");
                    check("two stacks (" + p.stacks().size() + ")", p.stacks().size() == 2);
                    ConflictBridge.Stack archers = p.stacks().get(0), chief = p.stacks().get(1);
                    check("the archers first, two of them", archers.slots().size() == 2 && !archers.slots().get(0).chief
                            && archers.state() == ConflictBridge.StackState.PLACED && archers.bodies().size() == 2);
                    check("then the chief alone", chief.slots().size() == 1 && chief.slots().get(0).chief);
                    check("placed, with a body", chief.state() == ConflictBridge.StackState.PLACED && chief.bodies().size() == 1
                            && chief.bodies().get(0).getType() == EntityType.VINDICATOR && RtsimApi.isPuppet(chief.bodies().get(0)));
                    check("every body counted, the chief too (" + m.aux() + ")", m.aux() == 3);
                    check("never rejected", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 1, c.x - 100, c.z);
                })
                .thenWaitUntil(() -> check("BAND_DEMATERIALIZED", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    Map<String, Integer> s = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, agent).get(0));
                    record(test, "recalled", "survivors " + s);
                    check("the archers, never the chief: " + s, s.equals(Map.of(ARCHER, 2)));
                    check("the party is gone", bridge().party(agent) == null);
                })
                .thenSucceed();
    }

    // ---- test 14: the envoy ----------------------------------------------------------------------

    /**
     * Test 14 (MOWZIE.md §8, §4.5): an envoy party is one group, its envoy named "<Culture> Envoy" and no
     * more (MZB-101: its demand is said only to the player who clicks). While the demand is open nothing
     * targets the party and it targets nothing: an iron golem's target and a guard's are refused, and a
     * golem's blow, an arrow with no owner (the stand-in for a Create Big Cannons shell) and a skeleton's
     * arrow do no damage. No click refuses (MZB-110, MZB-122). A click holding something else is an
     * ordinary click. The player it came to pays by clicking with the good in either hand (TRIBUTE_PAID with
     * a Java op id each): from the off hand when the main hand is empty, and through vanilla's off-hand
     * click when the main hand holds something else; the envoy says what is still owed, and the off-hand
     * click after an answered main-hand click does nothing. A plain click with empty hands shows the
     * demand (the same line as talk, to that player only); crouching with an empty hand talks (TALK): the
     * reason, what is still owed and the deadline; a guard warns with the reason (MZB-121, MZB-124), and
     * makes small talk only with a bystander the camp has no reason against (MZB-130). A
     * settlers' envoy only shows its demand: it takes nothing from a player. Past its deadline an envoy
     * takes nothing and says so. A player's arrow hurts a guard and is an insult (ENVOY_ANSWER 3, once):
     * the guard is lifted, and the struck envoy takes nothing and says so. ENVOY_ANSWER 2 is never sent.
     * Another envoy party (the settlers') killed by {@code /kill} sends no ENVOY_ANSWER, only BAND_CASUALTIES
     * with no killer, and its demand is withdrawn (DEMAND_SETTLED 5, scripted as M1's engine sends it, with
     * the envoy still standing as MZB-32 allows; MZB-128). DEMAND_SETTLED takes the name away, and the envoy
     * then takes nothing and says the settlement (withdrawn, paid). A recall keeps the bodies while a player
     * sees them, until its timeout.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 2400, batch = "conflict_envoy", skyAccess = true)
    public static void anEnvoyPartyIgnoresGolemsAndPlayersUntilStruckAndTakesPayment(GameTestHelper helper) {
        String test = "anEnvoyPartyIgnoresGolemsAndPlayersUntilStruckAndTakesPayment";
        ServerLevel level = helper.getLevel();
        String dim = RtsimTests.dimension(helper);
        Vec3 c = RtsimTests.centre(helper);
        long a = ConflictKit.newAgent(), b = ConflictKit.newAgent(), late = ConflictKit.newAgent();
        long faction = ConflictKit.newFaction();
        FakePlayers.Hearer player = FakePlayers.hearing(level, c.add(3, 0, 0), "conflict_envoy_target");
        ConflictKit.onEnd(helper, () -> FakePlayers.remove(player));
        int demand = ConflictRecords.Envoy.DEMAND_TRIBUTE | ConflictRecords.Envoy.DEMAND_STOP;
        long deadline = level.getGameTime() + 24_000, lateDeadline = level.getGameTime() + 200;
        ConflictKit.envoy(a, 1, faction, dim, c.x - 1.5, c.y, c.z, player.getUUID(), demand, ConflictRecords.SIGNAL_MINING,
                "minecraft:emerald", 6, deadline, List.of(new ConflictKit.MemberSpec(EMISSARY, 1), new ConflictKit.MemberSpec(WARRIOR, 2)));
        ConflictKit.envoy(b, 1, faction, dim, c.x + 1.0, c.y, c.z - 3.5, null, ConflictRecords.Envoy.DEMAND_TRIBUTE, 0,
                "minecraft:emerald", 1, deadline, List.of(new ConflictKit.MemberSpec(EMISSARY, 1), new ConflictKit.MemberSpec(WARRIOR, 1)));
        // Party "late": the player's, with a deadline that passes during the test.
        ConflictKit.envoy(late, 1, faction, dim, c.x - 2.5, c.y, c.z - 3.5, player.getUUID(), ConflictRecords.Envoy.DEMAND_TRIBUTE,
                ConflictRecords.SIGNAL_MINING, "minecraft:emerald", 2, lateDeadline, List.of(new ConflictKit.MemberSpec(EMISSARY, 1)));
        Mob[] envoy = new Mob[1], guard = new Mob[1], envoyB = new Mob[1], guardB = new Mob[1], envoyLate = new Mob[1];
        FakePlayers.Hearer[] bystanderHolder = new FakePlayers.Hearer[1];
        int[] heardByHolder = new int[1];
        IronGolem[] golem = new IronGolem[1];
        long[] blockedHits = {0}, blockedTargets = {0}, recalledAt = {0}, saidA = {0}, saidB = {0};
        float[] health = {0};
        java.util.function.BooleanSupplier aMayTalk = () -> level.getGameTime() - saidA[0] >= RateLimit.PAIR;
        java.util.function.BooleanSupplier bMayTalk = () -> level.getGameTime() - saidB[0] >= RateLimit.PAIR;

        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("party A placed", party(a).stacks().size() == 1 && party(a).stacks().get(0).bodies().size() == 3);
                    check("party B placed", party(b).stacks().size() == 1 && party(b).stacks().get(0).bodies().size() == 2);
                    check("the late party placed", party(late).stacks().size() == 1 && party(late).stacks().get(0).bodies().size() == 1);
                    // Journaled at the placing; it reaches received() at the next hand-over.
                    check("BAND_MATERIALIZED", !about(ConflictRecords.BAND_MATERIALIZED, a).isEmpty());
                })
                .thenExecute(() -> {
                    check("one group: one stack", party(a).stacks().get(0).state() == ConflictBridge.StackState.PLACED);
                    check("BAND_MATERIALIZED with all three", about(ConflictRecords.BAND_MATERIALIZED, a).get(0).aux() == 3);
                    for (ConflictBridge.Slot sl : party(a).stacks().get(0).slots()) {
                        if (sl.envoy)
                            envoy[0] = sl.entity();
                        else if (guard[0] == null)
                            guard[0] = sl.entity();
                    }
                    for (ConflictBridge.Slot sl : party(b).stacks().get(0).slots()) {
                        if (sl.envoy)
                            envoyB[0] = sl.entity();
                        else
                            guardB[0] = sl.entity();
                    }
                    envoyLate[0] = party(late).stacks().get(0).bodies().get(0);
                    check("the envoy is a pillager, its guards vindicators", envoy[0].getType() == EntityType.PILLAGER
                            && guard[0].getType() == EntityType.VINDICATOR);
                    String n = name(envoy[0]);
                    record(test, "envoy name", n + " / " + name(envoyB[0]));
                    check("the envoy is named by its culture, no demand: " + n, envoyNamed(envoy[0]) && "Goblin Envoy".equals(n)
                            && envoy[0].isCustomNameVisible());
                    check("party B's too: " + name(envoyB[0]), envoyNamed(envoyB[0]) && "Goblin Envoy".equals(name(envoyB[0])));
                    check("only the envoy is named", guard[0].getCustomName() == null);
                    check("both guarded", party(a).guarded() && party(b).guarded());
                    for (Mob m : party(a).stacks().get(0).bodies())
                        check("no target goals while guarded", m.targetSelector.getAvailableGoals().isEmpty());
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    for (Mob m : bridge().bodies(a))
                        check("a survival player 4 blocks off is not targeted", m.getTarget() == null);
                    blockedTargets[0] = bridge().blockedTargets;
                    blockedHits[0] = bridge().blockedHits;
                    golem[0] = EntityType.IRON_GOLEM.create(level);
                    golem[0].moveTo(c.x, c.y, c.z + 3.0);
                    level.addFreshEntity(golem[0]);
                    golem[0].setTarget(guard[0]);
                    check("the golem may not target a guard", golem[0].getTarget() == null);
                    guard[0].setTarget(player);
                    check("a guard may not target the player", guard[0].getTarget() == null);
                    check("both refusals counted", bridge().blockedTargets >= blockedTargets[0] + 2);
                    health[0] = guard[0].getHealth();
                    golem[0].doHurtTarget(guard[0]);
                    Arrow arrow = EntityType.ARROW.create(level);
                    guard[0].hurt(level.damageSources().arrow(arrow, null), 4f);
                    Skeleton skeleton = EntityType.SKELETON.create(level);
                    guard[0].hurt(level.damageSources().arrow(arrow, skeleton), 4f);
                    envoy[0].hurt(level.damageSources().mobAttack(golem[0]), 4f);
                    arrow.discard();
                    skeleton.discard();
                    check("no damage (" + guard[0].getHealth() + " of " + health[0] + ")", guard[0].getHealth() == health[0]
                            && envoy[0].getHealth() == envoy[0].getMaxHealth());
                    check("four blocked hits (" + (bridge().blockedHits - blockedHits[0]) + ")", bridge().blockedHits == blockedHits[0] + 4);
                    check("no answer for any of it", about(ConflictRecords.ENVOY_ANSWER, a).isEmpty());
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the golem never got a guarded target", golem[0].getTarget() == null);
                    record(test, "guard", (bridge().blockedHits - blockedHits[0]) + " hits and " + (bridge().blockedTargets - blockedTargets[0])
                            + " targets refused");
                    long answered = bridge().answers;
                    int heard = player.heard.size();
                    // Something else in hand: an ordinary click; nothing taken, nothing said.
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.DIRT, 4));
                    click(player, envoy[0]);
                    check("dirt pays nothing, says nothing", party(a).demand().remaining() == 6 && player.getMainHandItem().getCount() == 4
                            && player.heard.size() == heard);
                    // Part of the tribute: 3 of 6 emeralds in the main hand. The envoy says what is still owed.
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 3));
                    click(player, envoy[0]);
                    check("3 paid, 3 owed", party(a).demand().remaining() == 3 && player.getMainHandItem().isEmpty());
                    check("the envoy spoke once: " + talkKeys(player), player.heard.size() == heard + 1);
                    TranslatableContents owed = spoken(player.heard.get(heard));
                    record(test, "after paying", owed.getKey() + " " + player.heard.get(heard).getString());
                    check("what is still owed: " + owed.getKey(), owed.getKey().equals("mic_rtsim.talk.envoy.envoy.demand.tribute_or_stop.0")
                            && owed.getArgs()[1] instanceof Integer o && o == 3);
                    check("the name stays: " + name(envoy[0]), "Goblin Envoy".equals(name(envoy[0])));
                    saidA[0] = level.getGameTime();
                    // A guard warns, with the camp's reason (TALK: crouching with an empty hand).
                    player.setShiftKeyDown(true);
                    player.interactOn(guard[0], InteractionHand.MAIN_HAND);
                    player.setShiftKeyDown(false);
                    check("the guard spoke once: " + talkKeys(player), player.heard.size() == heard + 2);
                    TranslatableContents warn = spoken(player.heard.get(heard + 1));
                    record(test, "guard line", warn.getKey() + " " + player.heard.get(heard + 1).getString());
                    check("a warning with the reason: " + warn.getKey(), warn.getKey().startsWith("mic_rtsim.talk.")
                            && warn.getKey().contains(".hostile.warn.mining."));
                    check("no answer", bridge().answers == answered);
                })
                .thenWaitUntil(() -> check("the envoy may talk again", aMayTalk.getAsBoolean()))
                .thenExecute(() -> {
                    // A plain click with empty hands shows the demand; a second click in the same tick is
                    // TALK's rate limit's. Neither refuses.
                    long answered = bridge().answers, shown = bridge().demandsShown;
                    int heard = player.heard.size();
                    click(player, envoy[0]);
                    click(player, envoy[0]);
                    check("the demand, shown once: " + talkKeys(player), player.heard.size() == heard + 1 && bridge().demandsShown == shown + 1);
                    TranslatableContents line = spoken(player.heard.get(heard));
                    check("the demand: " + line.getKey(), line.getKey().equals("mic_rtsim.talk.envoy.envoy.demand.tribute_or_stop.0"));
                    check("no click refuses", bridge().answers == answered);
                    saidA[0] = level.getGameTime();
                })
                .thenWaitUntil(() -> check("the envoy may talk again", aMayTalk.getAsBoolean()))
                .thenExecute(() -> {
                    // TALK: crouching with an empty hand says the same line, through TALK's own path.
                    long answered = bridge().answers, shown = bridge().demandsShown;
                    int heard = player.heard.size();
                    player.setShiftKeyDown(true);
                    player.interactOn(envoy[0], InteractionHand.MAIN_HAND);
                    player.setShiftKeyDown(false);
                    check("the envoy spoke once: " + talkKeys(player), player.heard.size() == heard + 1);
                    check("through TALK, not the bridge", bridge().demandsShown == shown);
                    TranslatableContents line = spoken(player.heard.get(heard));
                    record(test, "envoy line", line.getKey() + " " + player.heard.get(heard).getString());
                    check("its demand: " + line.getKey(), line.getKey().equals("mic_rtsim.talk.envoy.envoy.demand.tribute_or_stop.0"));
                    Object[] args = line.getArgs();
                    check("the reason: " + args[0], args[0] instanceof Component r && r.getContents() instanceof TranslatableContents rt
                            && rt.getKey().equals("mic_rtsim.talk.envoy.envoy.reason.mining.0"));
                    check("3 still owed: " + args[1], args[1] instanceof Integer n && n == 3);
                    check("emeralds: " + args[2], args[2] instanceof Component it && it.getString().equals(Items.EMERALD.getDescription().getString()));
                    check("the deadline: " + args[4], args[4] instanceof Component dl && dl.getContents() instanceof TranslatableContents dt
                            && dt.getKey().startsWith("mic_rtsim.talk.deadline.") && !dt.getKey().equals("mic_rtsim.talk.deadline.now"));
                    check("not a refusal", bridge().answers == answered);
                    saidA[0] = level.getGameTime();
                })
                .thenWaitUntil(() -> check("the envoy may talk again", aMayTalk.getAsBoolean()))
                .thenExecute(() -> {
                    int heard = player.heard.size();
                    // The good in the off hand pays when the main hand is empty.
                    player.setItemInHand(InteractionHand.OFF_HAND, new ItemStack(Items.EMERALD, 1));
                    click(player, envoy[0]);
                    check("1 paid from the off hand, 2 owed", party(a).demand().remaining() == 2 && player.getOffhandItem().isEmpty());
                    check("and the envoy says so: " + talkKeys(player), player.heard.size() == heard + 1
                            && spoken(player.heard.get(heard)).getArgs()[1] instanceof Integer o && o == 2);
                    // Something else in the main hand: vanilla's off-hand click pays.
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.DIRT, 1));
                    player.setItemInHand(InteractionHand.OFF_HAND, new ItemStack(Items.EMERALD, 1));
                    player.interactOn(envoy[0], InteractionHand.MAIN_HAND);
                    check("the main-hand click took nothing", party(a).demand().remaining() == 2);
                    player.interactOn(envoy[0], InteractionHand.OFF_HAND);
                    check("the off-hand click paid 1, 1 owed", party(a).demand().remaining() == 1 && player.getOffhandItem().isEmpty()
                            && player.getMainHandItem().is(Items.DIRT));
                    player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    saidA[0] = level.getGameTime();
                    // A settlers' envoy only shows its demand: it takes nothing from a player.
                    int heardB = player.heard.size();
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 2));
                    click(player, envoyB[0]);
                    check("B took nothing", party(b).demand().remaining() == 1 && player.getMainHandItem().getCount() == 2);
                    check("B showed its demand: " + talkKeys(player), player.heard.size() == heardB + 1
                            && spoken(player.heard.get(heardB)).getKey().equals("mic_rtsim.talk.envoy.envoy.demand.tribute.0"));
                    saidB[0] = level.getGameTime();
                    player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                })
                .thenWaitUntil(() -> check("the late party's deadline passed", level.getGameTime() >= lateDeadline))
                .thenExecute(() -> {
                    long refused = bridge().paymentsRefused;
                    int heard = player.heard.size();
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 2));
                    click(player, envoyLate[0]);
                    check("past its deadline it took nothing", party(late).demand().remaining() == 2 && player.getMainHandItem().getCount() == 2
                            && bridge().paymentsRefused == refused + 1);
                    check("and said so: " + talkKeys(player), player.heard.size() == heard + 1
                            && spoken(player.heard.get(heard)).getKey().equals("mic_rtsim.talk.envoy.envoy.settled.expired.0"));
                    player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    // A player's arrow: it hurts, and it is an insult.
                    golem[0].discard();
                    player.setGameMode(GameType.CREATIVE);
                    health[0] = guard[0].getHealth();
                    Arrow arrow = EntityType.ARROW.create(level);
                    guard[0].hurt(level.damageSources().arrow(arrow, player), 2f);
                    arrow.discard();
                    check("the player's arrow hurt (" + guard[0].getHealth() + " of " + health[0] + ")", guard[0].getHealth() < health[0]);
                    check("the guard is lifted", !party(a).guarded());
                    for (Mob m : bridge().bodies(a))
                        check("their target goals are back", !m.targetSelector.getAvailableGoals().isEmpty());
                    envoy[0].hurt(level.damageSources().playerAttack(player), 1f);
                    check("party B still guarded", party(b).guarded());
                })
                .thenWaitUntil(() -> check("the envoy may talk again", aMayTalk.getAsBoolean()))
                .thenExecute(() -> {
                    long refused = bridge().paymentsRefused;
                    int heard = player.heard.size();
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 1));
                    click(player, envoy[0]);
                    check("struck, the envoy took nothing", party(a).demand().remaining() == 1 && player.getMainHandItem().getCount() == 1
                            && bridge().paymentsRefused == refused + 1);
                    check("and said so: " + talkKeys(player), player.heard.size() == heard + 1
                            && spoken(player.heard.get(heard)).getKey().equals("mic_rtsim.talk.envoy.envoy.settled.struck.0"));
                    player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    saidA[0] = level.getGameTime();
                    // Party B's guard killed by /kill: no answer, a casualty with no killer.
                    guardB[0].kill();
                })
                .thenWaitUntil(() -> check("B's casualty", about(ConflictRecords.BAND_CASUALTIES, b).size() == 1))
                .thenWaitUntil(() -> check("TRIBUTE_PAID three times", ConflictKit.tributes(a).size() == 3))
                .thenExecute(() -> {
                    check("one insult (" + ConflictKit.answers(a, ConflictRecords.ANSWER_ATTACKED) + ")", ConflictKit.answers(a, ConflictRecords.ANSWER_ATTACKED) == 1);
                    ScriptedEngine.Received insult = first(about(ConflictRecords.ENVOY_ANSWER, a), ConflictRecords.ANSWER_ATTACKED);
                    check("by the player", player.getUUID().equals(ConflictKit.uuidAt(insult, ConflictRecords.EnvoyAnswer.PLAYER_MOST)));
                    for (long x : new long[]{a, b, late})
                        check("ENVOY_ANSWER 2 is never sent", ConflictKit.answers(x, ConflictRecords.ANSWER_REFUSED) == 0);
                    ScriptedEngine.Received cb = about(ConflictRecords.BAND_CASUALTIES, b).get(0);
                    check("/kill: no killer", cb.aux() == ConflictRecords.BY_NONE && ConflictKit.uuidAt(cb, ConflictRecords.Casualties.KILLER_MOST) == null);
                    check("the guard", cb.view().getInt(ConflictRecords.Casualties.BODY) == ConflictKit.bodyId(WARRIOR));
                    check("and no ENVOY_ANSWER from B", about(ConflictRecords.ENVOY_ANSWER, b).isEmpty());
                    List<ScriptedEngine.Received> ts = ConflictKit.tributes(a);
                    int[] counts = {3, 1, 1};
                    for (int i = 0; i < 3; i++) {
                        ScriptedEngine.Received t = ts.get(i);
                        check("one item", t.aux() == 1);
                        check(counts[i] + " emeralds", t.view().getInt(ConflictRecords.TributePaid.ITEMS + ConflictRecords.I_ITEM) == RtsimTests.session().itemId(Items.EMERALD)
                                && t.view().getInt(ConflictRecords.TributePaid.ITEMS + ConflictRecords.I_COUNT) == counts[i]);
                        check("from the player", player.getUUID().equals(ConflictKit.uuidAt(t, ConflictRecords.TributePaid.PLAYER_MOST)));
                        check("to the faction", t.view().getLong(ConflictRecords.TributePaid.FACTION) == faction);
                        check("with a Java op id", (t.view().getLong(ConflictRecords.TributePaid.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    }
                    check("none to B or the late party", ConflictKit.tributes(b).isEmpty() && ConflictKit.tributes(late).isEmpty());
                    // The engine withdraws a demand whose party fell to no player (DEMAND_SETTLED 5, M1). It
                    // sends it when its own count of the party reaches nothing, and a member may still stand
                    // in Minecraft then (MZB-32): here the envoy, so its name and its line can be seen.
                    ConflictKit.settled(faction, b, null, ConflictRecords.SETTLED_WITHDRAWN, 0);
                })
                .thenWaitUntil(() -> check("B's demand withdrawn", party(b).demand() == null))
                .thenWaitUntil(() -> check("B's envoy may talk again", bMayTalk.getAsBoolean()))
                .thenExecute(() -> {
                    check("DEMAND_SETTLED 5 taken (" + bridge().lastSettled + ")", bridge().lastSettled == ConflictRecords.SETTLED_WITHDRAWN);
                    check("B's name is gone", envoyB[0].getCustomName() == null && !envoyB[0].isCustomNameVisible());
                    int heard = player.heard.size();
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 1));
                    click(player, envoyB[0]);
                    check("withdrawn, B took nothing", player.getMainHandItem().getCount() == 1);
                    check("B spoke once: " + talkKeys(player), player.heard.size() == heard + 1);
                    TranslatableContents line = spoken(player.heard.get(heard));
                    check("the withdrawal: " + line.getKey(), line.getKey().equals("mic_rtsim.talk.envoy.envoy.settled.withdrawn.0"));
                    player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    // TALK slice 4 (MZB-130): a member hostile to the player (the camp has a reason against
                    // him) makes no small talk; to a bystander with no reason, now that no demand is to
                    // anyone, it chats. The envoy never does.
                    check("the guard does not chat with the player it has a reason against", !chats(guard[0], player)
                            && Talkers.find(guard[0], player).attitude(guard[0], player) == Talker.Attitude.HOSTILE);
                    check("nor the envoy", Talkers.find(envoy[0], player) instanceof ConflictTalker && !chats(envoy[0], player));
                    FakePlayers.Hearer bystander = FakePlayers.hearing(level, c.add(-1.5, 0, 2.5), "conflict_envoy_bystander");
                    bystanderHolder[0] = bystander;
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(bystander));
                    check("the guard chats with him", chats(guard[0], bystander)
                            && Talkers.find(guard[0], bystander).attitude(guard[0], bystander) == Talker.Attitude.WARY);
                    check("the envoy does not", !chats(envoy[0], bystander));
                    heardByHolder[0] = bystander.heard.size();
                    bystander.setShiftKeyDown(true);
                    bystander.interactOn(guard[0], InteractionHand.MAIN_HAND);
                    bystander.setShiftKeyDown(false);
                })
                // A chatting member asks the engine first (Talk.askable): on the native engine its
                // line is spoken when the answer comes, a step later or at TalkAsks.TIMEOUT_TICKS; the
                // stub speaks at once.
                .thenWaitUntil(() -> check("the guard spoke to him: " + talkKeys(bystanderHolder[0]),
                        bystanderHolder[0].heard.size() > heardByHolder[0]))
                .thenExecute(() -> {
                    FakePlayers.Hearer bystander = bystanderHolder[0];
                    int heardBy = heardByHolder[0];
                    check("the guard spoke to him once: " + talkKeys(bystander), bystander.heard.size() == heardBy + 1);
                    TranslatableContents chat = spoken(bystander.heard.get(heardBy));
                    record(test, "bystander line", chat.getKey() + " " + bystander.heard.get(heardBy).getString());
                    check("no reason, no insult: " + chat.getKey(), !chat.getKey().matches("mic_rtsim\\.talk\\.[a-z_]+\\.hostile\\.warn\\.[a-z_]+\\.\\d+")
                            && !chat.getKey().contains(".hostile.insult."));
                    FakePlayers.remove(bystander);
                    // The rest of B by /kill too: no answer from the whole party.
                    envoyB[0].kill();
                    ConflictKit.settled(faction, a, player.getUUID(), ConflictRecords.SETTLED_PAID, 0);
                })
                .thenWaitUntil(() -> check("B's second casualty", about(ConflictRecords.BAND_CASUALTIES, b).size() == 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received cb = about(ConflictRecords.BAND_CASUALTIES, b).get(1);
                    check("/kill: no killer", cb.aux() == ConflictRecords.BY_NONE && ConflictKit.uuidAt(cb, ConflictRecords.Casualties.KILLER_MOST) == null);
                    check("the emissary", cb.view().getInt(ConflictRecords.Casualties.BODY) == ConflictKit.bodyId(EMISSARY));
                    check("still no ENVOY_ANSWER from B", about(ConflictRecords.ENVOY_ANSWER, b).isEmpty());
                    ConflictKit.recall(b, 1, ConflictRecords.RECALL_SETTLED, 0, c.x, c.z);
                })
                .thenWaitUntil(() -> check("A's demand settled", party(a).demand() == null))
                .thenWaitUntil(() -> check("the envoy may talk again", aMayTalk.getAsBoolean()))
                .thenExecute(() -> {
                    check("its name is gone", envoy[0].getCustomName() == null && !envoy[0].isCustomNameVisible());
                    check("the outcome", bridge().lastSettled == ConflictRecords.SETTLED_PAID);
                    int heard = player.heard.size();
                    player.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.EMERALD, 1));
                    click(player, envoy[0]);
                    check("settled, the envoy took nothing", player.getMainHandItem().getCount() == 1);
                    check("the envoy spoke once: " + talkKeys(player), player.heard.size() == heard + 1);
                    TranslatableContents line = spoken(player.heard.get(heard));
                    check("the settlement: " + line.getKey(), line.getKey().equals("mic_rtsim.talk.envoy.envoy.settled.paid.0"));
                    player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    recalledAt[0] = level.getGameTime();
                    ConflictKit.recall(a, 1, ConflictRecords.RECALL_SETTLED, 60, c.x - 100, c.z);
                    ConflictKit.recall(late, 1, ConflictRecords.RECALL_SETTLED, 1, c.x - 100, c.z);
                })
                .thenWaitUntil(() -> check("B ended", !about(ConflictRecords.BAND_DEMATERIALIZED, b).isEmpty()))
                .thenExecute(() -> check("B had no one left", ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, b).get(0)).isEmpty()))
                .thenWaitUntil(() -> check("recalling", party(a).recalling()))
                .thenIdle(20)
                .thenExecute(() -> check("a player sees A: the bodies stay", bridge().bodies(a).size() == 3))
                .thenWaitUntil(() -> check("A ended at its timeout", !about(ConflictRecords.BAND_DEMATERIALIZED, a).isEmpty()))
                .thenWaitUntil(() -> check("the late party ended", !about(ConflictRecords.BAND_DEMATERIALIZED, late).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = about(ConflictRecords.BAND_DEMATERIALIZED, a).get(0);
                    long after = RtsimTests.gameTimeOf(d) - recalledAt[0];
                    Map<String, Integer> s = ConflictKit.survivors(d);
                    record(test, "recalled", "after " + after + " ticks, survivors " + s);
                    check("at the timeout (" + after + ")", after >= 60);
                    check("all three: " + s, s.equals(Map.of(EMISSARY, 1, WARRIOR, 2)));
                    check("the bodies are gone", !envoy[0].isAlive() || envoy[0].isRemoved());
                    FakePlayers.remove(player);
                })
                .thenSucceed();
    }

    /** A player's click as vanilla sends it: the main hand, then the off hand. */
    private static void click(ServerPlayer player, Entity e) {
        player.interactOn(e, InteractionHand.MAIN_HAND);
        player.interactOn(e, InteractionHand.OFF_HAND);
    }

    /** Named "<Culture> Envoy" (the speaker's key), not its demand. */
    private static boolean envoyNamed(Mob m) {
        return m.getCustomName() != null && m.getCustomName().getContents() instanceof TranslatableContents t
                && t.getKey().equals("mic_rtsim.talk.speaker.envoy");
    }

    /** The line inside a talk message ({@code mic_rtsim.talk.say}: speaker, line). */
    private static TranslatableContents spoken(Component say) {
        check("a talk message: " + say, say.getContents() instanceof TranslatableContents t && t.getKey().equals("mic_rtsim.talk.say")
                && t.getArgs().length == 2 && t.getArgs()[1] instanceof Component);
        Component line = (Component) ((TranslatableContents) say.getContents()).getArgs()[1];
        check("a line of ours: " + line, line.getContents() instanceof TranslatableContents);
        return (TranslatableContents) line.getContents();
    }

    /** Whether the conflict talker makes small talk with this player (TALK slice 4, MZB-130). */
    private static boolean chats(Entity e, ServerPlayer p) {
        Talker t = Talkers.find(e, p);
        return t instanceof ConflictTalker && t.chats(e, p);
    }

    private static String talkKeys(FakePlayers.Hearer p) {
        StringBuilder sb = new StringBuilder("[");
        for (Component c : p.heard)
            sb.append(c.getContents() instanceof TranslatableContents t && t.getArgs().length == 2 && t.getArgs()[1] instanceof Component l
                    && l.getContents() instanceof TranslatableContents lt ? lt.getKey() : c.getString()).append("; ");
        return sb.append("]").toString();
    }

    // ---- test 18 and ledger 19: fights ----------------------------------------------------------

    private static final String STRONG_TAG = "mic_rtsim_test_strong";
    private static boolean strengthPluginRegistered;

    /** A plugin that knows one entity: a zombie with the test's tag is 2.5 ordinary mobs (ledger 19). */
    private static synchronized void registerStrengthPlugin() {
        if (strengthPluginRegistered)
            return;
        strengthPluginRegistered = true;
        IndustryApi.register(new IndustryPlugin() {
            @Override
            public float fightStrength(Entity attacker) {
                return attacker instanceof Zombie && attacker.getTags().contains(STRONG_TAG) ? 2.5f : Float.NaN;
            }
        });
    }

    /**
     * Test 18 and ledger 19 (MOWZIE.md §4.13): an observed fight in a district counts each attacker's
     * strength as the first plugin that knows it says (a tagged zombie 2.5) or 1 (a plain zombie), and
     * never a mob of {@code mic_rtsim:threat/ignored} (the test pack's endermite, standing in for
     * Mowzie's foliaath, MZB-114): IND_FIGHT melee with 2 attackers, strength 3.5.
     */
    @GameTest(template = "industry_yard", batch = "conflict_strength", timeoutTicks = 800)
    public static void aNaturalFoliaathIsNotCountedAsPressure(GameTestHelper helper) {
        String test = "aNaturalFoliaathIsNotCountedAsPressure";
        IndustryTestKit.fastConfig();
        IndustryTestKit.forgetAll(helper);
        // Reset at the start too: a run that failed before its end left the gap set for every later test.
        ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, null);
        ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, 20);
        ConflictKit.onEnd(helper, () -> ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, null));
        registerStrengthPlugin();
        ServerLevel level = helper.getLevel();
        BlockPos chest = IndustryTestKit.inChunk(helper, 2, 2);
        long[] id = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    IndustryTestKit.set(helper, chest, TestMachines.MACHINE.get().defaultBlockState());
                    IndustryTestKit.playerPlaced(helper, chest);
                })
                .thenWaitUntil(() -> {
                    District d = IndustryTestKit.districtAt(helper, chest);
                    check("the district", d != null);
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    check("the core's tag ignores Mowzie's foliaath, not required", shippedIgnoresFoliaath());
                    check("the endermite is ignored", EntityType.ENDERMITE.is(com.minecraftindustrialcomplex.mic_rtsim.industry.ThreatClasses.IGNORED));
                    IronGolem golem = EntityType.IRON_GOLEM.create(level);
                    golem.setPlayerCreated(true);
                    golem.moveTo(chest.getX() + 3.5, chest.getY(), chest.getZ() + 0.5);
                    level.addFreshEntity(golem);
                    Zombie strong = EntityType.ZOMBIE.create(level);
                    strong.addTag(STRONG_TAG);
                    strong.moveTo(chest.getX() + 4.5, chest.getY(), chest.getZ() + 0.5);
                    level.addFreshEntity(strong);
                    Zombie plain = EntityType.ZOMBIE.create(level);
                    plain.moveTo(chest.getX() + 4.5, chest.getY(), chest.getZ() + 2.5);
                    level.addFreshEntity(plain);
                    Endermite mite = EntityType.ENDERMITE.create(level);
                    mite.moveTo(chest.getX() + 4.5, chest.getY(), chest.getZ() + 4.5);
                    level.addFreshEntity(mite);
                    for (Mob m : List.of(strong, plain, mite))
                        m.hurt(level.damageSources().mobAttack(golem), 1f);
                    for (Mob m : List.of(strong, plain, mite, golem))
                        m.discard();
                })
                .thenWaitUntil(() -> {
                    ByteBuffer melee = null;
                    for (ByteBuffer f : IndustryTestKit.received(IndRecords.IND_FIGHT, IndRecords.Fight.DISTRICT, id[0]))
                        if (IndustryTestKit.aux(f) == IndRecords.ThreatClass.GROUND_MELEE)
                            melee = f;
                    check("a melee fight", melee != null);
                    int attackers = melee.getInt(IndRecords.Fight.ATTACKERS);
                    float strength = melee.getFloat(IndRecords.Fight.ATTACKER_STRENGTH);
                    record(test, "melee (" + IndustryTestKit.engineName() + ")", "attackers " + attackers + ", strength " + strength);
                    check("two attackers, the endermite not one (" + attackers + ")", attackers == 2);
                    check("strength 2.5 + 1 (" + strength + ")", Math.abs(strength - 3.5f) < 1e-6);
                    ThreatConfig.setForTest(ThreatConfig.FIGHT_GAP, null);
                })
                .thenSucceed();
    }

    /** The shipped {@code mic_rtsim:threat/ignored} (the core's, not the test pack's) lists mowziesmobs:foliaath, required false. */
    private static boolean shippedIgnoresFoliaath() {
        String path = "/data/mic_rtsim/tags/entity_type/threat/ignored.json";
        try (InputStream in = ConflictGameTests.class.getResourceAsStream(path)) {
            check("the core ships " + path, in != null);
            JsonObject tag = JsonParser.parseReader(new InputStreamReader(in, StandardCharsets.UTF_8)).getAsJsonObject();
            for (JsonElement v : tag.getAsJsonArray("values"))
                if (v.isJsonObject() && v.getAsJsonObject().has("id") && "mowziesmobs:foliaath".equals(v.getAsJsonObject().get("id").getAsString()))
                    return v.getAsJsonObject().has("required") && !v.getAsJsonObject().get("required").getAsBoolean();
            return false;
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    // ---- the difficulty --------------------------------------------------------------------------

    /**
     * DIFFICULTY (M1, MZB-17): journaled (0x0815) on each change of the world's difficulty, with Minecraft's
     * id. On Peaceful a hostile stack waits pending, as Peaceful would remove its bodies, and the change back
     * places it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "conflict_peaceful", skyAccess = true)
    public static void peacefulIsJournaledAndHoldsAHostileBand(GameTestHelper helper) {
        String test = "peacefulIsJournaledAndHoldsAHostileBand";
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        Difficulty before = server.getWorldData().getDifficulty();
        Difficulty after = before == Difficulty.PEACEFUL ? Difficulty.NORMAL : before;
        ConflictKit.onEnd(helper, () -> server.setDifficulty(before, true));
        Vec3 c = RtsimTests.centre(helper);
        long agent = ConflictKit.newAgent();
        ConflictKit.Band band = ConflictKit.band(agent, ConflictKit.newFaction(), RtsimTests.dimension(helper));
        band.arcX = c.x + 0.5;
        band.arcZ = c.z;
        band.targetX = c.x + 100;
        band.targetY = c.y;
        band.targetZ = c.z;
        band.speed = 0f;
        int[] seen = {0};
        java.util.function.Supplier<List<ScriptedEngine.Received>> difficulties = () -> RtsimTests.engine().received(ConflictRecords.DIFFICULTY);
        java.util.function.IntPredicate lastIs = id -> {
            List<ScriptedEngine.Received> ds = difficulties.get();
            return ds.size() > seen[0] && ds.get(ds.size() - 1).view().getInt(ConflictRecords.DifficultyRecord.DIFFICULTY) == id;
        };
        helper.startSequence()
                .thenExecute(() -> {
                    seen[0] = difficulties.get().size();
                    server.setDifficulty(Difficulty.PEACEFUL, true);
                })
                .thenWaitUntil(() -> check("DIFFICULTY Peaceful", lastIs.test(Difficulty.PEACEFUL.getId())))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = difficulties.get().get(difficulties.get().size() - 1);
                    check("journaled: an observation sequence (" + d.view().getLong(8) + ")", d.view().getLong(8) > 0);
                    check("its own kind, 0x0815", d.kind() == 0x0815 && d.view().capacity() == ConflictRecords.DifficultyRecord.SIZE);
                    ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(WARRIOR, 2, false)));
                })
                .thenWaitUntil(() -> check("the party, answered", party(agent) != null && party(agent).stacks().size() == 1
                        && !about(ConflictRecords.BAND_MATERIALIZED, agent).isEmpty()))
                .thenIdle(20)
                .thenExecute(() -> {
                    ConflictBridge.Stack st = party(agent).stacks().get(0);
                    check("pending on Peaceful (" + st.state() + ")", st.state() == ConflictBridge.StackState.PENDING && st.bodies().isEmpty());
                    List<ScriptedEngine.Received> m = about(ConflictRecords.BAND_MATERIALIZED, agent);
                    check("BAND_MATERIALIZED with none placed", m.size() == 1 && m.get(0).aux() == 0);
                    seen[0] = difficulties.get().size();
                    server.setDifficulty(after, true);
                })
                .thenWaitUntil(() -> check("DIFFICULTY " + after, lastIs.test(after.getId())))
                .thenWaitUntil(() -> check("placed once Peaceful ends", party(agent).stacks().get(0).state() == ConflictBridge.StackState.PLACED
                        && party(agent).stacks().get(0).bodies().size() == 2))
                .thenExecute(() -> {
                    record(test, "difficulty", "Peaceful held the band; " + after + " placed it");
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 1, c.x - 100, c.z);
                })
                .thenWaitUntil(() -> check("BAND_DEMATERIALIZED", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty()))
                .thenSucceed();
    }

    // ---- notices ---------------------------------------------------------------------------------

    /**
     * Test 19 (MOWZIE.md §7): FACTION_NOTICE is a sound only (Ben, 2026-10-22: nothing in chat, the action
     * bar or a title), kept until its player is in range: a notice to a player away from the territory
     * waits; the player logs out (PlayerList.remove: the logged-out event, the entity gone) and logs in
     * again inside it (a new entity with the same id), and it plays once, to the new entity: NOTICE_SHOWN
     * with the notice's serial in its aux. A notice to the settlers plays to every player in range
     * (MZB-118), here two. The territory command adds and removes the faction's circle. The restart half
     * (the engine resends a kept notice at session start) is M1's rtsim-jni test: the GameTest server
     * has one bridge session, and the resent notice is the same FACTION_NOTICE this test sends (MZB-129).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 600, batch = "conflict_notice", skyAccess = true)
    public static void aNoticeWaitsForThePlayersNextEntryAcrossALogout(GameTestHelper helper) {
        String test = "aNoticeWaitsForThePlayersNextEntryAcrossALogout";
        ServerLevel level = helper.getLevel();
        String dim = RtsimTests.dimension(helper);
        Vec3 c = RtsimTests.centre(helper);
        long faction = ConflictKit.newFaction();
        int tx = (int) Math.floor(c.x), tz = (int) Math.floor(c.z);
        // The player stands 600 blocks off: outside the territory (radius 100) and NOTICE_RANGE.
        ServerPlayer player = FakePlayers.add(level, c.add(600, 0, 0), "conflict_notice_target");
        ServerPlayer[] second = new ServerPlayer[1], back = new ServerPlayer[1];
        ConflictKit.onEnd(helper, () -> {
            FakePlayers.remove(player);
            for (ServerPlayer p : new ServerPlayer[]{second[0], back[0]})
                if (p != null)
                    FakePlayers.remove(p);
        });
        int serial = 0x5E41_0019;
        long[] played = {0}, posted = {0}, hearers = {0};
        helper.startSequence()
                .thenExecute(() -> ConflictKit.territory(faction, dim, (1 << ConflictRecords.SIGNAL_MINING) | (1 << ConflictRecords.SIGNAL_KILLING), tx, tz, 100))
                .thenWaitUntil(() -> check("the territory", bridge().territories().get(faction) != null))
                .thenExecute(() -> {
                    played[0] = bridge().noticesPlayed;
                    posted[0] = ConflictNews.posted();
                    ConflictKit.notice(faction, player.getUUID(), dim, ConflictRecords.SIGNAL_MINING, tx, tz, 12f, 3, serial);
                })
                .thenWaitUntil(() -> check("kept", bridge().noticesKept() >= 1))
                .thenIdle(ConflictBridge.NOTICE_CHECK * 3)
                .thenExecute(() -> {
                    check("not played while the player is away", bridge().noticesPlayed == played[0] && bridge().noticesKept() >= 1);
                    // A real logout: the logged-out event fires and the entity is removed. The level's player
                    // list is the entity manager's to change, which never had this player: take it out by hand.
                    level.getServer().getPlayerList().remove(player);
                    check("logged out", player.isRemoved());
                    FakePlayers.remove(player);
                })
                .thenIdle(ConflictBridge.NOTICE_CHECK * 2)
                .thenExecute(() -> {
                    check("not played to a player not in the level", bridge().noticesPlayed == played[0]);
                    // A login inside the territory: a new entity with the player's id.
                    back[0] = FakePlayers.add(level, c.add(20, 0, 0), "conflict_notice_target", player.getUUID());
                })
                .thenWaitUntil(() -> check("played on the player's entry", bridge().noticesPlayed == played[0] + 1))
                .thenExecute(() -> {
                    check("forgotten once played", bridge().noticesKept() == 0);
                    check("the news hook reached (MZB-101: it sends nothing)", ConflictNews.posted() > posted[0]);
                    if (ConflictBridge.m1Records()) {
                        List<ScriptedEngine.Received> shown = new ArrayList<>();
                        for (ScriptedEngine.Received r : RtsimTests.engine().received(ConflictRecords.NOTICE_SHOWN))
                            if (r.view().getLong(ConflictRecords.NoticeShown.FACTION) == faction)
                                shown.add(r);
                        check("NOTICE_SHOWN with the notice's serial", shown.size() == 1 && shown.get(0).aux() == serial
                                && player.getUUID().equals(ConflictKit.uuidAt(shown.get(0), ConflictRecords.NoticeShown.PLAYER_MOST)));
                    }
                    // To the settlers: every player in range, here two.
                    second[0] = FakePlayers.add(level, c.add(30, 0, 5), "conflict_notice_second");
                    hearers[0] = bridge().noticeHearers;
                    ConflictKit.notice(faction, null, dim, ConflictRecords.SIGNAL_KILLING, tx, tz, 20f, 1, serial + 1);
                })
                .thenWaitUntil(() -> check("the settlers' notice played", bridge().noticesPlayed == played[0] + 2 && bridge().noticesKept() == 0))
                .thenExecute(() -> {
                    long heard = bridge().noticeHearers - hearers[0];
                    record(test, "notices", "kept while away, played on entry; the settlers' to " + heard + " players in range");
                    check("the settlers' notice reached both players (" + heard + ")", heard >= 2);
                    FakePlayers.remove(back[0]);
                    FakePlayers.remove(second[0]);
                    ConflictKit.territoryRemoved(faction, dim);
                })
                .thenWaitUntil(() -> check("the territory removed", bridge().territories().get(faction) == null))
                .thenSucceed();
    }

    // ---- territory signals and camps --------------------------------------------------------------

    /** A structure no culture claims: the engine ignores its FACTION_FOUND, so no engine faction answers the test's. */
    private static final String UNCLAIMED_CAMP = "mic_rtsim:conflict/test_unclaimed_camp";

    /**
     * Territory signals (MOWZIE.md §4.4): inside a faction's territory a player's broken log is logging
     * (2), a broken ore mining (3), a placed block building (5), and a kill of the camp's kin killing (4,
     * with the faction); each kind is one TERRITORY_SIGNAL a batch, its count summed, after
     * {@link ConflictBridge#SIGNAL_FLUSH} ticks. A signal the territory does not mind, a block outside it
     * and a kill of a body that is no kin count nothing. {@code reportCamp} sends FACTION_FOUND with the
     * structure and the census.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 800, batch = "conflict_signals", skyAccess = true)
    public static void signalsInsideATerritoryAreCountedAndBatched(GameTestHelper helper) {
        String test = "signalsInsideATerritoryAreCountedAndBatched";
        ServerLevel level = helper.getLevel();
        String dim = RtsimTests.dimension(helper);
        Vec3 c = RtsimTests.centre(helper);
        BlockPos camp = BlockPos.containing(c);
        ServerPlayer player = FakePlayers.add(level, c, "conflict_signal_maker");
        String kinTag = "mic_rtsim_test_kin";
        long[] faction = {0};
        long[] counted = {0};
        BlockPos log = BlockPos.containing(c).offset(2, 0, 2), ore = BlockPos.containing(c).offset(-2, 0, 2),
                dirt = BlockPos.containing(c).offset(2, 0, -2);
        helper.startSequence()
                .thenExecute(() -> {
                    faction[0] = bridge().reportCamp(level, UNCLAIMED_CAMP, camp,
                            e -> e.getTags().contains(kinTag), null, false, 7, ConflictRecords.FactionFound.CENSUS_CHIEF);
                    check("a faction key", faction[0] != 0 && bridge().hasCamp(faction[0]));
                })
                .thenWaitUntil(() -> check("FACTION_FOUND", !ConflictKit.factionsFound(faction[0]).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received f = ConflictKit.factionsFound(faction[0]).get(0);
                    check("the structure", f.view().getInt(ConflictRecords.FactionFound.STRUCTURE)
                            == RtsimTests.session().dictId(Obs.DICT_STRUCTURE, UNCLAIMED_CAMP));
                    check("its position", f.view().getInt(ConflictRecords.FactionFound.X) == camp.getX()
                            && f.view().getInt(ConflictRecords.FactionFound.Z) == camp.getZ());
                    check("the census", (f.view().getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == 7
                            && f.view().get(ConflictRecords.FactionFound.CENSUS) == ConflictRecords.FactionFound.CENSUS_CHIEF);
                    check("not late", f.aux() == 0);
                    // Its territory minds everything but building.
                    int mask = (1 << ConflictRecords.SIGNAL_LOGGING) | (1 << ConflictRecords.SIGNAL_MINING) | (1 << ConflictRecords.SIGNAL_KILLING);
                    ConflictKit.territory(faction[0], dim, mask, camp.getX(), camp.getZ(), 64);
                })
                .thenWaitUntil(() -> check("the territory", bridge().territories().get(faction[0]) != null))
                .thenExecute(() -> {
                    counted[0] = bridge().signalsCounted;
                    level.setBlock(log, Blocks.OAK_LOG.defaultBlockState(), 3);
                    level.setBlock(ore, Blocks.IRON_ORE.defaultBlockState(), 3);
                    level.setBlock(dirt, Blocks.DIRT.defaultBlockState(), 3);
                    // Two logs, one ore, a block placed (building: not minded), dirt broken (nothing).
                    post(new net.neoforged.neoforge.event.level.BlockEvent.BreakEvent(level, log, level.getBlockState(log), player));
                    post(new net.neoforged.neoforge.event.level.BlockEvent.BreakEvent(level, log, level.getBlockState(log), player));
                    post(new net.neoforged.neoforge.event.level.BlockEvent.BreakEvent(level, ore, level.getBlockState(ore), player));
                    post(new net.neoforged.neoforge.event.level.BlockEvent.BreakEvent(level, dirt, level.getBlockState(dirt), player));
                    post(new net.neoforged.neoforge.event.level.BlockEvent.EntityPlaceEvent(
                            net.neoforged.neoforge.common.util.BlockSnapshot.create(level.dimension(), level, dirt), level.getBlockState(dirt.below()), player));
                    // A log far outside the territory.
                    BlockPos far = camp.offset(500, 0, 0);
                    post(new net.neoforged.neoforge.event.level.BlockEvent.BreakEvent(level, far, Blocks.OAK_LOG.defaultBlockState(), player));
                    // The camp's kin killed by the player, and a zombie that is no kin.
                    Zombie kin = EntityType.ZOMBIE.create(level);
                    kin.addTag(kinTag);
                    kin.moveTo(c.x + 1, c.y, c.z + 1);
                    level.addFreshEntity(kin);
                    Zombie other = EntityType.ZOMBIE.create(level);
                    other.moveTo(c.x - 1, c.y, c.z - 1);
                    level.addFreshEntity(other);
                    kin.hurt(level.damageSources().playerAttack(player), 1000f);
                    other.hurt(level.damageSources().playerAttack(player), 1000f);
                    check("counted: 2 logs, an ore, a kill (" + (bridge().signalsCounted - counted[0]) + ")", bridge().signalsCounted == counted[0] + 4);
                })
                .thenIdle(ConflictBridge.SIGNAL_FLUSH / 2)
                .thenExecute(() -> check("batched: nothing journaled yet", ConflictKit.signals(ConflictRecords.SIGNAL_LOGGING, player.getUUID()).isEmpty()))
                .thenWaitUntil(() -> check("journaled", !ConflictKit.signals(ConflictRecords.SIGNAL_LOGGING, player.getUUID()).isEmpty()
                        && !ConflictKit.signals(ConflictRecords.SIGNAL_MINING, player.getUUID()).isEmpty()
                        && !ConflictKit.signals(ConflictRecords.SIGNAL_KILLING, player.getUUID()).isEmpty()))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> logs = ConflictKit.signals(ConflictRecords.SIGNAL_LOGGING, player.getUUID());
                    check("one logging record of 2", logs.size() == 1 && logs.get(0).view().getInt(ConflictRecords.TerritorySignal.COUNT) == 2);
                    check("at the first log", logs.get(0).view().getInt(ConflictRecords.TerritorySignal.X) == log.getX());
                    check("for no faction: the engine finds it", logs.get(0).view().getLong(ConflictRecords.TerritorySignal.FACTION) == 0);
                    List<ScriptedEngine.Received> ores = ConflictKit.signals(ConflictRecords.SIGNAL_MINING, player.getUUID());
                    check("one mining record of 1", ores.size() == 1 && ores.get(0).view().getInt(ConflictRecords.TerritorySignal.COUNT) == 1);
                    List<ScriptedEngine.Received> kills = ConflictKit.signals(ConflictRecords.SIGNAL_KILLING, player.getUUID());
                    check("one killing record of 1, the camp's faction", kills.size() == 1
                            && kills.get(0).view().getInt(ConflictRecords.TerritorySignal.COUNT) == 1
                            && kills.get(0).view().getLong(ConflictRecords.TerritorySignal.FACTION) == faction[0]);
                    check("no building record: not minded", ConflictKit.signals(ConflictRecords.SIGNAL_BUILDING, player.getUUID()).isEmpty());
                    record(test, "signals", "logging 2, mining 1, killing 1 in one record each");
                    FakePlayers.remove(player);
                    ConflictKit.territoryRemoved(faction[0], dim);
                })
                .thenWaitUntil(() -> check("the territory removed", bridge().territories().get(faction[0]) == null))
                .thenSucceed();
    }

    // ---- the feud driven to a raid, on the native engine ------------------------------------------

    /** The faction's embodied party: its envoy's ({@code purpose} 0) or a band of that purpose; null for none. */
    private static ConflictBridge.Party partyOf(long faction, int purpose) {
        for (ConflictBridge.Party p : bridge().parties())
            if (p.faction == faction && (purpose == 0 ? p.envoy : !p.envoy && p.purpose == purpose))
                return p;
        return null;
    }

    /**
     * The feud driven to a raid on the native engine (MOWZIE.md §9 M3), at a place far from every earlier
     * run's with forced chunks standing in for the simulation distance. A goblin camp reported (a structure
     * the goblin culture claims) gets its territory back (TERRITORY, radius 400). A player 40 blocks off
     * kills 32 of the camp's kin there: one TERRITORY_SIGNAL (killing, 4), and the tribe takes notice
     * (FACTION_NOTICE, its sound played to the player, who stands inside). The phase 1 world's clock, which
     * the conflict runs on, is moved on 1,200 ticks at a time (P1_ADVANCE) with the player reporting where
     * it stands before each: the envoy comes into view (ENVOY_MATERIALIZE) asking emeralds or the killing
     * stopped, named "Goblin Envoy" and not its demand, is placed and struck by the player (ENVOY_ANSWER 3,
     * the only refusal there is: no click refuses); the demand is settled as refused (DEMAND_SETTLED 4), and
     * the ladder climbs on to a raid that musters and is seen setting out (BAND_MATERIALIZE, purpose 3, the
     * war chief with it), whose stacks are placed. A stack still pending has its chunks forced too (the
     * engine picks where a party comes into view). Chunks an earlier failed run left forced in the far area
     * are released first, and this run's are released however it ends. Skipped on the stub, which decides
     * nothing.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 6000, batch = "conflict_feud")
    public static void aPlayersKillingsClimbTheLadderFromNoticeToARaid(GameTestHelper helper) {
        String test = "aPlayersKillingsClimbTheLadderFromNoticeToARaid";
        if (!(RtsimTests.engine() instanceof NativeEngine e)) {
            record(test, "skipped", "the stub engine decides nothing; this test needs the native engine's conflict");
            helper.succeed();
            return;
        }
        ServerLevel level = helper.getLevel();
        int released = ConflictKit.releaseFarChunks(level);
        int[] base = ConflictKit.farBase();
        int bx = base[0] + 8, bz = base[1] + 8; // the camp; the player 40 blocks east
        int px = bx + 40;
        String kinTag = "mic_rtsim_test_goblin_kin";
        // Killing weighs 8: grievance 256. The chief's temper bends each rung's threshold up to 1/0.6 of
        // the culture's (the raid's 70 to 117), and the climb to the raid takes about three days of a
        // six-day half-life (to 181): past it for any chief. Sixteen kills (128) fell short of a meek one.
        int kills = 32;
        int step = 1_200; // under the conflict's player_stale_ticks (2,400): the player keeps watching
        List<Long> forced = new ArrayList<>();
        java.util.function.BiConsumer<Integer, Integer> force = (cx, cz) -> {
            if (ConflictKit.force(level, cx, cz, true))
                forced.add(chunk(cx, cz));
        };
        int box = 0;
        for (int cx = (bx - 32) >> 4; cx <= (px + 32) >> 4; cx++)
            for (int cz = (bz - 32) >> 4; cz <= (bz + 32) >> 4; cz++) {
                force.accept(cx, cz);
                box++;
            }
        int boxChunks = box;
        ServerPlayer[] player = new ServerPlayer[1];
        long[] faction = {0};
        long[] lastReport = {Long.MIN_VALUE};
        long[] mark = new long[4];
        int[] asks = {0, 0};
        boolean[] scouts = {false};
        ConflictBridge.Party[] envoy = new ConflictBridge.Party[1];
        ConflictBridge.Party[] raid = new ConflictBridge.Party[1];
        // Each tick of a wait: the player's PLAYER record every 20 ticks, and every pending stack of the
        // faction's parties gets its chunks forced.
        Runnable keep = () -> {
            long now = level.getGameTime();
            if (player[0] != null && now - lastReport[0] >= 20) {
                ConflictKit.reportPlayer(player[0]);
                lastReport[0] = now;
            }
            for (ConflictBridge.Party p : bridge().parties())
                if (p.faction == faction[0])
                    for (ConflictBridge.Stack s : p.stacks())
                        if (s.state() == ConflictBridge.StackState.PENDING)
                            for (long c : s.chunksNow(now))
                                force.accept(ChunkPos.getX(c), ChunkPos.getZ(c));
        };
        // Every fourth tick: the player reports, then the world's clock moves on a step.
        Runnable advance = () -> {
            if (asks[0]++ % 4 == 0) {
                ConflictKit.reportPlayer(player[0]);
                lastReport[0] = level.getGameTime();
                e.phase1Advance(step);
                asks[1]++;
            }
        };
        Runnable unforce = () -> {
            for (long c : forced)
                ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
            forced.clear();
        };
        ConflictKit.onEnd(helper, () -> {
            unforce.run();
            if (player[0] != null)
                FakePlayers.remove(player[0]);
        });

        helper.startSequence()
                .thenWaitUntil(io(() -> {
                    for (int cx = (bx - 32) >> 4; cx <= (px + 32) >> 4; cx++)
                        for (int cz = (bz - 32) >> 4; cz <= (bz + 32) >> 4; cz++)
                            check("the forced chunks tick entities", RtsimSession.entitiesTickAt(level, new BlockPos(cx * 16 + 8, 64, cz * 16 + 8)));
                }))
                .thenExecute(() -> {
                    // A reused world's clock, restored from its pages, runs ahead of game time by the last
                    // run's P1_ADVANCE asks (the offset lives only for a session), and no ask moves it until
                    // game time plus this session's offset passes it. One ask of the gap catches it up before
                    // the camp exists (MZB-126).
                    mark[2] = e.phase1Stats()[NativeEngine.P1_WORLD_TICK];
                    long gap = mark[2] - level.getGameTime();
                    if (gap > 0)
                        e.phase1Advance(gap);
                })
                .thenWaitUntil(() -> check("the world's clock moves", e.phase1Stats()[NativeEngine.P1_WORLD_TICK] > mark[2]))
                .thenExecute(() -> {
                    int py = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, px, bz);
                    player[0] = FakePlayers.add(level, new Vec3(px + 0.5, py, bz + 0.5), "conflict_feud");
                    // Creative: the scouts' crossbows find no target in it (the fake player is never ticked).
                    player[0].setGameMode(GameType.CREATIVE);
                    BlockPos camp = new BlockPos(bx, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, bx, bz), bz);
                    faction[0] = bridge().reportCamp(level, "mic_rtsim:conflict/goblin_camp", camp,
                            x -> x.getTags().contains(kinTag), null, false, 0, 0);
                    check("a faction key", faction[0] != 0L);
                    record(test, "camp", "(" + bx + ", " + bz + "), the player at (" + px + ", " + bz + "), " + boxChunks
                            + " chunks forced, " + released + " an earlier run left forced released");
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    check("TERRITORY from the engine", bridge().territories().get(faction[0]) != null);
                })
                .thenExecute(() -> {
                    Territories.Territory t = bridge().territories().get(faction[0]);
                    record(test, "territory", "radius " + t.radius() + ", minds " + Integer.toBinaryString(t.mask()));
                    check("the goblins' radius, 400 (" + t.radius() + ")", t.radius() == 400);
                    check("it minds killing", t.minds(ConflictRecords.SIGNAL_KILLING));
                    mark[0] = bridge().signalsCounted;
                    mark[1] = bridge().noticesPlayed;
                    ServerPlayer pl = player[0];
                    for (int i = 0; i < kills; i++) {
                        Zombie z = EntityType.ZOMBIE.create(level);
                        z.addTag(kinTag);
                        z.moveTo(pl.getX() + 1, pl.getY(), pl.getZ() + 1);
                        level.addFreshEntity(z);
                        z.hurt(level.damageSources().playerAttack(pl), 1000f);
                    }
                    check("each kill counted (" + (bridge().signalsCounted - mark[0]) + ")", bridge().signalsCounted == mark[0] + kills);
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    List<ScriptedEngine.Received> rs = ConflictKit.signals(ConflictRecords.SIGNAL_KILLING, player[0].getUUID());
                    check("one TERRITORY_SIGNAL of " + kills, rs.size() == 1 && rs.get(0).view().getInt(ConflictRecords.TerritorySignal.COUNT) == kills
                            && rs.get(0).view().getLong(ConflictRecords.TerritorySignal.FACTION) == faction[0]);
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    check("FACTION_NOTICE played to the player", bridge().noticesPlayed > mark[1]);
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    if (partyOf(faction[0], 0) != null)
                        return;
                    // Two dwells of half a day and the hand-overs: 10 asks on a fresh world. The P1_ADVANCE
                    // orders queue when the engine lags, so a loaded machine asks more (MZB-126).
                    check("an envoy within 80 steps of " + step + " ticks (asks " + asks[1] + ", world tick "
                            + e.phase1Stats()[NativeEngine.P1_WORLD_TICK] + ")", asks[1] < 80);
                    advance.run();
                    check("the envoy comes", false);
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    envoy[0] = partyOf(faction[0], 0);
                    check("the envoy party is still here", envoy[0] != null);
                    check("placed", envoy[0].stacks().get(0).state() == ConflictBridge.StackState.PLACED);
                })
                .thenExecute(() -> {
                    record(test, "envoy after", asks[1] + " steps of " + step + " ticks, world tick "
                            + e.phase1Stats()[NativeEngine.P1_WORLD_TICK]);
                    ConflictBridge.Demand d = envoy[0].demand();
                    check("a demand", d != null);
                    check("emeralds (" + d.itemName + ")", d.item == Items.EMERALD && d.remaining() > 0);
                    check("or the killing stopped", (d.flags & ConflictRecords.Envoy.DEMAND_STOP) != 0
                            && d.signal == ConflictRecords.SIGNAL_KILLING);
                    check("guarded", envoy[0].guarded());
                    Mob emissary = null;
                    for (ConflictBridge.Slot sl : envoy[0].stacks().get(0).slots())
                        if (sl.envoy)
                            emissary = sl.entity();
                    check("the emissary", emissary != null && emissary.isAlive());
                    String n = name(emissary);
                    record(test, "the envoy's name", n);
                    check("named by its culture, not its demand: " + n, envoyNamed(emissary) && "Goblin Envoy".equals(n));
                    mark[2] = bridge().settledCommands;
                    long answered = bridge().answers;
                    ServerPlayer pl = player[0];
                    // A plain click shows the demand and refuses nothing; a blow refuses.
                    check("an empty hand", pl.getMainHandItem().isEmpty());
                    pl.setShiftKeyDown(false);
                    click(pl, emissary);
                    check("a click is no answer", bridge().answers == answered);
                    emissary.hurt(level.damageSources().playerAttack(pl), 1f);
                    check("struck", bridge().answers == answered + 1 && !envoy[0].guarded());
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    check("ENVOY_ANSWER 3 journaled", ConflictKit.answers(envoy[0].agent, ConflictRecords.ANSWER_ATTACKED) == 1);
                    check("DEMAND_SETTLED", bridge().settledCommands > mark[2]);
                })
                .thenExecute(() -> {
                    check("no ENVOY_ANSWER 2", ConflictKit.answers(envoy[0].agent, ConflictRecords.ANSWER_REFUSED) == 0);
                    check("settled as refused (" + bridge().lastSettled + ")", bridge().lastSettled == ConflictRecords.SETTLED_REFUSED);
                    asks[0] = 0;
                    mark[3] = asks[1];
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    if (partyOf(faction[0], ConflictRecords.PURPOSE_SCOUTS) != null)
                        scouts[0] = true;
                    if (partyOf(faction[0], ConflictRecords.PURPOSE_RAID) != null)
                        return;
                    check("a raid within 160 steps of " + step + " ticks (asks " + (asks[1] - mark[3]) + ", world tick "
                            + e.phase1Stats()[NativeEngine.P1_WORLD_TICK] + ")", asks[1] - mark[3] < 160);
                    advance.run();
                    check("the raid comes", false);
                })
                .thenWaitUntil(() -> {
                    keep.run();
                    raid[0] = partyOf(faction[0], ConflictRecords.PURPOSE_RAID);
                    check("the raid is still here", raid[0] != null);
                    for (ConflictBridge.Stack s : raid[0].stacks())
                        check("every stack placed", s.state() == ConflictBridge.StackState.PLACED);
                })
                .thenExecute(() -> {
                    ConflictBridge.Party r = raid[0];
                    int bodies = 0;
                    boolean chief = false;
                    for (ConflictBridge.Stack s : r.stacks())
                        for (ConflictBridge.Slot sl : s.slots()) {
                            bodies++;
                            chief |= sl.chief;
                        }
                    record(test, "raid after", (asks[1] - mark[3]) + " more steps; scouts seen: " + scouts[0] + "; " + r.stacks().size()
                            + " stacks, " + bodies + " bodies; " + (forced.size() - boxChunks) + " more chunks forced");
                    check("BAND_MATERIALIZED", !about(ConflictRecords.BAND_MATERIALIZED, r.agent).isEmpty());
                    check("never rejected", about(ConflictRecords.BAND_SPAWN_REJECTED, r.agent).isEmpty());
                    check("the war chief comes with it", chief);
                    check("every body in Minecraft (" + bridge().bodies(r.agent).size() + " of " + bodies + ")",
                            bridge().bodies(r.agent).size() == bodies);
                    FakePlayers.remove(player[0]);
                    unforce.run();
                })
                .thenSucceed();
    }

    private static void post(net.neoforged.bus.api.Event e) {
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.post(e);
    }
}
