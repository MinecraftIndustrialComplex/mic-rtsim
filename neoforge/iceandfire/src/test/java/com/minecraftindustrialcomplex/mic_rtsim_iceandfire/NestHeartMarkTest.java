package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import net.minecraft.nbt.CompoundTag;

import org.junit.jupiter.api.Test;

/**
 * The nest id a heart carries (RP-240, the slice U audit's D4): a PLACE_NEST again for a nest the Nests file lost finds the
 * chamber it already carved by the spawner marked with the nest's id, instead of carving a second chamber.
 * {@code BlockEntity.saveCustomOnly} writes the NeoForge persistent data under "NeoForgeData" (BlockEntity.java:95 of
 * NeoForge 21.1.249), which is the compound these tests build by hand.
 */
class NestHeartMarkTest {
    private static final long NEST = 0x0001_0000_0000_0007L;
    /** A nest id that fits an int, so a mark of the wrong (int) type reads back as the same number through getLong. */
    private static final long SMALL_NEST = 7L;

    /** What a spawner block entity's {@code saveCustomOnly} holds once its persistent data carries {@code persistent}. */
    private static CompoundTag spawnerSave(CompoundTag persistent) {
        CompoundTag save = new CompoundTag();
        save.putShort("RequiredPlayerRange", (short) 0);
        if (persistent != null)
            save.put("NeoForgeData", persistent);
        return save;
    }

    @Test
    void aMarkedHeartIsTheNestsAndNoOtherNests() {
        CompoundTag persistent = new CompoundTag();
        Nests.markHeart(persistent, NEST);
        assertTrue(Nests.marksNest(spawnerSave(persistent), NEST));
        assertFalse(Nests.marksNest(spawnerSave(persistent), NEST + 1), "another nest's chamber in the same chunk is not this one's");
    }

    @Test
    void aSpawnerOfTheWorldsOwnIsNoNestsHeart() {
        assertFalse(Nests.marksNest(spawnerSave(null), NEST), "a dungeon's spawner: no persistent data at all");
        CompoundTag other = new CompoundTag();
        other.putString("some_other_mod", "x");
        assertFalse(Nests.marksNest(spawnerSave(other), NEST), "another mod's persistent data");
    }

    @Test
    void aMarkOfTheWrongTypeIsNotOne() {
        // A nest id that fits an int: without the type check, getLong on an int tag would read it back as the very id.
        CompoundTag persistent = new CompoundTag();
        persistent.putInt(Nests.HEART_TAG, (int) SMALL_NEST);
        assertFalse(Nests.marksNest(spawnerSave(persistent), SMALL_NEST));
        persistent = new CompoundTag();
        persistent.putString(Nests.HEART_TAG, Long.toString(SMALL_NEST));
        assertFalse(Nests.marksNest(spawnerSave(persistent), SMALL_NEST));
        // And the right type for the same id is the nest's.
        persistent = new CompoundTag();
        Nests.markHeart(persistent, SMALL_NEST);
        assertTrue(Nests.marksNest(spawnerSave(persistent), SMALL_NEST));
    }

    @Test
    void theMarkIsNamespacedAndBesideTheWormsTag() {
        // Keys of the module: an unknown one is harmless with the module off (UR-1): NeoForge keeps it and writes it back.
        assertTrue(Nests.HEART_TAG.startsWith("mic_rtsim_"));
        assertTrue(Nests.ORDINAL_TAG.startsWith(Nests.TAG));
    }
}
