package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.MarchPath;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.Heightmap;
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

    // @@END@@
}
