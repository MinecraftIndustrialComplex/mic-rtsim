package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlock;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.item.Items;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 2: brass mixing on a blaze burner. Copper and zinc ingots each drop from a chest through a
 * chute onto their own two-block belt, which ends in the basin (copper from the west, zinc from the
 * north: one belt for both would deadlock once the basin's one copper slot is full and a copper stack
 * blocks the zinc behind it, {@code BasinInventory.insertItem}). A mechanical mixer two above the basin
 * runs at 128 RPM off a cogwheel (the mixer is a cog and has no shaft, {@code MechanicalMixerBlock}).
 * The basin's spout faces east into a chute over the output chest. The burner under the basin burns
 * regular fuel that a player topped up to the 10,000-tick maximum ({@code BlazeBurnerBlock.tryInsert}
 * overflows only for a non-FakePlayer): it stays KINDLED through warm-up and window (it would fade
 * below 125 ticks), so heat is constant. Fed burners are corpus 8's subject.
 *
 * <pre>
 *   y5   chest Cu           mixer      cogwheel (Y)
 *   y4   chute              .          motor (up)
 *   y3   belt Cu  belt Cu   basin      (air)
 *   y2                      burner     chute
 *   y1                                 chest (brass)
 *        x6       x7        x8         x9          (z 8; zinc: chest y5, chute y4, belt y3 at x8, z6 to z7)
 * </pre>
 *
 * <p><b>Rates</b> ({@code MechanicalMixerBlockEntity.tick}, {@code continueWithPreviousRecipe};
 * {@code BasinOperatingBlockEntity.applyBasinRecipe}):
 * <ul>
 *   <li>The first craft winds up 20 ticks. At runningTicks 20 the mixer sets processingTicks =
 *       max(log2((int)(512 / 128)) x ceil(1 x 15) + 1, 1) = 2 x 15 + 1 = 31 (brass has no
 *       processing_time, so the recipe factor is 1), counts it down one per tick and applies the recipe
 *       when it reaches 0. If the basin's spout buffer was empty and the recipe still matches,
 *       {@code continueWithPreviousRecipe} puts runningTicks back to 20, and the next tick sets 31
 *       again: one craft per 31 + 1 = 32 ticks. Each takes one copper and one zinc ingot and makes 2
 *       brass ({@code mixing/brass_ingot.json}, heated): 1200 / 32 = 37.5 crafts per minute, exact.</li>
 *   <li>The chutes and belts deliver 16-stacks far faster; the spout hands each craft's 2 brass to the
 *       chute, which drops them in about 6 ticks, so the buffer is empty at every apply.</li>
 * </ul>
 */
public final class BrassMixingFactory implements CorpusFactory {
    static final int RPM = 128;
    static final int CYCLE = 32;
    static final int BELT_RPM = 64;

    @Override
    public String name() {
        return "brass_mixing";
    }

    @Override
    public int warmupTicks() {
        return 300;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockPos basin = at(h, 8, 3, 8), mixer = at(h, 8, 5, 8), burner = at(h, 8, 2, 8);
        BlockPos copper = at(h, 6, 5, 8), zinc = at(h, 8, 5, 6), out = at(h, 9, 1, 8);
        // The default state is the empty burner (HEAT_LEVEL NONE), which has no block entity
        // (BlazeBurnerBlock.newBlockEntity); a burner with a blaze in it starts SMOULDERING.
        set(h, burner, AllBlocks.BLAZE_BURNER.getDefaultState().setValue(BlazeBurnerBlock.HEAT_LEVEL, BlazeBurnerBlock.HeatLevel.SMOULDERING));
        topUpBurner(h, burner);
        set(h, basin, AllBlocks.BASIN.getDefaultState());
        set(h, mixer, AllBlocks.MECHANICAL_MIXER.getDefaultState());
        cog(h, at(h, 9, 5, 8), Direction.Axis.Y);
        motor(h, at(h, 9, 4, 8), Direction.UP, RPM);
        // Copper: belt along +X ending in the basin's west side. An X belt moves +X at negative speed; a
        // motor facing north generates minus its value (KineticBlockEntity.convertToDirection).
        belt(h, at(h, 6, 3, 8), at(h, 7, 3, 8));
        motor(h, at(h, 6, 3, 9), Direction.NORTH, BELT_RPM);
        set(h, at(h, 6, 4, 8), AllBlocks.CHUTE.getDefaultState());
        chest(h, copper);
        fill(h, copper, Items.COPPER_INGOT, 768);
        // Zinc: belt along +Z ending in the basin's north side; a Z belt moves +Z at positive speed.
        belt(h, at(h, 8, 3, 6), at(h, 8, 3, 7));
        motor(h, at(h, 7, 3, 6), Direction.EAST, BELT_RPM);
        set(h, at(h, 8, 4, 6), AllBlocks.CHUTE.getDefaultState());
        chest(h, zinc);
        fill(h, zinc, AllItems.ZINC_INGOT.get(), 768);
        // Output: the spout pours into the chute diagonally below on the east side.
        set(h, at(h, 9, 2, 8), AllBlocks.CHUTE.getDefaultState());
        chest(h, out);

        double c = TICKS_PER_MINUTE / (double) CYCLE;
        return new Built(mixer, List.of(copper, zinc), List.of(out), List.of(
                Expect.exact("minecraft:copper_ingot", -c, "MechanicalMixerBlockEntity: log2(512/128) x 15 + 1 = 31, + 1 per continue: 32 ticks"),
                Expect.exact("create:zinc_ingot", -c, "mixing/brass_ingot: one zinc per craft"),
                Expect.exact("create:brass_ingot", 2 * c, "mixing/brass_ingot: two brass per craft")),
                List.of(), 7200, "mixer at 128 RPM over a basin on a player-fuelled burner (10,000 ticks), belts from two sides");
    }

    /** What a player's repeated right-clicks with coal leave: regular fuel at the 10,000-tick cap. */
    static void topUpBurner(GameTestHelper h, BlockPos pos) {
        BlazeBurnerBlockEntity b = be(h, pos, BlazeBurnerBlockEntity.class);
        CompoundTag tag = b.saveWithFullMetadata(h.getLevel().registryAccess());
        tag.putInt("fuelLevel", BlazeBurnerBlockEntity.FuelType.NORMAL.ordinal());
        tag.putInt("burnTimeRemaining", BlazeBurnerBlockEntity.MAX_HEAT_CAPACITY);
        b.loadWithComponents(tag, h.getLevel().registryAccess());
        b.updateBlockState();
    }
}
