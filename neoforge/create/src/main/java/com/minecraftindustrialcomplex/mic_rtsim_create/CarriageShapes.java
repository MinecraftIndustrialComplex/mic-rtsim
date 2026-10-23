package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.far.CarriageShape;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarShape;
import com.simibubi.create.content.contraptions.Contraption;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageBogey;
import com.simibubi.create.content.trains.entity.CarriageContraption;
import com.simibubi.create.content.trains.entity.CarriageContraptionEntity;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.WeakHashMap;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.HolderGetter;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.Tag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;

/**
 * What each carriage looks like, for the far feed ({@link CarriageShape}, docs/choices-dh-feed.md DHT61): its
 * blocks from where Create 6 keeps them on the server, read only when they change.
 *
 * <ul>
 *   <li><b>A carriage with an entity</b>: its live contraption's blocks.</li>
 *   <li><b>A carriage with none</b> (the far case: past every player's entity range): the entity's saved form
 *       Create holds in {@code Carriage.serialisedEntity} (package-private, read by reflection), whose
 *       {@code Contraption.Blocks} compound is the palette and block list.</li>
 * </ul>
 * A carriage's shape is kept while its source (the contraption, or the saved tag) is the same object, so each
 * is encoded once per change, not once a second. Server thread.
 */
final class CarriageShapes {
    private static final Field SERIALISED = serialisedField();

    private record Kept(Object source, FarShape shape) {
    }

    private static final Map<Carriage, Kept> KEPT = new WeakHashMap<>();
    private static int failures;
    static long encodes, encodeNanos;

    private CarriageShapes() {}

    private static Field serialisedField() {
        try {
            Field f = Carriage.class.getDeclaredField("serialisedEntity");
            f.setAccessible(true);
            return f;
        } catch (ReflectiveOperationException | RuntimeException e) {
            MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] Carriage.serialisedEntity not found; far carriages without an "
                    + "entity are drawn as boxes", e);
            return null;
        }
    }

    /** Carriage {@code c}'s far shape, or null when its blocks cannot be read (it is then a box). */
    static FarShape of(Carriage c, ServerLevel level) {
        CarriageContraptionEntity e = c.anyAvailableEntity();
        Object source = e != null && e.getContraption() != null ? e.getContraption() : serialised(c);
        if (source == null)
            return null;
        Kept had = KEPT.get(c);
        if (had != null && had.source == source)
            return had.shape;
        FarShape shape = null;
        long t0 = System.nanoTime();
        try {
            CarriageShape cs = source instanceof CompoundTag tag ? fromTag(c, tag, level.holderLookup(Registries.BLOCK))
                    : fromContraption(c, (Contraption) source);
            shape = cs == null ? null : cs.shape();
        } catch (RuntimeException ex) {
            if (failures++ < 3)
                MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] a carriage's far shape failed; it is drawn as a box", ex);
        }
        encodes++;
        encodeNanos += System.nanoTime() - t0;
        KEPT.put(c, new Kept(source, shape));
        return shape;
    }

    private static CompoundTag serialised(Carriage c) {
        if (SERIALISED == null)
            return null;
        try {
            return SERIALISED.get(c) instanceof CompoundTag t ? t : null;
        } catch (IllegalAccessException e) {
            return null;
        }
    }

    private static CarriageShape fromTag(Carriage c, CompoundTag tag, HolderGetter<Block> blocks) {
        CompoundTag con = tag.getCompound("Contraption");
        if (!con.contains("Blocks", Tag.TAG_COMPOUND))
            return null;
        Direction assembly = direction(con.getString("AssemblyDirection"));
        CompoundTag b = con.getCompound("Blocks");
        ListTag pal = b.getList("Palette", Tag.TAG_COMPOUND);
        int[] ids = new int[pal.size()];
        for (int i = 0; i < ids.length; i++)
            ids[i] = Block.getId(NbtUtils.readBlockState(blocks, pal.getCompound(i)));
        List<CarriageShape.Block> list = new ArrayList<>();
        for (Tag t : b.getList("BlockList", Tag.TAG_COMPOUND)) {
            CompoundTag k = (CompoundTag) t;
            int s = k.getInt("State");
            if (s < 0 || s >= ids.length)
                continue;
            BlockPos p = BlockPos.of(k.getLong("Pos"));
            list.add(new CarriageShape.Block(p.getX(), p.getY(), p.getZ(), ids[s]));
        }
        return new CarriageShape(assembly.get3DDataValue(), list, bogeys(c, assembly));
    }

    private static CarriageShape fromContraption(Carriage c, Contraption contraption) {
        Direction assembly = contraption instanceof CarriageContraption cc ? cc.getAssemblyDirection() : Direction.WEST;
        List<CarriageShape.Block> list = new ArrayList<>(contraption.getBlocks().size());
        for (StructureTemplate.StructureBlockInfo info : contraption.getBlocks().values())
            list.add(new CarriageShape.Block(info.pos().getX(), info.pos().getY(), info.pos().getZ(), Block.getId(info.state())));
        return new CarriageShape(assembly.get3DDataValue(), list, bogeys(c, assembly));
    }

    /** The leading bogey at the contraption's anchor, the trailing one the bogey spacing along the assembly. */
    private static List<CarriageShape.Bogey> bogeys(Carriage c, Direction assembly) {
        List<CarriageShape.Bogey> out = new ArrayList<>(2);
        CarriageBogey lead = c.bogeys.getFirst(), trail = c.bogeys.getSecond();
        if (lead != null)
            out.add(bogey(lead, BlockPos.ZERO));
        if (trail != null)
            out.add(bogey(trail, BlockPos.ZERO.relative(assembly, c.bogeySpacing)));
        return out;
    }

    private static CarriageShape.Bogey bogey(CarriageBogey b, BlockPos at) {
        return new CarriageShape.Bogey(at.getX(), at.getY(), at.getZ(), b.getStyle().id.toString(), b.getSize().id().toString(),
                b.isUpsideDown());
    }

    private static Direction direction(String s) {
        try {
            return Direction.valueOf(s.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException e) {
            return Direction.WEST;
        }
    }
}
