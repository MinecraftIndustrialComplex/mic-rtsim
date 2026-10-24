package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.ModifyReturnValue;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.RotationPropagator;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

import java.util.ArrayList;
import java.util.List;

/**
 * Rotation never propagates into a chunk that is not loaded (SECTIONS.md §2.8, slice S1; PATCHES.md row 55).
 * Create 6.0.10's {@code getPotentialNeighbourLocations} keeps the six neighbours whose chunk
 * {@code Level.isLoaded} passes, then appends the block entity's own {@code addPropagationLocations} with no
 * test: a chain conveyor's connections (up to 32 blocks away) and a large cog's diagonals.
 * {@code findConnectedNeighbour} and {@code handleRemoved} then read each position through
 * {@code getBlockState} and {@code getBlockEntity}, which load: a speed change, an added source or a
 * removed one on a chain into an away district loaded the far conveyor's chunk.
 *
 * <p>The list keeps only positions whose chunk is loaded ({@code getChunkNow}). A far conveyor then keeps
 * its last speed until its chunk loads, as Create already does for a shaft into a chunk that is not
 * loaded. Server levels only; where every position is loaded the list is Create's own.
 */
@Mixin(value = RotationPropagator.class, remap = false)
public abstract class RotationPropagatorMixin {
    @ModifyReturnValue(method = "getPotentialNeighbourLocations", at = @At("RETURN"))
    private static List<BlockPos> mic_rtsim$loadedOnly(List<BlockPos> list, @Local(argsOnly = true) KineticBlockEntity be) {
        if (!(be.getLevel() instanceof ServerLevel level))
            return list;
        // Create's list is a LinkedList: walk it, never index it
        List<BlockPos> out = null;
        int i = 0;
        for (BlockPos p : list) {
            if (Loaded.at(level, p)) {
                if (out != null)
                    out.add(p);
            } else if (out == null) {
                out = new ArrayList<>(list.subList(0, i));
            }
            i++;
        }
        return out == null ? list : out;
    }
}
