package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaFollowerToRaptor;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaRaptor;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBodies;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.Mth;
import net.minecraft.world.Difficulty;
import net.minecraft.world.entity.Mob;

import java.util.List;

/**
 * The Umvuthana's raptor as a conflict body (MZB-428, MZB-429): the jar's raptor and its pack of four
 * followers, which the bridge adds, tags and removes with the body's slot.
 *
 * <p>Not {@code ConflictBodies.create}: the raptor's {@code finalizeSpawn} adds 4 or 5 followers through
 * {@code addFreshEntity}, untagged, and every raptor made that way would leak them. This makes exactly 4,
 * on the jar's 0.1-block circle with its weapon roll, and does not call {@code finalizeSpawn} at all (the
 * raptor's own is the one that makes the pack; what it would also do is {@code Mob}'s follow-range
 * bonus and left-handedness, which a stand-in has no need of). The followers find their leader in their
 * first tick, as the jar's do, and join its pack there; nothing here calls {@code addPackMember}, or
 * each would be in the pack twice.
 *
 * <p>The raptor and its followers discard themselves on Peaceful in their own {@code tick}, the jar's
 * mobs being no exception to the difficulty, so there is no body to make then: the maker returns null and
 * the stack stays pending. (The bridge already keeps a hostile stand-in out on Peaceful; this is the
 * second guard, for a difficulty that changes between that test and the spawn.)
 */
final class RaptorMaker implements ConflictBodies.Maker {
    /** The followers a raptor brings: the jar makes 4 or 5, and §4.13's 2.6 for a pack fits 4. */
    static final int PACK = 4;

    /** A raptor travels as a pack (MZB-469): the column gives each its own stack, a pair abreast and well apart. */
    @Override
    public boolean pack() {
        return true;
    }

    @Override
    public Mob make(ServerLevel level, double x, double y, double z, float yaw, List<Mob> extras) {
        if (level.getDifficulty() == Difficulty.PEACEFUL)
            return null;
        EntityUmvuthanaRaptor raptor = EntityHandler.UMVUTHANA_RAPTOR.get().create(level);
        if (raptor == null)
            return null;
        raptor.moveTo(x, y, z, yaw, 0f);
        raptor.setYHeadRot(yaw);
        raptor.setYBodyRot(yaw);
        float theta = (float) (Math.PI * 2) / PACK;
        for (int i = 0; i < PACK; i++) {
            EntityUmvuthanaFollowerToRaptor follower =
                    new EntityUmvuthanaFollowerToRaptor(EntityHandler.UMVUTHANA_FOLLOWER_TO_RAPTOR.get(), level, raptor);
            follower.setPos(x + 0.1 * Mth.cos(theta * i), y, z + 0.1 * Mth.sin(theta * i));
            follower.setWeapon(raptor.getRandom().nextInt(3) == 0 ? 1 : 0);
            extras.add(follower);
        }
        return raptor;
    }
}
