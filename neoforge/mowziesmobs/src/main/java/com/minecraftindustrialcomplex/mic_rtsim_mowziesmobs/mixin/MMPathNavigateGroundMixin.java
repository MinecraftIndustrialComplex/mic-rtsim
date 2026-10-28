package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.bobmowzie.mowziesmobs.server.ai.MMPathNavigateGround;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.PathColumns;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.PathSweep;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.navigation.GroundPathNavigation;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The path shortcut of Mowzie's Mobs 1.8.2 does not run away far from the origin (MZB-951, MZB-950). The jar's
 * {@code MMPathNavigateGround.sweep(Vec3 vec, Vec3 base, Vec3 max)} (private; the Frostmaw, the Wroughtnaut and the Grottol walk
 * it every tick they follow a path, once per node that may be skipped) walks a grid in {@code float} on world coordinates; beyond
 * 2^22 blocks (it began at about 500,000) its trailing edge overtakes the leading one, its loops count through the int range, and
 * every step reads a block, which loads a chunk: one sweep of 40 blocks held the server thread for minutes and loaded tens
 * of thousands of chunks. The call is cancelled at the head of the method and answered by {@link PathSweep}, the same walk in
 * {@code double} measured from the box's own cell, with loops that cannot count past their end; {@link PathColumns} reads the
 * world as the jar's loop body does. Where the jar's float walk is exact (any box within some 100,000 blocks of the origin) the
 * answer is the same. The jar's own body of the method stays in the class and is never reached.
 *
 * <p>Holds no state and writes nothing: a world saved with the mixin loads without it.
 */
@Mixin(MMPathNavigateGround.class)
public abstract class MMPathNavigateGroundMixin extends GroundPathNavigation {

    private MMPathNavigateGroundMixin(Mob mob, Level level) {
        super(mob, level);
    }

    @Inject(method = "sweep(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/phys/Vec3;)Z",
            at = @At("HEAD"), cancellable = true, require = 1, allow = 1)
    private void mic_rtsim_mowziesmobs$sweep(Vec3 vec, Vec3 base, Vec3 max, CallbackInfoReturnable<Boolean> cir) {
        cir.setReturnValue(PathSweep.sweep(new double[] {vec.x, vec.y, vec.z}, new double[] {base.x, base.y, base.z},
                new double[] {max.x, max.y, max.z}, new PathColumns(this.level, this.nodeEvaluator, this.mob)));
    }
}
