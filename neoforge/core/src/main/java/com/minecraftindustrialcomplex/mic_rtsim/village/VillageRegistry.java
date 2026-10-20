package com.minecraftindustrialcomplex.mic_rtsim.village;

import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.saveddata.SavedData;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The adopted villages of this world ({@code data/mic_rtsim_villages.dat}), with the growth work still
 * in progress (houses being built, residents waiting to spawn). It is saved in the same world save
 * as the journal, so a command the journal says was applied is always here too (both are overworld
 * SavedData written by one save).
 *
 * <p>Lookups by position go through a 64-block cell index, so finding the village a trade or a hurt
 * resident belongs to costs one map lookup and a box test.
 */
public final class VillageRegistry extends SavedData {
    public static final String FILE = "mic_rtsim_villages";
    /** How far outside its structure box a village still counts (PLACEHOLDER). */
    public static final int MARGIN = 16;
    private static final int CELL_SHIFT = 6;

    /** A PLACE_HOUSE in progress (C7, P17). */
    public static final class Build {
        public long op;
        public long cmdSeq;
        public long village;
        public ResourceKey<Level> dimension;
        public ResourceLocation template;
        public ResourceLocation processors;
        public int x, y, z, rotation, mirror, flags, radius, yOffset;
        public long seed;
        /** Null while searching for a plot; the template's origin once placing. */
        public BlockPos origin;
        /** Placed up to this layer (template-relative y, exclusive). */
        public int nextLayer;
        public long nextTry;

        CompoundTag save() {
            CompoundTag t = new CompoundTag();
            t.putLong("op", op);
            t.putLong("cmdSeq", cmdSeq);
            t.putLong("village", village);
            t.putString("dim", dimension.location().toString());
            t.putString("template", template.toString());
            if (processors != null)
                t.putString("processors", processors.toString());
            t.putIntArray("i", new int[] {x, y, z, rotation, mirror, flags, radius, yOffset, nextLayer});
            t.putLong("seed", seed);
            if (origin != null)
                t.putLong("origin", origin.asLong());
            return t;
        }

        static Build load(CompoundTag t) {
            Build b = new Build();
            b.op = t.getLong("op");
            b.cmdSeq = t.getLong("cmdSeq");
            b.village = t.getLong("village");
            b.dimension = ResourceKey.create(Registries.DIMENSION, ResourceLocation.parse(t.getString("dim")));
            b.template = ResourceLocation.parse(t.getString("template"));
            b.processors = t.contains("processors") ? ResourceLocation.parse(t.getString("processors")) : null;
            int[] i = t.getIntArray("i");
            b.x = i[0]; b.y = i[1]; b.z = i[2]; b.rotation = i[3]; b.mirror = i[4]; b.flags = i[5]; b.radius = i[6];
            b.yOffset = i[7]; b.nextLayer = i[8];
            b.seed = t.getLong("seed");
            b.origin = t.contains("origin") ? BlockPos.of(t.getLong("origin")) : null;
            return b;
        }
    }

    /** A SPAWN_RESIDENT waiting for its house or its chunk. */
    public static final class ResidentOrder {
        public long op;
        public long cmdSeq;
        public long village;
        public ResourceKey<Level> dimension;
        public String body;
        public String profession;
        public int flags;
        public double x, y, z;
        public long house;
        public long nextTry;

        CompoundTag save() {
            CompoundTag t = new CompoundTag();
            t.putLong("op", op);
            t.putLong("cmdSeq", cmdSeq);
            t.putLong("village", village);
            t.putString("dim", dimension.location().toString());
            t.putString("body", body);
            if (profession != null)
                t.putString("profession", profession);
            t.putInt("flags", flags);
            t.putDouble("x", x);
            t.putDouble("y", y);
            t.putDouble("z", z);
            t.putLong("house", house);
            return t;
        }

        static ResidentOrder load(CompoundTag t) {
            ResidentOrder o = new ResidentOrder();
            o.op = t.getLong("op");
            o.cmdSeq = t.getLong("cmdSeq");
            o.village = t.getLong("village");
            o.dimension = ResourceKey.create(Registries.DIMENSION, ResourceLocation.parse(t.getString("dim")));
            o.body = t.getString("body");
            o.profession = t.contains("profession") ? t.getString("profession") : null;
            o.flags = t.getInt("flags");
            o.x = t.getDouble("x");
            o.y = t.getDouble("y");
            o.z = t.getDouble("z");
            o.house = t.getLong("house");
            return o;
        }
    }

    private final Map<Long, Village> byId = new LinkedHashMap<>();
    private final Map<ResourceKey<Level>, Map<Long, List<Village>>> index = new HashMap<>();
    public final Map<Long, Build> builds = new LinkedHashMap<>();
    public final Map<Long, ResidentOrder> residentOrders = new LinkedHashMap<>();

    public static final Factory<VillageRegistry> FACTORY = new Factory<>(VillageRegistry::new, VillageRegistry::load);

    public static VillageRegistry get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
    }

    public Village get(long id) {
        return byId.get(id);
    }

    public Collection<Village> all() {
        return byId.values();
    }

    public int size() {
        return byId.size();
    }

    /** Adds a village. Returns false (and changes nothing) when its id is already adopted. */
    public boolean add(Village v) {
        if (byId.putIfAbsent(v.id, v) != null)
            return false;
        indexVillage(v);
        setDirty();
        return true;
    }

    /** Forgets a village (GameTests clean up after themselves; the engine is not told). */
    public void remove(long id) {
        Village v = byId.remove(id);
        if (v == null)
            return;
        Map<Long, List<Village>> cells = index.get(v.dimension);
        if (cells != null)
            for (List<Village> list : cells.values())
                list.remove(v);
        setDirty();
    }

    private static long cell(int x, int z) {
        return ((long) (x >> CELL_SHIFT) << 32) ^ ((z >> CELL_SHIFT) & 0xFFFF_FFFFL);
    }

    private void indexVillage(Village v) {
        Map<Long, List<Village>> cells = index.computeIfAbsent(v.dimension, k -> new HashMap<>());
        for (int x = (v.box.minX() - MARGIN) >> CELL_SHIFT; x <= (v.box.maxX() + MARGIN) >> CELL_SHIFT; x++)
            for (int z = (v.box.minZ() - MARGIN) >> CELL_SHIFT; z <= (v.box.maxZ() + MARGIN) >> CELL_SHIFT; z++)
                cells.computeIfAbsent(((long) x << 32) ^ (z & 0xFFFF_FFFFL), k -> new ArrayList<>(1)).add(v);
    }

    /** The village whose box (widened by {@link #MARGIN}) holds {@code pos}, or null. The smallest box wins. */
    public Village at(ResourceKey<Level> dimension, BlockPos pos) {
        Map<Long, List<Village>> cells = index.get(dimension);
        if (cells == null)
            return null;
        List<Village> list = cells.get(cell(pos.getX(), pos.getZ()));
        if (list == null)
            return null;
        Village best = null;
        long bestArea = Long.MAX_VALUE;
        for (Village v : list) {
            if (!v.contains(pos, v.margin))
                continue;
            long area = (long) v.box.getXSpan() * v.box.getZSpan();
            if (area < bestArea) {
                best = v;
                bestArea = area;
            }
        }
        return best;
    }

    /** The village that holds a house placed under {@code op}, or null. */
    public Village houseOwner(long op) {
        for (Village v : byId.values())
            if (v.house(op) != null)
                return v;
        return null;
    }

    // ---- NBT ------------------------------------------------------------------------------------

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        ListTag villages = new ListTag();
        for (Village v : byId.values())
            villages.add(v.save());
        tag.put("villages", villages);
        ListTag b = new ListTag();
        for (Build build : builds.values())
            b.add(build.save());
        tag.put("builds", b);
        ListTag r = new ListTag();
        for (ResidentOrder o : residentOrders.values())
            r.add(o.save());
        tag.put("residentOrders", r);
        return tag;
    }

    private static VillageRegistry load(CompoundTag tag, HolderLookup.Provider registries) {
        VillageRegistry reg = new VillageRegistry();
        for (Tag t : tag.getList("villages", Tag.TAG_COMPOUND))
            reg.add(Village.load((CompoundTag) t));
        for (Tag t : tag.getList("builds", Tag.TAG_COMPOUND)) {
            Build b = Build.load((CompoundTag) t);
            reg.builds.put(b.op, b);
        }
        for (Tag t : tag.getList("residentOrders", Tag.TAG_COMPOUND)) {
            ResidentOrder o = ResidentOrder.load((CompoundTag) t);
            reg.residentOrders.put(o.op, o);
        }
        reg.setDirty(false);
        return reg;
    }
}
