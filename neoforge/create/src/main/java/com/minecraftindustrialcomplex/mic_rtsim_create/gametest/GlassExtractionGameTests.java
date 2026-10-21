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
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
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
     * A chest of wheat over a chute over a millstone on a creative motor at 64 RPM. The compile reaches
     * the engine complete; its graph has the chest's STORE, the chute's MOVER (store to chute to
     * millstone), the millstone MACHINE in a one-level domain at 64 RPM with the milling recipe for wheat
     * and a rate entry of one item per {@code Cycles.millstone(150, 64)} ticks, and the domain's member for
     * the millstone. The Create accessor resolved every member (a Create update fails here first).
     */
    @GameTest(template = YARD, batch = "glass_extract_01", timeoutTicks = 600)
    public static void aCompileReachesTheEngineComplete(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        check("the Create accessor resolved TorquePropagator.networks, the deployer's mode and the arm's points", CreateGlassAccess.allResolved());
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
}
