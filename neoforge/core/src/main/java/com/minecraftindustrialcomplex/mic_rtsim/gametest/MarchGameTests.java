package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeEvents;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.MarchPath;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.ARCHER;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.WARRIOR;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.about;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.bridge;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.party;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The march's GameTests (MOWZIE.md slice M10c, §7 items 33 and 34 and the core halves of the others; docs/choices-mowzie-build.md,
 * MZB-720 to MZB-731 and MZB-740 on): the column's rows and where they appear, the stop, the trim, the march record's round
 * trip, the homecoming, the empty flagged materialize, a saved copy of a puppet that left, and MZB-466's counter. They script
 * conflict commands as the engine's conflict would send them ({@link ConflictKit}) for test agents and factions the engine does
 * not know, and read back what the bridge journaled. Each test has its own batch (the simulation distance of the column and stop
 * tests is server-wide) and its own far base, with forced chunks or a seen player standing in for the simulation distance. Every
 * test logs what it compared on a {@code [gametest]} line, and every failure message names the values and ticks compared.
 *
 * <p>The GameTest world is flat, so no test builds a floor. A test that waits for chunks waits on {@link ChunkWait}'s two clocks.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class MarchGameTests {
    private MarchGameTests() {}

    private static final ConflictBridge.StackState PENDING = ConflictBridge.StackState.PENDING,
            PLACED = ConflictBridge.StackState.PLACED, GONE = ConflictBridge.StackState.GONE;
    private static final int HOSTILE = ConflictRecords.Materialize.FLAG_HOSTILE, CHIEF = ConflictRecords.Materialize.FLAG_CHIEF;

    // ---- helpers ---------------------------------------------------------------------------------

    /** The states of a party's stacks, for a failure message. */
    private static String states(ConflictBridge.Party p) {
        StringBuilder out = new StringBuilder("[");
        for (ConflictBridge.Stack st : p.stacks())
            out.append(out.length() > 1 ? ", " : "").append(st.index).append(' ').append(st.state()).append(" x").append(st.placings());
        return out.append(']').toString();
    }

    private static String xz(double[] a) {
        return String.format("(%.2f, %.2f)", a[0], a[1]);
    }

    /** Forces chunks and lets them go when the test ends (a forced chunk is saved with the world and would stay in every later run). */
    private static final class Forced {
        final ServerLevel level;
        private final List<Long> forced = new ArrayList<>();

        Forced(GameTestHelper helper) {
            level = helper.getLevel();
            ConflictKit.releaseFarChunks(level); // chunks an earlier failed run left forced in the far area
            ConflictKit.onEnd(helper, () -> {
                for (long c : forced)
                    ConflictKit.force(level, ChunkPos.getX(c), ChunkPos.getZ(c), false);
                forced.clear();
            });
        }

        void square(int cx, int cz, int r) {
            for (int x = cx - r; x <= cx + r; x++)
                for (int z = cz - r; z <= cz + r; z++)
                    if (ConflictKit.force(level, x, z, true))
                        forced.add(ChunkPos.asLong(x, z));
        }

        /**
         * Sets the server's simulation distance (the stop's square reads it) and puts it back when the test ends. The distance is
         * server-wide, so a test using this has a batch of its own.
         */
        void simulation(GameTestHelper helper, int s) {
            var players = level.getServer().getPlayerList();
            int saved = players.getSimulationDistance();
            players.setSimulationDistance(s);
            ConflictKit.onEnd(helper, () -> players.setSimulationDistance(saved));
        }

        /** The y a player or a body stands at in the forced area (the world is flat). */
        double ground(double x, double z) {
            return level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) Math.floor(x), (int) Math.floor(z));
        }

        /** Fails (until they do) unless every chunk of the square ticks entities. */
        void checkSquare(int cx, int cz, int r) {
            int n = 0, total = (2 * r + 1) * (2 * r + 1);
            for (int x = cx - r; x <= cx + r; x++)
                for (int z = cz - r; z <= cz + r; z++)
                    if (ticks(level, x, z))
                        n++;
            if (n < total)
                throw new GameTestAssertException(n + " of " + total + " chunks within " + r + " of (" + cx + ", " + cz + ") tick entities");
        }
    }

    private static boolean ticks(ServerLevel level, int cx, int cz) {
        return RtsimSession.entitiesTickAt(level, new BlockPos(16 * cx + 8, level.getSeaLevel(), 16 * cz + 8));
    }

    /**
     * A server player the world sees and a simulation distance set for it: the ticking area of a real player, in a far place
     * with no other. The simulation and view distances are the server's, so each is put back when the test ends (the player
     * first, before any chunk is let go), and a test using this has a batch of its own. The player stands on its chunk's west
     * column ({@code px = 16 * pcx + 0.5}) and mid-chunk in z, so the ticking edge is {@code 16 * (s + 1) - 0.5} blocks east of it.
     */
    private static final class Ticking {
        final ServerLevel level;
        final int s, pcx, pcz;
        final double px, pz;
        ServerPlayer player;
        private final int savedSimulation, savedView;

        Ticking(GameTestHelper helper, int s) {
            this.level = helper.getLevel();
            this.s = s;
            ConflictKit.releaseFarChunks(level);
            int[] base = ConflictKit.farBase();
            pcx = (base[0] + 512) >> 4;
            pcz = (base[1] + 512) >> 4;
            px = 16.0 * pcx + 0.5;
            pz = 16.0 * pcz + 8.0;
            var players = level.getServer().getPlayerList();
            savedSimulation = players.getSimulationDistance();
            savedView = players.getViewDistance();
            ConflictKit.onEnd(helper, () -> {
                if (player != null)
                    FakePlayers.removeSeen(player);
                players.setSimulationDistance(savedSimulation);
                players.setViewDistance(savedView);
            });
        }

        /** The first x outside the ticking area, on the player's row. */
        double edge() {
            return 16.0 * (pcx + s + 1);
        }

        /** Sets the distances and joins the player (its chunks load and tick over the next ticks). */
        void start() {
            var players = level.getServer().getPlayerList();
            players.setViewDistance(s + 2);
            players.setSimulationDistance(s);
            player = FakePlayers.addSeen(level, new Vec3(px, level.getMinBuildHeight() + 64, pz), "conflict_march_watcher");
            player.setNoGravity(true);
        }

        /** Stands the player on the ground once its chunk is there. */
        void ground() {
            double y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) Math.floor(px), (int) Math.floor(pz));
            player.moveTo(px, y, pz);
        }

        /** Fails until the square of chunks within {@code s} of the player's ticks entities and the first chunk past it east does not. */
        void check() {
            int n = 0, total = (2 * s + 1) * (2 * s + 1);
            for (int x = pcx - s; x <= pcx + s; x++)
                for (int z = pcz - s; z <= pcz + s; z++)
                    if (ticks(level, x, z))
                        n++;
            if (n < total)
                throw new GameTestAssertException(n + " of " + total + " chunks within " + s + " of the player's (" + pcx + ", " + pcz
                        + ") tick entities (simulation distance " + level.getServer().getPlayerList().getSimulationDistance() + ")");
            if (ticks(level, pcx + s + 1, pcz))
                throw new GameTestAssertException("the chunk " + (s + 1) + " east of the player's ticks entities: the area is wider than " + s);
        }
    }

    private static int count(List<ScriptedEngine.Received> rs, int aux) {
        int n = 0;
        for (ScriptedEngine.Received r : rs)
            if (r.aux() == aux)
                n++;
        return n;
    }

    // ---- test 33: the column in the ticking area -------------------------------------------------

    /**
     * Test 33 (MOWZIE.md §7, MZB-722, MZB-481) at a simulation distance of 8, which replaces M3's stack test as the proof that a
     * band is made where the world ticks. A seen player (a real player's entity-ticking area, {@link Ticking}) stands on its
     * chunk's west column. A band met on the march (radius 0, bearing 180: heading -x, toward the player) of six warriors and
     * their chief comes in 170 blocks east of it, on its row, and walks at 0.1 blocks a tick to a target 60 blocks east: the
     * column is three rows of two (1.5 apart) and the chief at the rear, four stacks. Asserted:
     * <ul>
     * <li>BAND_MATERIALIZED places none (the whole column is outside the area) and is never rejected, NOT_TICKING least of all;</li>
     * <li>the stacks appear one by one, front first, each in the tick its last member is inside: at its first sight every body
     * stands in an entity-ticking chunk and the stack's place is within a block of the edge (logged against 128 to 144 blocks
     * from the player);</li>
     * <li>the stacks already in stay in: none is requeued, none has a member moved back, each is placed once;</li>
     * <li>a pending stack is tested only on a change: its tests are one at the start and one for each time its place moved into
     * other chunks (the spawn quota may add one), counted ({@code tests}, {@code moves});</li>
     * <li>steered 240 blocks east at 0.8 a tick, the stacks walk out of the area and go back to pending (each body leaves
     * the ticking chunks, MZB-125), and steered in again they come again, each placed twice;</li>
     * <li>a second band, recalled while its front stack is in and its chief's is not, reports every living member as a survivor;</li>
     * <li>an envoy appears as one group.</li>
     * </ul>
     * The same test at a simulation distance of 10 is {@link #aColumnAppearsInsideTheTickingAreaAtTen}.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_column")
    public static void aColumnAppearsInsideTheTickingArea(GameTestHelper helper) {
        column(helper, "aColumnAppearsInsideTheTickingArea", 8);
    }

    /** Test 33 at a simulation distance of 10 and a view distance of 12: the edge is 160 to 176 blocks from the player. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_column10")
    public static void aColumnAppearsInsideTheTickingAreaAtTen(GameTestHelper helper) {
        column(helper, "aColumnAppearsInsideTheTickingAreaAtTen", 10);
    }

    private static void column(GameTestHelper helper, String test, int s) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Ticking t = new Ticking(helper, s);
        ServerLevel level = t.level;
        String dim = RtsimTests.dimension(helper);
        double edge = t.edge();
        long agent = ConflictKit.newAgent(), other = ConflictKit.newAgent(), envoy = ConflictKit.newAgent();
        long faction = ConflictKit.newFaction();
        ConflictKit.Band band = ConflictKit.band(agent, faction, dim);
        band.arcX = t.px + (s == 8 ? 170 : 200);
        band.arcZ = t.pz;
        band.bearing = 180f;
        band.targetX = t.px + 60;
        band.targetZ = t.pz;
        band.speed = 0.1f;
        band.flags = HOSTILE | CHIEF;
        // The second band comes from 6.5 blocks outside the edge, three chunks north of the first, and is recalled part way.
        ConflictKit.Band second = ConflictKit.band(other, faction, dim);
        second.arcX = edge + 6.5;
        second.arcZ = t.pz + 48;
        second.bearing = 180f;
        second.targetX = t.px + 60;
        second.targetZ = t.pz + 48;
        second.speed = 0.1f;
        second.flags = HOSTILE | CHIEF;
        List<ConflictKit.StackSpec> six = List.of(new ConflictKit.StackSpec(WARRIOR, 6, true));

        ConflictBridge.Stack[] st = new ConflictBridge.Stack[4], bst = new ConflictBridge.Stack[4];
        // Observed every tick: when each stack first stood placed, and what it stood on then.
        long[] firstSeen = new long[4];
        double[] anchorAt = new double[4], bodiesMin = new double[4], bodiesMax = new double[4];
        boolean[] allTicking = {true, true, true, true};
        boolean[] watching = {false};
        int[] flickers = {0};
        long[] before = new long[3]; // requeued, strays, tests snapshots
        int[] testsAtPlaced = new int[4], movesAtPlaced = new int[4];
        waits.eachTick(() -> {
            if (st[0] == null)
                return;
            long now = level.getGameTime();
            for (int i = 0; i < 4; i++) {
                ConflictBridge.StackState state = st[i].state();
                if (state == PLACED && firstSeen[i] == 0) {
                    firstSeen[i] = now;
                    anchorAt[i] = st[i].anchorAt(now)[0];
                    bodiesMin[i] = Double.MAX_VALUE;
                    bodiesMax[i] = -Double.MAX_VALUE;
                    for (Mob m : st[i].bodies()) {
                        bodiesMin[i] = Math.min(bodiesMin[i], m.getX());
                        bodiesMax[i] = Math.max(bodiesMax[i], m.getX());
                        allTicking[i] &= RtsimSession.entitiesTickAt(level, m.blockPosition());
                    }
                    testsAtPlaced[i] = st[i].tests();
                    movesAtPlaced[i] = st[i].moves();
                } else if (state != PLACED && firstSeen[i] != 0 && watching[0]) {
                    flickers[0]++;
                }
            }
        });
        int[] released = {0};

        helper.startSequence()
                .thenExecute(t::start)
                .thenWaitUntil(waits.chunks(t::check))
                .thenExecute(() -> {
                    t.ground();
                    record(test, "ticking area", "simulation distance " + s + ", the player's chunk (" + t.pcx + ", " + t.pcz + "), edge x "
                            + edge + " is " + (edge - t.px) + " blocks east of the player");
                    ConflictKit.materialize(band, six);
                    ConflictKit.materialize(second, six);
                })
                .thenWaitUntil(waits.logic(() -> check("BAND_MATERIALIZED for both bands", !about(ConflictRecords.BAND_MATERIALIZED, agent).isEmpty()
                        && !about(ConflictRecords.BAND_MATERIALIZED, other).isEmpty())))
                .thenExecute(() -> {
                    for (long a : new long[] {agent, other}) {
                        ScriptedEngine.Received m = about(ConflictRecords.BAND_MATERIALIZED, a).get(0);
                        check("one BAND_MATERIALIZED, none placed now (aux " + m.aux() + "): the column is outside the area",
                                about(ConflictRecords.BAND_MATERIALIZED, a).size() == 1 && m.aux() == 0);
                        check("never rejected (" + about(ConflictRecords.BAND_SPAWN_REJECTED, a).size() + ", NOT_TICKING least of all)",
                                about(ConflictRecords.BAND_SPAWN_REJECTED, a).isEmpty());
                    }
                    ConflictBridge.Party p = party(agent), q = party(other);
                    check("three rows and the chief make four stacks (" + p.stacks().size() + "/" + q.stacks().size() + ")",
                            p.stacks().size() == 4 && q.stacks().size() == 4);
                    for (int i = 0; i < 4; i++) {
                        st[i] = p.stacks().get(i);
                        bst[i] = q.stacks().get(i);
                        check("stack " + i + " is a row of " + (i < 3 ? 2 : 1) + " (" + st[i].slots().size() + "), pending, " + (1.5 * i)
                                + " behind the head (" + st[i].back() + ")", st[i].slots().size() == (i < 3 ? 2 : 1) && st[i].state() == PENDING
                                && Math.abs(st[i].back() - 1.5 * i) < 1e-9 && st[i].chief() == (i == 3));
                    }
                    before[0] = bridge().requeued;
                    before[1] = bridge().strays;
                    before[2] = bridge().pendingTests;
                    watching[0] = true;
                })
                // The second band's front stack comes in first, and its chief's stack 45 ticks after it (1.5 blocks a row at 0.1 a tick).
                .thenWaitUntil(waits.logic(() -> check("the second band's front stack in and its chief's not yet (" + states(party(other)) + ")",
                        bst[0].state() == PLACED && bst[3].state() == PENDING)))
                .thenExecute(() -> {
                    record(test, "second band at its recall", states(party(other)) + " at tick " + level.getGameTime());
                    ConflictKit.recall(other, 1, ConflictRecords.RECALL_DONE, 1, t.px - 100, t.pz + 48);
                })
                .thenWaitUntil(waits.logic(() -> check("all four stacks of the first band placed (" + states(party(agent)) + ")",
                        st[0].state() == PLACED && st[1].state() == PLACED && st[2].state() == PLACED && st[3].state() == PLACED)))
                .thenIdle(100)
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    for (int i = 0; i < 4; i++) {
                        double distMin = bodiesMin[i] - t.px, distMax = bodiesMax[i] - t.px;
                        record(test, "stack " + i + " appeared", "tick " + firstSeen[i] + ", anchor x " + String.format("%.2f", anchorAt[i]) + " is "
                                + String.format("%.2f", edge - anchorAt[i]) + " inside the edge, bodies " + String.format("%.2f to %.2f", distMin, distMax)
                                + " blocks east of the player (the edge is " + (edge - t.px) + "), tests " + testsAtPlaced[i] + ", moves " + movesAtPlaced[i]);
                        check("stack " + i + " was placed with every body in an entity-ticking chunk", allTicking[i]);
                        check("stack " + i + " was placed the tick its last member was inside: its place is " + (edge - anchorAt[i]) + " inside the edge "
                                + edge + " (0 to 1 block)", anchorAt[i] < edge && edge - anchorAt[i] <= 1.0);
                        check("stack " + i + " is placed once (" + st[i].placings() + ")", st[i].placings() == 1);
                        int extra = testsAtPlaced[i] - (1 + movesAtPlaced[i]);
                        check("stack " + i + " was tested at the start and once for each move into other chunks: " + testsAtPlaced[i]
                                + " tests, " + movesAtPlaced[i] + " moves", movesAtPlaced[i] >= 1 && extra >= 0 && extra <= 1);
                        if (i > 0)
                            check("front first: stack " + (i - 1) + " at tick " + firstSeen[i - 1] + ", stack " + i + " at " + firstSeen[i],
                                    firstSeen[i - 1] < firstSeen[i]);
                    }
                    check("the stacks already in stayed in: " + flickers[0] + " stack-ticks of a placed stack not placed, " + (bridge().requeued - before[0])
                            + " requeued, " + (bridge().strays - before[1]) + " members moved back", flickers[0] == 0
                            && bridge().requeued == before[0] && bridge().strays == before[1]);
                    check("still never rejected", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    watching[0] = false;
                    before[0] = bridge().requeued;
                    // Out of the area at 0.8 a tick: the rows follow their places, leave the ticking chunks and go back to pending.
                    ConflictKit.steer(agent, 1, 0.8f, t.px + 240, t.pz, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("all four stacks back to pending (" + states(party(agent)) + ")",
                        st[0].state() == PENDING && st[1].state() == PENDING && st[2].state() == PENDING && st[3].state() == PENDING)))
                .thenExecute(() -> {
                    record(test, "walked out", states(party(agent)) + ", " + (bridge().requeued - before[0]) + " requeued, at tick " + level.getGameTime());
                    check("each stack was taken back once (" + (bridge().requeued - before[0]) + ")", bridge().requeued - before[0] == 4);
                    ConflictKit.steer(agent, 1, 0.8f, t.px + 60, t.pz, 0, false);
                })
                .thenWaitUntil(waits.logic(() -> check("all four stacks placed again (" + states(party(agent)) + ")",
                        st[0].state() == PLACED && st[1].state() == PLACED && st[2].state() == PLACED && st[3].state() == PLACED)))
                .thenExecute(() -> {
                    for (int i = 0; i < 4; i++)
                        check("stack " + i + " came again, placed twice (" + st[i].placings() + ")", st[i].placings() == 2);
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 1, t.px - 100, t.pz);
                    // An envoy at the player's side is one group, placed at once.
                    ConflictKit.envoy(envoy, 1, faction, dim, t.px + 12, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) t.px + 12, (int) t.pz),
                            t.pz, null, ConflictRecords.Envoy.DEMAND_TRIBUTE, 0, "minecraft:emerald", 1, level.getGameTime() + 24_000,
                            List.of(new ConflictKit.MemberSpec(ConflictKit.EMISSARY, 1), new ConflictKit.MemberSpec(WARRIOR, 2)));
                })
                .thenWaitUntil(waits.logic(() -> check("both recalls answered and the envoy placed",
                        !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty() && !about(ConflictRecords.BAND_DEMATERIALIZED, other).isEmpty()
                                && !about(ConflictRecords.BAND_MATERIALIZED, envoy).isEmpty())))
                .thenExecute(() -> {
                    Map<String, Integer> first = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, agent).get(0));
                    Map<String, Integer> second2 = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, other).get(0));
                    record(test, "recalls", "the first band's survivors " + first + ", the second's (a front stack in, the chief's pending) " + second2);
                    check("the first band's survivors, the chief not counted: " + first, first.equals(Map.of(WARRIOR, 6)));
                    check("the second band's survivors, in Minecraft or pending: " + second2, second2.equals(Map.of(WARRIOR, 6)));
                    ConflictBridge.Party e = party(envoy);
                    check("an envoy is one group: " + e.stacks().size() + " stack, " + states(e), e.stacks().size() == 1
                            && e.stacks().get(0).state() == PLACED && e.stacks().get(0).bodies().size() == 3);
                    check("BAND_MATERIALIZED with all three (aux " + about(ConflictRecords.BAND_MATERIALIZED, envoy).get(0).aux() + ")",
                            about(ConflictRecords.BAND_MATERIALIZED, envoy).get(0).aux() == 3);
                    ConflictKit.recall(envoy, 1, ConflictRecords.RECALL_DONE, 1, t.px - 100, t.pz);
                })
                .thenWaitUntil(waits.logic(() -> check("the envoy recalled", !about(ConflictRecords.BAND_DEMATERIALIZED, envoy).isEmpty())))
                .thenSucceed();
    }

    // ---- shared pieces of the march tests ---------------------------------------------------------

    /** A far base's centre chunk and the block centre of it. */
    private record Spot(int cx, int cz, double x, double z) {}

    private static Spot spot() {
        int[] base = ConflictKit.farBase();
        int cx = (base[0] + 512) >> 4, cz = (base[1] + 512) >> 4;
        return new Spot(cx, cz, 16.0 * cx + 8.0, 16.0 * cz + 8.0);
    }

    /** A waypoint of a march record: block x and z of the route's chart, and the game tick the band reaches it. */
    private static long[] wp(double x, double z, long tick) {
        return new long[] {(long) Math.floor(x), (long) Math.floor(z), tick};
    }

    /** BAND_MARCH of {@code agent}: its revision, phase and flags, the units it has by body, and its waypoints. */
    private static void say(long agent, int revision, String dim, long faction, int phase, int flags, Map<String, Integer> units, long[]... waypoints) {
        ConflictKit.march(new ConflictKit.MarchSpec(agent, revision, dim, faction, ConflictRecords.PURPOSE_RAID, phase, flags, 0L, units,
                List.of(waypoints)));
    }

    private static final int MARCH_CHIEF = ConflictRecords.March.FLAG_CHIEF, MARCH_ENDED = ConflictRecords.March.FLAG_ENDED;
    private static final int OUT = ConflictRecords.March.OUT, WAITING = ConflictRecords.March.WAITING, HOME = ConflictRecords.March.HOME;

    /** Every stack of the party is in the state, or the check fails naming the states. */
    private static void allStacks(ConflictBridge.Party p, ConflictBridge.StackState want, String what) {
        for (ConflictBridge.Stack st : p.stacks())
            if (st.state() != want)
                throw new GameTestAssertException(what + ": wanted every stack " + want + ", saw " + states(p));
    }

    // ---- the stop (MZB-472) --------------------------------------------------------------------------

    /**
     * The stop (MZB-472, CL-D35) at a simulation distance of 8: a band the engine says is waiting or fighting keeps to the square of
     * blocks one chunk inside the ticking edge of the player it stands near. The chunks of the area are forced (a seen player's
     * area is test 33's), a plain {@link FakePlayers#add} player stands in the middle, and three bands of six warriors and
     * the chief are met on the march (radius 0, rows of two, the chief at the rear):
     * <ul>
     * <li>W appears outside the ticking area, held (speed 0). A march in phase WAITING walks each of its four pending stacks in
     * ({@code walkIns} up by four, each stack's place inside the square), and each is placed once and stays placed;</li>
     * <li>C stands placed inside the square, walking east at 0.3 toward a target outside the ticking area. The same march clamps each
     * walk end into the square ({@code clamps} up by four) and no stack's place passes the square's east edge;</li>
     * <li>K is C with no march: its stacks walk out of the ticking area and are taken back to pending, so it is the stop that held C;</li>
     * <li>a placed stack of W is none the worse for the player standing still: no stack of W or C is ever not placed after it was;</li>
     * <li>one body of W killed with no killer is one BAND_CASUALTIES of kind 0;</li>
     * <li>a march in phase HOME and a BAND_STEER to the head: each stack walks to its column place there (its end is the point plus
     * its offset, {@code back} from the point along the new heading).</li>
     * </ul>
     * The same at a simulation distance of 4 is {@link #aStoppedBandKeepsToTheSquareAtSimulationDistanceFour} (P3-2 of the M10b
     * review: the square is a chunk's margin from the edge, where bodies pushed off their places stay in the ticking area).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_stop")
    public static void aStoppedBandKeepsToThePlayersSquare(GameTestHelper helper) {
        stop(helper, "aStoppedBandKeepsToThePlayersSquare", 8);
    }

    /** The stop at a simulation distance of 4: the ticking area is nine chunks across and the square seven. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_stop4")
    public static void aStoppedBandKeepsToTheSquareAtSimulationDistanceFour(GameTestHelper helper) {
        stop(helper, "aStoppedBandKeepsToTheSquareAtSimulationDistanceFour", 4);
    }

    private static void stop(GameTestHelper helper, String test, int s) {
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.simulation(helper, s);
        f.square(at.cx(), at.cz(), s);
        String dim = RtsimTests.dimension(helper);
        int[] sq = ConflictBridge.stopSquare(at.cx(), at.cz(), s);
        double east = sq[2] + 1.0;                    // the square's east edge (the first x outside it)
        double tickEdge = 16.0 * (at.cx() + s + 1);   // the first x outside the forced, ticking area
        double wx = tickEdge + 24, wz = at.z() - 10;  // W: appears outside the ticking area, in z inside the square
        double cx0 = east - 30, cz0 = at.z() + 14;    // C and K: 30 blocks inside the square, walking east
        double kz = at.z() - 40;
        double farX = tickEdge + 60;
        long faction = ConflictKit.newFaction();
        long w = ConflictKit.newAgent(), c = ConflictKit.newAgent(), k = ConflictKit.newAgent();
        List<ConflictKit.StackSpec> six = List.of(new ConflictKit.StackSpec(WARRIOR, 6, true));
        Map<String, Integer> units = Map.of(WARRIOR, 6);

        ConflictKit.Band bw = ConflictKit.band(w, faction, dim);
        bw.arcX = wx;
        bw.arcZ = wz;
        bw.bearing = 180f;
        bw.targetX = wx;
        bw.targetZ = wz;
        bw.speed = 0f;
        bw.flags = HOSTILE | CHIEF;
        ConflictKit.Band bc = ConflictKit.band(c, faction, dim);
        bc.arcX = cx0;
        bc.arcZ = cz0;
        bc.bearing = 0f;
        bc.targetX = farX;
        bc.targetZ = cz0;
        bc.speed = 0.3f;
        bc.flags = HOSTILE | CHIEF;
        ConflictKit.Band bk = ConflictKit.band(k, faction, dim);
        bk.arcX = cx0;
        bk.arcZ = kz;
        bk.bearing = 0f;
        bk.targetX = farX;
        bk.targetZ = kz;
        bk.speed = 0.3f;
        bk.flags = HOSTILE | CHIEF;

        ConflictBridge.Stack[][] stacks = new ConflictBridge.Stack[3][4]; // W, C, K
        long[][] firstPlaced = new long[3][4];
        int[] flicker = new int[2];      // W, C: observed ticks of a stack that was placed and is not
        long[] kOut = {0};               // the first tick a stack of K was seen taken back
        double[] maxAnchorX = {-1e18, -1e18}; // W, C: the farthest east a stack's place was, once the march was said
        boolean[] watching = {false};
        waits.eachTick(() -> {
            long now = level.getGameTime();
            for (int b = 0; b < 3; b++) {
                if (stacks[b][0] == null)
                    continue;
                for (int i = 0; i < 4; i++) {
                    ConflictBridge.Stack st = stacks[b][i];
                    if (st.state() == PLACED && firstPlaced[b][i] == 0) {
                        firstPlaced[b][i] = now;
                    } else if (st.state() != PLACED && firstPlaced[b][i] != 0) {
                        if (b < 2)
                            flicker[b]++;
                        else if (kOut[0] == 0)
                            kOut[0] = now;
                    }
                    if (watching[0] && b < 2)
                        maxAnchorX[b] = Math.max(maxAnchorX[b], st.anchorAt(now)[0]);
                }
            }
        });
        ServerPlayer[] player = new ServerPlayer[1];
        long[] before = new long[2];   // walkIns, clamps
        long[] said = new long[2];     // the tick the march was sent, the tick W was seen placed
        long[] cas = new long[1];

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), s)))
                .thenExecute(() -> {
                    player[0] = FakePlayers.add(level, new Vec3(at.x(), f.ground(at.x(), at.z()), at.z()), "conflict_march_stop_watcher");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    record(test, "area", "simulation distance " + s + ", the player's chunk (" + at.cx() + ", " + at.cz() + "), the square x "
                            + sq[0] + " to " + sq[2] + " and z " + sq[1] + " to " + sq[3] + ", the ticking area ends at x " + tickEdge);
                    check("the square lies inside the ticking area by at least a chunk on the east: edge " + east + ", ticking edge " + tickEdge,
                            tickEdge - east >= 16.0);
                    ConflictKit.materialize(bw, six);
                    ConflictKit.materialize(bc, six);
                    ConflictKit.materialize(bk, six);
                })
                .thenWaitUntil(waits.logic(() -> check("a BAND_MATERIALIZED for each band",
                        !about(ConflictRecords.BAND_MATERIALIZED, w).isEmpty() && !about(ConflictRecords.BAND_MATERIALIZED, c).isEmpty()
                                && !about(ConflictRecords.BAND_MATERIALIZED, k).isEmpty())))
                .thenExecute(() -> {
                    long[] agents = {w, c, k};
                    for (int b = 0; b < 3; b++) {
                        ConflictBridge.Party p = party(agents[b]);
                        check("band " + b + " is four stacks (three rows and the chief): " + states(p), p.stacks().size() == 4);
                        for (int i = 0; i < 4; i++)
                            stacks[b][i] = p.stacks().get(i);
                    }
                    check("W appears outside the ticking area, every stack pending: " + states(party(w)), stacks[0][0].state() == PENDING
                            && stacks[0][3].state() == PENDING);
                })
                .thenWaitUntil(waits.logic(() -> {
                    allStacks(party(c), PLACED, "C is placed inside the square");
                    allStacks(party(k), PLACED, "K is placed inside the square");
                }))
                .thenIdle(20)
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    before[0] = ConflictKit.walkIns();
                    before[1] = ConflictKit.clamps();
                    said[0] = now;
                    watching[0] = true;
                    say(w, 1, dim, faction, WAITING, MARCH_CHIEF, units, wp(at.x() + 8, wz, now));
                    say(c, 1, dim, faction, WAITING, MARCH_CHIEF, units, wp(cx0 - 20, cz0, now));
                })
                .thenWaitUntil(waits.logic(() -> allStacks(party(w), PLACED, "W's pending stacks walk in and are placed")))
                .thenExecute(() -> {
                    said[1] = level.getGameTime();
                    long walked = ConflictKit.walkIns() - before[0];
                    record(test, "W walked in", walked + " walk-ins, placed " + (said[1] - said[0]) + " ticks after the march was said (the stop's beat is "
                            + ConflictBridge.STOP_BEAT + ")");
                    check("each of W's four pending stacks walked in once (" + walked + ")", walked == 4);
                    check("the walk-in came by the first beat, " + (said[1] - said[0]) + " ticks after the march was said", said[1] - said[0] <= ConflictBridge.STOP_BEAT + 2);
                    for (ConflictBridge.Stack st : stacks[0]) {
                        double[] end = st.target();
                        check("W stack " + st.index + " walks to a place inside the square: end " + xz(end) + ", square x " + sq[0] + " to " + (sq[2] + 1)
                                + ", z " + sq[1] + " to " + (sq[3] + 1), end[0] >= sq[0] && end[0] < east && end[1] >= sq[1] && end[1] < sq[3] + 1.0);
                    }
                })
                .thenIdle(300)
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    long clamped = ConflictKit.clamps() - before[1];
                    record(test, "C held", clamped + " clamps, the farthest east a place reached x " + String.format("%.2f", maxAnchorX[1])
                            + " (the square's edge is " + east + "), K first taken back at tick " + kOut[0] + " (the march was said at " + said[0] + ")");
                    check("each of C's four placed stacks had its end clamped once (" + clamped + ")", clamped == 4);
                    check("no place of C passed the square's east edge " + east + ": the farthest was " + maxAnchorX[1], maxAnchorX[1] < east);
                    check("no place of W passed it either: " + maxAnchorX[0], maxAnchorX[0] < east);
                    check("no stack of W or C was taken back after it was placed (" + flicker[0] + " and " + flicker[1] + " observed ticks)",
                            flicker[0] == 0 && flicker[1] == 0);
                    for (int b = 0; b < 2; b++)
                        for (ConflictBridge.Stack st : stacks[b])
                            check("band " + b + " stack " + st.index + " is placed once and still placed (" + st.placings() + " placings, " + st.state() + ")",
                                    st.placings() == 1 && st.state() == PLACED);
                    check("K, with no march, left the ticking area and was taken back (first at tick " + kOut[0] + "): " + states(party(k)), kOut[0] != 0
                            && stacks[2][0].state() != PLACED);
                    // A body of W killed with no killer: one BAND_CASUALTIES of kind 0 (by none).
                    cas[0] = about(ConflictRecords.BAND_CASUALTIES, w).size();
                    Mob body = stacks[0][0].bodies().get(0);
                    body.hurt(level.damageSources().generic(), 1000f);
                })
                .thenWaitUntil(waits.logic(() -> check("a casualty for W was journaled", about(ConflictRecords.BAND_CASUALTIES, w).size() == cas[0] + 1)))
                .thenExecute(() -> {
                    check("the casualty is of kind 0 (a death with no killer), aux " + about(ConflictRecords.BAND_CASUALTIES, w).get((int) cas[0]).aux(),
                            about(ConflictRecords.BAND_CASUALTIES, w).get((int) cas[0]).aux() == 0);
                    // The walk home: the march turns to HOME and the band is steered to its head's spot.
                    long now = level.getGameTime();
                    double hx = at.x() + 8, hz = wz;
                    say(w, 2, dim, faction, HOME, MARCH_CHIEF, units, wp(hx, hz, now), wp(hx, hz, now + 100_000));
                    ConflictKit.steer(w, 1, 0.5f, hx, hz, 0, false);
                    said[0] = now;
                })
                .thenWaitUntil(waits.logic(() -> {
                    long now = level.getGameTime();
                    for (ConflictBridge.Stack st : stacks[0]) {
                        double[] a = st.anchorAt(now), e = st.target();
                        if (Math.hypot(a[0] - e[0], a[1] - e[1]) > 0.05)
                            throw new GameTestAssertException("W stack " + st.index + " still walks home: place " + xz(a) + ", end " + xz(e));
                    }
                }))
                .thenExecute(() -> {
                    double hx = at.x() + 8, hz = wz;
                    for (ConflictBridge.Stack st : stacks[0]) {
                        double[] e = st.target();
                        double d = Math.hypot(e[0] - hx, e[1] - hz);
                        record(test, "W home, stack " + st.index, "end " + xz(e) + " is " + String.format("%.3f", d) + " from the point (" + hx + ", " + hz + "), back " + st.back());
                        check("W stack " + st.index + " stands " + st.back() + " behind the point along the column (" + d + " away, end " + xz(e) + ")",
                                Math.abs(d - st.back()) < 0.02);
                    }
                    check("no stack of W went to pending on the way: " + states(party(w)) + ", flicker " + flicker[0], flicker[0] == 0);
                    for (long a : new long[] {w, c, k})
                        ConflictKit.recall(a, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("all three recalled", !about(ConflictRecords.BAND_DEMATERIALIZED, w).isEmpty()
                        && !about(ConflictRecords.BAND_DEMATERIALIZED, c).isEmpty() && !about(ConflictRecords.BAND_DEMATERIALIZED, k).isEmpty())))
                .thenSucceed();
    }

    /** The members of a stack that are alive (the stack's own count is package-private). */
    private static int aliveIn(ConflictBridge.Stack st) {
        int n = 0;
        for (ConflictBridge.Slot sl : st.slots())
            if (sl.alive())
                n++;
        return n;
    }

    // ---- the trim (MZB-473) ---------------------------------------------------------------------------

    /**
     * The trim (MZB-473) with a player's sight in it. A band of six warriors and the chief stands placed, held (speed 0), in forced
     * chunks. A march that matches it cuts nothing. A march one warrior short, with a plain player 30 blocks from the band:
     * <ul>
     * <li>nothing is cut while the player stands there, for 60 ticks ({@code trimmed} unmoved), and the party stays due ({@code trimDue});</li>
     * <li>the player taken out at the middle of a beat, the cut comes on the beat that follows (9 to 13 ticks on, never at once)
     * and it is the rearmost member (the last of the last row), the one the march says is gone; the party is due no more;</li>
     * <li>a march that lacks the chief, with no one near, cuts the chief in the tick the record arrives;</li>
     * <li>neither cut journals a casualty nor a chief's death, and the recall lists five warriors as survivors (a cut member is no survivor).</li>
     * </ul>
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_trim")
    public static void theTrimNeverCutsInSightAndCutsTheRearmostOnTheBeat(GameTestHelper helper) {
        String test = "theTrimNeverCutsInSightAndCutsTheRearmostOnTheBeat";
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 2);
        String dim = RtsimTests.dimension(helper);
        long faction = ConflictKit.newFaction(), agent = ConflictKit.newAgent();
        Map<String, Integer> six = Map.of(WARRIOR, 6), five = Map.of(WARRIOR, 5);
        ConflictKit.Band band = ConflictKit.band(agent, faction, dim);
        band.arcX = at.x();
        band.arcZ = at.z();
        band.bearing = 180f;
        band.targetX = at.x();
        band.targetZ = at.z();
        band.speed = 0f;
        band.flags = HOSTILE | CHIEF;

        ServerPlayer[] player = new ServerPlayer[1];
        long[] n = new long[4]; // trimmed before the record, marchRecords before, the tick the record was sent, the tick the player went
        boolean[] watching = {false};
        int[] cutInSight = {0}, notDue = {0};
        waits.eachTick(() -> {
            if (!watching[0])
                return;
            if (ConflictKit.trimmed() != n[0])
                cutInSight[0]++;
            if (level.getGameTime() > n[2] + 2 && !party(agent).trimDue())
                notDue[0]++;
        });

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 2)))
                .thenExecute(() -> ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(WARRIOR, 6, true))))
                .thenWaitUntil(waits.logic(() -> allStacks(party(agent), PLACED, "the band is placed")))
                .thenExecute(() -> {
                    ConflictBridge.Party p = party(agent);
                    check("three rows and the chief: " + states(p), p.stacks().size() == 4);
                    n[0] = ConflictKit.trimmed();
                    n[1] = bridge().marchRecords;
                    long now = level.getGameTime();
                    say(agent, 1, dim, faction, OUT, MARCH_CHIEF, six, wp(at.x(), at.z(), now), wp(at.x() - 200, at.z(), now + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("the march was kept", bridge().marchRecords == n[1] + 1)))
                .thenIdle(10)
                .thenExecute(() -> {
                    check("a march that matches the band cuts nothing (" + (ConflictKit.trimmed() - n[0]) + " cut)", ConflictKit.trimmed() == n[0]);
                    check("and the band is not due", !party(agent).trimDue());
                    double y = f.ground(at.x() - 30, at.z());
                    player[0] = FakePlayers.add(level, new Vec3(at.x() - 30, y, at.z()), "conflict_march_trim_watcher");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    n[1] = bridge().marchRecords;
                    n[2] = level.getGameTime();
                    watching[0] = true;
                    say(agent, 2, dim, faction, OUT, MARCH_CHIEF, five, wp(at.x(), at.z(), n[2]), wp(at.x() - 200, at.z(), n[2] + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("the march was kept", bridge().marchRecords == n[1] + 1)))
                .thenIdle(60)
                .thenExecute(() -> {
                    record(test, "in sight", "the player stood 30 blocks off for 60 ticks: " + (ConflictKit.trimmed() - n[0]) + " cut, " + cutInSight[0]
                            + " ticks with a cut, " + notDue[0] + " ticks not due");
                    check("nothing was cut while a player stood in sight (" + cutInSight[0] + " ticks saw a cut, trimmed " + (ConflictKit.trimmed() - n[0]) + ")",
                            cutInSight[0] == 0 && ConflictKit.trimmed() == n[0]);
                    check("the band stayed due the whole time (" + notDue[0] + " ticks not due)", notDue[0] == 0 && party(agent).trimDue());
                })
                // Out at the middle of a beat: the cut waits for the beat.
                .thenWaitUntil(waits.logic(() -> check("the middle of a beat, tick " + level.getGameTime(), level.getGameTime() % 20 == 10)))
                .thenExecute(() -> {
                    n[3] = level.getGameTime();
                    FakePlayers.remove(player[0]);
                    watching[0] = false;
                })
                .thenWaitUntil(waits.logic(() -> check("the member was cut (trimmed " + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 1)))
                .thenExecute(() -> {
                    long cutAt = level.getGameTime();
                    ConflictBridge.Party p = party(agent);
                    record(test, "on the beat", "the player went at tick " + n[3] + ", the cut was seen at tick " + cutAt + " (mod 20: " + cutAt % 20 + ")");
                    check("the cut waited for the beat: seen " + (cutAt - n[3]) + " ticks after the player went (tick " + cutAt + ")",
                            cutAt - n[3] >= 9 && cutAt - n[3] <= 13 && cutAt % 20 <= 2);
                    check("the party is due no more", !p.trimDue());
                    ConflictBridge.Slot last = p.stacks().get(2).slots().get(1);
                    check("the cut member is the rearmost, stack 2's second (alive " + last.alive() + ")", !last.alive());
                    int alive = 0;
                    for (ConflictBridge.Stack st : p.stacks())
                        for (ConflictBridge.Slot sl : st.slots())
                            if (sl.alive() && !sl.chief)
                                alive++;
                    check("five warriors are left (" + alive + ")", alive == 5);
                    check("its body is out of the world", last.entity() == null || last.entity().isRemoved());
                })
                // A march with no chief: the chief goes in the tick the record arrives, with no one near.
                .thenWaitUntil(waits.logic(() -> check("the middle of a beat, tick " + level.getGameTime(), level.getGameTime() % 20 == 10)))
                .thenExecute(() -> {
                    n[2] = level.getGameTime();
                    say(agent, 3, dim, faction, OUT, 0, five, wp(at.x(), at.z(), n[2]), wp(at.x() - 200, at.z(), n[2] + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("the chief was cut (trimmed " + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 2)))
                .thenExecute(() -> {
                    long cutAt = level.getGameTime();
                    ConflictBridge.Stack chief = party(agent).stacks().get(3);
                    record(test, "no chief", "the march was sent at tick " + n[2] + ", the cut was seen at tick " + cutAt);
                    check("the chief went with the record, not on a beat: " + (cutAt - n[2]) + " ticks", cutAt - n[2] <= 6);
                    check("the chief's stack is gone: " + states(party(agent)), chief.chief() && chief.state() == GONE);
                    check("no casualty and no chief killed were journaled (" + about(ConflictRecords.BAND_CASUALTIES, agent).size() + ", "
                            + about(ConflictRecords.CHIEF_KILLED, agent).size() + ")", about(ConflictRecords.BAND_CASUALTIES, agent).isEmpty()
                            && about(ConflictRecords.CHIEF_KILLED, agent).isEmpty());
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("recalled", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty())))
                .thenExecute(() -> {
                    Map<String, Integer> left = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, agent).get(0));
                    record(test, "survivors", left.toString());
                    check("a cut member is no survivor: " + left, left.equals(Map.of(WARRIOR, 5)));
                })
                .thenSucceed();
    }

    /**
     * The trim's other cases (MZB-473), in forced chunks with a plain player 18 blocks from the placed stacks:
     * <ul>
     * <li>P stands half in the ticking area: its first two rows are placed and its third row and chief are pending;</li>
     * <li>a march that matches cuts nothing; a march of another dimension (units 1 against six) trims nothing and is not due, and is held;</li>
     * <li>E, a band whose march is ENDED (units 1 against six): nothing is cut, the path is dropped and the agent remembered, and a
     * later record of it is stale (one more {@code marchStale}) and trims nothing;</li>
     * <li>P's march one warrior short, the player in sight of its placed rows: the rearmost pending member goes in the tick the record
     * arrives (the third row's second), and the band is not due; a march with no chief cuts the pending chief at once;</li>
     * <li>a march of three warriors then cuts the last pending member and no more: every member left is in sight, so the party is
     * due ({@code trimDue}) with four on it;</li>
     * <li>the recall lists four survivors for P, six for E and D.</li>
     * </ul>
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_trim2")
    public static void thePendingAreCutAtOnceAndAnEndedOrForeignRecordTrimsNothing(GameTestHelper helper) {
        String test = "thePendingAreCutAtOnceAndAnEndedOrForeignRecordTrimsNothing";
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 1);
        String dim = RtsimTests.dimension(helper);
        double east = 16.0 * (at.cx() + 2);
        long faction = ConflictKit.newFaction();
        long p = ConflictKit.newAgent(), e = ConflictKit.newAgent(), d = ConflictKit.newAgent();
        Map<String, Integer> six = Map.of(WARRIOR, 6), five = Map.of(WARRIOR, 5), three = Map.of(WARRIOR, 3), one = Map.of(WARRIOR, 1);
        double[][] heads = {{east - 2.25, at.z()}, {at.x() - 8, at.z() - 10}, {at.x() - 8, at.z() + 10}};
        long[] agents = {p, e, d};
        List<ConflictKit.StackSpec> six4 = List.of(new ConflictKit.StackSpec(WARRIOR, 6, true));
        ServerPlayer[] player = new ServerPlayer[1];
        long[] n = new long[6]; // trimmed, marchRecords, marchStale, marchEnds before a step; the tick a step began

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 1)))
                .thenExecute(() -> {
                    for (int b = 0; b < 3; b++) {
                        ConflictKit.Band band = ConflictKit.band(agents[b], faction, dim);
                        band.arcX = heads[b][0];
                        band.arcZ = heads[b][1];
                        band.bearing = 180f;
                        band.targetX = heads[b][0];
                        band.targetZ = heads[b][1];
                        band.speed = 0f;
                        band.flags = HOSTILE | CHIEF;
                        ConflictKit.materialize(band, six4);
                    }
                })
                .thenWaitUntil(waits.logic(() -> check("every band appeared", bridge().party(p) != null && bridge().party(e) != null && bridge().party(d) != null
                        && bridge().party(e).stacks().get(3).state() == PLACED && bridge().party(d).stacks().get(3).state() == PLACED
                        && bridge().party(p).stacks().get(1).state() == PLACED)))
                .thenExecute(() -> {
                    ConflictBridge.Party pp = party(p);
                    record(test, "P at the edge", "head x " + heads[0][0] + ", the ticking edge " + east + ": " + states(pp));
                    check("P's first two rows are placed and its third row and chief pending: " + states(pp), pp.stacks().size() == 4
                            && pp.stacks().get(0).state() == PLACED && pp.stacks().get(1).state() == PLACED
                            && pp.stacks().get(2).state() == PENDING && pp.stacks().get(3).state() == PENDING);
                    n[0] = ConflictKit.trimmed();
                    n[1] = bridge().marchRecords;
                    long now = level.getGameTime();
                    say(p, 1, dim, faction, OUT, MARCH_CHIEF, six, wp(heads[0][0], heads[0][1], now), wp(heads[0][0] - 200, heads[0][1], now + 10_000));
                    say(e, 1, dim, faction, OUT, MARCH_CHIEF, six, wp(heads[1][0], heads[1][1], now), wp(heads[1][0] - 200, heads[1][1], now + 10_000));
                    // A march of another level: its units are one against the band's six, and it is not this band's books.
                    say(d, 1, "minecraft:the_nether", faction, OUT, MARCH_CHIEF, one, wp(0, 0, now), wp(100, 0, now + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("three marches kept", bridge().marchRecords == n[1] + 3)))
                .thenIdle(5)
                .thenExecute(() -> {
                    check("a matching march and a march of another dimension cut nothing (" + (ConflictKit.trimmed() - n[0]) + " cut)", ConflictKit.trimmed() == n[0]);
                    check("none is due", !party(p).trimDue() && !party(e).trimDue() && !party(d).trimDue());
                    ConflictBridge.Party dp = party(d);
                    check("the other dimension's march is held as it is said: " + (bridge().march(d) == null ? "none" : bridge().march(d).dimension()),
                            bridge().march(d) != null && "minecraft:the_nether".equals(bridge().march(d).dimension()));
                    check("D still has its six warriors and chief: " + bridge().bodies(d).size() + " bodies", bridge().bodies(d).size() == 7);
                    // E's march ends, saying one unit: nothing is cut.
                    n[2] = bridge().marchEnds;
                    say(e, 2, dim, faction, OUT, MARCH_CHIEF | MARCH_ENDED, one, wp(heads[1][0], heads[1][1], level.getGameTime()));
                })
                .thenWaitUntil(waits.logic(() -> check("E's end was counted", bridge().marchEnds == n[2] + 1)))
                .thenIdle(5)
                .thenExecute(() -> {
                    check("an ENDED march trims nothing (" + (ConflictKit.trimmed() - n[0]) + " cut)", ConflictKit.trimmed() == n[0]);
                    check("E's path is dropped and its agent remembered: path " + bridge().march(e) + ", ended " + bridge().marchEnded(e),
                            bridge().march(e) == null && bridge().marchEnded(e));
                    check("E keeps its seven bodies (" + bridge().bodies(e).size() + ")", bridge().bodies(e).size() == 7);
                    n[3] = bridge().marchStale;
                    say(e, 3, dim, faction, OUT, MARCH_CHIEF, one, wp(heads[1][0], heads[1][1], level.getGameTime()));
                })
                .thenWaitUntil(waits.logic(() -> check("E's later record was stale", bridge().marchStale == n[3] + 1)))
                .thenIdle(5)
                .thenExecute(() -> {
                    check("a later record of an ended agent trims nothing and keeps no path (" + (ConflictKit.trimmed() - n[0]) + " cut, path " + bridge().march(e) + ")",
                            ConflictKit.trimmed() == n[0] && bridge().march(e) == null);
                    // The player, in sight of P's placed rows and every other band.
                    double px = heads[0][0] + 18;
                    player[0] = FakePlayers.add(level, new Vec3(px, f.ground(px, at.z()), at.z()), "conflict_march_trim2_watcher");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                })
                // P is one warrior short with a player in sight of its placed rows: the pending third row gives it up at once.
                .thenWaitUntil(waits.logic(() -> check("the middle of a beat, tick " + level.getGameTime(), level.getGameTime() % 20 == 10)))
                .thenExecute(() -> {
                    n[4] = level.getGameTime();
                    say(p, 2, dim, faction, OUT, MARCH_CHIEF, five, wp(heads[0][0], heads[0][1], n[4]), wp(heads[0][0] - 200, heads[0][1], n[4] + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("a pending member was cut (" + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 1)))
                .thenExecute(() -> {
                    long cutAt = level.getGameTime();
                    ConflictBridge.Party pp = party(p);
                    record(test, "pending first", "the march was sent at tick " + n[4] + ", the cut was seen at tick " + cutAt + ", " + states(pp));
                    check("the pending member went with the record, in sight of the rest: " + (cutAt - n[4]) + " ticks", cutAt - n[4] <= 6);
                    check("it is stack 2's second member, and its stack still stands pending with the first: " + states(pp), !pp.stacks().get(2).slots().get(1).alive()
                            && pp.stacks().get(2).slots().get(0).alive() && pp.stacks().get(2).state() == PENDING);
                    check("no placed member was cut", aliveIn(pp.stacks().get(0)) == 2 && aliveIn(pp.stacks().get(1)) == 2);
                    check("the band is not due", !pp.trimDue());
                    // No chief on the march: the pending chief goes at once.
                    n[4] = level.getGameTime();
                    say(p, 3, dim, faction, OUT, 0, five, wp(heads[0][0], heads[0][1], n[4]), wp(heads[0][0] - 200, heads[0][1], n[4] + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("the pending chief was cut (" + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 2)))
                .thenExecute(() -> {
                    ConflictBridge.Party pp = party(p);
                    check("the chief's stack is gone: " + states(pp), pp.stacks().get(3).state() == GONE);
                    check("the band is not due", !pp.trimDue());
                    // Three warriors: two to cut, one pending and the rest in sight.
                    n[4] = level.getGameTime();
                    say(p, 4, dim, faction, OUT, 0, three, wp(heads[0][0], heads[0][1], n[4]), wp(heads[0][0] - 200, heads[0][1], n[4] + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("the last pending member was cut (" + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 3)))
                .thenIdle(40)
                .thenExecute(() -> {
                    ConflictBridge.Party pp = party(p);
                    record(test, "blocked", "trimmed " + (ConflictKit.trimmed() - n[0]) + " after 40 ticks, due " + pp.trimDue() + ", " + states(pp));
                    check("only the pending member went: every member left is in sight (" + (ConflictKit.trimmed() - n[0]) + " cut)", ConflictKit.trimmed() == n[0] + 3);
                    check("stack 2 is gone and the placed rows have their four: " + states(pp), pp.stacks().get(2).state() == GONE
                            && aliveIn(pp.stacks().get(0)) + aliveIn(pp.stacks().get(1)) == 4);
                    check("the band is due, waiting for the player to go", pp.trimDue());
                    FakePlayers.remove(player[0]);
                    for (long a : agents)
                        ConflictKit.recall(a, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("every band recalled", !about(ConflictRecords.BAND_DEMATERIALIZED, p).isEmpty()
                        && !about(ConflictRecords.BAND_DEMATERIALIZED, e).isEmpty() && !about(ConflictRecords.BAND_DEMATERIALIZED, d).isEmpty())))
                .thenExecute(() -> {
                    Map<String, Integer> sp = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, p).get(0));
                    Map<String, Integer> se = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, e).get(0));
                    Map<String, Integer> sd = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, d).get(0));
                    record(test, "survivors", "P " + sp + ", E " + se + ", D " + sd);
                    check("P's survivors are the four left: " + sp, sp.equals(Map.of(WARRIOR, 4)));
                    check("E's and D's survivors are all six: " + se + ", " + sd, se.equals(Map.of(WARRIOR, 6)) && sd.equals(Map.of(WARRIOR, 6)));
                })
                .thenSucceed();
    }

    // ---- test 34: the march record and the bodies --------------------------------------------------

    /**
     * Test 34 (MOWZIE.md §7, MZB-468, MZB-477, MZB-722; the JUnit half is {@code MarchPathTest}, MZB-720 and MZB-477's three
     * vectors). Three parts, in forced chunks of a far base:
     * <ol>
     * <li>The record's layout and rules, with an agent that has no band: a march of two units and three waypoints reads back
     * whole (revision, dimension, faction, kind, phase, flags, units, every waypoint); a lower and an equal revision are
     * dropped ({@code marchStale}) and the held record stays the same object; a higher one replaces it whole; an ENDED one
     * drops the path and remembers the agent ({@code marchEnds}); a later record of the agent is dropped.</li>
     * <li>A band met on the march (radius 0, bearing 180) of four warriors and their chief stands as three stacks: back
     * 0, 1.5 and 3.0, the chief's last, rows of two with the bodies 1.2 blocks apart, the first stack's anchor (the head) in the
     * block {@code march.at(now)} and the rows 1.5 blocks apart along the heading.</li>
     * <li>The moving form: the band walks 8 blocks east and 8 south at 0.02 blocks a tick, two steers aligned to the march's
     * waypoints (400 ticks each). The head (the first stack's anchor) is compared with {@code march.at(now)} every 20 ticks
     * and logged (it must stay within 1.5 blocks on each axis: the chart is whole blocks). After each steer's walk the stacks'
     * anchors lie where the turned column puts them (the head, and behind it along the new heading by each stack's back), and the
     * bodies are within the follow goal's leash of their places. These are positions, not counters.</li>
     * </ol>
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_march")
    public static void theMarchRecordMatchesTheBodies(GameTestHelper helper) {
        String test = "theMarchRecordMatchesTheBodies";
        ChunkWait waits = ChunkWait.of(helper, 8000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 2);
        String dim = RtsimTests.dimension(helper);
        long faction = ConflictKit.newFaction();
        long ghost = ConflictKit.newAgent(), agent = ConflictKit.newAgent();
        Map<String, Integer> two = new LinkedHashMap<>();
        two.put(WARRIOR, 4);
        two.put(ARCHER, 2);
        MarchPath[] held = new MarchPath[1];
        long[] n = new long[6]; // marchRecords, marchStale, marchEnds before a step; the tick the walk began; spare
        ConflictKit.Band band = ConflictKit.band(agent, faction, dim);
        band.arcX = at.x();
        band.arcZ = at.z();
        band.bearing = 180f;
        band.targetX = at.x();
        band.targetZ = at.z();
        band.speed = 0f;
        band.flags = HOSTILE | CHIEF;
        Map<String, Integer> four = Map.of(WARRIOR, 4);

        // The walk's samples: the head against march.at(now) every 20 ticks.
        boolean[] sampling = {false};
        StringBuilder samples = new StringBuilder();
        double[] worst = new double[1];
        waits.eachTick(() -> {
            if (!sampling[0])
                return;
            long now = level.getGameTime();
            ConflictBridge.Party sp = bridge().party(agent);
            if ((now - n[3]) % 20 != 0 || sp == null)
                return;
            MarchPath m = bridge().march(agent);
            int[] want = m == null ? null : m.at(now, RtsimApi.topology(level));
            double[] head = sp.stacks().get(0).anchorAt(now);
            if (want == null)
                return;
            double dx = head[0] - (want[0] + 0.5), dz = head[1] - (want[1] + 0.5);
            worst[0] = Math.max(worst[0], Math.max(Math.abs(dx), Math.abs(dz)));
            samples.append(now - n[3]).append(": head ").append(xz(head)).append(" want (").append(want[0]).append(", ").append(want[1])
                    .append(") d (").append(String.format("%.2f, %.2f", dx, dz)).append(")\n");
        });

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 2)))
                // ---- 1. the record
                .thenExecute(() -> {
                    n[0] = bridge().marchRecords;
                    long now = level.getGameTime();
                    n[3] = now;
                    say(ghost, 5, dim, faction, OUT, MARCH_CHIEF, two, wp(at.x(), at.z(), now), wp(at.x() + 100, at.z() + 37, now + 100),
                            wp(at.x() + 100, at.z() - 50, now + 200));
                })
                .thenWaitUntil(waits.logic(() -> check("the record was read", bridge().marchRecords == n[0] + 1 && bridge().march(ghost) != null)))
                .thenExecute(() -> {
                    MarchPath m = bridge().march(ghost);
                    held[0] = m;
                    long t0 = n[3];
                    check("the revision, dimension and faction come back: " + m.revision() + ", " + m.dimension() + ", " + m.faction(),
                            m.revision() == 5 && dim.equals(m.dimension()) && m.faction() == faction && m.agent() == ghost);
                    check("the kind, phase and flags come back: " + m.kind() + ", " + m.phase() + ", " + m.flags(),
                            m.kind() == ConflictRecords.PURPOSE_RAID && m.phase() == OUT && m.flags() == MARCH_CHIEF && m.chief() && !m.ended());
                    check("two units, with their counts: " + m.units(), m.units().size() == 2 && m.count(WARRIOR) == 4 && m.count(ARCHER) == 2
                            && m.count("minecraft:pig") == 0);
                    check("three waypoints", m.waypoints() == 3);
                    long[][] want = {{(long) Math.floor(at.x()), (long) Math.floor(at.z()), t0}, {(long) Math.floor(at.x() + 100), (long) Math.floor(at.z() + 37), t0 + 100},
                            {(long) Math.floor(at.x() + 100), (long) Math.floor(at.z() - 50), t0 + 200}};
                    for (int i = 0; i < 3; i++)
                        check("waypoint " + i + " is (" + m.waypointX(i) + ", " + m.waypointZ(i) + ") at " + m.waypointTick(i) + ", wanted ("
                                        + want[i][0] + ", " + want[i][1] + ") at " + want[i][2],
                                m.waypointX(i) == want[i][0] && m.waypointZ(i) == want[i][1] && m.waypointTick(i) == want[i][2]);
                    // The route through the record: the ends, a waypoint, the middle (MZB-477's flat vector is MarchPathTest's).
                    int[] before = m.chartAt(t0 - 5), mid = m.chartAt(t0 + 50), second = m.chartAt(t0 + 100), after = m.chartAt(t0 + 999);
                    record(test, "route", "before " + before[0] + "," + before[1] + " mid " + mid[0] + "," + mid[1] + " second " + second[0] + ","
                            + second[1] + " after " + after[0] + "," + after[1]);
                    check("the route starts at the first waypoint and ends at the last", before[0] == want[0][0] && before[1] == want[0][1]
                            && after[0] == want[2][0] && after[1] == want[2][1]);
                    check("it is at the second waypoint at its tick", second[0] == want[1][0] && second[1] == want[1][1]);
                    check("half way along the first leg it is half way, to a block: (" + mid[0] + ", " + mid[1] + ")",
                            Math.abs(mid[0] - (want[0][0] + 50)) <= 1 && Math.abs(mid[1] - (want[0][1] + 18)) <= 1);
                    int[] folded = m.at(t0 + 100, RtsimApi.topology(level));
                    check("in the flat test level the folded position is the chart's: (" + folded[0] + ", " + folded[1] + ")",
                            folded[0] == want[1][0] && folded[1] == want[1][1]);
                    // A lower revision, and the same one: dropped.
                    n[1] = bridge().marchStale;
                    say(ghost, 4, dim, faction, WAITING, 0, four, wp(at.x(), at.z(), t0));
                    say(ghost, 5, dim, faction, WAITING, 0, four, wp(at.x(), at.z(), t0));
                })
                .thenWaitUntil(waits.logic(() -> check("both records were dropped (stale " + (bridge().marchStale - n[1]) + ")", bridge().marchStale == n[1] + 2)))
                .thenExecute(() -> {
                    check("the held record is the same, whole", bridge().march(ghost) == held[0] && held[0].revision() == 5 && held[0].phase() == OUT);
                    // A higher revision replaces it whole.
                    say(ghost, 6, dim, faction, WAITING, 0, four, wp(at.x() + 7, at.z() + 9, n[3]));
                })
                .thenWaitUntil(waits.logic(() -> check("the newer record replaced the old", bridge().march(ghost) != held[0])))
                .thenExecute(() -> {
                    MarchPath m = bridge().march(ghost);
                    check("the newer record is whole: rev " + m.revision() + ", phase " + m.phase() + ", units " + m.units() + ", waypoints " + m.waypoints(),
                            m.revision() == 6 && m.phase() == WAITING && !m.chief() && m.units().equals(Map.of(WARRIOR, 4)) && m.waypoints() == 1);
                    int[] w = m.chartAt(n[3] + 12345);
                    check("waiting, it stands at its last waypoint: (" + w[0] + ", " + w[1] + ")", w[0] == (int) Math.floor(at.x() + 7) && w[1] == (int) Math.floor(at.z() + 9));
                    // ENDED drops the path and the agent's later records.
                    n[2] = bridge().marchEnds;
                    say(ghost, 7, dim, faction, WAITING, MARCH_CHIEF | MARCH_ENDED, four, wp(at.x(), at.z(), n[3]));
                })
                .thenWaitUntil(waits.logic(() -> check("the end was counted (" + (bridge().marchEnds - n[2]) + ")", bridge().marchEnds == n[2] + 1)))
                .thenExecute(() -> {
                    check("the path is dropped and the agent remembered: path " + bridge().march(ghost) + ", ended " + bridge().marchEnded(ghost),
                            bridge().march(ghost) == null && bridge().marchEnded(ghost));
                    n[1] = bridge().marchStale;
                    say(ghost, 8, dim, faction, OUT, MARCH_CHIEF, two, wp(at.x(), at.z(), n[3]), wp(at.x() + 9, at.z(), n[3] + 9));
                })
                .thenWaitUntil(waits.logic(() -> check("a later record of the agent was dropped", bridge().marchStale == n[1] + 1)))
                .thenExecute(() -> {
                    check("and holds no path: " + bridge().march(ghost), bridge().march(ghost) == null);
                    record(test, "record", "layout, stale (lower and equal), replace, ENDED and a later record all as the rules say");
                    // ---- 2. a band met on the march
                    long now = level.getGameTime();
                    n[3] = now;
                    ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(WARRIOR, 4, true)));
                    say(agent, 1, dim, faction, OUT, MARCH_CHIEF, four, wp(at.x(), at.z(), now), wp(at.x(), at.z(), now + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the band appeared", bridge().party(agent) != null && bridge().march(agent) != null);
                    allStacks(party(agent), PLACED, "the column is placed");
                }))
                .thenExecute(() -> {
                    ConflictBridge.Party p = party(agent);
                    long now = level.getGameTime();
                    List<ConflictBridge.Stack> st = p.stacks();
                    check("three stacks: two rows of two and the chief: " + states(p), st.size() == 3);
                    double[] backs = {0.0, 1.5, 3.0};
                    for (int i = 0; i < 3; i++)
                        check("stack " + i + " is " + backs[i] + " back, not the chief's unless last: back " + st.get(i).back() + ", chief " + st.get(i).chief(),
                                Math.abs(st.get(i).back() - backs[i]) < 0.01 && st.get(i).chief() == (i == 2));
                    check("the rows hold two and two and the chief one: " + aliveIn(st.get(0)) + ", " + aliveIn(st.get(1)) + ", " + aliveIn(st.get(2)),
                            aliveIn(st.get(0)) == 2 && aliveIn(st.get(1)) == 2 && aliveIn(st.get(2)) == 1);
                    int[] want = bridge().march(agent).at(now, RtsimApi.topology(level));
                    double[] head = st.get(0).anchorAt(now);
                    record(test, "head", "the head " + xz(head) + ", march.at(now) (" + want[0] + ", " + want[1] + ")");
                    check("the head lies in the block march.at(now): head " + xz(head) + ", block (" + want[0] + ", " + want[1] + ")",
                            (int) Math.floor(head[0]) == want[0] && (int) Math.floor(head[1]) == want[1]);
                    for (int i = 1; i < 3; i++) {
                        double[] a = st.get(i).anchorAt(now);
                        // Bearing 180 heads west: the column lies east of the head.
                        check("stack " + i + " is " + backs[i] + " blocks behind the head, east of it: anchor " + xz(a) + ", head " + xz(head),
                                Math.abs(a[0] - head[0] - backs[i]) < 0.05 && Math.abs(a[1] - head[1]) < 0.05);
                    }
                    for (int i = 0; i < 2; i++) {
                        ConflictBridge.Slot a = st.get(i).slots().get(0), b = st.get(i).slots().get(1);
                        double dx = Math.abs(a.entity().getX() - b.entity().getX()), dz = Math.abs(a.entity().getZ() - b.entity().getZ());
                        double ax = st.get(i).anchorAt(now)[0];
                        record(test, "row " + i, "bodies " + String.format("(%.2f, %.2f) and (%.2f, %.2f)", a.entity().getX(), a.entity().getZ(), b.entity().getX(), b.entity().getZ()));
                        check("row " + i + " stands two abreast, 1.2 blocks apart across the heading: dx " + dx + ", dz " + dz,
                                dx < 0.35 && Math.abs(dz - 1.2) < 0.35);
                        check("row " + i + " stands at its anchor's x " + ax + ": body x " + a.entity().getX(), Math.abs(a.entity().getX() - ax) < 0.5);
                    }
                    // ---- 3. the walk: east 8 blocks, then south 8, 400 ticks each, 0.02 a tick
                    n[3] = level.getGameTime();
                    long t0 = n[3];
                    say(agent, 2, dim, faction, OUT, MARCH_CHIEF, four, wp(at.x(), at.z(), t0), wp(at.x() + 8, at.z(), t0 + 400),
                            wp(at.x() + 8, at.z() + 8, t0 + 800));
                    ConflictKit.steer(agent, 1, 0.02f, at.x() + 8, at.z(), t0 + 400, false);
                    sampling[0] = true;
                })
                .thenWaitUntil(waits.logic(() -> check("the first leg is walked, tick " + (level.getGameTime() - n[3]) + " of 460", level.getGameTime() >= n[3] + 460)))
                .thenExecute(() -> turned(test, level, agent, n[3] + 460, at.x() + 8, at.z(), 1.0, 0.0, "east"))
                .thenExecute(() -> ConflictKit.steer(agent, 1, 0.02f, at.x() + 8, at.z() + 8, n[3] + 800, false))
                .thenWaitUntil(waits.logic(() -> check("the second leg is walked, tick " + (level.getGameTime() - n[3]) + " of 860", level.getGameTime() >= n[3] + 860)))
                .thenExecute(() -> {
                    sampling[0] = false;
                    turned(test, level, agent, n[3] + 860, at.x() + 8, at.z() + 8, 0.0, 1.0, "south");
                    record(test, "walk", "the head against march.at(now) every 20 ticks, worst deviation " + String.format("%.2f", worst[0]) + ":\n" + samples);
                    check("the head stayed within 1.5 blocks of march.at(now) on each axis (worst " + String.format("%.2f", worst[0]) + "):\n" + samples,
                            worst[0] <= 1.5 && samples.length() > 0);
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("recalled", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty())))
                .thenSucceed();
    }

    /**
     * After a leg's walk: the head (the first stack's anchor) is at the leg's end and every other stack lies behind it by its
     * back along the new heading ({@code hx, hz}), and each body is within the follow goal's leash of its place.
     */
    private static void turned(String test, ServerLevel level, long agent, long now, double endX, double endZ, double hx, double hz, String leg) {
        ConflictBridge.Party p = party(agent);
        double[] head = p.stacks().get(0).anchorAt(now);
        check("the head is at the end of the " + leg + " leg (" + xz(new double[] {endX, endZ}) + "): " + xz(head),
                Math.abs(head[0] - endX) < 0.1 && Math.abs(head[1] - endZ) < 0.1);
        StringBuilder log = new StringBuilder();
        for (ConflictBridge.Stack st : p.stacks()) {
            double[] a = st.anchorAt(now);
            double along = (a[0] - head[0]) * hx + (a[1] - head[1]) * hz, across = -(a[0] - head[0]) * hz + (a[1] - head[1]) * hx;
            log.append("stack ").append(st.index).append(" anchor ").append(xz(a)).append(" along ").append(String.format("%.2f", along))
                    .append(" across ").append(String.format("%.2f", across)).append("; ");
            check("after the " + leg + " turn stack " + st.index + " is " + st.back() + " behind the head along the heading and level with it: along "
                    + along + ", across " + across + " (" + log + ")", Math.abs(along + st.back()) < 0.1 && Math.abs(across - st.side()) < 0.1);
            for (ConflictBridge.Slot sl : st.slots()) {
                if (!sl.alive() || sl.entity() == null)
                    continue;
                double d = Math.hypot(sl.entity().getX() - a[0], sl.entity().getZ() - a[1]);
                check("a body of stack " + st.index + " is within the leash of its place after the " + leg + " leg: " + String.format("%.2f", d)
                        + " blocks from the anchor " + xz(a) + ", body (" + String.format("%.2f, %.2f", sl.entity().getX(), sl.entity().getZ()) + ")", d <= 8.0);
            }
        }
        record(test, "turn " + leg, log.toString());
    }

    // ---- the homecoming (MZB-464, MZB-211), core's half ------------------------------------------------

    /** A band that stands still ({@code speed} 0) at (x, z), heading west with its column behind it to the east. */
    private static ConflictKit.Band heldBand(long agent, long faction, String dim, double x, double z, int flags) {
        ConflictKit.Band band = ConflictKit.band(agent, faction, dim);
        band.arcX = x;
        band.arcZ = z;
        band.bearing = 180f;
        band.targetX = x;
        band.targetZ = z;
        band.speed = 0f;
        band.flags = flags;
        return band;
    }

    /** The loaded mobs of the agent that carry its tag, within 64 blocks of (x, z). */
    private static int tagged(ServerLevel level, long agent, double x, double z) {
        int n = 0;
        for (Mob m : level.getEntitiesOfClass(Mob.class, new AABB(x - 64, level.getMinBuildHeight(), z - 64, x + 64, level.getMaxBuildHeight(), z + 64))) {
            AgentTag t = RtsimAttachments.tag(m);
            if (t != null && t.agent() == agent && !m.isRemoved())
                n++;
        }
        return n;
    }

    /**
     * The homecoming of a band the engine handed back (MOWZIE.md §4.9, MZB-464, MZB-211), with a camp that is not in: HAND_BACK
     * lines of a faction that has no site (core has none without a camp kind; the lines of a camp that is in are
     * {@code MarchMowzieGameTests}'). Three bands of two warriors and their chief stand placed:
     * <ul>
     * <li>A, named by a line at its own epoch with no player near: every line is answered STALE (the camp is not in), and A still
     * goes home and goes in the tick the line is read (BAND_DEMATERIALIZED, reason 1, its two warriors as survivors);</li>
     * <li>C, named by a line of another epoch: untouched, not recalling, 40 ticks on;</li>
     * <li>B, named by a line with a player 20 blocks off: B is recalling and stays (no BAND_DEMATERIALIZED) while the player is in
     * view, 60 ticks, then goes the beat after the player does, with its two warriors as survivors.</li>
     * </ul>
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_home")
    public static void aBandHandedBackToACampThatIsNotInStillWalksHome(GameTestHelper helper) {
        String test = "aBandHandedBackToACampThatIsNotInStillWalksHome";
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 1);
        String dim = RtsimTests.dimension(helper);
        long faction = ConflictKit.newFaction(), camp = ConflictKit.newFaction(); // the camp has no site: it is not in
        long a = ConflictKit.newAgent(), b = ConflictKit.newAgent(), c = ConflictKit.newAgent();
        long[] agents = {a, b, c};
        double[] zs = {at.z() - 12, at.z(), at.z() + 12};
        ServerPlayer[] player = new ServerPlayer[1];
        long[] n = new long[3];
        List<ConflictKit.StackSpec> two = List.of(new ConflictKit.StackSpec(WARRIOR, 2, true));

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 1)))
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++)
                        ConflictKit.materialize(heldBand(agents[i], faction, dim, at.x() - 8, zs[i], HOSTILE | CHIEF), two);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (long agent : agents)
                        check("a band appeared", bridge().party(agent) != null);
                    for (long agent : agents)
                        allStacks(party(agent), PLACED, "a band is placed");
                }))
                .thenExecute(() -> {
                    n[0] = level.getGameTime();
                    ConflictKit.reconcile(camp, 1, 101, List.of(
                            ConflictKit.CampLine.party(0, 1, WARRIOR, ConflictRecords.CampReconcile.HAND_BACK, 0, a, 1),
                            ConflictKit.CampLine.party(1, 1, WARRIOR, ConflictRecords.CampReconcile.HAND_BACK, 0, c, 99)));
                })
                .thenWaitUntil(waits.logic(() -> check("A went home", !about(ConflictRecords.BAND_DEMATERIALIZED, a).isEmpty())))
                .thenExecute(() -> {
                    long took = level.getGameTime() - n[0];
                    var done = about(ConflictRecords.BAND_DEMATERIALIZED, a).get(0);
                    Map<String, Integer> left = ConflictKit.survivors(done);
                    Map<Integer, Integer> outcomes = ConflictKit.outcomes(camp, 101);
                    record(test, "no camp", "A went home in " + took + " ticks, reason " + done.aux() + ", survivors " + left + ", outcomes " + outcomes);
                    check("the lines of a camp that is not in are answered stale, both of them: " + outcomes, outcomes.size() == 2
                            && outcomes.get(0) == ConflictRecords.CampBodies.STALE && outcomes.get(1) == ConflictRecords.CampBodies.STALE);
                    check("A went in the line's own tick or the next (" + took + " ticks)", took <= 3);
                    check("the reason is the command's, 1 (" + done.aux() + ")", done.aux() == ConflictRecords.Dematerialized.COMMAND);
                    check("its two warriors are its survivors: " + left, left.equals(Map.of(WARRIOR, 2)));
                    check("none of A's bodies is in the world (" + tagged(level, a, at.x(), at.z()) + ")", tagged(level, a, at.x(), at.z()) == 0);
                    check("no casualty was booked for A", about(ConflictRecords.BAND_CASUALTIES, a).isEmpty());
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    ConflictBridge.Party pc = party(c);
                    check("C, named at another epoch, is untouched: recalling " + pc.recalling(), !pc.recalling());
                    check("and was not dematerialized", about(ConflictRecords.BAND_DEMATERIALIZED, c).isEmpty());
                    check("C still has its three bodies (" + bridge().bodies(c).size() + ")", bridge().bodies(c).size() == 3);
                    // B, with a player in view.
                    double px = at.x() + 12;
                    player[0] = FakePlayers.add(level, new Vec3(px, f.ground(px, at.z()), at.z()), "conflict_march_home_watcher");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    n[1] = level.getGameTime();
                    ConflictKit.reconcile(camp, 1, 102, List.of(ConflictKit.CampLine.party(0, 1, WARRIOR, ConflictRecords.CampReconcile.HAND_BACK, 0, b, 1)));
                })
                .thenWaitUntil(waits.logic(() -> check("B is recalling", party(b).recalling())))
                .thenIdle(60)
                .thenExecute(() -> {
                    check("B stays while a player sees it: recalling " + party(b).recalling(), party(b).recalling());
                    check("and has not gone", about(ConflictRecords.BAND_DEMATERIALIZED, b).isEmpty());
                    check("its bodies are still in the world (" + bridge().bodies(b).size() + ")", bridge().bodies(b).size() == 3);
                    Map<Integer, Integer> outcomes = ConflictKit.outcomes(camp, 102);
                    check("its line was answered stale: " + outcomes, outcomes.size() == 1 && outcomes.get(0) == ConflictRecords.CampBodies.STALE);
                    n[2] = level.getGameTime();
                    FakePlayers.remove(player[0]);
                })
                .thenWaitUntil(waits.logic(() -> check("B went once the player did", !about(ConflictRecords.BAND_DEMATERIALIZED, b).isEmpty())))
                .thenExecute(() -> {
                    long took = level.getGameTime() - n[2];
                    Map<String, Integer> left = ConflictKit.survivors(about(ConflictRecords.BAND_DEMATERIALIZED, b).get(0));
                    record(test, "in view", "B stayed 60 ticks in view and went " + took + " ticks after the player did, survivors " + left);
                    check("B went within " + took + " ticks (a few at most) of the player's going", took <= 5);
                    check("its two warriors are its survivors: " + left, left.equals(Map.of(WARRIOR, 2)));
                    ConflictKit.recall(c, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("C recalled", !about(ConflictRecords.BAND_DEMATERIALIZED, c).isEmpty())))
                .thenSucceed();
    }

    // ---- the empty flagged BAND_MATERIALIZE (MZB-728) ---------------------------------------------------

    /**
     * An empty BAND_MATERIALIZE with the fight flag (a band that walked in with every member placed already, MZB-728) is echoed and
     * the wave taken as had; the wave that comes next is accepted:
     * <ul>
     * <li>wave 1, empty, flags 8, on a placed band: BAND_MATERIALIZED with aux 0, wave 1, flags 8, epoch 1; no BAND_SPAWN_REJECTED;
     * the band's stacks unchanged;</li>
     * <li>wave 1 again: one BAND_SPAWN_REJECTED, ALREADY_EMBODIED (4), and no second echo;</li>
     * <li>wave 2 with a stack of two: accepted, BAND_MATERIALIZED aux 2 with its flags, two more bodies;</li>
     * <li>an empty fight wave of an agent that has no party: echoed all the same (aux 0), and no party is made;</li>
     * <li>an empty fight wave of a band that is recalling (a player holds it in view): one rejection, ALREADY_EMBODIED, no echo and
     * nothing placed.</li>
     * </ul>
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_empty")
    public static void anEmptyFightWaveIsEchoedAndTheNextWaveComes(GameTestHelper helper) {
        String test = "anEmptyFightWaveIsEchoedAndTheNextWaveComes";
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 1);
        String dim = RtsimTests.dimension(helper);
        long faction = ConflictKit.newFaction();
        long agent = ConflictKit.newAgent(), nobody = ConflictKit.newAgent();
        int fight = ConflictRecords.Materialize.FLAG_FIGHT;
        ServerPlayer[] player = new ServerPlayer[1];
        int[] n = new int[4];
        ConflictKit.Band first = heldBand(agent, faction, dim, at.x() - 8, at.z() - 10, HOSTILE);
        ConflictKit.Band empty = heldBand(agent, faction, dim, at.x() - 8, at.z() - 10, fight);
        empty.wave = 1;
        ConflictKit.Band next = heldBand(agent, faction, dim, at.x() - 8, at.z() + 10, HOSTILE);
        next.wave = 2;
        ConflictKit.Band orphan = heldBand(nobody, faction, dim, at.x(), at.z(), fight);
        orphan.wave = 1;
        ConflictKit.Band late = heldBand(agent, faction, dim, at.x() - 8, at.z() - 10, fight);
        late.wave = 3;

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 1)))
                .thenExecute(() -> ConflictKit.materialize(first, List.of(new ConflictKit.StackSpec(WARRIOR, 2, false))))
                .thenWaitUntil(waits.logic(() -> {
                    check("the band appeared", bridge().party(agent) != null);
                    allStacks(party(agent), PLACED, "the first wave is placed");
                }))
                .thenExecute(() -> {
                    n[0] = party(agent).stacks().size();
                    check("one stack of two to begin with: " + states(party(agent)), n[0] == 1 && bridge().bodies(agent).size() == 2);
                    ConflictKit.materialize(empty, List.of());
                })
                .thenWaitUntil(waits.logic(() -> check("two BAND_MATERIALIZED about the band (" + about(ConflictRecords.BAND_MATERIALIZED, agent).size() + ")",
                        about(ConflictRecords.BAND_MATERIALIZED, agent).size() == 2)))
                .thenExecute(() -> {
                    var echo = about(ConflictRecords.BAND_MATERIALIZED, agent).get(1);
                    int wave = echo.view().getInt(ConflictRecords.Materialized.WAVE), flags = echo.view().getInt(ConflictRecords.Materialized.FLAGS),
                            epoch = echo.view().getInt(ConflictRecords.Materialized.EPOCH);
                    record(test, "echo", "aux " + echo.aux() + ", wave " + wave + ", flags " + flags + ", epoch " + epoch);
                    check("the echo places no member: aux " + echo.aux(), echo.aux() == 0);
                    check("it says the wave and the flags it was sent: wave " + wave + ", flags " + flags + ", epoch " + epoch,
                            wave == 1 && flags == fight && epoch == 1);
                    check("nothing was rejected (" + about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() + ")", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    check("the stacks are as they were: " + states(party(agent)), party(agent).stacks().size() == n[0] && bridge().bodies(agent).size() == 2);
                    // The same wave again: refused once.
                    ConflictKit.materialize(empty, List.of());
                })
                .thenWaitUntil(waits.logic(() -> check("the wave was refused (" + about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() + ")",
                        about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() == 1)))
                .thenIdle(5)
                .thenExecute(() -> {
                    var rejected = about(ConflictRecords.BAND_SPAWN_REJECTED, agent);
                    check("one refusal, ALREADY_EMBODIED (4): " + rejected.size() + ", aux " + rejected.get(0).aux(),
                            rejected.size() == 1 && rejected.get(0).aux() == ConflictRecords.SpawnRejected.ALREADY_EMBODIED);
                    check("and wave 1: " + rejected.get(0).view().getInt(ConflictRecords.SpawnRejected.WAVE), rejected.get(0).view().getInt(ConflictRecords.SpawnRejected.WAVE) == 1);
                    check("no second echo (" + about(ConflictRecords.BAND_MATERIALIZED, agent).size() + ")", about(ConflictRecords.BAND_MATERIALIZED, agent).size() == 2);
                    // The next wave, with a stack.
                    ConflictKit.materialize(next, List.of(new ConflictKit.StackSpec(WARRIOR, 2, false)));
                })
                .thenWaitUntil(waits.logic(() -> check("three BAND_MATERIALIZED (" + about(ConflictRecords.BAND_MATERIALIZED, agent).size() + ")",
                        about(ConflictRecords.BAND_MATERIALIZED, agent).size() == 3)))
                .thenExecute(() -> {
                    var third = about(ConflictRecords.BAND_MATERIALIZED, agent).get(2);
                    int wave = third.view().getInt(ConflictRecords.Materialized.WAVE);
                    record(test, "next wave", "aux " + third.aux() + ", wave " + wave + ", stacks " + states(party(agent)));
                    check("the next wave places its two: aux " + third.aux() + ", wave " + wave, third.aux() == 2 && wave == 2);
                    check("the band has its four bodies in two stacks: " + states(party(agent)) + ", bodies " + bridge().bodies(agent).size(),
                            party(agent).stacks().size() == 2 && bridge().bodies(agent).size() == 4);
                    check("still never rejected but the once", about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() == 1);
                    // An empty fight wave of an agent that has no party.
                    ConflictKit.materialize(orphan, List.of());
                })
                .thenWaitUntil(waits.logic(() -> check("the orphan's wave was echoed", about(ConflictRecords.BAND_MATERIALIZED, nobody).size() == 1)))
                .thenExecute(() -> {
                    var echo = about(ConflictRecords.BAND_MATERIALIZED, nobody).get(0);
                    check("echoed with aux 0 and flags 8: aux " + echo.aux() + ", flags " + echo.view().getInt(ConflictRecords.Materialized.FLAGS),
                            echo.aux() == 0 && echo.view().getInt(ConflictRecords.Materialized.FLAGS) == fight);
                    check("no party was made for it", bridge().party(nobody) == null);
                    check("and no refusal", about(ConflictRecords.BAND_SPAWN_REJECTED, nobody).isEmpty());
                    // A band that is recalling (held by a player in view) refuses an empty wave once.
                    double px = at.x() + 12;
                    player[0] = FakePlayers.add(level, new Vec3(px, f.ground(px, at.z()), at.z()), "conflict_march_empty_watcher");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    n[1] = about(ConflictRecords.BAND_MATERIALIZED, agent).size();
                    n[2] = about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size();
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 600, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("the band is recalling", party(agent).recalling())))
                .thenExecute(() -> ConflictKit.materialize(late, List.of()))
                .thenWaitUntil(waits.logic(() -> check("the late wave was refused (" + (about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() - n[2]) + ")",
                        about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() == n[2] + 1)))
                .thenIdle(5)
                .thenExecute(() -> {
                    var rejected = about(ConflictRecords.BAND_SPAWN_REJECTED, agent);
                    var last = rejected.get(rejected.size() - 1);
                    record(test, "recalling", "refusals " + rejected.size() + ", last aux " + last.aux() + ", wave " + last.view().getInt(ConflictRecords.SpawnRejected.WAVE));
                    check("one refusal for the recalling band, ALREADY_EMBODIED, wave 3: aux " + last.aux() + ", wave " + last.view().getInt(ConflictRecords.SpawnRejected.WAVE),
                            rejected.size() == n[2] + 1 && last.aux() == ConflictRecords.SpawnRejected.ALREADY_EMBODIED
                                    && last.view().getInt(ConflictRecords.SpawnRejected.WAVE) == 3);
                    check("and no echo (" + (about(ConflictRecords.BAND_MATERIALIZED, agent).size() - n[1]) + " more)", about(ConflictRecords.BAND_MATERIALIZED, agent).size() == n[1]);
                    check("nothing was placed: " + states(party(agent)), party(agent).stacks().size() == 2);
                    FakePlayers.remove(player[0]);
                })
                .thenWaitUntil(waits.logic(() -> check("the band went", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty())))
                .thenSucceed();
    }

    // ---- MZB-729: a saved copy of a puppet that left -----------------------------------------------------

    private static Entity copyOf(Mob puppet, ServerLevel level) {
        CompoundTag nbt = new CompoundTag();
        puppet.saveWithoutId(nbt);
        nbt.putString("id", EntityType.getKey(puppet.getType()).toString());
        Entity copy = EntityType.loadEntityRecursive(nbt, level, e -> e);
        check("the copy is rebuilt from the puppet's NBT", copy != null);
        check("the copy carries the puppet's tag and uuid", RtsimAttachments.isTagged(copy) && copy.getUUID().equals(puppet.getUUID()));
        return copy;
    }

    /**
     * MZB-729: a chunk saved with a placed puppet comes back with a copy of it, tag and UUID and all. When the band has left
     * since (its member cut by the trim, the band sent home, its epoch ended by a new one) the copy must not join: it is discarded
     * on join, {@code staleDiscarded} rises by one, and the unit's count and the tagged bodies in the world do not change.
     * <ol>
     * <li>The trim: a march of three warriors says one fewer than the four placed; the rearmost is cut, and its saved copy,
     * the one that was in the world at the time, is refused;</li>
     * <li>sent home: the band is recalled and goes; the copy of one of its warriors is refused;</li>
     * <li>a new epoch: a second band's materialize at epoch 2 ends its epoch 1; a copy of an epoch 1 body is refused, and the epoch 2
     * band's bodies are the same two.</li>
     * </ol>
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_copy")
    public static void aSavedCopyOfAPuppetThatLeftIsDiscardedOnJoin(GameTestHelper helper) {
        String test = "aSavedCopyOfAPuppetThatLeftIsDiscardedOnJoin";
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 1);
        String dim = RtsimTests.dimension(helper);
        long faction = ConflictKit.newFaction();
        long a = ConflictKit.newAgent(), b = ConflictKit.newAgent();
        ConflictKit.Band bandA = heldBand(a, faction, dim, at.x() - 8, at.z() - 10, HOSTILE | CHIEF);
        ConflictKit.Band bandB = heldBand(b, faction, dim, at.x() - 8, at.z() + 10, HOSTILE);
        ConflictKit.Band bandB2 = heldBand(b, faction, dim, at.x() - 8, at.z() + 10, HOSTILE);
        bandB2.epoch = 2;
        Entity[] copies = new Entity[3];
        long[] n = new long[4];
        Map<String, Integer> three = Map.of(WARRIOR, 3), four = Map.of(WARRIOR, 4);

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 1)))
                .thenExecute(() -> {
                    ConflictKit.materialize(bandA, List.of(new ConflictKit.StackSpec(WARRIOR, 4, true)));
                    ConflictKit.materialize(bandB, List.of(new ConflictKit.StackSpec(WARRIOR, 2, false)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("both bands appeared", bridge().party(a) != null && bridge().party(b) != null);
                    allStacks(party(a), PLACED, "A is placed");
                    allStacks(party(b), PLACED, "B is placed");
                }))
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    // The saved copies: the rearmost warrior (to be cut), one of the front row (to go home with the band), and one of B's epoch 1.
                    copies[0] = copyOf(party(a).stacks().get(1).slots().get(1).entity(), level);
                    copies[1] = copyOf(party(a).stacks().get(0).slots().get(0).entity(), level);
                    copies[2] = copyOf(party(b).stacks().get(0).slots().get(0).entity(), level);
                    say(a, 1, dim, faction, OUT, MARCH_CHIEF, four, wp(at.x(), at.z(), now), wp(at.x() - 200, at.z(), now + 10_000));
                    n[0] = ConflictKit.trimmed();
                    n[1] = RtsimTests.session().staleDiscarded();
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    check("a march that matches cuts nothing (" + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0]);
                    say(a, 2, dim, faction, OUT, MARCH_CHIEF, three, wp(at.x(), at.z(), level.getGameTime()), wp(at.x() - 200, at.z(), level.getGameTime() + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("the rearmost warrior was cut (" + (ConflictKit.trimmed() - n[0]) + ")", ConflictKit.trimmed() == n[0] + 1)))
                .thenExecute(() -> {
                    int bodies = bridge().bodies(a).size(), tagged = tagged(level, a, at.x(), at.z());
                    check("the cut member's stack still has its other warrior: bodies " + bodies + ", tagged " + tagged, bodies == 4 && tagged == 4);
                    boolean joined = level.addFreshEntity(copies[0]);
                    long discarded = RtsimTests.session().staleDiscarded() - n[1];
                    record(test, "trimmed", "joined " + joined + ", discarded " + discarded);
                    check("the copy of the cut member does not join", !joined && level.getEntity(copies[0].getUUID()) == null);
                    check("one stale copy was counted (" + discarded + ")", discarded == 1);
                    check("the unit's count is the march's: bodies " + bridge().bodies(a).size() + ", tagged " + tagged(level, a, at.x(), at.z()),
                            bridge().bodies(a).size() == 4 && tagged(level, a, at.x(), at.z()) == 4);
                    ConflictKit.recall(a, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("A went home", !about(ConflictRecords.BAND_DEMATERIALIZED, a).isEmpty())))
                .thenExecute(() -> {
                    boolean joined = level.addFreshEntity(copies[1]);
                    long discarded = RtsimTests.session().staleDiscarded() - n[1];
                    record(test, "sent home", "joined " + joined + ", discarded " + discarded);
                    check("the copy of a warrior of the band that went home does not join", !joined && level.getEntity(copies[1].getUUID()) == null);
                    check("two stale copies counted (" + discarded + ")", discarded == 2);
                    check("no body of A is in the world (bodies " + bridge().bodies(a).size() + ", tagged " + tagged(level, a, at.x(), at.z()) + ")",
                            bridge().bodies(a).isEmpty() && tagged(level, a, at.x(), at.z()) == 0);
                    // A new epoch of B ends the old one.
                    ConflictKit.materialize(bandB2, List.of(new ConflictKit.StackSpec(WARRIOR, 2, false)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("B's epoch 1 ended", !about(ConflictRecords.BAND_DEMATERIALIZED, b).isEmpty());
                    check("B's epoch 2 appeared", bridge().party(b) != null && bridge().party(b).epoch == 2);
                    allStacks(party(b), PLACED, "B's epoch 2 is placed");
                }))
                .thenExecute(() -> {
                    int bodies = bridge().bodies(b).size(), tagged = tagged(level, b, at.x(), at.z());
                    check("epoch 2 has its two warriors before the copy: bodies " + bodies + ", tagged " + tagged, bodies == 2 && tagged == 2);
                    boolean joined = level.addFreshEntity(copies[2]);
                    long discarded = RtsimTests.session().staleDiscarded() - n[1];
                    record(test, "new epoch", "joined " + joined + ", discarded " + discarded);
                    check("the copy of an epoch 1 body does not join", !joined && level.getEntity(copies[2].getUUID()) == null);
                    check("three stale copies counted (" + discarded + ")", discarded == 3);
                    check("epoch 2's unit count is unchanged: bodies " + bridge().bodies(b).size() + ", tagged " + tagged(level, b, at.x(), at.z()),
                            bridge().bodies(b).size() == 2 && tagged(level, b, at.x(), at.z()) == 2);
                    ConflictKit.recall(b, 2, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("B went", about(ConflictRecords.BAND_DEMATERIALIZED, b).size() == 2)))
                .thenSucceed();
    }

    // ---- MZB-466: a death after the march ended ----------------------------------------------------------

    /**
     * MZB-466's Java half: a member of a band the engine has ended (its march says ENDED, its HAND_BACK lines are in flight) that dies
     * is a loss the engine books nowhere, and the bridge counts it ({@code unbookedHomeDeaths}) and still journals it. Four bands of
     * three warriors stand placed and a member of each dies in the same tick:
     * <ul>
     * <li>X, whose march is live: not counted;</li>
     * <li>Y, whose march ended: counted, once;</li>
     * <li>Z, whose march ended and which is recalling (a player holds it in view): not counted, the recall owns its members;</li>
     * <li>W, which the engine has said no march for: not counted.</li>
     * </ul>
     * Each death is journaled (BAND_CASUALTIES, once), counted or not.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_unbooked")
    public static void aDeathAfterTheMarchEndedIsCountedUnbooked(GameTestHelper helper) {
        String test = "aDeathAfterTheMarchEndedIsCountedUnbooked";
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 1);
        String dim = RtsimTests.dimension(helper);
        long faction = ConflictKit.newFaction();
        long x = ConflictKit.newAgent(), y = ConflictKit.newAgent(), z = ConflictKit.newAgent(), w = ConflictKit.newAgent();
        long[] agents = {x, y, z, w};
        double[] zs = {at.z() - 15, at.z() - 5, at.z() + 5, at.z() + 15};
        Map<String, Integer> three = Map.of(WARRIOR, 3);
        ServerPlayer[] player = new ServerPlayer[1];
        long[] n = new long[3];
        List<ConflictKit.StackSpec> row = List.of(new ConflictKit.StackSpec(WARRIOR, 3, false));

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 1)))
                .thenExecute(() -> {
                    for (int i = 0; i < 4; i++)
                        ConflictKit.materialize(heldBand(agents[i], faction, dim, at.x() - 8, zs[i], HOSTILE), row);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (long agent : agents) {
                        check("a band appeared", bridge().party(agent) != null);
                        allStacks(party(agent), PLACED, "a band is placed");
                    }
                }))
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    n[0] = bridge().marchRecords;
                    // X and Z and Y say their marches; Y's and Z's end.
                    for (long agent : new long[] {x, y, z})
                        say(agent, 1, dim, faction, OUT, 0, three, wp(at.x(), at.z(), now), wp(at.x() - 200, at.z(), now + 10_000));
                    for (long agent : new long[] {y, z})
                        say(agent, 2, dim, faction, OUT, MARCH_ENDED, three, wp(at.x(), at.z(), now));
                })
                .thenWaitUntil(waits.logic(() -> check("five records read (" + (bridge().marchRecords - n[0]) + ")", bridge().marchRecords == n[0] + 5
                        && bridge().marchEnded(y) && bridge().marchEnded(z))))
                .thenExecute(() -> {
                    check("X's march is live, W has none: " + bridge().march(x) + ", " + bridge().march(w) + ", ended " + bridge().marchEnded(x) + ", "
                            + bridge().marchEnded(w), bridge().march(x) != null && bridge().march(w) == null && !bridge().marchEnded(x) && !bridge().marchEnded(w));
                    // A player holds Z in view while it is recalled.
                    double px = at.x() + 12;
                    player[0] = FakePlayers.add(level, new Vec3(px, f.ground(px, at.z()), at.z()), "conflict_march_unbooked_watcher");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    ConflictKit.recall(z, 1, ConflictRecords.RECALL_DONE, 600, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("Z is recalling", party(z).recalling())))
                .thenExecute(() -> {
                    n[1] = ConflictKit.unbookedHomeDeaths();
                    for (long agent : agents)
                        party(agent).stacks().get(0).slots().get(0).entity().hurt(level.damageSources().generic(), 1000f);
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (long agent : agents)
                        check("a loss was journaled for " + Long.toHexString(agent), about(ConflictRecords.BAND_CASUALTIES, agent).size() == 1);
                }))
                .thenIdle(5)
                .thenExecute(() -> {
                    long counted = ConflictKit.unbookedHomeDeaths() - n[1];
                    record(test, "unbooked", "counted " + counted + " for four deaths (X live march, Y ended, Z ended and recalling, W no march)");
                    check("exactly Y's death was counted as unbooked (" + counted + ")", counted == 1);
                    for (long agent : agents)
                        check("one casualty for " + Long.toHexString(agent) + ": " + about(ConflictRecords.BAND_CASUALTIES, agent).size(),
                                about(ConflictRecords.BAND_CASUALTIES, agent).size() == 1);
                    FakePlayers.remove(player[0]);
                    for (long agent : new long[] {x, y, w})
                        ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> {
                    for (long agent : agents)
                        check("the band went", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty());
                }))
                .thenSucceed();
    }

    // ---- the conflict module off and on --------------------------------------------------------------------

    /**
     * The conflict module switched off and on again (MZB-731's review, MZB-468): a band with a march and its column stands placed.
     * With the module off the bridge captures it as it stands (BAND_DEMATERIALIZED, reason SESSION_END 4, its four warriors as
     * survivors), holds no party, no body and no march for it, and a march said while it is off is not kept. With the module on
     * again the same agent is materialized and its column forms again (three stacks placed, nothing rejected), and a march of a lower
     * revision than the one said before the switch is kept, not dropped as stale (the table was emptied).
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "conflict_switch")
    public static void aColumnFormsAgainAfterTheModuleIsSwitchedOffAndOn(GameTestHelper helper) {
        String test = "aColumnFormsAgainAfterTheModuleIsSwitchedOffAndOn";
        ChunkWait waits = ChunkWait.of(helper, 6000);
        Forced f = new Forced(helper);
        ServerLevel level = f.level;
        Spot at = spot();
        f.square(at.cx(), at.cz(), 2);
        MinecraftServer server = level.getServer();
        String dim = RtsimTests.dimension(helper);
        long faction = ConflictKit.newFaction();
        long agent = ConflictKit.newAgent();
        Map<String, Integer> four = Map.of(WARRIOR, 4);
        long[] n = new long[3];
        ConflictKit.Band band = heldBand(agent, faction, dim, at.x(), at.z(), HOSTILE | CHIEF);
        ConflictKit.Band again = heldBand(agent, faction, dim, at.x(), at.z(), HOSTILE | CHIEF);
        ConflictKit.onEnd(helper, () -> {
            RtsimConfig.Test.setModule(Module.CONFLICT, null);
            BridgeEvents.moduleSwitchesChanged(server);
        });

        helper.startSequence()
                .thenWaitUntil(waits.chunks(() -> f.checkSquare(at.cx(), at.cz(), 2)))
                .thenExecute(() -> {
                    ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(WARRIOR, 4, true)));
                    say(agent, 5, dim, faction, OUT, MARCH_CHIEF, four, wp(at.x(), at.z(), level.getGameTime()), wp(at.x() - 200, at.z(), level.getGameTime() + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the band appeared with its march", bridge().party(agent) != null && bridge().march(agent) != null);
                    allStacks(party(agent), PLACED, "the column is placed");
                }))
                .thenExecute(() -> {
                    check("a column of three stacks: " + states(party(agent)), party(agent).stacks().size() == 3);
                    check("the march is revision 5", bridge().march(agent).revision() == 5);
                    RtsimConfig.Test.setModule(Module.CONFLICT, false);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(waits.logic(() -> check("the band was captured", !about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty())))
                .thenExecute(() -> {
                    var done = about(ConflictRecords.BAND_DEMATERIALIZED, agent).get(0);
                    Map<String, Integer> left = ConflictKit.survivors(done);
                    record(test, "off", "reason " + done.aux() + ", survivors " + left);
                    check("captured as the session's end, 4: " + done.aux(), done.aux() == ConflictRecords.Dematerialized.SESSION_END);
                    check("its four warriors are the survivors: " + left, left.equals(Map.of(WARRIOR, 4)));
                    check("no party, no body in the world and no march: party " + bridge().party(agent) + ", tagged " + tagged(level, agent, at.x(), at.z())
                            + ", march " + bridge().march(agent), bridge().party(agent) == null && tagged(level, agent, at.x(), at.z()) == 0
                            && bridge().march(agent) == null);
                    // A march said while the module is off is not kept.
                    n[0] = bridge().marchRecords;
                    say(agent, 6, dim, faction, OUT, MARCH_CHIEF, four, wp(at.x(), at.z(), level.getGameTime()), wp(at.x() - 200, at.z(), level.getGameTime() + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("the record was read", bridge().marchRecords == n[0] + 1)))
                .thenIdle(5)
                .thenExecute(() -> {
                    check("no march is kept while the module is off: " + bridge().march(agent), bridge().march(agent) == null);
                    RtsimConfig.Test.setModule(Module.CONFLICT, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                    n[1] = bridge().marchStale;
                    n[2] = about(ConflictRecords.BAND_MATERIALIZED, agent).size();
                    ConflictKit.materialize(again, List.of(new ConflictKit.StackSpec(WARRIOR, 4, true)));
                })
                .thenWaitUntil(waits.logic(() -> {
                    check("the band appeared again", bridge().party(agent) != null);
                    allStacks(party(agent), PLACED, "the column is placed again");
                }))
                .thenExecute(() -> {
                    ConflictBridge.Party p = party(agent);
                    check("the column formed again: three stacks, " + states(p) + ", bodies " + bridge().bodies(agent).size(),
                            p.stacks().size() == 3 && bridge().bodies(agent).size() == 5 && p.stacks().get(2).chief());
                    check("it was materialized and not rejected (materialized " + (about(ConflictRecords.BAND_MATERIALIZED, agent).size() - n[2])
                            + ", rejected " + about(ConflictRecords.BAND_SPAWN_REJECTED, agent).size() + ")",
                            about(ConflictRecords.BAND_MATERIALIZED, agent).size() == n[2] + 1 && about(ConflictRecords.BAND_SPAWN_REJECTED, agent).isEmpty());
                    // A lower revision than the one said before the switch: kept, since the table was emptied.
                    say(agent, 1, dim, faction, OUT, MARCH_CHIEF, four, wp(at.x(), at.z(), level.getGameTime()), wp(at.x() - 200, at.z(), level.getGameTime() + 10_000));
                })
                .thenWaitUntil(waits.logic(() -> check("a march of revision 1 was kept after the switch (stale " + (bridge().marchStale - n[1]) + ")",
                        bridge().march(agent) != null && bridge().march(agent).revision() == 1)))
                .thenExecute(() -> {
                    record(test, "on", "the column formed again and revision 1 was kept (marchStale " + (bridge().marchStale - n[1]) + " more)");
                    check("it was not dropped as stale: " + (bridge().marchStale - n[1]), bridge().marchStale == n[1]);
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 1, at.x() - 100, at.z());
                })
                .thenWaitUntil(waits.logic(() -> check("the band went", about(ConflictRecords.BAND_DEMATERIALIZED, agent).size() == 2)))
                .thenSucceed();
    }

    // @@END@@
}
