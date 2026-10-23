package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassExitKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import com.simibubi.create.AllBlocks;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.Arrays;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.YARD;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.borderX;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.districtAt;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.fastConfig;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.floorY;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.forget;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.ind;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.midZ;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.playerPlaced;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.record;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.barrel;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.fill;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.set;

/**
 * Residual groups (SECTIONS.md §3.1, choices SC14, SC50 and SC55; slice S2d's GameTest of them, SC47): the
 * block entities with a capability that no extractor claims and that are no district stock (vanilla droppers
 * here) are joined by their faces into groups, each one BLACK_BOX node at its lowest corner with its extent
 * and the FNV-1a hash of its sorted positions, made when an edge first reaches it. A group between stores is
 * an unknown machine: its own working section and unit, whose box holds the group's extent grown by one.
 *
 * <p>The build, in one chunk on the yard's floor: line A is three droppers in a row along x (A1, A2, A3), with
 * a chute from a barrel above into A1 and another into A2, so two edges reach the one group; line B is one
 * dropper fed by a chute from its own barrel, four blocks east of A3; C is a dropper alone, which no edge
 * reaches. Then a player breaks A3: the recompile shrinks group A's extent and hash, unit A starts afresh,
 * and unit B, whose blocks did not change, carries on.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class ResidualGroupsGameTests {
    private ResidualGroupsGameTests() {}

    /** FNV-1a over each position's {@code asLong} bytes, low byte first, in sorted order (SC55). */
    static long hash(BlockPos... ps) {
        long[] sorted = Arrays.stream(ps).mapToLong(BlockPos::asLong).sorted().toArray();
        long h = 0xcbf29ce484222325L;
        for (long p : sorted)
            for (int b = 0; b < 8; b++) {
                h ^= (p >>> (8 * b)) & 0xff;
                h *= 0x100000001b3L;
            }
        return h;
    }

    static GlassKit.Node at(List<GlassKit.Node> ns, BlockPos p) {
        for (GlassKit.Node n : ns)
            if (n.x() == p.getX() && n.y() == p.getY() && n.z() == p.getZ())
                return n;
        return null;
    }

    static String params(GlassKit.Node n) {
        return n == null ? "none" : Arrays.toString(n.params()) + " flags " + n.flags();
    }

    /** The group node at {@code low} with extent {@code (dx, 0, 0)} and the hash of {@code held}, BOUNDED. */
    static void checkGroup(String name, List<GlassKit.Node> boxes, BlockPos low, int dx, BlockPos... held) {
        GlassKit.Node n = at(boxes, low);
        check("group " + name + " is a BLACK_BOX node at its lowest corner " + low.toShortString(), n != null);
        long[] p = n.params();
        check("group " + name + "'s params are its extent (" + dx + " 0 0) and its hash: " + params(n),
                p[0] == dx && p[1] == 0 && p[2] == 0 && p[3] == hash(held));
        check("group " + name + " is BOUNDED", n.has(GlassRecords.Node.BOUNDED));
    }

    /** The one chute edge out of the chute node at {@code chute} ends at {@code to}. */
    static void checkChuteInto(GlassKit.Graph g, BlockPos chute, GlassKit.Node to, String what) {
        GlassKit.Node c = at(g.nodes(-1, GlassRecords.Model.CHUTE), chute);
        check("a chute node at " + chute.toShortString(), c != null);
        List<GlassKit.Edge> out = g.edgesFrom(c.index()).stream().filter(e -> e.kind() == GlassRecords.Edge.CHUTE).toList();
        check("the chute at " + chute.toShortString() + " feeds " + what + ": " + out.size() + " chute edges out, to "
                + out.stream().map(e -> String.valueOf(e.to())).toList() + ", the group is " + to.index(),
                out.size() == 1 && out.get(0).to() == to.index());
    }

    static String units(GlassReport r) {
        StringBuilder sb = new StringBuilder();
        for (GlassReport.Unit u : r.units())
            sb.append("[").append(u.summary()).append(", ready at ").append(u.readyAt()).append("] ");
        return sb.toString();
    }

    /** The one unit whose box holds {@code p}. */
    static GlassReport.Unit unitAt(GlassReport r, BlockPos p, String what) {
        List<GlassReport.Unit> us = r.unitsAt(p.getX(), p.getY(), p.getZ());
        check("one unit holds " + what + " " + p.toShortString() + ", not " + us.size() + ": " + units(r), us.size() == 1);
        return us.get(0);
    }

    @GameTest(template = YARD, batch = "sec1_res_1", timeoutTicks = 1600)
    public static void residualBlocksAreGroupsByTheirFaces(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos a1 = new BlockPos(bx + 2, y, z), a2 = a1.east(), a3 = a2.east();
        BlockPos b = new BlockPos(bx + 8, y, z), c = new BlockPos(bx + 12, y, z);
        BlockPos[] chutes = {a1.above(), a2.above(), b.above()};
        for (BlockPos d : new BlockPos[] {a1, a2, a3, b, c})
            set(helper, d, Blocks.DROPPER.defaultBlockState());
        for (BlockPos ch : chutes) {
            set(helper, ch, AllBlocks.CHUTE.getDefaultState());
            barrel(helper, ch.above());
            fill(helper, ch.above(), Items.COBBLESTONE, 64);
        }
        for (BlockPos ch : chutes)
            playerPlaced(helper, ch.above());
        String t = "residual groups";
        record(t, "build", "droppers A " + a1.toShortString() + " to " + a3.toShortString() + ", B " + b.toShortString() + ", C "
                + c.toShortString() + "; chutes at " + Arrays.stream(chutes).map(BlockPos::toShortString).toList());
        long[] id = {0};
        int[] mark = {-1};
        long[] readyB = {0}, readyA = {0};
        boolean natives = IndustryKit.nativeEngine();
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, a1);
                    check("the build's chunk is a district", d != null && districtAt(helper, c) == d && districtAt(helper, b) == d);
                    GlassKit.Graph g = GlassKit.latest(d.id);
                    check("a whole compile: " + (g == null ? "none" : g.describe()) + " (compile " + ind().glass().compileNo(d.id)
                            + ", pending " + ind().glass().pending(d.id) + ", " + ind().glass().recordsDropped() + " records dropped)",
                            g != null && g.whole());
                    id[0] = d.id;
                })
                .thenExecute(() -> {
                    GlassKit.Graph g = GlassKit.latest(id[0]);
                    List<GlassKit.Node> boxes = g.nodes(GlassRecords.Node.BLACK_BOX, GlassRecords.Model.BLACK_BOX);
                    record(t, "compile", g.end().compile() + ": " + g.nodes().size() + " nodes, " + g.edges().size() + " edges, "
                            + boxes.size() + " residual groups " + boxes.stream().map(n -> "(" + n.x() + " " + n.y() + " " + n.z() + ") "
                            + params(n)).toList());
                    check("two residual group nodes, one per face-connected group an edge reaches, not " + boxes.size(), boxes.size() == 2);
                    checkGroup("A", boxes, a1, 2, a1, a2, a3);
                    checkGroup("B", boxes, b, 0, b);
                    check("no node for dropper C, which no edge reaches", at(g.nodes(), c) == null);
                    check("a compile with a residual group is a black box one", (g.end().flags() & GlassRecords.End.BLACK_BOX) != 0);
                    GlassKit.Node ga = at(boxes, a1), gb = at(boxes, b);
                    checkChuteInto(g, chutes[0], ga, "group A");
                    checkChuteInto(g, chutes[1], ga, "group A");
                    checkChuteInto(g, chutes[2], gb, "group B");
                    mark[0] = g.end().compile();
                })
                .thenWaitUntil(() -> {
                    if (!natives)
                        return;
                    // a later compile of the same blocks (a place event's edit) is the same graph
                    GlassReport r = GlassExitKit.report(id[0], null);
                    int now = GlassKit.latest(id[0]).end().compile();
                    check("the engine's report of the latest compile " + now + " (from " + mark[0] + ")", r != null && now >= mark[0] && r.compile() == now);
                    // SC50: the box holds the group's extent (to A3) grown by one, so x + 1 past A3 and no further
                    GlassReport.Unit ua = unitAt(r, a3.east(), "one past group A's extent");
                    GlassReport.Unit ub = unitAt(r, b, "group B");
                    check("unit A holds the whole group", ua.holds(a1.getX(), a1.getY(), a1.getZ()) && ua.holds(a2.getX(), a2.getY(), a2.getZ()));
                    check("groups A and B are apart: unit A does not hold B, nor two past A3", !ua.equals(ub) && !ua.holds(b.getX(), b.getY(), b.getZ())
                            && !ua.holds(a3.getX() + 2, a3.getY(), a3.getZ()));
                    check("dropper C is in no unit (idle)", r.unitsAt(c.getX(), c.getY(), c.getZ()).isEmpty());
                    readyA[0] = ua.readyAt();
                    readyB[0] = ub.readyAt();
                    record(t, "units", units(r));
                })
                .thenExecute(() -> {
                    NeoForge.EVENT_BUS.post(new BlockEvent.BreakEvent(level, a3, level.getBlockState(a3), FakePlayers.editor(level)));
                    level.removeBlock(a3, false);
                    ind().glass().requestCompile(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    GlassKit.Graph g = GlassKit.latest(id[0]);
                    check("a new whole compile", g != null && g.whole() && g.end().compile() > mark[0]);
                })
                .thenExecute(() -> {
                    GlassKit.Graph g = GlassKit.latest(id[0]);
                    List<GlassKit.Node> boxes = g.nodes(GlassRecords.Node.BLACK_BOX, GlassRecords.Model.BLACK_BOX);
                    record(t, "recompile", g.end().compile() + ": " + boxes.stream().map(n -> "(" + n.x() + " " + n.y() + " " + n.z() + ") "
                            + params(n)).toList());
                    check("still two residual group nodes, not " + boxes.size(), boxes.size() == 2);
                    checkGroup("A without A3", boxes, a1, 1, a1, a2);
                    checkGroup("B", boxes, b, 0, b);
                    mark[0] = g.end().compile();
                })
                .thenWaitUntil(() -> {
                    if (!natives)
                        return;
                    // a later compile of the same blocks (a place event's edit) is the same graph
                    GlassReport r = GlassExitKit.report(id[0], null);
                    int now = GlassKit.latest(id[0]).end().compile();
                    check("the engine's report of the latest compile " + now + " (from " + mark[0] + ")", r != null && now >= mark[0] && r.compile() == now);
                    GlassReport.Unit ua = unitAt(r, a2, "group A");
                    GlassReport.Unit ub = unitAt(r, b, "group B");
                    check("unit A's box shrank with the group: it no longer holds one past A2's neighbour " + a3.east().toShortString(),
                            !ua.holds(a3.getX() + 1, a3.getY(), a3.getZ()));
                    check("unit A is new (its group's hash changed): ready at " + ua.readyAt() + ", before " + readyA[0], ua.readyAt() > readyA[0]);
                    check("unit B carried on: ready at " + ub.readyAt() + ", before " + readyB[0], ub.readyAt() == readyB[0]);
                    record(t, "units after the edit", units(r));
                })
                .thenSucceed();
    }
}
