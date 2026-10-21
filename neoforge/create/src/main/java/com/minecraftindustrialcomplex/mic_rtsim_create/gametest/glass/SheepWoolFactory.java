package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.base.DirectionalAxisKineticBlock;
import com.simibubi.create.content.kinetics.base.DirectionalKineticBlock;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.Sheep;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameRules;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.phys.Vec3;

import java.util.List;
import java.util.Locale;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 13 (C32, GLASSBOX.md §6.2): a sheep wool farm with mechanical shearing. {@value #SHEEP} adult
 * white sheep stand in one 1 x 1 pen on a grass block; a deployer holding shears (USE mode, the default)
 * targets the pen, 2 blocks in front of it; a hopper under the deployer pulls its overflow into a chest.
 *
 * <pre>
 *   y2   motor(S)                                   (z 11)
 *   y2   deployer(E) . glass . pen(16 sheep) . glass (z 12)
 *   y1   hopper(S)          grass                    (z 12)
 *   y1   chest (out)                                 (z 13)
 *        x 10        11      12               13
 * </pre>
 * The pen is walled 2 high (glass) on all four sides; the deployer reaches through the glass, because
 * {@code DeployerHandler.activateInner} takes the entities in {@code new AABB(clickedPos)} with no line of
 * sight test.
 *
 * <p><b>Game rule.</b> {@code mobGriefing} is set false in {@link #build} and restored when the test
 * passes or fails (a {@link GameTestListener} on the test): with it true, the first sheep to eat turns
 * the one grass block to dirt ({@code EatBlockGoal.tick}, {@code EventHooks.canEntityGrief}) and no other
 * sheep can eat again (random ticking is 0, so the grass never comes back).
 *
 * <p><b>Regrowth</b> (vanilla 1.21.1, NeoForge 21.1):
 * <ul>
 *   <li>{@code Mob.serverAiStep} runs {@code goalSelector.tick()} only when {@code (tickCount + id)} is
 *       even (else {@code tickRunningGoals(false)}, which skips goals that do not need every tick): a
 *       goal's {@code canUse} is tried every 2 ticks.</li>
 *   <li>{@code EatBlockGoal.canUse}: {@code nextInt(isBaby ? 50 : 1000) == 0}, and short grass at the
 *       sheep's block or a grass block under it. Adults only here: p = 1/1000 per try.</li>
 *   <li>Once started it runs {@code adjustedTickDelay(40)} = ceil(40 / 2) = 20 goal ticks (one per 2
 *       game ticks; the start tick ticks it once): it eats ({@code Sheep.ate}, which regrows the wool)
 *       when the count reaches {@code adjustedTickDelay(4)} = 2, 34 game ticks after the start, and stops
 *       6 ticks later.</li>
 *   <li>From a shearing at a random time, the next eat is 1.5 (first even tick) + 2 x (1/p - 1) + 34 =
 *       T_r = 2,033.5 ticks away on average (the wait is geometric, so memoryless).</li>
 * </ul>
 *
 * <p><b>Shearing</b> ({@code DeployerBlockEntity.tick}, {@code getTimerSpeed}, {@code DeployerHandler}):
 * <ul>
 *   <li>Timer speed u = clamp(|256 x 2|, 8, 512) = 512. Expanding 1,000, retracting 1,000, waiting 500,
 *       each counted down by u and then one tick to act: a cycle that does not shear is 8 ticks, with the
 *       activation on its 4th tick.</li>
 *   <li>Each activation picks {@code entities.get(random.nextInt(size))} among the sheep in the pen. A
 *       sheared sheep: {@code ShearsItem.interactLivingEntity} returns PASS ({@code isShearable} is
 *       {@code readyForShearing}), the deployer then uses the shears on the empty block and nothing
 *       happens: the cycle is wasted, 8 ticks. A woolly one: {@code IShearable.onSheared} calls
 *       {@code Sheep.shear}, 1 + nextInt(3) wool, which the deployer captures
 *       ({@code entity.captureDrops}) into its fake player; the next wait moves it to the overflow and
 *       waits 10u (11 ticks), and again while the overflow is not empty. The hopper (one item per 8 ticks,
 *       {@code HopperBlockEntity.MOVE_ITEM_SPEED}) has 1 or 2 wool out by then, 3 only by the second
 *       check: a shearing cycle is 19 + 11/3 = 22.67 ticks on average.</li>
 *   <li>A sheep whose wool regrew waits W = (N - 1/2) x D for the deployer to pick it (each pick is 1/N,
 *       D the mean cycle). D = 8 + f x 14.67 with f = R x D the share of shearing cycles; solved with R
 *       below, D = 8.97 and W = 139 ticks for N = 16.</li>
 *   <li>Each sheep alternates sheared (T_r) and woolly (W): R = N / (T_r + W) shearings per tick =
 *       16 / 2,172.5 = 0.007365 per tick, 8.84 per minute. Far under the deployer's cap of one shearing
 *       per 22.67 ticks (52.9 per minute).</li>
 * </ul>
 * Wool: 1 + nextInt(3) per shearing, mean 2, second moment 14/3. Expected wool 2R = 17.68 per minute;
 * variance per minute R x 14/3 = 41.2 (the shearings taken as a Poisson stream; the alternating renewal
 * process is a little more regular, c^2 about 0.91, so this overstates the variance by about 8%).
 * Neglected, about 1%: a sheep sheared in the 34 ticks between the start of an eat and the eat regrows at
 * once (UNVERIFIED in game).
 *
 * <p><b>Window.</b> 7,200 ticks (the cap) sees about 53 shearings: the slope's sigma is 2.9 wool per
 * minute, so the smoke test's 3 sigma band is about +-49% of the rate. A +-20% band would need about 315
 * shearings (43,000 ticks with 16 sheep); the cap does not allow it. The check still catches a factor of 2
 * (a goal tried every tick instead of every 2, or a wrong eat chance). The sheep start sheared, so the
 * window has no burst of first shearings. Shears: 238 uses; 53 in the window, so one pair is enough.
 */
public final class SheepWoolFactory implements CorpusFactory {
    static final int SHEEP = 16;
    static final int RPM = 256;
    /** Ticks per goal try, eat chance per try, ticks from a start to the eat. */
    static final double TRY_TICKS = 2, EAT_P = 1.0 / 1000, START_TO_EAT = 34;
    /** Mean ticks from a shearing to the next eat: 1.5 + 2 (1/p - 1) + 34. */
    static final double T_REGROW = 1.5 + TRY_TICKS * (1 / EAT_P - 1) + START_TO_EAT;
    /** Deployer cycles at u = 512: no shearing, and shearing (19, plus 11 when the third wool is still in the overflow). */
    static final double MISS_TICKS = 8, HIT_TICKS = 19 + 11.0 / 3;
    static final double WOOL_MEAN = 2, WOOL_SECOND_MOMENT = 14.0 / 3;

    /** Shearings per tick (the fixed point of R = N / (T_r + (N - 1/2) D), D = 8 + R D (HIT - MISS)). */
    static final double SHEARS_PER_TICK;
    static final double MEAN_CYCLE;

    static {
        double d = MISS_TICKS, r = 0;
        for (int i = 0; i < 50; i++) {
            r = SHEEP / (T_REGROW + (SHEEP - 0.5) * d);
            d = MISS_TICKS + r * d * (HIT_TICKS - MISS_TICKS);
        }
        SHEARS_PER_TICK = r;
        MEAN_CYCLE = d;
    }

    /** {@code mobGriefing} before {@link #build} changed it; null when nothing is to be restored. */
    private Boolean savedMobGriefing;

    @Override
    public String name() {
        return "sheep_wool";
    }

    @Override
    public int warmupTicks() {
        return 100;
    }

    @Override
    public Built build(GameTestHelper h) {
        keepGrass(h);

        BlockPos grass = at(h, 12, 1, 12), pen = at(h, 12, 2, 12);
        set(h, grass, Blocks.GRASS_BLOCK.defaultBlockState());
        for (int y = 2; y <= 3; y++) {
            set(h, at(h, 11, y, 12), Blocks.GLASS.defaultBlockState());
            set(h, at(h, 13, y, 12), Blocks.GLASS.defaultBlockState());
            set(h, at(h, 12, y, 11), Blocks.GLASS.defaultBlockState());
            set(h, at(h, 12, y, 13), Blocks.GLASS.defaultBlockState());
        }

        BlockPos deployer = at(h, 10, 2, 12), hopper = at(h, 10, 1, 12), out = at(h, 10, 1, 13);
        set(h, deployer, AllBlocks.DEPLOYER.getDefaultState()
                .setValue(DirectionalKineticBlock.FACING, Direction.EAST)
                .setValue(DirectionalAxisKineticBlock.AXIS_ALONG_FIRST_COORDINATE, false));
        motor(h, at(h, 10, 2, 11), Direction.SOUTH, RPM);
        chest(h, out);
        set(h, hopper, Blocks.HOPPER.defaultBlockState().setValue(HopperBlock.FACING, Direction.SOUTH));

        // Shears in the hand, and the filter set to shears so the hopper cannot pull them out
        // (DeployerItemHandler.extractItem refuses the held stack while it matches the filter).
        if (!items(h, deployer).insertItem(0, new ItemStack(Items.SHEARS), false).isEmpty())
            throw new GameTestAssertException("the deployer did not take the shears");
        FilteringBehaviour filter = BlockEntityBehaviour.get(h.getLevel(), deployer, FilteringBehaviour.TYPE);
        if (filter == null || !filter.setFilter(new ItemStack(Items.SHEARS)))
            throw new GameTestAssertException("could not set the deployer's filter");

        for (int i = 0; i < SHEEP; i++) {
            Sheep s = h.spawn(EntityType.SHEEP, new Vec3(12.5, 2, 12.5));
            s.setColor(DyeColor.WHITE);
            s.setSheared(true);
            s.setAge(0);
        }

        double r = SHEARS_PER_TICK;
        String derivation = String.format(Locale.ROOT,
                "%d sheep / (T_r %.1f + W %.1f) = %.6f shearings per tick, 1 + nextInt(3) wool each (mean 2, second moment 14/3)",
                SHEEP, T_REGROW, (SHEEP - 0.5) * MEAN_CYCLE, r);
        return new Built(deployer, List.of(), List.of(out), List.of(
                Expect.expected("minecraft:white_wool", perMinute(WOOL_MEAN * r), perMinute(WOOL_SECOND_MOMENT * r), derivation)),
                List.of(), 7200,
                String.format(Locale.ROOT,
                        "%d sheared adult sheep in one pen on grass, deployer with shears at %d RPM (cycle %.2f ticks), hopper to chest; mobGriefing false for the test",
                        SHEEP, RPM, MEAN_CYCLE));
    }

    /**
     * Sets {@code mobGriefing} false so eating keeps the grass block, and restores the old value when the
     * test passes or fails. Game rules are shared by every test on the server; this factory runs alone in
     * its batch.
     */
    private void keepGrass(GameTestHelper h) {
        MinecraftServer server = h.getLevel().getServer();
        GameRules.BooleanValue rule = h.getLevel().getGameRules().getRule(GameRules.RULE_MOBGRIEFING);
        if (savedMobGriefing == null)
            savedMobGriefing = rule.get();
        rule.set(false, server);
        h.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                restore(server);
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                restore(server);
            }

            @Override
            public void testAddedForRerun(GameTestInfo oldTest, GameTestInfo newTest, GameTestRunner runner) {}
        });
    }

    private void restore(MinecraftServer server) {
        if (savedMobGriefing == null)
            return;
        server.getGameRules().getRule(GameRules.RULE_MOBGRIEFING).set(savedMobGriefing, server);
        savedMobGriefing = null;
    }
}
