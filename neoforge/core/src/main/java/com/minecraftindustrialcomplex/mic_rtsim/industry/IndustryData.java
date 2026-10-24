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

    /**
     * 3 (SEC-1, slice S4): districts cut at links (SECTIONS.md §2), and the plugins' state ({@link #plugin},
     * a Create package store's identity). A save of an older format with districts has them dropped on load
     * (SECTIONS.md §6, choice SC98): their ids stay used ({@code nextId} is kept), the engine is told each
     * is gone at the session's start ({@link #droppedOnLoad}), and detection finds them again from the
     * seeds their chunks keep. The pins name dropped districts and go too; exclusions and merge links are
     * chunks and stay. No migration (nothing is released).
     *
     * <p>4 (slice S6): each district's join pairs ({@link District#joins}) and the stations' served chunks
     * ({@link #stationJoins}); bare 8-adjacency no longer joins chunks (SECTIONS.md §2.1). A format 3 save's
     * districts are dropped the same way; its plugins' state stays (choice SC155): a package store is chunks
     * and positions, and its next walk declares it with the new districts.
     */
    public static final int FORMAT = 4;
    /** The first format whose plugins' state a later format keeps (SC155). */
    static final int PLUGINS_KEPT_SINCE = 3;

    /** The operator's one line when an older format's districts were dropped (SECTIONS.md §6). */
    public static final String FORMAT_RESET_LINE = "rtsim industry: the save format changed (SEC-1); learned factory state was reset and districts are detected again.";

    /** Districts by id, in id order. */
    final TreeMap<Long, District> districts = new TreeMap<>();
    long nextId = 1;
    /** Chunks a {@code split} pinned to a district, per dimension. */
    final Map<ResourceKey<Level>, Long2LongOpenHashMap> pins = new HashMap<>();
    /** Chunks {@code exclude} took out of every district, per dimension. */
    final Map<ResourceKey<Level>, LongOpenHashSet> excluded = new HashMap<>();
    /** {@code merge}: pairs of chunks whose districts are one, per dimension. */
    final Map<ResourceKey<Level>, List<long[]>> mergeLinks = new HashMap<>();
    /**
     * J6 (SECTIONS.md §2.1, choice SC6): per dimension, a station's chunk to a chunk of the district its docked
     * interfaces reached last ({@code TrainWatch.ports}), so its chunk joins that district, also after a restart
     * before the next docking. Dropped when either chunk leaves every district.
     */
    final Map<ResourceKey<Level>, Long2LongOpenHashMap> stationJoins = new HashMap<>();
    /** GOOD component hashes and the patches behind them ({@link Goods}). */
    final Map<String, DataComponentPatch> patches = new HashMap<>();
    /** Each plugin's own saved state, by a key the plugin chose ({@link #plugin}). */
    final Map<String, CompoundTag> plugins = new TreeMap<>();
    /**
     * Districts an older format's save held and a load dropped (format 3): the engine may still hold them,
     * so the session's start removes each (IND_DISTRICT_REMOVED) and clears the list. Saved until then
     * ({@code droppedDistricts}), so a save made before a session starts (the industry switched off) keeps
     * them to remove.
     */
    final List<Long> droppedOnLoad = new ArrayList<>();
    /**
     * After a reinstall reset (UR-1): member chunks that held seeds, per dimension, not loaded since. At its first
     * load each leaves the set, and one that lost its seed attachment in the gap has its machines adopted again
     * ({@code Districts.readopt}). An optional key: a world without one has nothing to adopt again.
     */
    final Map<ResourceKey<Level>, LongOpenHashSet> readopt = new HashMap<>();

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

    Long2LongOpenHashMap stationJoins(ResourceKey<Level> dim) {
        return stationJoins.computeIfAbsent(dim, d -> new Long2LongOpenHashMap());
    }

    public java.util.Collection<District> districts() {
        return districts.values();
    }

    /** A plugin's saved state under {@code key}, or null. The plugin owns the tag; {@link #putPlugin} replaces it. */
    public CompoundTag plugin(String key) {
        return plugins.get(key);
    }

    /** Replaces a plugin's saved state (null removes it) and marks the index to save. */
    public void putPlugin(String key, CompoundTag tag) {
        if (tag == null)
            plugins.remove(key);
        else
            plugins.put(key, tag);
        setDirty();
    }

    // ---- NBT ------------------------------------------------------------------------------------

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        tag.putInt("format", FORMAT);
        tag.putLong("nextId", nextId);
        ListTag list = new ListTag();
        for (District d : districts.values()) {
            CompoundTag t = new CompoundTag();
            t.putLong("id", d.id);
            t.putString("dim", d.dim.location().toString());
            t.putLongArray("chunks", d.chunks.toLongArray());
            long[] seeds = new long[d.seedCounts.size() * 2];
            int sk = 0;
            for (it.unimi.dsi.fastutil.longs.Long2IntMap.Entry e : d.seedCounts.long2IntEntrySet()) {
                seeds[sk++] = e.getLongKey();
                seeds[sk++] = e.getIntValue();
            }
            t.putLongArray("seeds", seeds);
            long[] joins = new long[d.joins.size() * 2];
            int jk = 0;
            for (District.Join j : d.joins) {
                joins[jk++] = j.a();
                joins[jk++] = j.b();
            }
            t.putLongArray("joins", joins);
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
        CompoundTag stationTag = new CompoundTag();
        for (Map.Entry<ResourceKey<Level>, Long2LongOpenHashMap> e : stationJoins.entrySet()) {
            if (e.getValue().isEmpty())
                continue;
            long[] flat = new long[e.getValue().size() * 2];
            int k = 0;
            for (Long2LongOpenHashMap.Entry p : e.getValue().long2LongEntrySet()) {
                flat[k++] = p.getLongKey();
                flat[k++] = p.getLongValue();
            }
            stationTag.putLongArray(e.getKey().location().toString(), flat);
        }
        tag.put("stationJoins", stationTag);

        CompoundTag patchTag = new CompoundTag();
        RegistryOps<Tag> ops = RegistryOps.create(NbtOps.INSTANCE, registries);
        for (Map.Entry<String, DataComponentPatch> e : patches.entrySet())
            DataComponentPatch.CODEC.encodeStart(ops, e.getValue()).result().ifPresent(t -> patchTag.put(e.getKey(), t));
        tag.put("patches", patchTag);
        CompoundTag pluginTag = new CompoundTag();
        for (Map.Entry<String, CompoundTag> e : plugins.entrySet())
            pluginTag.put(e.getKey(), e.getValue());
        tag.put("plugins", pluginTag);
        if (!droppedOnLoad.isEmpty())
            tag.putLongArray("droppedDistricts", droppedOnLoad.stream().mapToLong(Long::longValue).toArray());
        CompoundTag readoptTag = new CompoundTag();
        for (Map.Entry<ResourceKey<Level>, LongOpenHashSet> e : readopt.entrySet())
            if (!e.getValue().isEmpty())
                readoptTag.putLongArray(e.getKey().location().toString(), e.getValue().toLongArray());
        if (!readoptTag.isEmpty())
            tag.put("readopt", readoptTag);
        return tag;
    }

    public static IndustryData load(CompoundTag tag, HolderLookup.Provider registries) {
        IndustryData data = new IndustryData();
        data.nextId = Math.max(1, tag.getLong("nextId"));
        boolean reset = tag.getInt("format") < FORMAT && !tag.getList("districts", Tag.TAG_COMPOUND).isEmpty();
        for (long id : tag.getLongArray("droppedDistricts"))
            data.droppedOnLoad.add(id);
        if (reset) {
            for (Tag raw : tag.getList("districts", Tag.TAG_COMPOUND)) {
                long id = ((CompoundTag) raw).getLong("id");
                data.droppedOnLoad.add(id);
                data.nextId = Math.max(data.nextId, id + 1);
            }
            com.minecraftindustrialcomplex.mic_rtsim.MicRtsim.LOGGER.warn("{} (industry data format {}, now {}: {} districts dropped)",
                    FORMAT_RESET_LINE, tag.getInt("format"), FORMAT, tag.getList("districts", Tag.TAG_COMPOUND).size());
            data.setDirty();
        }
        for (Tag raw : reset ? new ListTag() : tag.getList("districts", Tag.TAG_COMPOUND)) {
            CompoundTag t = (CompoundTag) raw;
            ResourceKey<Level> dim = dim(t.getString("dim"));
            if (dim == null)
                continue;
            District d = new District(t.getLong("id"), dim);
            for (long c : t.getLongArray("chunks"))
                d.chunks.add(c);
            long[] seeds = t.getLongArray("seeds");
            for (int i = 0; i + 1 < seeds.length; i += 2)
                d.seedCounts.put(seeds[i], (int) seeds[i + 1]);
            long[] joins = t.getLongArray("joins");
            for (int i = 0; i + 1 < joins.length; i += 2)
                d.joins.add(District.join(joins[i], joins[i + 1]));
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
        CompoundTag pinTag = reset ? new CompoundTag() : tag.getCompound("pins");
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
        CompoundTag stationTag = reset ? new CompoundTag() : tag.getCompound("stationJoins");
        for (String key : stationTag.getAllKeys()) {
            ResourceKey<Level> dim = dim(key);
            if (dim == null)
                continue;
            long[] flat = stationTag.getLongArray(key);
            Long2LongOpenHashMap m = data.stationJoins(dim);
            for (int k = 0; k + 1 < flat.length; k += 2)
                m.put(flat[k], flat[k + 1]);
        }
        CompoundTag patchTag = tag.getCompound("patches");
        RegistryOps<Tag> ops = RegistryOps.create(NbtOps.INSTANCE, registries);
        for (String key : patchTag.getAllKeys())
            DataComponentPatch.CODEC.parse(ops, patchTag.get(key)).result().ifPresent(p -> data.patches.put(key, p));
        CompoundTag pluginTag = reset && tag.getInt("format") < PLUGINS_KEPT_SINCE ? new CompoundTag() : tag.getCompound("plugins");
        for (String key : pluginTag.getAllKeys())
            data.plugins.put(key, pluginTag.getCompound(key));
        CompoundTag readoptTag = tag.getCompound("readopt");
        for (String key : readoptTag.getAllKeys()) {
            ResourceKey<Level> dim = dim(key);
            if (dim != null)
                for (long c : readoptTag.getLongArray(key))
                    data.readopt.computeIfAbsent(dim, d -> new LongOpenHashSet()).add(c);
        }
        return data;
    }

    private static ResourceKey<Level> dim(String name) {
        ResourceLocation loc = ResourceLocation.tryParse(name);
        return loc == null ? null : ResourceKey.create(Registries.DIMENSION, loc);
    }
}
