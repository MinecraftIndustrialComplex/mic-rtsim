package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Shared helpers for the mic_rtsim GameTests ({@code ./gametest.sh}).
 *
 * <p>The tests drive the server's real bridge session, whose engine is the {@link StubEngine}: a
 * test scripts commands into the stub, waits for the bridge to apply them on a later tick (the bridge
 * never waits, so neither can a test), and reads back the observations the stub received. Tests in
 * the default batch run at the same time against the one session, so each uses its own agent ids
 * and only looks at records for them.
 */
public final class RtsimTests {
    private RtsimTests() {}

    /**
     * {@code data/mic_rtsim/structure/arena.nbt}: an 11 x 5 x 11 box, polished andesite floor and a
     * two-block glass ring (tools/make_test_structure.py).
     */
    public static final String TEMPLATE = "arena";

    private static final AtomicLong NEXT_AGENT = new AtomicLong(10_000);

    public static long newAgent() {
        return NEXT_AGENT.getAndIncrement();
    }

    public static RtsimSession session() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s;
    }

    public static StubEngine stub() {
        if (!(session().engine() instanceof StubEngine stub))
            throw new GameTestAssertException("the engine is not the stub: " + session().engine().describe());
        return stub;
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
    public static List<StubEngine.Received> recordsFor(StubEngine stub, int kind, long agent) {
        List<StubEngine.Received> out = new ArrayList<>();
        for (StubEngine.Received r : stub.received(kind))
            if (r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    /** Every journaled record for one agent, in the order the stub received them. */
    public static List<StubEngine.Received> allFor(StubEngine stub, long agent) {
        List<StubEngine.Received> out = new ArrayList<>();
        for (StubEngine.Received r : stub.received())
            if (Obs.isJournaled(r.kind()) && r.kind() != Obs.TRANSFER && r.kind() != Obs.COMMAND_RESULT
                    && r.bytes().length >= Obs.BODY + 8 && r.view().getLong(Obs.BODY) == agent && !r.skipped())
                out.add(r);
        return out;
    }

    public static long gameTimeOf(StubEngine.Received r) {
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
     */
    public static com.minecraftindustrialcomplex.mic_rtsim.village.Village adoptArena(GameTestHelper helper, String culture) {
        net.minecraft.core.BlockPos lo = helper.absolutePos(net.minecraft.core.BlockPos.ZERO);
        net.minecraft.core.BlockPos hi = helper.absolutePos(new net.minecraft.core.BlockPos(10, 5, 10));
        net.minecraft.world.level.levelgen.structure.BoundingBox box = net.minecraft.world.level.levelgen.structure.BoundingBox
                .fromCorners(lo, hi);
        // Random low bits: the GameTest world (and its village registry) is reused between runs (B37).
        long id = 0x0F00_0000_0000_0000L | (java.util.concurrent.ThreadLocalRandom.current().nextLong() & 0xFFFF_FFFF_0000L)
                | (NEXT_VILLAGE.getAndIncrement() & 0xFFFF);
        RtsimSession s = session();
        com.minecraftindustrialcomplex.mic_rtsim.village.Village v = com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption.adoptDirect(
                s, s.phase1.villages, id, helper.getLevel().dimension(), net.minecraft.resources.ResourceLocation.parse("minecraft:village_plains"),
                culture, box, net.minecraft.core.BlockPos.containing(centre(helper)), s.gameTime());
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
    public static List<StubEngine.Received> recordsWith(StubEngine stub, int kind, int offset, long value) {
        List<StubEngine.Received> out = new ArrayList<>();
        for (StubEngine.Received r : stub.received(kind))
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
