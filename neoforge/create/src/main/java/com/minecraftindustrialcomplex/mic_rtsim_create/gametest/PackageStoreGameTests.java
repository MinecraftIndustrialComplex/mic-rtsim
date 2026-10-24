package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim_create.Frontier;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.PackageStores;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorPackage;
import com.simibubi.create.content.logistics.box.PackageItem;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * SEC-1 slice S4a, package stores (SECTIONS.md §2.5): a Create chain whose frogports sit in two districts is a
 * store owned by neither, with a port at each frogport. Far chunks, forced: district A around a millstone in
 * chunk A, district B around one in chunk B two chunks east, a chain conveyor CA in A turned by a creative
 * motor and joined to CB in B, 23 blocks away. Frogports hang on the conveyors' loops, two blocks from them.
 * Packages are put into a frogport's own inventory, which it sends (Create's {@code tryPullingFrom}: a
 * package its own filter does not match). Slice S4b adds the engine's writes into a loaded frogport
 * (IND_WRITE_CHAIN), slice S4c1 the frontier (a far frogport whose chunk does not tick).
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class PackageStoreGameTests {
    private PackageStoreGameTests() {}

    private static final int Y = 180;
    private static final int TIMEOUT = 8000;
    /** A journaled record's game time, in the bridge's prefix (IndRecords). */
    private static final int JOURNALED_GAME_TIME = 16;

    // ---- the fixture -----------------------------------------------------------------------------

    /** A frogport to build: where, the conveyor whose loop it hangs on, its address filter. */
    private record Frog(BlockPos pos, BlockPos conveyor, String filter) {}

    /** Chunks A (ax) and B (bx = ax + 2) in row az; CA in A and CB in B, the seeds above them. */
    private record Yard(int ax, int az, int bx, BlockPos ca, BlockPos cb, long[] ids) {
        static Yard at(GameTestHelper h, int offset, boolean oneDistrict) {
            BlockPos o = h.absolutePos(BlockPos.ZERO);
            int ax = (o.getX() >> 4) + offset, az = (o.getZ() >> 4) + offset, bx = ax + 2, z = az * 16 + 8;
            BlockPos ca = new BlockPos(ax * 16 + 12, Y, z);
            BlockPos cb = oneDistrict ? new BlockPos(ax * 16 + 2, Y, z) : new BlockPos(bx * 16 + 3, Y, z);
            return new Yard(ax, az, bx, ca, cb, new long[] {-1, -1});
        }

        long da() {
            return ids[0];
        }

        long db() {
            return ids[1];
        }

        BlockPos millA() {
            return ca.above().south();
        }

        BlockPos millB() {
            return cb.above().south();
        }
    }

    private static float loopAngle(BlockPos conveyor, BlockPos port) {
        BlockPos d = port.subtract(conveyor);
        float deg = (float) Math.toDegrees(Math.atan2(d.getX(), d.getZ()));
        return deg < 0 ? deg + 360 : deg;
    }

    private static ItemStack box(String address, int iron) {
        ItemStack box = PackageItem.containing(List.of(new ItemStack(Items.IRON_INGOT, iron)));
        PackageItem.addAddress(box, address);
        return box;
    }

    private static ChainConveyorBlockEntity conveyor(ServerLevel level, BlockPos p) {
        return Loaded.blockEntity(level, p) instanceof ChainConveyorBlockEntity c ? c : null;
    }

    private static FrogportBlockEntity frog(ServerLevel level, BlockPos p) {
        return Loaded.blockEntity(level, p) instanceof FrogportBlockEntity f ? f : null;
    }

    /** Packages a frogport holds (one per slot). */
    private static int held(ServerLevel level, BlockPos p) {
        FrogportBlockEntity f = frog(level, p);
        if (f == null)
            return -1;
        int n = 0;
        for (int i = 0; i < f.inventory.getSlots(); i++)
            if (!f.inventory.getStackInSlot(i).isEmpty())
                n++;
        return n;
    }

    private static int looping(ServerLevel level, BlockPos p) {
        ChainConveyorBlockEntity c = conveyor(level, p);
        return c == null ? -1 : c.getLoopingPackages().size();
    }

    private static int travelling(ServerLevel level, BlockPos p) {
        ChainConveyorBlockEntity c = conveyor(level, p);
        if (c == null)
            return -1;
        int n = 0;
        for (List<ChainConveyorPackage> l : c.getTravellingPackages().values())
            n += l.size();
        return n;
    }

    /** Sends a package from a frogport: into its own inventory, which it empties onto its chain. */
    private static void send(ServerLevel level, BlockPos from, ItemStack box) {
        FrogportBlockEntity f = frog(level, from);
        check("the frogport is loaded", f != null);
        check("the package fits in the frogport", ItemHandlerHelper.insertItem(f.inventory, box, false).isEmpty());
    }

    /** Block entities from {@code Y - 3} to {@code Y + 3} in the loaded chunks from A to B go, then the area's industry. */
    private static void clear(ServerLevel level, Yard y) {
        for (int cx = y.ax - 1; cx <= y.bx + 1; cx++)
            for (int cz = y.az - 1; cz <= y.az + 1; cz++) {
                LevelChunk c = level.getChunkSource().getChunkNow(cx, cz);
                if (c == null)
                    continue;
                for (BlockEntity be : new ArrayList<>(c.getBlockEntities().values()))
                    if (be.getBlockPos().getY() >= Y - 3 && be.getBlockPos().getY() <= Y + 3)
                        level.removeBlock(be.getBlockPos(), false);
            }
        ind().districts().forgetArea(level, y.ax - 1, y.az - 1, y.bx + 1, y.az + 1);
    }

    private static void force(ServerLevel level, Yard y, boolean on) {
        for (int cx = y.ax; cx <= y.bx; cx++)
            level.setChunkForced(cx, y.az, on);
    }

    /**
     * Forces A to B, clears an earlier run, builds the chain, the seeds and the frogports, and waits for the
     * districts. {@code oneDistrict}: both conveyors and one seed in A.
     */
    private static GameTestSequence build(GameTestHelper h, Yard y, boolean oneDistrict, List<Frog> frogs) {
        ServerLevel level = h.getLevel();
        return h.startSequence()
                .thenExecute(() -> force(level, y, true))
                .thenWaitUntil(IndustryKit.paced(() -> {
                    for (int cx = y.ax; cx <= y.bx; cx++)
                        check("A to B load", level.getChunkSource().getChunkNow(cx, y.az) != null);
                }))
                .thenExecute(() -> {
                    clear(level, y);
                    GlassCorpusKit.motor(h, y.ca.below(), Direction.UP, 64);
                    GlassCorpusKit.set(h, y.ca, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    GlassCorpusKit.set(h, y.cb, AllBlocks.CHAIN_CONVEYOR.getDefaultState());
                    List<BlockPos> seeds = oneDistrict ? List.of(y.ca) : List.of(y.ca, y.cb);
                    for (BlockPos c : seeds) {
                        GlassCorpusKit.cog(h, c.above(), Direction.Axis.Y);
                        GlassCorpusKit.set(h, c.above().south(), AllBlocks.MILLSTONE.getDefaultState());
                    }
                    ChainConveyorBlockEntity ca = conveyor(level, y.ca), cb = conveyor(level, y.cb);
                    check("both conveyors are built", ca != null && cb != null);
                    ca.prepareStats();
                    cb.prepareStats();
                    check("the conveyors join", cb.addConnectionTo(y.ca) && ca.addConnectionTo(y.cb));
                    for (Frog f : frogs) {
                        GlassCorpusKit.set(h, f.pos, AllBlocks.PACKAGE_FROGPORT.getDefaultState());
                        FrogportBlockEntity fb = GlassCorpusKit.be(h, f.pos, FrogportBlockEntity.class);
                        fb.target = new PackagePortTarget.ChainConveyorFrogportTarget(f.conveyor.subtract(f.pos), loopAngle(f.conveyor, f.pos),
                                (BlockPos) null, false);
                        fb.addressFilter = f.filter;
                        fb.filterChanged();
                        fb.notifyUpdate();
                    }
                    for (BlockPos c : seeds)
                        playerPlaced(h, c.above().south());
                    for (Frog f : frogs)
                        playerPlacedBare(h, f.pos);
                })
                .thenWaitUntil(() -> {
                    District a = districtAt(h, y.millA());
                    check("district A exists", a != null);
                    y.ids[0] = a.id;
                    if (oneDistrict)
                        return;
                    District b = districtAt(h, y.millB());
                    check("district B exists", b != null);
                    check("A and B are two districts", a.id != b.id);
                    y.ids[1] = b.id;
                })
                .thenWaitUntil(() -> {
                    for (Frog f : frogs) {
                        District d = districtAt(h, f.pos);
                        check("frogport " + f.pos.toShortString() + " is an inventory of its district",
                                d != null && d.inventoryAt(f.pos) != null);
                    }
                    check("the chain turns", conveyor(level, y.ca).getSpeed() != 0 && conveyor(level, y.cb).getSpeed() == conveyor(level, y.ca).getSpeed());
                });
    }

    private static GameTestSequence restore(GameTestSequence s, ServerLevel level, Yard y) {
        return s.thenExecute(() -> {
            clear(level, y);
            force(level, y, false);
        });
    }

    // ---- reading what Java sent ----------------------------------------------------------------------

    /** The store a frogport is a port of, as Java has it; null when none. */
    private static PackageStores.Store storeOf(ServerLevel level, BlockPos frog) {
        return PackageStores.of(ind()).storeOfPort(level.dimension(), frog);
    }

    /** The last IND_PACKAGE_STORE of a store. */
    private static ByteBuffer declaration(long store) {
        List<ByteBuffer> l = received(IndRecords.IND_PACKAGE_STORE, IndRecords.PackageStore.STORE, store);
        return l.isEmpty() ? null : l.get(l.size() - 1);
    }

    /** A declaration's frogport index at a position, or -1. */
    private static int indexIn(ByteBuffer decl, BlockPos p) {
        int n = aux(decl);
        for (int i = 0; i < n; i++) {
            int o = IndRecords.PackageStore.FROGPORTS + IndRecords.PackageStore.FROGPORT * i;
            if (decl.getInt(o + IndRecords.PackageStore.F_X) == p.getX() && decl.getInt(o + IndRecords.PackageStore.F_Y) == p.getY()
                    && decl.getInt(o + IndRecords.PackageStore.F_Z) == p.getZ())
                return i;
        }
        return -1;
    }

    private static long districtIn(ByteBuffer decl, int i) {
        return decl.getLong(IndRecords.PackageStore.FROGPORTS + IndRecords.PackageStore.FROGPORT * i + IndRecords.PackageStore.F_DISTRICT);
    }

    /**
     * Waits for the store of {@code frogs} with them all as ports and its last declaration listing each in the
     * district given; returns through {@code id}.
     */
    private static void checkStore(ServerLevel level, List<Frog> frogs, long[] districts, long[] id) {
        PackageStores.Store s = storeOf(level, frogs.get(0).pos);
        check("the frogports are a store", s != null);
        check("the store has " + frogs.size() + " frogports: " + s.ports.size(), s.ports.size() == frogs.size());
        ByteBuffer d = declaration(s.id);
        check("the store was declared", d != null);
        check("its declaration lists " + frogs.size() + " frogports: " + aux(d), aux(d) == frogs.size());
        for (int i = 0; i < frogs.size(); i++) {
            check("frogport " + i + " is on the same store", storeOf(level, frogs.get(i).pos) == s);
            int k = indexIn(d, frogs.get(i).pos);
            check("frogport " + i + " is declared", k >= 0);
            check("frogport " + i + " is in its district: " + districtIn(d, k) + " for " + districts[i], districtIn(d, k) == districts[i]);
            int inv = d.getInt(IndRecords.PackageStore.FROGPORTS + IndRecords.PackageStore.FROGPORT * k + IndRecords.PackageStore.F_INVENTORY);
            check("frogport " + i + " names its inventory", inv != IndRecords.NO_INVENTORY);
        }
        id[0] = s.id;
    }

    /** The latest complete compile of a district, with a store node at each frogport flagged CHAIN_PORT and no CHAIN edge. */
    private static void checkPortsInCompile(long district, List<BlockPos> ports) {
        GlassKit.Graph g = GlassKit.latest(district);
        check("district " + district + " compiled", g != null && g.complete());
        for (BlockPos p : ports) {
            boolean found = false;
            for (GlassKit.Node n : g.nodes())
                if (n.kind() == GlassRecords.Node.STORE && n.x() == p.getX() && n.y() == p.getY() && n.z() == p.getZ())
                    found |= n.has(GlassRecords.Node.CHAIN_PORT);
            check("the frogport at " + p.toShortString() + " is a store port in " + g.describe(), found);
        }
        for (GlassKit.Edge e : g.edges())
            check("no chain edge in district " + district + "'s compile", e.kind() != GlassRecords.Edge.CHAIN);
    }

    /** IND_CHAIN_PACKAGE records of a store in one direction. */
    private static List<ByteBuffer> chainRecords(long store, int direction) {
        return received(IndRecords.IND_CHAIN_PACKAGE,
                b -> b.getLong(IndRecords.ChainPackage.STORE) == store && b.getInt(IndRecords.ChainPackage.DIRECTION) == direction);
    }

    private static long goodsIn(ByteBuffer r, int entriesAt, int n, int stride) {
        long t = 0;
        for (int i = 0; i < n; i++)
            t += r.getLong(entriesAt + i * stride + 8);
        return t;
    }

    /** The IND_STORE_CENSUS records of a store from game time {@code since} on. */
    private static List<ByteBuffer> censuses(long store, long since) {
        return received(IndRecords.IND_STORE_CENSUS,
                b -> b.getLong(IndRecords.StoreCensus.STORE) == store && b.getLong(IndRecords.StoreCensus.GAME_TIME) >= since);
    }

    /** The packages a frogport holds: their addresses into {@code addresses}, and the iron in them. */
    private static long ironHeld(ServerLevel level, BlockPos p, List<String> addresses) {
        FrogportBlockEntity f = frog(level, p);
        long iron = 0;
        for (int i = 0; f != null && i < f.inventory.getSlots(); i++) {
            ItemStack box = f.inventory.getStackInSlot(i);
            if (box.isEmpty())
                continue;
            addresses.add(PackageItem.getAddress(box));
            var contents = PackageItem.getContents(box);
            for (int k = 0; k < contents.getSlots(); k++)
                if (contents.getStackInSlot(k).is(Items.IRON_INGOT))
                    iron += contents.getStackInSlot(k).getCount();
        }
        return iron;
    }

    /** The CHAIN_IN records of a store that an engine op wrote. */
    private static List<ByteBuffer> writtenIn(long store, long op) {
        return received(IndRecords.IND_CHAIN_PACKAGE, b -> b.getLong(IndRecords.ChainPackage.STORE) == store
                && b.getInt(IndRecords.ChainPackage.DIRECTION) == IndRecords.ChainPackage.CHAIN_IN && b.getLong(IndRecords.ChainPackage.OP_ID) == op);
    }

    // ---- the tests ---------------------------------------------------------------------------------

    /**
     * Two frogports on one chain inside one district stay what they were before S4: a CHAIN edge between them in
     * the compile, no store, no chain records.
     */
    @GameTest(template = YARD, batch = "sec1_s4a", timeoutTicks = TIMEOUT)
    public static void aChainInsideOneDistrictStaysEdges(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aChainInsideOneDistrictStaysEdges";
        Yard y = Yard.at(helper, 5600, true);
        Frog f1 = new Frog(y.ca.east(2), y.ca, "f1"), f2 = new Frog(y.cb.west(2), y.cb, "f2");
        GameTestSequence s = build(helper, y, true, List.of(f1, f2))
                .thenWaitUntil(() -> {
                    GlassKit.Graph g = GlassKit.latest(y.da());
                    check("A compiled", g != null && g.complete());
                    boolean chain = false;
                    for (GlassKit.Edge e : g.edges())
                        chain |= e.kind() == GlassRecords.Edge.CHAIN;
                    check("a chain edge between the frogports in " + g.describe(), chain);
                    for (GlassKit.Node n : g.nodes())
                        check("no store port in A", !n.has(GlassRecords.Node.CHAIN_PORT));
                })
                .thenExecute(() -> {
                    check("no store", storeOf(level, f1.pos) == null && storeOf(level, f2.pos) == null);
                    send(level, f1.pos, box("f2", 2));
                })
                .thenWaitUntil(() -> check("the package reaches F2: " + held(level, f2.pos), held(level, f2.pos) == 1))
                .thenExecute(() -> {
                    List<ByteBuffer> recs = received(IndRecords.IND_CHAIN_PACKAGE, b -> b.getLong(IndRecords.ChainPackage.DISTRICT) == y.da());
                    record(t, "chain records of A", recs.size());
                    check("no chain record for A", recs.isEmpty());
                    for (ByteBuffer d : received(IndRecords.IND_PACKAGE_STORE, b -> true))
                        check("no store declares F1", indexIn(d, f1.pos) < 0);
                });
        restore(s, level, y).thenSucceed();
    }

    /**
     * A chain between two factories leaves them two districts: the chain is a store with a port at each frogport
     * (a store node flagged CHAIN_PORT in each compile, no CHAIN edge), and a package from A to B is one
     * IND_CHAIN_PACKAGE out at FA, routed to FB, and one in at FB.
     */
    @GameTest(template = YARD, batch = "sec1_s4a", timeoutTicks = TIMEOUT)
    public static void aChainBetweenTwoFactoriesLeavesThemTwo(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aChainBetweenTwoFactoriesLeavesThemTwo";
        Yard y = Yard.at(helper, 5700, false);
        Frog fa = new Frog(y.ca.west(2), y.ca, "west"), fb = new Frog(y.cb.east(2), y.cb, "east");
        long[] store = {-1};
        long[] sent = {-1};
        GameTestSequence s = build(helper, y, false, List.of(fa, fb))
                .thenWaitUntil(() -> checkStore(level, List.of(fa, fb), new long[] {y.da(), y.db()}, store))
                .thenWaitUntil(() -> {
                    checkPortsInCompile(y.da(), List.of(fa.pos));
                    checkPortsInCompile(y.db(), List.of(fb.pos));
                })
                .thenExecute(() -> {
                    record(t, "store", store[0] + ": " + PackageStores.of(ind()).perfLine());
                    sent[0] = level.getGameTime();
                    send(level, fa.pos, box("east", 3));
                })
                .thenWaitUntil(() -> check("the package reaches FB: " + held(level, fb.pos), held(level, fb.pos) == 1))
                .thenWaitUntil(() -> check("one package in at FB", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN).size() == 1))
                .thenExecute(() -> {
                    ByteBuffer d = declaration(store[0]);
                    int ia = indexIn(d, fa.pos), ib = indexIn(d, fb.pos);
                    List<ByteBuffer> out = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_OUT);
                    check("one package out: " + out.size(), out.size() == 1);
                    ByteBuffer o = out.get(0);
                    record(t, "out: frogport, destination, packages, goods", o.getInt(IndRecords.ChainPackage.FROGPORT) + ", "
                            + o.getInt(IndRecords.ChainPackage.DESTINATION) + ", " + o.getInt(IndRecords.ChainPackage.PACKAGES) + ", "
                            + goodsIn(o, IndRecords.ChainPackage.ENTRIES, aux(o), IndRecords.ENTRY16));
                    check("out at FA", o.getInt(IndRecords.ChainPackage.FROGPORT) == ia && o.getLong(IndRecords.ChainPackage.DISTRICT) == y.da());
                    check("routed to FB", o.getInt(IndRecords.ChainPackage.DESTINATION) == ib);
                    check("one package of 3 iron", o.getInt(IndRecords.ChainPackage.PACKAGES) == 1
                            && goodsIn(o, IndRecords.ChainPackage.ENTRIES, aux(o), IndRecords.ENTRY16) == 3);
                    check("its address", "east".equals(javaName(o.getInt(IndRecords.ChainPackage.ADDRESS))));
                    check("FA's inventory", o.getInt(IndRecords.ChainPackage.INVENTORY) == districtAt(helper, fa.pos).inventoryAt(fa.pos).index());
                    ByteBuffer in = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN).get(0);
                    check("in at FB", in.getInt(IndRecords.ChainPackage.FROGPORT) == ib && in.getLong(IndRecords.ChainPackage.DISTRICT) == y.db()
                            && goodsIn(in, IndRecords.ChainPackage.ENTRIES, aux(in), IndRecords.ENTRY16) == 3);
                    check("in after out", in.getLong(JOURNALED_GAME_TIME) >= o.getLong(JOURNALED_GAME_TIME) && o.getLong(JOURNALED_GAME_TIME) >= sent[0]);
                });
        restore(s, level, y).thenSucceed();
    }

    /**
     * Two frogports of one district on a chain shared with another: three ports, two in A; a package from FA1
     * addressed to FA2 is routed to FA2 and lands there, all inside the store.
     */
    @GameTest(template = YARD, batch = "sec1_s4a", timeoutTicks = TIMEOUT)
    public static void twoFrogportsOfOneDistrictOnASharedChain(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "twoFrogportsOfOneDistrictOnASharedChain";
        Yard y = Yard.at(helper, 5800, false);
        Frog fa1 = new Frog(y.ca.west(2), y.ca, "a1"), fa2 = new Frog(y.ca.north(2), y.ca, "a2"), fb = new Frog(y.cb.east(2), y.cb, "east");
        long[] store = {-1};
        GameTestSequence s = build(helper, y, false, List.of(fa1, fa2, fb))
                .thenWaitUntil(() -> checkStore(level, List.of(fa1, fa2, fb), new long[] {y.da(), y.da(), y.db()}, store))
                .thenWaitUntil(() -> {
                    checkPortsInCompile(y.da(), List.of(fa1.pos, fa2.pos));
                    checkPortsInCompile(y.db(), List.of(fb.pos));
                })
                .thenExecute(() -> {
                    ByteBuffer d = declaration(store[0]);
                    int o1 = IndRecords.PackageStore.FROGPORTS + IndRecords.PackageStore.FROGPORT * indexIn(d, fa1.pos);
                    int o2 = IndRecords.PackageStore.FROGPORTS + IndRecords.PackageStore.FROGPORT * indexIn(d, fa2.pos);
                    check("FA1 and FA2 are two inventories", d.getInt(o1 + IndRecords.PackageStore.F_INVENTORY) != d.getInt(o2 + IndRecords.PackageStore.F_INVENTORY));
                    send(level, fa1.pos, box("a2", 4));
                })
                .thenWaitUntil(() -> check("the package reaches FA2: " + held(level, fa2.pos), held(level, fa2.pos) == 1))
                .thenWaitUntil(() -> check("one package in", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN).size() == 1))
                .thenExecute(() -> {
                    ByteBuffer d = declaration(store[0]);
                    List<ByteBuffer> out = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_OUT);
                    check("one package out: " + out.size(), out.size() == 1);
                    ByteBuffer o = out.get(0), in = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN).get(0);
                    record(t, "out frogport, destination; in frogport", o.getInt(IndRecords.ChainPackage.FROGPORT) + ", "
                            + o.getInt(IndRecords.ChainPackage.DESTINATION) + "; " + in.getInt(IndRecords.ChainPackage.FROGPORT));
                    check("out at FA1, routed to FA2", o.getInt(IndRecords.ChainPackage.FROGPORT) == indexIn(d, fa1.pos)
                            && o.getInt(IndRecords.ChainPackage.DESTINATION) == indexIn(d, fa2.pos));
                    check("in at FA2, in A", in.getInt(IndRecords.ChainPackage.FROGPORT) == indexIn(d, fa2.pos) && in.getLong(IndRecords.ChainPackage.DISTRICT) == y.da());
                });
        restore(s, level, y).thenSucceed();
    }

    /**
     * A package no frogport takes circles CA: its record out has no destination, and the census counts it with
     * no route. A player takes it off the chain (as {@code ChainPackageInteractionPacket} does): the next census
     * counts none, and no record says so (the engine keeps the difference as the store's residue).
     */
    @GameTest(template = YARD, batch = "sec1_s4a", timeoutTicks = TIMEOUT)
    public static void aPlayerTakesAPackageOffTheChain(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aPlayerTakesAPackageOffTheChain";
        Yard y = Yard.at(helper, 5900, false);
        Frog fa = new Frog(y.ca.west(2), y.ca, "west"), fb = new Frog(y.cb.east(2), y.cb, "east");
        long[] store = {-1};
        long[] at = {-1, -1};
        GameTestSequence s = build(helper, y, false, List.of(fa, fb))
                .thenWaitUntil(() -> checkStore(level, List.of(fa, fb), new long[] {y.da(), y.db()}, store))
                .thenExecute(() -> send(level, fa.pos, box("nowhere", 5)))
                .thenWaitUntil(() -> check("the package circles CA: " + looping(level, y.ca) + "/" + travelling(level, y.ca),
                        looping(level, y.ca) == 1 && travelling(level, y.ca) == 0 && held(level, fa.pos) == 0))
                // The engine may take the record a step after the package is seen looping (under load).
                .thenWaitUntil(() -> check("one package out: " + chainRecords(store[0], IndRecords.ChainPackage.CHAIN_OUT).size(),
                        chainRecords(store[0], IndRecords.ChainPackage.CHAIN_OUT).size() == 1))
                .thenExecute(() -> {
                    List<ByteBuffer> out = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_OUT);
                    check("still one package out: " + out.size(), out.size() == 1);
                    check("with no destination", out.get(0).getInt(IndRecords.ChainPackage.DESTINATION) == IndRecords.NO_INVENTORY);
                    at[0] = level.getGameTime();
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> c = censuses(store[0], at[0]);
                    check("a census after the package left FA", !c.isEmpty());
                    ByteBuffer r = c.get(c.size() - 1);
                    int m = r.getInt(IndRecords.StoreCensus.M);
                    check("one destination: " + m, m == 1);
                    check("no route", r.getInt(IndRecords.StoreCensus.PACKAGES) == IndRecords.NO_INVENTORY);
                    check("one package", r.getInt(IndRecords.StoreCensus.PACKAGES + 4) == 1);
                    check("of 5 iron", goodsIn(r, IndRecords.StoreCensus.goodsAt(m), aux(r), 16) == 5);
                })
                .thenExecute(() -> {
                    ChainConveyorBlockEntity ca = conveyor(level, y.ca);
                    check("a looping package to take", ca.getLoopingPackages().size() == 1);
                    ca.getLoopingPackages().remove(0);
                    ca.sendData();
                    at[1] = level.getGameTime();
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> c = censuses(store[0], at[1] + 1);
                    check("a census after the player took it", !c.isEmpty());
                    ByteBuffer r = c.get(c.size() - 1);
                    record(t, "census after: destinations, entries", r.getInt(IndRecords.StoreCensus.M) + ", " + aux(r));
                    check("no package on the chain", r.getInt(IndRecords.StoreCensus.M) == 0 && aux(r) == 0);
                    check("no package in", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN).isEmpty());
                });
        restore(s, level, y).thenSucceed();
    }

    /**
     * Slice S4b (choice SC113): IND_WRITE_CHAIN writes whole packages into a loaded frogport, addressed to its
     * filter, and the frogport keeps them (Create never sends out a package its filter matches). One CHAIN_IN
     * with the engine's op says what was written; the frogport's chunk carries it with the COMMAND_RESULT until
     * the engine acknowledges them. The op again is DUPLICATE; more packages than free slots write what fits,
     * keeping one slot free for a package Create catches; a frogport with that one slot left is FAILED, with
     * no record.
     */
    @GameTest(template = YARD, batch = "sec1_s4b", timeoutTicks = TIMEOUT)
    public static void anEngineWriteLandsInALoadedFrogport(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "anEngineWriteLandsInALoadedFrogport";
        String iron = "i/64/minecraft:iron_ingot";
        Yard y = Yard.at(helper, 6000, false);
        Frog fa = new Frog(y.ca.west(2), y.ca, "west"), fb = new Frog(y.cb.east(2), y.cb, "east");
        long[] store = {-1};
        int[] at = {-1, -1};
        long op1 = engineOp(), op2 = engineOp(), op3 = engineOp();
        boolean[] carried = {false};
        GameTestSequence s = build(helper, y, false, List.of(fa, fb))
                .thenWaitUntil(() -> checkStore(level, List.of(fa, fb), new long[] {y.da(), y.db()}, store))
                .thenExecute(() -> {
                    at[0] = indexIn(declaration(store[0]), fb.pos);
                    at[1] = districtAt(helper, fb.pos).inventoryAt(fb.pos).index();
                    IndustryTestKit.writeChain(op1, store[0], y.db(), at[1], at[0], fb.pos, 2, Map.of(iron, 4L));
                })
                .thenWaitUntil(() -> {
                    carried[0] |= ind().chunkCarriesOp(level.getChunkAt(fb.pos), op1);
                    check("op 1 applied", commandResults(op1).contains(Obs.CommandResult.APPLIED));
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    List<String> addresses = new ArrayList<>();
                    long got = ironHeld(level, fb.pos, addresses);
                    List<ByteBuffer> in = writtenIn(store[0], op1);
                    record(t, "FB after op 1 (" + IndustryTestKit.engineName() + ")", addresses + ", " + got + " iron, chunk carried " + carried[0]
                            + ", CHAIN_INs with the op " + in.size());
                    check("the chunk carried the write", carried[0]);
                    check("two packages to east in FB: " + addresses, addresses.equals(List.of("east", "east")));
                    check("8 iron in them: " + got, got == 8);
                    check("one CHAIN_IN with the op: " + in.size(), in.size() == 1);
                    ByteBuffer r = in.get(0);
                    check("at FB, in B", r.getInt(IndRecords.ChainPackage.FROGPORT) == at[0] && r.getLong(IndRecords.ChainPackage.DISTRICT) == y.db()
                            && r.getInt(IndRecords.ChainPackage.INVENTORY) == at[1]);
                    check("two packages of 8 iron", r.getInt(IndRecords.ChainPackage.PACKAGES) == 2
                            && goodsIn(r, IndRecords.ChainPackage.ENTRIES, aux(r), IndRecords.ENTRY16) == 8);
                    check("addressed east", "east".equals(javaName(r.getInt(IndRecords.ChainPackage.ADDRESS))));
                    check("FB sent nothing out", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_OUT).isEmpty());
                    IndustryTestKit.writeChain(op1, store[0], y.db(), at[1], at[0], fb.pos, 2, Map.of(iron, 4L));
                })
                .thenWaitUntil(() -> check("op 1 again is a duplicate", commandResults(op1).contains(Obs.CommandResult.DUPLICATE_OP)))
                .thenExecute(() -> {
                    check("still two packages", held(level, fb.pos) == 2);
                    check("still one CHAIN_IN with op 1", writtenIn(store[0], op1).size() == 1);
                    IndustryTestKit.writeChain(op2, store[0], y.db(), at[1], at[0], fb.pos, 20, Map.of(iron, 4L));
                })
                .thenWaitUntil(() -> check("op 2 applied", commandResults(op2).contains(Obs.CommandResult.APPLIED)))
                .thenExecute(() -> {
                    List<ByteBuffer> in = writtenIn(store[0], op2);
                    record(t, "FB after op 2", held(level, fb.pos) + " packages, CHAIN_INs " + in.size()
                            + (in.isEmpty() ? "" : ", packages " + in.get(0).getInt(IndRecords.ChainPackage.PACKAGES)));
                    check("FB has one slot left: " + held(level, fb.pos), held(level, fb.pos) == 17);
                    check("op 2 wrote the 15 that fitted", in.size() == 1 && in.get(0).getInt(IndRecords.ChainPackage.PACKAGES) == 15
                            && goodsIn(in.get(0), IndRecords.ChainPackage.ENTRIES, aux(in.get(0)), IndRecords.ENTRY16) == 60);
                    IndustryTestKit.writeChain(op3, store[0], y.db(), at[1], at[0], fb.pos, 1, Map.of(iron, 4L));
                })
                .thenWaitUntil(() -> check("op 3 failed", commandResults(op3).contains(Obs.CommandResult.FAILED)))
                .thenExecute(() -> {
                    check("no CHAIN_IN for op 3", writtenIn(store[0], op3).isEmpty());
                    FrogportBlockEntity f = frog(level, fb.pos);
                    for (int i = 0; i < f.inventory.getSlots(); i++)
                        f.inventory.setStackInSlot(i, ItemStack.EMPTY);
                });
        restore(s, level, y).thenSucceed();
    }

    // ---- slice S4c1: the frontier ---------------------------------------------------------------------

    /** The frontier's CHAIN_HELD records of a store. */
    private static List<ByteBuffer> heldRecords(long store) {
        return chainRecords(store, IndRecords.ChainPackage.CHAIN_HELD);
    }

    /** The IND_HELD_LIST records of frontier epoch {@code epoch}. */
    private static List<ByteBuffer> heldLists(long epoch) {
        return received(IndRecords.IND_HELD_LIST, b -> b.getInt(IndRecords.HeldList.EPOCH) == (int) epoch);
    }

    /** The keys of the packages circling a conveyor; a package with no key adds 0. */
    private static List<Long> keysAt(ServerLevel level, BlockPos p) {
        List<Long> out = new ArrayList<>();
        ChainConveyorBlockEntity c = conveyor(level, p);
        for (ChainConveyorPackage box : c == null ? List.<ChainConveyorPackage>of() : c.getLoopingPackages()) {
            Frontier.Key k = Frontier.key(box.item);
            out.add(k == null ? 0 : k.serial());
        }
        return out;
    }

    /**
     * Slice S4c1 (SECTIONS.md §2.5, Q5; choices SC135 to SC139): B stops ticking (FULL, around A's forced chunk),
     * so Create's route to FB expires on CA, the boundary conveyor. The frontier keeps it, and packages from FA
     * for FB circle CA, held: one CHAIN_HELD each, keyed, toward FB at CA, and none again while they circle. A
     * restart (CA's table emptied and its packages saved and loaded, the stores and the frontier loaded again
     * from {@code IndustryData}) brings the route back and keeps the keys, each told once more in the new
     * session; one package was gone from CA's saved packages (a crash after the chunk's save lost its
     * release), and CA's first tick releases it; the session's IND_HELD_LIST names the other two. New
     * packages are held once each. One package then leaves the chain unseen (taken out of CA's list). Once B
     * ticks again, the kept route goes, the three on the chain land in FB as CHAIN_INs carrying their keys,
     * FB's packages carry no key, and the census releases the fifth's (a second CHAIN_RELEASED).
     */
    @GameTest(template = YARD, batch = "sec1_s4c", timeoutTicks = TIMEOUT)
    public static void aRouteToAnAwayFrogportIsKept(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aRouteToAnAwayFrogportIsKept";
        Yard y = Yard.at(helper, 6100, false);
        Frog fa = new Frog(y.ca.west(2), y.ca, "west"), fb = new Frog(y.cb.east(2), y.cb, "east");
        BlockPos toB = y.cb.subtract(y.ca);
        long[] store = {-1};
        int[] at = {-1, 0};
        long[] gone = {-1};
        long[] lost = {0};
        long[] unfound = {0};
        Set<Long> serials = new HashSet<>();
        GameTestSequence s = build(helper, y, false, List.of(fa, fb))
                .thenWaitUntil(() -> checkStore(level, List.of(fa, fb), new long[] {y.da(), y.db()}, store))
                .thenExecute(() -> {
                    at[0] = indexIn(declaration(store[0]), fb.pos);
                    check("CA routes east to CB", conveyor(level, y.ca).routingTable.getExitFor(box("east", 1)).equals(toB));
                    // A stays forced (level 31): the chunk between ticks (32), B is FULL and does not tick (33).
                    level.setChunkForced(y.ax + 1, y.az, false);
                    level.setChunkForced(y.bx, y.az, false);
                    gone[0] = level.getGameTime();
                })
                .thenWaitUntil(() -> check("B is FULL and does not tick", level.getChunkSource().getChunkNow(y.bx, y.az) != null
                        && !level.shouldTickBlocksAt(net.minecraft.world.level.ChunkPos.asLong(y.bx, y.az)) && Loaded.ticks(level, y.ca)
                        && !Loaded.ticks(level, fb.pos)))
                .thenWaitUntil(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    List<Frontier.Kept> k = f.keptAt(level.dimension(), y.ca);
                    check("CA keeps the route to FB: " + k, k != null && k.size() == 1 && k.get(0).port().equals("east")
                            && k.get(0).connection().equals(toB));
                })
                .thenExecute(() -> {
                    record(t, "route kept after (ticks)", level.getGameTime() - gone[0]);
                    check("no package held yet", heldRecords(store[0]).isEmpty());
                    for (int i = 1; i <= 3; i++)
                        send(level, fa.pos, box("east", i));
                })
                .thenWaitUntil(() -> check("three packages circle CA, held: " + looping(level, y.ca) + ", " + heldRecords(store[0]).size(),
                        looping(level, y.ca) == 3 && held(level, fa.pos) == 0 && heldRecords(store[0]).size() == 3))
                .thenIdle(100)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    List<ByteBuffer> h = heldRecords(store[0]);
                    record(t, "held: records, CA's keys, " + f.perfLine(), h.size() + ", " + keysAt(level, y.ca));
                    check("still three, each held once: " + h.size(), h.size() == 3 && looping(level, y.ca) == 3);
                    check("nothing reached FB", held(level, fb.pos) == 0);
                    long iron = 0;
                    for (ByteBuffer r : h) {
                        check("toward FB", r.getInt(IndRecords.ChainPackage.FROGPORT) == at[0] && r.getInt(IndRecords.ChainPackage.DESTINATION) == at[0]);
                        check("at CA", r.getInt(IndRecords.ChainPackage.X) == y.ca.getX() && r.getInt(IndRecords.ChainPackage.Y) == y.ca.getY()
                                && r.getInt(IndRecords.ChainPackage.Z) == y.ca.getZ());
                        check("no district, no inventory", r.getLong(IndRecords.ChainPackage.DISTRICT) == 0
                                && r.getInt(IndRecords.ChainPackage.INVENTORY) == IndRecords.NO_INVENTORY);
                        check("one package", r.getInt(IndRecords.ChainPackage.PACKAGES) == 1 && r.getLong(IndRecords.ChainPackage.OP_ID) == 0);
                        check("keyed in this epoch", r.getInt(IndRecords.ChainPackage.EPOCH) == f.epoch() && f.epoch() != 0);
                        serials.add(r.getLong(IndRecords.ChainPackage.SERIAL));
                        iron += goodsIn(r, IndRecords.ChainPackage.ENTRIES, aux(r), IndRecords.ENTRY16);
                    }
                    check("three keys", serials.size() == 3 && f.held().keySet().equals(serials));
                    check("the packages carry them: " + keysAt(level, y.ca), new HashSet<>(keysAt(level, y.ca)).equals(serials));
                    check("6 iron held: " + iron, iron == 6);
                    ByteBuffer d = declaration(store[0]);
                    check("the declaration carries the epoch", d.getInt(IndRecords.PackageStore.EPOCH) == f.epoch());
                    at[1] = f.epoch();
                    // A restart: Create's tables start empty, CA's packages come back from its saved tag, the stores
                    // and the frontier load from IndustryData. One package is not in CA's tag: the chunk was saved
                    // after it left, and a crash lost its release before IndustryData was saved.
                    ChainConveyorBlockEntity ca = conveyor(level, y.ca);
                    ChainConveyorPackage first = ca.getLoopingPackages().get(0);
                    unfound[0] = Frontier.key(first.item).serial();
                    ca.getLoopingPackages().remove(first);
                    ca.loadWithComponents(ca.saveWithoutMetadata(level.registryAccess()), level.registryAccess());
                    ca.routingTable.entriesByDistance.clear();
                    conveyor(level, y.cb).routingTable.entriesByDistance.clear();
                    Set<Long> kept = new HashSet<>(serials);
                    kept.remove(unfound[0]);
                    check("CA's packages kept their keys: " + keysAt(level, y.ca), new HashSet<>(keysAt(level, y.ca)).equals(kept));
                    PackageStores.forgetForTest(ind());
                    Frontier.firstTickForTest(ca);
                })
                .thenWaitUntil(() -> check("the kept route is back on CA",
                        conveyor(level, y.ca).routingTable.getExitFor(box("east", 1)).equals(toB)))
                .thenWaitUntil(() -> check("a held list of this epoch", !heldLists(at[1]).isEmpty()))
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    Set<Long> kept = new HashSet<>(serials);
                    kept.remove(unfound[0]);
                    check("the same epoch: " + f.epoch(), f.epoch() == at[1]);
                    check("the two on CA still held", f.held().keySet().equals(kept));
                    List<ByteBuffer> rel = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED);
                    check("CA's first tick released the one not there: " + rel.size(), rel.size() == 1
                            && rel.get(0).getLong(IndRecords.ChainPackage.SERIAL) == unfound[0]);
                    List<ByteBuffer> lists = heldLists(at[1]);
                    Set<Long> listed = new HashSet<>();
                    ByteBuffer l = lists.get(lists.size() - 1);
                    for (int i = 0; i < aux(l); i++)
                        listed.add(l.getLong(IndRecords.HeldList.SERIALS + 8 * i));
                    record(t, "after the restart: held list, " + f.perfLine(), listed.size());
                    check("the held list names the two: " + listed, listed.containsAll(kept) && !listed.contains(unfound[0]));
                    send(level, fa.pos, box("east", 4));
                    send(level, fa.pos, box("east", 5));
                })
                .thenWaitUntil(() -> check("four circle CA, seven held records: " + looping(level, y.ca) + ", " + heldRecords(store[0]).size(),
                        looping(level, y.ca) == 4 && heldRecords(store[0]).size() == 7))
                .thenIdle(60)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    List<ByteBuffer> h = heldRecords(store[0]);
                    record(t, "after the restart: records, CA's keys, " + f.perfLine(), h.size() + ", " + keysAt(level, y.ca));
                    check("the two kept told once more, the new two once: " + h.size(), h.size() == 7);
                    Set<Long> before = new HashSet<>(serials);
                    before.remove(unfound[0]);
                    java.util.Map<Long, Integer> times = new java.util.HashMap<>();
                    for (ByteBuffer r : h) {
                        serials.add(r.getLong(IndRecords.ChainPackage.SERIAL));
                        times.merge(r.getLong(IndRecords.ChainPackage.SERIAL), 1, Integer::sum);
                    }
                    for (long k : before)
                        check("told twice: " + times, times.get(k) == 2);
                    check("the one not there told once: " + times, times.get(unfound[0]) == 1);
                    Set<Long> held = new HashSet<>(serials);
                    held.remove(unfound[0]);
                    check("five keys, four held", serials.size() == 5 && f.held().keySet().equals(held));
                    // One package leaves the chain unseen by any hook.
                    ChainConveyorBlockEntity ca = conveyor(level, y.ca);
                    ChainConveyorPackage gone1 = ca.getLoopingPackages().get(0);
                    Frontier.Key k = Frontier.key(gone1.item);
                    check("a keyed package to take", k != null && serials.contains(k.serial()));
                    lost[0] = k.serial();
                    ca.getLoopingPackages().remove(gone1);
                    ca.setChanged();
                    level.setChunkForced(y.ax + 1, y.az, true);
                    level.setChunkForced(y.bx, y.az, true);
                })
                .thenWaitUntil(() -> check("the three land in FB once B ticks: FB " + held(level, fb.pos) + ", CA " + looping(level, y.ca),
                        held(level, fb.pos) == 3 && looping(level, y.ca) == 0 && travelling(level, y.ca) == 0))
                .thenWaitUntil(() -> {
                    check("three in at FB", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN).size() == 3);
                    check("the kept route went", PackageStores.of(ind()).frontier.keptAt(level.dimension(), y.ca) == null);
                    check("the census released the fifth", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED).size() == 2);
                })
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    Set<Long> landed = new HashSet<>();
                    for (ByteBuffer r : chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN)) {
                        check("in at FB", r.getInt(IndRecords.ChainPackage.FROGPORT) == at[0]);
                        check("with its key", r.getInt(IndRecords.ChainPackage.EPOCH) == f.epoch());
                        landed.add(r.getLong(IndRecords.ChainPackage.SERIAL));
                    }
                    ByteBuffer rel = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED).get(1);
                    record(t, "landed: " + f.perfLine(), landed.size());
                    check("the released key is the lost one", rel.getLong(IndRecords.ChainPackage.SERIAL) == lost[0]
                            && rel.getInt(IndRecords.ChainPackage.EPOCH) == f.epoch() && rel.getInt(IndRecords.ChainPackage.PACKAGES) == 0);
                    Set<Long> rest = new HashSet<>(serials);
                    rest.remove(lost[0]);
                    rest.remove(unfound[0]);
                    check("every other key landed once", landed.equals(rest));
                    check("nothing held", f.held().isEmpty());
                    FrogportBlockEntity frog = frog(level, fb.pos);
                    for (int i = 0; i < frog.inventory.getSlots(); i++)
                        check("FB's packages carry no key", frog.inventory.getStackInSlot(i).isEmpty()
                                || Frontier.key(frog.inventory.getStackInSlot(i)) == null);
                    for (int i = 0; i < frog.inventory.getSlots(); i++)
                        frog.inventory.setStackInSlot(i, ItemStack.EMPTY);
                });
        restore(s, level, y).thenSucceed();
    }

    // ---- slice S4c2: the draw ------------------------------------------------------------------------

    /** The frontier's CHAIN_DRAWN records of a store. */
    private static List<ByteBuffer> drawnRecords(long store) {
        return chainRecords(store, IndRecords.ChainPackage.CHAIN_DRAWN);
    }

    private static Set<Long> serialsOf(List<ByteBuffer> records) {
        Set<Long> out = new HashSet<>();
        for (ByteBuffer r : records)
            out.add(r.getLong(IndRecords.ChainPackage.SERIAL));
        return out;
    }

    /**
     * Slice S4c2 (SECTIONS.md §2.5; choices SC141 to SC143), scripted as the engine's IND_HELD_DRAW (the engine gives
     * back a draw that no replay pool of its own covers, and no pool here does, so the test holds its Return asks off). Five
     * packages for FB are held on CA, as in {@link #aRouteToAnAwayFrogportIsKept}. Draw four: each is marked and told
     * once (CHAIN_DRAWN, key only, at CA). The Remove of the first, CA ticking, takes it off the chain at once and
     * releases it (CHAIN_RELEASED). Once B ticks again the fifth lands in FB, the other three drawn ones keep circling
     * CA, and the census does not count them (their goods are the far replay's). After a restart (as there) the
     * session's held list names them last, as drawn. A and B then unload, and the Remove of the second waits for CA,
     * through a restart while CA is unloaded: at CA's first tick once it loads that package is taken off and released,
     * and only it. The third, given back (Return), lands in FB with its key. After an engine reset (a new epoch) the
     * last one's key is not Java's: it lands as a package with no key, and nothing is held.
     */
    @GameTest(template = YARD, batch = "sec1_s4c2", timeoutTicks = TIMEOUT)
    public static void aHeldPackageDrawnIsRemovedOnce(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        String t = "aHeldPackageDrawnIsRemovedOnce";
        Yard y = Yard.at(helper, 6200, false);
        Frog fa = new Frog(y.ca.west(2), y.ca, "west"), fb = new Frog(y.cb.east(2), y.cb, "east");
        long[] store = {-1};
        int[] epoch = {0};
        // d[0] removed at once, d[1] removed through a restart, d[2] given back, d[3] lands after a reset; other[0]
        // lands.
        long[] d = {0, 0, 0, 0};
        long[] other = {0};
        long[] at = {0};
        int[] lists = {0};
        Set<Long> serials = new HashSet<>();
        GameTestSequence s = build(helper, y, false, List.of(fa, fb))
                .thenWaitUntil(() -> checkStore(level, List.of(fa, fb), new long[] {y.da(), y.db()}, store))
                .thenExecute(() -> {
                    Frontier.ignoreReturnsForTest = true;
                    level.setChunkForced(y.ax + 1, y.az, false);
                    level.setChunkForced(y.bx, y.az, false);
                })
                .thenWaitUntil(() -> check("B is FULL and does not tick", level.getChunkSource().getChunkNow(y.bx, y.az) != null
                        && !level.shouldTickBlocksAt(net.minecraft.world.level.ChunkPos.asLong(y.bx, y.az)) && Loaded.ticks(level, y.ca)
                        && !Loaded.ticks(level, fb.pos)))
                .thenWaitUntil(() -> {
                    List<Frontier.Kept> k = PackageStores.of(ind()).frontier.keptAt(level.dimension(), y.ca);
                    check("CA keeps the route to FB: " + k, k != null && k.size() == 1);
                })
                .thenExecute(() -> {
                    for (int i = 1; i <= 5; i++)
                        send(level, fa.pos, box("east", i));
                })
                .thenWaitUntil(() -> check("five packages circle CA, held: " + looping(level, y.ca) + ", " + heldRecords(store[0]).size(),
                        looping(level, y.ca) == 5 && heldRecords(store[0]).size() == 5))
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    serials.addAll(serialsOf(heldRecords(store[0])));
                    check("five keys: " + serials, serials.size() == 5 && f.held().keySet().equals(serials));
                    List<Long> sorted = new ArrayList<>(serials);
                    java.util.Collections.sort(sorted);
                    for (int i = 0; i < 4; i++)
                        d[i] = sorted.get(i);
                    other[0] = sorted.get(4);
                    epoch[0] = f.epoch();
                    IndustryTestKit.heldDraw(store[0], epoch[0], IndRecords.HeldDraw.DRAW, d[0], d[1], d[2], d[3]);
                })
                .thenWaitUntil(() -> check("four drawn, each told: " + drawnRecords(store[0]).size(),
                        serialsOf(drawnRecords(store[0])).equals(Set.of(d[0], d[1], d[2], d[3]))))
                .thenIdle(40)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    record(t, "drawn: " + f.perfLine(), drawnRecords(store[0]).size());
                    check("told once each", drawnRecords(store[0]).size() == 4);
                    for (ByteBuffer r : drawnRecords(store[0])) {
                        check("a key only", r.getInt(IndRecords.ChainPackage.PACKAGES) == 0 && aux(r) == 0
                                && r.getInt(IndRecords.ChainPackage.EPOCH) == epoch[0]);
                        check("at CA", r.getInt(IndRecords.ChainPackage.X) == y.ca.getX() && r.getInt(IndRecords.ChainPackage.Z) == y.ca.getZ());
                    }
                    for (long x : d)
                        check("marked", f.held().get(x).drawn());
                    check("not marked", !f.held().get(other[0]).drawn());
                    // CA ticks: the first drawn one is taken off at once.
                    IndustryTestKit.heldDraw(store[0], epoch[0], IndRecords.HeldDraw.REMOVE, d[0]);
                })
                .thenWaitUntil(() -> check("the first taken off at once: " + keysAt(level, y.ca),
                        !keysAt(level, y.ca).contains(d[0]) && looping(level, y.ca) == 4
                                && serialsOf(chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED)).equals(Set.of(d[0]))))
                .thenExecute(() -> {
                    check("no longer held", !PackageStores.of(ind()).frontier.held().containsKey(d[0]));
                    level.setChunkForced(y.ax + 1, y.az, true);
                    level.setChunkForced(y.bx, y.az, true);
                })
                .thenWaitUntil(() -> check("the fifth lands in FB: FB " + held(level, fb.pos) + ", CA " + looping(level, y.ca),
                        held(level, fb.pos) == 1 && looping(level, y.ca) == 3))
                .thenIdle(100)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    check("the drawn three still circle CA: " + keysAt(level, y.ca) + ", FB " + held(level, fb.pos),
                            new HashSet<>(keysAt(level, y.ca)).equals(Set.of(d[1], d[2], d[3])) && held(level, fb.pos) == 1
                                    && travelling(level, y.ca) == 0);
                    check("held: the drawn three", f.held().keySet().equals(Set.of(d[1], d[2], d[3])));
                    at[0] = level.getGameTime();
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> c = censuses(store[0], at[0]);
                    check("a census with the drawn three circling", !c.isEmpty());
                    ByteBuffer r = c.get(c.size() - 1);
                    record(t, "census of the drawn: destinations, entries", r.getInt(IndRecords.StoreCensus.M) + ", " + aux(r));
                    check("the drawn three are not counted", r.getInt(IndRecords.StoreCensus.M) == 0 && aux(r) == 0);
                })
                .thenExecute(() -> {
                    // A restart, as in aRouteToAnAwayFrogportIsKept.
                    ChainConveyorBlockEntity ca = conveyor(level, y.ca);
                    ca.loadWithComponents(ca.saveWithoutMetadata(level.registryAccess()), level.registryAccess());
                    PackageStores.forgetForTest(ind());
                    Frontier.firstTickForTest(ca);
                })
                .thenWaitUntil(() -> {
                    checkDrawnList(heldLists(epoch[0]), Set.of(d[1], d[2], d[3]));
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    record(t, "after the restart: " + f.perfLine(), keysAt(level, y.ca).size());
                    check("still drawn after the restart", f.held().get(d[1]).drawn() && f.held().get(d[2]).drawn() && f.held().get(d[3]).drawn());
                    check("still circling CA: " + keysAt(level, y.ca), looping(level, y.ca) == 3 && held(level, fb.pos) == 1);
                    lists[0] = heldLists(epoch[0]).size();
                    force(level, y, false);
                })
                .thenWaitUntil(() -> check("A unloads", level.getChunkSource().getChunkNow(y.ax, y.az) == null))
                .thenExecute(() -> IndustryTestKit.heldDraw(store[0], epoch[0], IndRecords.HeldDraw.REMOVE, d[1]))
                .thenWaitUntil(() -> {
                    Frontier.Held h = PackageStores.of(ind()).frontier.held().get(d[1]);
                    check("the removal waits for CA", h != null && h.removing());
                })
                .thenExecute(() -> {
                    check("not released yet", !serialsOf(chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED)).contains(d[1]));
                    // A restart while CA is unloaded: the removal is read back from the save.
                    PackageStores.forgetForTest(ind());
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> l = heldLists(epoch[0]);
                    check("a held list after the restart: " + l.size() + " after " + lists[0], l.size() > lists[0]);
                    checkDrawnList(l, Set.of(d[1], d[2], d[3]));
                })
                .thenExecute(() -> {
                    Frontier.Held h = PackageStores.of(ind()).frontier.held().get(d[1]);
                    check("still waiting for CA after the restart", h != null && h.drawn() && h.removing());
                    force(level, y, true);
                })
                .thenWaitUntil(() -> check("CA's first tick takes it off: " + keysAt(level, y.ca), Loaded.ticks(level, y.ca)
                        && new HashSet<>(keysAt(level, y.ca)).equals(Set.of(d[2], d[3])) && looping(level, y.ca) == 2))
                .thenWaitUntil(() -> check("released",
                        serialsOf(chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED)).contains(d[1])))
                .thenIdle(60)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    record(t, "removed: " + f.perfLine(), keysAt(level, y.ca).size());
                    List<ByteBuffer> rel = chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED);
                    check("only the removed two released, once each: " + serialsOf(rel),
                            rel.size() == 2 && serialsOf(rel).equals(Set.of(d[0], d[1])));
                    check("held: the last drawn two", f.held().keySet().equals(Set.of(d[2], d[3])));
                    check("nothing more in FB", held(level, fb.pos) == 1);
                    Frontier.ignoreReturnsForTest = false;
                    IndustryTestKit.heldDraw(store[0], epoch[0], IndRecords.HeldDraw.RETURN, d[2]);
                })
                .thenWaitUntil(() -> check("given back, it lands in FB: FB " + held(level, fb.pos) + ", CA " + keysAt(level, y.ca),
                        held(level, fb.pos) == 2 && keysAt(level, y.ca).equals(List.of(d[3])) && travelling(level, y.ca) == 0))
                .thenWaitUntil(() -> check("in at FB with its key",
                        serialsOf(chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN)).equals(Set.of(other[0], d[2]))))
                .thenIdle(60)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    record(t, "given back: " + f.perfLine(), held(level, fb.pos));
                    check("held: the last drawn one", f.held().keySet().equals(Set.of(d[3])) && f.held().get(d[3]).drawn());
                    check("still circling CA", keysAt(level, y.ca).equals(List.of(d[3])));
                    // An engine reset: a new epoch, in which no key given out before is Java's.
                    PackageStores.of(ind()).announce();
                    check("a new epoch", f.epoch() != epoch[0] && f.held().isEmpty());
                })
                .thenWaitUntil(() -> check("after the reset it lands in FB: FB " + held(level, fb.pos) + ", CA " + looping(level, y.ca),
                        held(level, fb.pos) == 3 && looping(level, y.ca) == 0 && travelling(level, y.ca) == 0))
                .thenWaitUntil(() -> check("in at FB with no key",
                        serialsOf(chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN)).equals(Set.of(other[0], d[2], 0L))))
                .thenIdle(60)
                .thenExecute(() -> {
                    Frontier f = PackageStores.of(ind()).frontier;
                    record(t, "after the reset: " + f.perfLine(), held(level, fb.pos));
                    check("nothing held", f.held().isEmpty());
                    check("three in at FB", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_IN).size() == 3);
                    FrogportBlockEntity in = frog(level, fb.pos);
                    for (int i = 0; i < in.inventory.getSlots(); i++)
                        check("no key left in FB", Frontier.key(in.inventory.getStackInSlot(i)) == null);
                    check("two releases in all", chainRecords(store[0], IndRecords.ChainPackage.CHAIN_RELEASED).size() == 2);
                    FrogportBlockEntity frog = frog(level, fb.pos);
                    for (int i = 0; i < frog.inventory.getSlots(); i++)
                        frog.inventory.setStackInSlot(i, ItemStack.EMPTY);
                });
        restore(s, level, y).thenExecute(() -> Frontier.ignoreReturnsForTest = false).thenSucceed();
    }

    /** The newest of {@code lists} names {@code drawn} last, its only drawn keys (other tests may hold keys). */
    private static void checkDrawnList(List<ByteBuffer> lists, Set<Long> drawn) {
        ByteBuffer l = lists.isEmpty() ? null : lists.get(lists.size() - 1);
        int n = l == null ? 0 : aux(l), m = drawn.size();
        Set<Long> last = new HashSet<>();
        for (int i = Math.max(0, n - m); i < n; i++)
            last.add(l.getLong(IndRecords.HeldList.SERIALS + 8 * i));
        check("a held list naming the drawn last: " + last, l != null && n >= m && l.getInt(IndRecords.HeldList.DRAWN) == m
                && last.equals(drawn));
    }
}
