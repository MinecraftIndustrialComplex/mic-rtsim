package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.structure.BoundingBox;

import org.junit.jupiter.api.Test;

/**
 * L's placed-lair record (RP-243, RP-244, the slice U audit's D2): each placed lair keeps the chunk its structure start is
 * stored in, so a session can look for the start the first time that chunk loads, and place the lair again at the same
 * chunk when it is missing. The record survives the saved state's save and load.
 */
class LairPlacedRecordTest {
    private static LairPlacer.Placed placed(long field, ChunkPos start) {
        return new LairPlacer.Placed(field, 0x7000_0000_0000_0042L, "minecraft:overworld", "iceandfire:fire_dragon_roost",
                new BoundingBox(-100, 60, 3200, -60, 90, 3240), new BlockPos(-80, 70, 3220), 123_456L, 3, 9_000L, start);
    }

    @Test
    void theStartChunkSurvivesASaveAndLoad() {
        LairPlacer.Data d = new LairPlacer.Data();
        d.fields.add(77L);
        d.placed.add(placed(77L, new ChunkPos(-5, 201)));
        LairPlacer.Data back = LairPlacer.Data.load(d.save(new CompoundTag(), null), null);
        assertEquals(1, back.placed.size());
        LairPlacer.Placed p = back.placed.get(0);
        assertEquals(new ChunkPos(-5, 201), p.start());
        assertEquals(77L, p.field());
        assertEquals(0x7000_0000_0000_0042L, p.lair());
        assertEquals(new BlockPos(-80, 70, 3220), p.pivot());
        assertEquals(new BoundingBox(-100, 60, 3200, -60, 90, 3240), p.box());
        assertEquals(d.fields, back.fields);
    }

    @Test
    void aFileFromBeforeTheStartLoadsNone() {
        LairPlacer.Data d = new LairPlacer.Data();
        d.placed.add(placed(77L, new ChunkPos(-5, 201)));
        CompoundTag tag = d.save(new CompoundTag(), null);
        tag.getList("placed", 10).getCompound(0).remove("start");
        assertNull(LairPlacer.Data.load(tag, null).placed.get(0).start(), "no start recorded: never checked");
    }

    @Test
    void aStartAtTheOriginIsNotTakenForNone() {
        LairPlacer.Data d = new LairPlacer.Data();
        d.placed.add(placed(77L, new ChunkPos(0, 0)));
        assertEquals(new ChunkPos(0, 0), LairPlacer.Data.load(d.save(new CompoundTag(), null), null).placed.get(0).start());
    }
}
