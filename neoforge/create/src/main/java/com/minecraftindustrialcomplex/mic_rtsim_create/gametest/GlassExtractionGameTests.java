package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlassAccess;
import com.minecraftindustrialcomplex.mic_rtsim_create.glass.Cycles;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;
import com.simibubi.create.content.redstone.DirectedDirectionalBlock;
import com.simibubi.create.content.redstone.thresholdSwitch.ThresholdSwitchBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * The glass box's extraction end to end (GLASSBOX.md §11.3, the compile's half): a real Create build is
 * detected as a district, compiled by the Create plugin and sent; the engine's received log holds a
 * complete compile (END's count equals the records of that compile number), and the decoded graph is
 * the build: its store, chute, millstone, kinetic domain, recipe and rate table.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class GlassExtractionGameTests {
    private GlassExtractionGameTests() {}

    private static int good(ItemStack s) {
        return ind().goodId(ind().goods().name(s));
    }

    /**
     * One block entity's fingerprint (G-B3) through the caches a pass uses (per type, per block state, per
     * block): a hopper's facing is structure and moves it, its enabled flag is model state and does not,
     * and a state seen before hashes as before after other states came between. A Create chute has a
     * part; a vanilla chest has none.
     */
    @GameTest(template = YARD, batch = "glass_extract_fp", timeoutTicks = 100)
    public static void fingerprintsCountStructureNotModelState(GameTestHelper helper) {
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos hopper = new BlockPos(bx + 3, y, z), chute = hopper.east(), chest = chute.east();
        net.minecraft.server.level.ServerLevel level = helper.getLevel();
        set(helper, hopper, Blocks.HOPPER.defaultBlockState());
        set(helper, chute, AllBlocks.CHUTE.getDefaultState());
        set(helper, chest, Blocks.CHEST.defaultBlockState());
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassExtractor x = com.minecraftindustrialcomplex.mic_rtsim_create.glass.CreateGlass.INSTANCE;
        java.util.function.LongSupplier print = () -> x.fingerprint(level.getBlockEntity(hopper));
        long down = print.getAsLong();
        level.setBlock(hopper, level.getBlockState(hopper).setValue(net.minecraft.world.level.block.HopperBlock.ENABLED, false), 2);
        long off = print.getAsLong();
        level.setBlock(hopper, level.getBlockState(hopper).setValue(net.minecraft.world.level.block.HopperBlock.FACING, Direction.NORTH), 2);
        long north = print.getAsLong();
        level.setBlock(hopper, level.getBlockState(hopper).setValue(net.minecraft.world.level.block.HopperBlock.FACING, Direction.DOWN)
                .setValue(net.minecraft.world.level.block.HopperBlock.ENABLED, true), 2);
        long again = print.getAsLong();
        check("a hopper has a part", down != 0);
        check("enabled is model state: " + off + " against " + down, off == down);
        check("facing is structure", north != down);
        check("the first state hashes as before: " + again + " against " + down, again == down);
        check("a Create chute has a part", x.fingerprint(level.getBlockEntity(chute)) != 0);
        check("a vanilla chest has none", x.fingerprint(level.getBlockEntity(chest)) == 0);
        helper.succeed();
    }

    /**
     * A chest of wheat over a chute over a millstone on a creative motor at 64 RPM. The compile reaches
     * the engine complete; its graph has the chest's STORE, the chute's MOVER (store to chute to
     * millstone), the millstone MACHINE in a one-level domain at 64 RPM with the milling recipe for wheat
     * and a rate entry of one item per {@code Cycles.millstone(150, 64)} ticks, and the domain's member for
     * the millstone. The Create and Minecraft accessors resolved every member (an update fails here first).
     */
    @GameTest(template = YARD, batch = "glass_extract_01", timeoutTicks = 600)
    public static void aCompileReachesTheEngineComplete(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        check("the Create accessor resolved TorquePropagator.networks, the deployer's mode and the arm's points", CreateGlassAccess.allResolved());
        check("the Minecraft accessor resolved the level's block entity tickers",
                com.minecraftindustrialcomplex.mic_rtsim_create.glass.MinecraftGlassAccess.allResolved());
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos motor = new BlockPos(bx + 3, y, z), mill = motor.above(), chute = mill.above(), chest = chute.above();
        set(helper, motor, AllBlocks.CREATIVE_MOTOR.getDefaultState().setValue(BlockStateProperties.FACING, Direction.UP));
        if (helper.getLevel().getBlockEntity(motor) instanceof CreativeMotorBlockEntity m)
            m.generatedSpeed.setValue(64);
        set(helper, mill, AllBlocks.MILLSTONE.getDefaultState());
        set(helper, chute, AllBlocks.CHUTE.getDefaultState());
        set(helper, chest, Blocks.CHEST.defaultBlockState());
        if (helper.getLevel().getBlockEntity(chest) instanceof ChestBlockEntity c)
            c.setItem(0, new ItemStack(Items.WHEAT, 64));
        playerPlaced(helper, chest);
        long[] id = {0};
        GlassKit.Graph[] seen = {null};
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the chest's chunk is a district", d != null);
                    id[0] = d.id;
                    check("the millstone turns", helper.getLevel().getBlockEntity(mill) instanceof KineticBlockEntity k && k.getSpeed() != 0);
                })
                .thenWaitUntil(() -> {
                    GlassKit.Graph g = GlassKit.latest(id[0]);
                    check("a compile's END arrived", g != null);
                    check("the compile is complete: " + g.describe(), g.complete());
                    // a compile from before the millstone joined the motor's network has it outside any
                    // domain; joining changes the fingerprint (the network id), so a new compile follows
                    check("a compile with the millstone in a domain", g.nodes(GlassRecords.Node.MACHINE, GlassRecords.Model.MILLSTONE).stream()
                            .anyMatch(n -> n.domain() != GlassRecords.NONE));
                    seen[0] = g;
                })
                .thenExecute(() -> {
                    GlassKit.Graph g = seen[0];
                    String t = "aCompileReachesTheEngineComplete";
                    record(t, "graph", g.describe());
                    GraphBuilder java = ind().glass().lastGraph(id[0]);
                    if (java != null)
                        record(t, "the Java side's last graph", java.nodeCount() + " nodes, " + java.edges().size() + " edges");
                    check("END counts its records: " + g.end().records(), g.end().records() == g.nodes().size() + g.edges().size() + g.recipes().size() + g.domains().size());
                    check("not a black box or not compilable: flags " + g.end().flags(), (g.end().flags() & (GlassRecords.End.NOT_COMPILABLE | GlassRecords.End.BLACK_BOX)) == 0);
                    check("on the level, not a moving frame", g.end().frame() == 0);

                    var stores = g.nodes(GlassRecords.Node.STORE, -1);
                    check("one store, the chest: " + stores.size(), stores.size() == 1 && stores.get(0).inventory() != GlassRecords.NONE);
                    var mills = g.nodes(GlassRecords.Node.MACHINE, GlassRecords.Model.MILLSTONE);
                    check("one millstone machine: " + mills.size(), mills.size() == 1);
                    GlassKit.Node m = mills.get(0);
                    var chutes = g.nodes(GlassRecords.Node.MOVER, GlassRecords.Model.CHUTE);
                    check("one chute mover: " + chutes.size(), chutes.size() == 1);
                    GlassKit.Node c = chutes.get(0);
                    check("store to chute", g.edgesFrom(stores.get(0).index()).stream().anyMatch(e -> e.to() == c.index() && e.kind() == GlassRecords.Edge.CHUTE));
                    check("chute to millstone", g.edgesFrom(c.index()).stream().anyMatch(e -> e.to() == m.index() && e.kind() == GlassRecords.Edge.CHUTE));
                    check("the chute moves 16 per grab: " + c.rates(), c.rates().size() == 1 && c.rates().get(0).batch() == Cycles.CHUTE_EXTRACTION
                            && (c.rates().get(0).flags() & GlassRecords.Node.RATE_SHARED) != 0);

                    check("the millstone is in a domain", m.domain() != GlassRecords.NONE);
                    GlassKit.Domain d = g.domains().get(m.domain());
                    check("one level at 64 RPM: " + d.levels(), d.levels().size() == 1 && d.levels().get(0).rpm() == 64.0);
                    check("the capacity is the motor's", d.levels().get(0).capacity() > 0);
                    check("the millstone is a member at ratio 1", d.members().stream().anyMatch(x -> x.node() == m.index() && x.ratioNum() == x.ratioDen()));

                    int wheat = good(new ItemStack(Items.WHEAT));
                    GlassKit.Recipe milling = null;
                    for (GlassKit.Recipe r : g.recipes())
                        if (r.type() == GlassRecords.RecipeType.MILLING && r.inputs().size() == 1 && r.inputs().get(0).good() == wheat)
                            milling = r;
                    check("the millstone's recipe for wheat is milling", milling != null);
                    record(t, "milling", milling + " named " + javaName(milling.name()));
                    int ticks = Cycles.millstone(milling.duration(), 64f);
                    final int rec = milling.index();
                    check("a rate entry: one wheat per " + ticks + " ticks, got " + m.rates(), m.rates().stream().anyMatch(r -> r.level() == 0 && r.recipe() == rec
                            && r.batch() == 1 && r.ticksNum() == ticks && r.ticksDen() == 1));
                })
                .thenSucceed();
    }

    /**
     * G-B8: a switch is a controller the engine simulates, not a recompile. A creative motor turns a
     * millstone through a clutch; a threshold switch beside the clutch reads a chest. With the chest empty
     * the switch is off and the clutch engaged; the compile has the millstone in the motor's domain with
     * its milling entry and a CONTROL edge from the switch. Filling the chest turns the switch on and the
     * clutch detaches the millstone (it stops); the fingerprint does not change, and a compile asked for
     * then is the same graph: the millstone still in the domain at the same ratio, the same entry, the
     * same gate (choice GB51).
     */
    @GameTest(template = YARD, batch = "glass_extract_02", timeoutTicks = 900)
    public static void aSwitchDrivenClutchCompilesTheSameOnAndOff(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        int bx = borderX(helper), y = floorY(helper), z = midZ(helper);
        BlockPos motor = new BlockPos(bx + 3, y, z), clutch = motor.above(), mill = clutch.above(), chute = mill.above(), chest = chute.above();
        BlockPos sw = clutch.north(), observed = sw.north();
        set(helper, motor, AllBlocks.CREATIVE_MOTOR.getDefaultState().setValue(BlockStateProperties.FACING, Direction.UP));
        if (helper.getLevel().getBlockEntity(motor) instanceof CreativeMotorBlockEntity m)
            m.generatedSpeed.setValue(64);
        set(helper, clutch, AllBlocks.CLUTCH.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.Y));
        set(helper, mill, AllBlocks.MILLSTONE.getDefaultState());
        set(helper, chute, AllBlocks.CHUTE.getDefaultState());
        set(helper, chest, Blocks.CHEST.defaultBlockState());
        if (helper.getLevel().getBlockEntity(chest) instanceof ChestBlockEntity c)
            c.setItem(0, new ItemStack(Items.WHEAT, 64));
        set(helper, observed, Blocks.CHEST.defaultBlockState());
        set(helper, sw, AllBlocks.THRESHOLD_SWITCH.getDefaultState()
                .setValue(DirectedDirectionalBlock.TARGET, AttachFace.WALL).setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.NORTH));
        if (helper.getLevel().getBlockEntity(sw) instanceof ThresholdSwitchBlockEntity t) {
            t.onWhenAbove = 10;
            t.offWhenBelow = 5;
        }
        playerPlaced(helper, chest);
        long[] id = {0};
        long[] print = {0};
        int[] compile = {0};
        String[] before = {null};
        String t = "aSwitchDrivenClutchCompilesTheSameOnAndOff";
        helper.startSequence()
                .thenWaitUntil(() -> {
                    District d = districtAt(helper, chest);
                    check("the chest's chunk is a district", d != null);
                    id[0] = d.id;
                    check("the millstone turns", helper.getLevel().getBlockEntity(mill) instanceof KineticBlockEntity k && k.getSpeed() != 0);
                })
                .thenWaitUntil(() -> {
                    GlassKit.Graph g = GlassKit.latest(id[0]);
                    check("a complete compile", g != null && g.complete());
                    before[0] = millstoneView(g);
                    check("the millstone in a domain with an entry, gated by the switch: " + before[0], before[0].contains("gated"));
                    compile[0] = g.end().compile();
                    print[0] = ind().glass().fingerprint(id[0]);
                    record(t, "engaged", before[0]);
                })
                .thenExecute(() -> {
                    if (helper.getLevel().getBlockEntity(observed) instanceof ChestBlockEntity c)
                        c.setItem(0, new ItemStack(Items.COBBLESTONE, 64));
                })
                .thenWaitUntil(() -> check("the switch turned on and the clutch stopped the millstone",
                        helper.getLevel().getBlockEntity(mill) instanceof KineticBlockEntity k && k.getSpeed() == 0))
                .thenIdle(40)
                .thenExecute(() -> {
                    check("no recompile from the switch: compile " + GlassKit.latest(id[0]).end().compile(), GlassKit.latest(id[0]).end().compile() == compile[0]);
                    check("the fingerprint did not move", ind().glass().fingerprint(id[0]) == print[0]);
                    ind().glass().requestCompile(ind().district(id[0]));
                })
                .thenWaitUntil(() -> {
                    GlassKit.Graph g = GlassKit.latest(id[0]);
                    check("a new compile with the clutch off", g != null && g.complete() && g.end().compile() > compile[0]);
                    String after = millstoneView(g);
                    record(t, "disengaged", after);
                    check("the same millstone, domain, entry and gate: " + after + " vs " + before[0], after.equals(before[0]));
                })
                .thenSucceed();
    }

    /** The millstone's place in a graph: its domain's level and its ratio there, its entries, the switch's gate. */
    private static String millstoneView(GlassKit.Graph g) {
        var mills = g.nodes(GlassRecords.Node.MACHINE, GlassRecords.Model.MILLSTONE);
        if (mills.size() != 1)
            return "millstones " + mills.size();
        GlassKit.Node m = mills.get(0);
        if (m.domain() == GlassRecords.NONE)
            return "no domain";
        GlassKit.Domain d = g.domains().get(m.domain());
        StringBuilder b = new StringBuilder("levels " + d.levels() + " ratio");
        for (GlassKit.Member x : d.members())
            if (x.node() == m.index())
                b.append(' ').append(x.ratioNum()).append('/').append(x.ratioDen());
        b.append(" rates ").append(m.rates());
        if (g.edgesTo(m.index()).stream().anyMatch(e -> e.kind() == GlassRecords.Edge.CONTROL))
            b.append(" gated");
        b.append(", ").append(g.nodes().size()).append(" nodes, ").append(g.edges().size()).append(" edges");
        return b.toString();
    }
}
