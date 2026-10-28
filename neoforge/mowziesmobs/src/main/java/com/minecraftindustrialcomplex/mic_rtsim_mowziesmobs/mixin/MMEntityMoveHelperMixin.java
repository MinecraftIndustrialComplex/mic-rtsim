package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.bobmowzie.mowziesmobs.server.ai.MMEntityMoveHelper;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.FrostmawLurker;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.control.MoveControl;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyArg;

/**
 * A held Frostmaw turns as vanilla turns any mob when the hold walks it back (MZB-770). The jar's move control
 * ({@code MMEntityMoveHelper}, made with {@code maxRotate} 7 in {@code EntityFrostmaw}'s constructor) turns a
 * Frostmaw at most 7 degrees a tick in {@code MOVE_TO} and walks it on at full speed meanwhile: its chase is a
 * lumbering charge. Under the hold that makes a walk back to a spot behind the body a loop of about 0.9 blocks'
 * radius, which carries it 2 blocks or more to one side, toward a player or the rail. For a body the hold has in
 * hand the turn is vanilla {@code MoveControl}'s 90 degrees ({@link FrostmawLurker#TURN}): it faces its way within
 * two ticks and walks the path's line. The one {@code rotlerp(FFF)F} call in {@code tick()} is the
 * 1.8.2 jar's (bytecode offset 388, its third argument the {@code maxRotate} read at 385); a jar with another
 * count fails to load. Any other mob that uses the helper, and a Frostmaw not held, keep the jar's turn.
 */
@Mixin(MMEntityMoveHelper.class)
public abstract class MMEntityMoveHelperMixin extends MoveControl {

    private MMEntityMoveHelperMixin(Mob mob) {
        super(mob);
    }

    @ModifyArg(method = "tick()V", at = @At(value = "INVOKE",
            target = "Lcom/bobmowzie/mowziesmobs/server/ai/MMEntityMoveHelper;rotlerp(FFF)F"),
            index = 2, require = 1, allow = 1)
    private float mic_rtsim_mowziesmobs$heldTurn(float maxRotate) {
        return this.mob instanceof EntityFrostmaw f && !f.level().isClientSide() && FrostmawLurker.holds(f)
                ? Math.max(maxRotate, FrostmawLurker.TURN) : maxRotate;
    }
}
