package com.minecraftindustrialcomplex.mic_rtsim_ribbits;

import com.yungnickyoung.minecraft.ribbits.data.RibbitData;
import com.yungnickyoung.minecraft.ribbits.data.RibbitProfession;
import com.yungnickyoung.minecraft.ribbits.data.RibbitUmbrellaType;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.module.RibbitInstrumentModule;
import com.yungnickyoung.minecraft.ribbits.module.RibbitUmbrellaTypeModule;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.phys.Vec3;

/**
 * Making ribbits, the way Ribbits itself makes one (verified in the Ribbits 4.1.6 bytecode):
 * {@code EntityType.create} runs {@code finalizeSpawn}, which sets the ribbit's home to where it stands
 * and reassesses its goals; then its {@code RibbitData(profession, umbrella, instrument)} is set and the
 * goals reassessed again, because the profession decides them (merchant, sorcerer, nitwit musician).
 * There is no home setter; a ribbit's home is where it was spawned (or where a player re-homed it with
 * an amethyst shard), so a resident spawned in its new house is homed there.
 */
public final class Ribbits {
    private Ribbits() {}

    public static final ResourceLocation RIBBIT = ResourceLocation.parse("ribbits:ribbit");

    public static EntityType<?> type() {
        return BuiltInRegistries.ENTITY_TYPE.get(RIBBIT);
    }

    /** The umbrella of a body bit (0 to 2), or a random one for -1 (as Ribbits rolls it). */
    public static RibbitUmbrellaType umbrella(int index) {
        return switch (index) {
            case 0 -> RibbitUmbrellaTypeModule.UMBRELLA_1;
            case 1 -> RibbitUmbrellaTypeModule.UMBRELLA_2;
            case 2 -> RibbitUmbrellaTypeModule.UMBRELLA_3;
            default -> RibbitUmbrellaTypeModule.getRandomUmbrellaType();
        };
    }

    public static int umbrellaIndex(RibbitUmbrellaType t) {
        if (t == null)
            return -1;
        if (t.equals(RibbitUmbrellaTypeModule.UMBRELLA_1))
            return 0;
        if (t.equals(RibbitUmbrellaTypeModule.UMBRELLA_2))
            return 1;
        if (t.equals(RibbitUmbrellaTypeModule.UMBRELLA_3))
            return 2;
        return -1;
    }

    /** A new ribbit of a profession at {@code pos}, not yet added to the level; null when it cannot be made. */
    public static RibbitEntity create(ServerLevel level, Vec3 pos, RibbitProfession profession, RibbitUmbrellaType umbrella) {
        EntityType<?> type = type();
        if (type == null || profession == null)
            return null;
        Entity e = type.create(level, null, BlockPos.containing(pos), MobSpawnType.EVENT, false, false);
        if (!(e instanceof RibbitEntity ribbit))
            return null;
        ribbit.moveTo(pos.x, pos.y, pos.z, ribbit.getYRot(), 0f);
        ribbit.setRibbitData(new RibbitData(profession, umbrella, RibbitInstrumentModule.NONE));
        ribbit.reassessGoals();
        return ribbit;
    }

    /** The profession's dictionary name, e.g. {@code ribbits:merchant}. */
    public static String professionName(RibbitEntity r) {
        RibbitData d = r.getRibbitData();
        return d == null || d.getProfession() == null ? null : d.getProfession().getId().toString();
    }
}
