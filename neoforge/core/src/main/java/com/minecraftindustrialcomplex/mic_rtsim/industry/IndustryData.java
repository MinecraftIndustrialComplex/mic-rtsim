package com.minecraftindustrialcomplex.mic_rtsim.industry;

import it.unimi.dsi.fastutil.longs.Long2LongOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.saveddata.SavedData;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * The district index, {@code mic_rtsim:industry} ({@code data/mic_rtsim_industry.dat} in the overworld's
 * data storage): every district (chunks, inventories, stations, name, revision), the next district id
 * (ids are never reused, F3), the players' edits ({@code /rtsim district}: pins, exclusions, merge
 * links), and the component patches behind GOOD hashes.
 *
 * <p>Membership is the Java side's (I2); the engine takes it as IND_DISTRICT records. Seeds (the
 * player-placed block entities detection starts from) live in their chunk, not here
 * ({@link IndustryAttachments#SEEDS}).
 */
public final class IndustryData extends SavedData {
    public static final String FILE = "mic_rtsim_industry";

    /** Districts by id, in id order. */
    final TreeMap<Long, District> districts = new TreeMap<>();
    long nextId = 1;
    /** Chunks a {@code split} pinned to a district, per dimension. */
    final Map<ResourceKey<Level>, Long2LongOpenHashMap> pins = new HashMap<>();
    /** Chunks {@code exclude} took out of every district, per dimension. */
    final Map<ResourceKey<Level>, LongOpenHashSet> excluded = new HashMap<>();
    /** {@code merge}: pairs of chunks whose districts are one, per dimension. */
    final Map<ResourceKey<Level>, List<long[]>> mergeLinks = new HashMap<>();
    /** GOOD component hashes and the patches behind them ({@link Goods}). */
    final Map<String, DataComponentPatch> patches = new HashMap<>();

    public static final Factory<IndustryData> FACTORY = new Factory<>(IndustryData::new, IndustryData::load);

    public static IndustryData get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
    }

    long allocateId() {
        setDirty();
        return nextId++;
    }

    Long2LongOpenHashMap pins(ResourceKey<Level> dim) {
        return pins.computeIfAbsent(dim, d -> new Long2LongOpenHashMap());
    }

    LongOpenHashSet excluded(ResourceKey<Level> dim) {
        return excluded.computeIfAbsent(dim, d -> new LongOpenHashSet());
    }

    List<long[]> mergeLinks(ResourceKey<Level> dim) {
        return mergeLinks.computeIfAbsent(dim, d -> new ArrayList<>());
    }

    public java.util.Collection<District> districts() {
        return districts.values();
    }

    // ---- NBT ------------------------------------------------------------------------------------

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        tag.putLong("nextId", nextId);
        ListTag list = new ListTag();
        for (District d : districts.values()) {
            CompoundTag t = new CompoundTag();
            t.putLong("id", d.id);
            t.putString("dim", d.dim.location().toString());
            t.putLongArray("chunks", d.chunks.toLongArray());
            if (d.customName != null)
                t.putString("customName", d.customName);
            if (d.stationName != null)
                t.putString("stationName", d.stationName);
            t.putInt("notLearnable", d.notLearnable);
            t.putInt("revision", d.revision);
            t.putInt("declared", d.declaredRevision);
            t.putBoolean("creative", d.creative);
            ListTag invs = new ListTag();
            for (District.Inventory inv : d.inventories) {
                CompoundTag i = new CompoundTag();
                i.putLong("pos", inv.pos().asLong());
                i.putInt("kind", inv.kind());
                i.putInt("slots", inv.itemSlots());
                i.putLong("fluid", inv.fluidCapacity());
                i.putLong("energy", inv.energyCapacity());
                invs.add(i);
            }
            t.put("inventories", invs);
            ListTag sts = new ListTag();
            for (District.Station s : d.stations) {
                CompoundTag st = new CompoundTag();
                st.putLong("id", s.id());
                st.putLong("pos", s.pos().asLong());
                st.putString("name", s.name() == null ? "" : s.name());
                sts.add(st);
            }
            t.put("stations", sts);
            long[] watch = new long[d.watch.size()];
            for (int i = 0; i < watch.length; i++)
                watch[i] = d.watch.get(i).asLong();
            t.putLongArray("watch", watch);
            list.add(t);
        }
        tag.put("districts", list);

        CompoundTag pinTag = new CompoundTag();
        for (Map.Entry<ResourceKey<Level>, Long2LongOpenHashMap> e : pins.entrySet()) {
            long[] flat = new long[e.getValue().size() * 2];
            int k = 0;
            for (Long2LongOpenHashMap.Entry p : e.getValue().long2LongEntrySet()) {
                flat[k++] = p.getLongKey();
                flat[k++] = p.getLongValue();
            }
            pinTag.putLongArray(e.getKey().location().toString(), flat);
        }
        tag.put("pins", pinTag);
        CompoundTag exTag = new CompoundTag();
        for (Map.Entry<ResourceKey<Level>, LongOpenHashSet> e : excluded.entrySet())
            exTag.putLongArray(e.getKey().location().toString(), e.getValue().toLongArray());
        tag.put("excluded", exTag);
        CompoundTag linkTag = new CompoundTag();
        for (Map.Entry<ResourceKey<Level>, List<long[]>> e : mergeLinks.entrySet()) {
            long[] flat = new long[e.getValue().size() * 2];
            int k = 0;
            for (long[] l : e.getValue()) {
                flat[k++] = l[0];
                flat[k++] = l[1];
            }
            linkTag.putLongArray(e.getKey().location().toString(), flat);
        }
        tag.put("mergeLinks", linkTag);

        CompoundTag patchTag = new CompoundTag();
        RegistryOps<Tag> ops = RegistryOps.create(NbtOps.INSTANCE, registries);
        for (Map.Entry<String, DataComponentPatch> e : patches.entrySet())
            DataComponentPatch.CODEC.encodeStart(ops, e.getValue()).result().ifPresent(t -> patchTag.put(e.getKey(), t));
        tag.put("patches", patchTag);
        return tag;
    }

    public static IndustryData load(CompoundTag tag, HolderLookup.Provider registries) {
        IndustryData data = new IndustryData();
        data.nextId = Math.max(1, tag.getLong("nextId"));
        for (Tag raw : tag.getList("districts", Tag.TAG_COMPOUND)) {
            CompoundTag t = (CompoundTag) raw;
            ResourceKey<Level> dim = dim(t.getString("dim"));
            if (dim == null)
                continue;
            District d = new District(t.getLong("id"), dim);
            for (long c : t.getLongArray("chunks"))
                d.chunks.add(c);
            if (t.contains("customName"))
                d.customName = t.getString("customName");
            if (t.contains("stationName"))
                d.stationName = t.getString("stationName");
            d.notLearnable = t.getInt("notLearnable");
            d.revision = t.getInt("revision");
            d.declaredRevision = t.getInt("declared");
            d.creative = t.getBoolean("creative");
            int index = 0;
            for (Tag ir : t.getList("inventories", Tag.TAG_COMPOUND)) {
                CompoundTag i = (CompoundTag) ir;
                d.inventories.add(new District.Inventory(index++, BlockPos.of(i.getLong("pos")), i.getInt("kind"),
                        i.getInt("slots"), i.getLong("fluid"), i.getLong("energy")));
            }
            for (Tag sr : t.getList("stations", Tag.TAG_COMPOUND)) {
                CompoundTag s = (CompoundTag) sr;
                d.stations.add(new District.Station(s.getLong("id"), BlockPos.of(s.getLong("pos")), s.getString("name")));
            }
            for (long w : t.getLongArray("watch"))
                d.watch.add(BlockPos.of(w));
            d.recomputeBounds();
            data.districts.put(d.id, d);
            data.nextId = Math.max(data.nextId, d.id + 1);
        }
        CompoundTag pinTag = tag.getCompound("pins");
        for (String key : pinTag.getAllKeys()) {
            ResourceKey<Level> dim = dim(key);
            if (dim == null)
                continue;
            long[] flat = pinTag.getLongArray(key);
            Long2LongOpenHashMap m = data.pins(dim);
            for (int k = 0; k + 1 < flat.length; k += 2)
                m.put(flat[k], flat[k + 1]);
        }
        CompoundTag exTag = tag.getCompound("excluded");
        for (String key : exTag.getAllKeys()) {
            ResourceKey<Level> dim = dim(key);
            if (dim != null)
                for (long c : exTag.getLongArray(key))
                    data.excluded(dim).add(c);
        }
        CompoundTag linkTag = tag.getCompound("mergeLinks");
        for (String key : linkTag.getAllKeys()) {
            ResourceKey<Level> dim = dim(key);
            if (dim == null)
                continue;
            long[] flat = linkTag.getLongArray(key);
            for (int k = 0; k + 1 < flat.length; k += 2)
                data.mergeLinks(dim).add(new long[] {flat[k], flat[k + 1]});
        }
        CompoundTag patchTag = tag.getCompound("patches");
        RegistryOps<Tag> ops = RegistryOps.create(NbtOps.INSTANCE, registries);
        for (String key : patchTag.getAllKeys())
            DataComponentPatch.CODEC.parse(ops, patchTag.get(key)).result().ifPresent(p -> data.patches.put(key, p));
        return data;
    }

    private static ResourceKey<Level> dim(String name) {
        ResourceLocation loc = ResourceLocation.tryParse(name);
        return loc == null ? null : ResourceKey.create(Registries.DIMENSION, loc);
    }
}
