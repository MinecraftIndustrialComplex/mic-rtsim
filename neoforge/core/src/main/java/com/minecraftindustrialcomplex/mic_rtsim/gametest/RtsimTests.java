package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import net.neoforged.fml.loading.FMLPaths;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Shared helpers for the mic_rtsim GameTests ({@code ./gametest.sh}).
 *
 * <p>The tests drive the server's real bridge session, whose engine is a {@link ScriptedEngine}: the
 * Rust engine ({@link NativeEngine}, the default) or the {@link StubEngine}
 * ({@code ./gametest.sh -PrtsimEngine=stub}). A test orders commands from the engine, waits for the
 * bridge to apply them on a later tick (the bridge never waits, so neither can a test), and reads back
 * the observations the engine received. Tests in the default batch run at the same time against the
 * one session, so each uses its own agent ids and only looks at records for them.
 */
public final class RtsimTests {
    private RtsimTests() {}

    /**
     * {@code data/mic_rtsim/structure/arena.nbt}: an 11 x 5 x 11 box, polished andesite floor and a
     * two-block glass ring (tools/make_test_structure.py).
     */
    public static final String TEMPLATE = "arena";

    /**
     * Agent ids start at a random base each server run. The GameTest world is reused (B37), so a run
     * starts by replaying what the last one journaled after its final hand-over (its shutdown
     * captures), and any journal the engine's pages do not include (a world first run on the stub,
     * then on the native engine): those records name the last run's agents, and must never be
     * mistaken for this run's. Far below the {@code /rtsim stub caravan} ids (2^40).
     */
    private static final AtomicLong NEXT_AGENT = new AtomicLong(
            10_000 + 1_000_000L * java.util.concurrent.ThreadLocalRandom.current().nextInt(1, 1 << 19));

    public static long newAgent() {
        return NEXT_AGENT.getAndIncrement();
    }

    public static RtsimSession session() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s;
    }

    /** The session's engine, as the test surface both engines implement. */
    public static ScriptedEngine engine() {
        if (!(session().engine() instanceof ScriptedEngine engine))
            throw new GameTestAssertException("the session's engine takes no scripted orders: " + session().engine().describe());
        return engine;
    }

    /**
     * A new engine of the same kind as the session's, with empty pages, for a test that runs its own
     * sessions: the native engine on a fresh directory under {@code <game dir>/mic_rtsim-test/}, or a
     * stub with no pages file.
     */
    public static ScriptedEngine freshEngine() {
        ScriptedEngine kind = engine();
        if (kind instanceof NativeEngine)
            return NativeEngine.open(FMLPaths.GAMEDIR.get().resolve("mic_rtsim-test").resolve(UUID.randomUUID().toString()),
                    0, NativeEngine.FLAG_TEST | NativeEngine.FLAG_NO_FSYNC);
        if (kind instanceof StubEngine)
            return new StubEngine(StubEngine.Pages.EMPTY, null, StubEngine.DEFAULT_CAPACITY);
        throw new GameTestAssertException("no fresh engine of the kind " + kind.getClass().getSimpleName());
    }

    /**
     * A wait on the world's IO or lighting (a far chunk loading its entities, the light engine catching up
     * with an edit): the GameTest server ticks as fast as it can, so on a busy machine a timeout in ticks
     * passes in wall-clock time too short for work on other threads. While the check fails, the tick waits
     * 5 ms, so a timeout of N ticks is at least N * 5 ms of real time. Use it inside {@code thenWaitUntil}.
     */
    public static Runnable io(Runnable check) {
        return () -> {
            try {
                check.run();
            } catch (RuntimeException e) {
                try {
                    Thread.sleep(5);
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                }
                throw e;
            }
        };
    }

    /** The arena's middle, standing on the floor, in absolute coordinates. */
    public static Vec3 centre(GameTestHelper helper) {
        // Relative y 1 is the arena's floor (the structure sits one block above the test's structure
        // block, which absolutePos counts from); y 2 stands on it.
        return helper.absoluteVec(new Vec3(5.5, 2, 5.5));
    }

    public static String dimension(GameTestHelper helper) {
        return helper.getLevel().dimension().location().toString();
    }

    public static PuppetGroup group(long agent) {
        return session().registry().byAgent(agent);
    }

    /** Waits (inside a thenWaitUntil) for the group to be embodied, and returns it. */
    public static PuppetGroup embodied(long agent) {
        PuppetGroup g = group(agent);
        if (g == null)
            throw new GameTestAssertException("agent " + agent + " is not embodied yet");
        return g;
    }

    /** Journaled records of one kind for one agent (every journaled agent field is at offset 24). */
    public static List<ScriptedEngine.Received> recordsFor(ScriptedEngine engine, int kind, long agent) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine.received(kind))
            if (r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    /**
     * The records of one kind for one agent that copies carried (REASSERT, docs/boundary.md §3.2), applied
     * or dropped ({@link ScriptedEngine.Received#skipped}), oldest first.
     */
    public static List<ScriptedEngine.Received> copiesFor(ScriptedEngine engine, int kind, long agent) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine.copies(kind))
            if (r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent)
                out.add(r);
        return out;
    }

    /** Every journaled record for one agent, in the order the engine received them. */
    public static List<ScriptedEngine.Received> allFor(ScriptedEngine engine, long agent) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine.received())
            if (Obs.isJournaled(r.kind()) && r.kind() != Obs.TRANSFER && r.kind() != Obs.COMMAND_RESULT
                    && r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    public static long gameTimeOf(ScriptedEngine.Received r) {
        return r.view().getLong(Obs.GAME_TIME);
    }

    public static void check(String what, boolean ok) {
        if (!ok)
            throw new GameTestAssertException(what);
    }

    public static void record(String test, String what, Object value) {
        MicRtsim.LOGGER.info("[gametest] {}: {} = {}", test, what, value);
    }

    public static Entity member(PuppetGroup g, int index) {
        PuppetGroup.Member m = g.members[index];
        check("member " + index + " of agent " + g.agent + " is still in the group", !m.gone && m.entity != null);
        return m.entity;
    }

    // ---- phase 1 helpers ------------------------------------------------------------------------

    private static final AtomicLong NEXT_VILLAGE = new AtomicLong(1);

    /**
     * Adopts a village over the test's arena, under an id no structure start can have, through the same
     * path a generated start takes (VILLAGE_ADOPTED is journaled). Remove it with {@link #forget}.
     *
     * <p>The village is <b>scripted</b>: {@link ScriptedEngine#scriptVillage} is ordered before the
     * adoption, so the engine never mixes its own decisions (markets, houses, residents, sentiment) into
     * what the test scripts for it. Use {@link #adoptArenaLive} for a village the engine drives.
     */
    public static com.minecraftindustrialcomplex.mic_rtsim.village.Village adoptArena(GameTestHelper helper, String culture) {
        net.minecraft.core.BlockPos lo = helper.absolutePos(net.minecraft.core.BlockPos.ZERO);
        net.minecraft.core.BlockPos hi = helper.absolutePos(new net.minecraft.core.BlockPos(10, 5, 10));
        net.minecraft.world.level.levelgen.structure.BoundingBox box = net.minecraft.world.level.levelgen.structure.BoundingBox
                .fromCorners(lo, hi);
        long id = newVillageId();
        // Before the adoption: the engine must know the village is scripted when it first hears of it.
        engine().scriptVillage(id);
        return adopt(helper, id, "minecraft:village_plains", culture, box, net.minecraft.core.BlockPos.containing(centre(helper)));
    }

    /**
     * A scripted village, as {@link #adoptArena}, with its own box and meeting point (absolute): for a test
     * that builds somewhere other than its arena, such as far chunks it forces. Remove it with {@link #forget}.
     */
    public static com.minecraftindustrialcomplex.mic_rtsim.village.Village adoptScripted(GameTestHelper helper, String culture,
                                                                                        net.minecraft.world.level.levelgen.structure.BoundingBox box,
                                                                                        net.minecraft.core.BlockPos meeting) {
        long id = newVillageId();
        engine().scriptVillage(id);
        return adopt(helper, id, "minecraft:village_plains", culture, box, meeting);
    }

    /**
     * Adopts a village the engine drives itself (no {@link ScriptedEngine#scriptVillage}): the same id
     * scheme as {@link #adoptArena}, the given structure id (for example {@code minecraft:village_taiga}),
     * a box of +-{@code halfWidth} blocks around {@code meeting} (y from {@code meeting.y - 2} to
     * {@code meeting.y + 6}) and margin 0. Remove it with {@link #forget}.
     */
    public static com.minecraftindustrialcomplex.mic_rtsim.village.Village adoptArenaLive(GameTestHelper helper, String culture,
                                                                                         String structure,
                                                                                         net.minecraft.core.BlockPos meeting,
                                                                                         int halfWidth) {
        return adoptArenaLive(helper, culture, structure, meeting, halfWidth, 0);
    }

    /** As above, with the structure's piece count (the engine's guess at its population until a census). */
    public static com.minecraftindustrialcomplex.mic_rtsim.village.Village adoptArenaLive(GameTestHelper helper, String culture,
                                                                                         String structure,
                                                                                         net.minecraft.core.BlockPos meeting,
                                                                                         int halfWidth, int pieces) {
        net.minecraft.world.level.levelgen.structure.BoundingBox box = new net.minecraft.world.level.levelgen.structure.BoundingBox(
                meeting.getX() - halfWidth, meeting.getY() - 2, meeting.getZ() - halfWidth,
                meeting.getX() + halfWidth, meeting.getY() + 6, meeting.getZ() + halfWidth);
        RtsimSession s = session();
        com.minecraftindustrialcomplex.mic_rtsim.village.Village v = com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption.adoptDirect(
                s, s.phase1.villages, newVillageId(), helper.getLevel().dimension(), net.minecraft.resources.ResourceLocation.parse(structure),
                culture, box, meeting, pieces, s.gameTime());
        v.margin = 0;
        return v;
    }

    /** Random low bits: the GameTest world (and its village registry) is reused between runs (B37). */
    private static long newVillageId() {
        return 0x0F00_0000_0000_0000L | (java.util.concurrent.ThreadLocalRandom.current().nextLong() & 0xFFFF_FFFF_0000L)
                | (NEXT_VILLAGE.getAndIncrement() & 0xFFFF);
    }

    private static com.minecraftindustrialcomplex.mic_rtsim.village.Village adopt(GameTestHelper helper, long id, String structure,
                                                                                  String culture,
                                                                                  net.minecraft.world.level.levelgen.structure.BoundingBox box,
                                                                                  net.minecraft.core.BlockPos meeting) {
        RtsimSession s = session();
        com.minecraftindustrialcomplex.mic_rtsim.village.Village v = com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption.adoptDirect(
                s, s.phase1.villages, id, helper.getLevel().dimension(), net.minecraft.resources.ResourceLocation.parse(structure),
                culture, box, meeting, s.gameTime());
        v.margin = 0; // test arenas sit side by side: a village is its own arena and nothing more
        return v;
    }

    public static void forget(com.minecraftindustrialcomplex.mic_rtsim.village.Village v) {
        session().phase1.villages.remove(v.id);
    }

    /**
     * One trade through vanilla's own trading screen code: the payment goes into the menu's payment
     * slots, the result slot offers the selected trade, and taking it runs {@code MerchantResultSlot.onTake}
     * (the hook the bridge observes). Returns the result taken, or EMPTY when no trade was offered.
     */
    public static net.minecraft.world.item.ItemStack trade(net.minecraft.world.item.trading.Merchant merchant,
                                                           net.minecraft.world.entity.player.Player player, int offer,
                                                           net.minecraft.world.item.ItemStack payA) {
        merchant.setTradingPlayer(player);
        try {
            net.minecraft.world.inventory.MerchantMenu menu = new net.minecraft.world.inventory.MerchantMenu(0, player.getInventory(), merchant);
            menu.setSelectionHint(offer);
            menu.getSlot(0).set(payA.copy());
            net.minecraft.world.inventory.Slot result = menu.getSlot(2);
            net.minecraft.world.item.ItemStack out = result.getItem();
            if (out.isEmpty())
                return net.minecraft.world.item.ItemStack.EMPTY;
            net.minecraft.world.item.ItemStack taken = result.remove(out.getCount());
            result.onTake(player, taken);
            return taken;
        } finally {
            merchant.setTradingPlayer(null);
        }
    }

    /** Journaled records of one kind whose u64 at {@code offset} is {@code value}. */
    public static List<ScriptedEngine.Received> recordsWith(ScriptedEngine stub, int kind, int offset, long value) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : stub.received(kind))
            if (!r.skipped() && r.bytes().length >= offset + 8 && r.view().getLong(offset) == value)
                out.add(r);
        return out;
    }

    /** Items of {@code item} lying in item entities within {@code radius} of the test's centre. */
    public static int itemsAround(GameTestHelper helper, net.minecraft.world.item.Item item, double radius) {
        int n = 0;
        for (net.minecraft.world.entity.item.ItemEntity e : helper.getLevel().getEntitiesOfClass(net.minecraft.world.entity.item.ItemEntity.class,
                new net.minecraft.world.phys.AABB(net.minecraft.core.BlockPos.containing(centre(helper))).inflate(radius)))
            if (!e.isRemoved() && e.getItem().is(item))
                n += e.getItem().getCount();
        return n;
    }
}
