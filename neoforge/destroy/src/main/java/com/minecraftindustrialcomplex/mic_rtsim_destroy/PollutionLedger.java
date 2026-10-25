package com.minecraftindustrialcomplex.mic_rtsim_destroy;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRings;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.saveddata.SavedData;
import petrolpark.mc.destroy.DestroyRegistries;
import petrolpark.mc.destroy.core.pollution.PollutionHelper;
import petrolpark.mc.destroy.core.pollution.PollutionType;

import java.util.ArrayDeque;
import java.util.Deque;
import java.util.HashMap;
import java.util.Map;
import java.util.TreeMap;

/**
 * Destroy pollution attributed to districts (I14): each district's cumulative change of each pollution
 * type, from the emissions at its positions (the mixin on {@code PollutionHelper.changePollution} by
 * position), saved as {@code data/mic_rtsim_destroy_pollution.dat} in the overworld. The sampler reports
 * each total as a deposit the district produces (choice Q9), so the engine learns its rate as an output of
 * the box; a reconcile's delta writes the replayed pollution back ({@link DestroyPlugin#applyDeposit}).
 */
public final class PollutionLedger extends SavedData {
    public static final String FILE = "mic_rtsim_destroy_pollution";

    /** District id to (pollution type id to cumulative change). */
    final Map<Long, Map<ResourceLocation, Long>> totals = new HashMap<>();
    /**
     * The engine ops booked here, by op and good (docs/choices-journal-barrier.md RP-207): this file is not the file
     * the pollution itself is in, so after a crash that kept one and lost the other the op is applied again, and
     * the booking only if this file lacks it.
     */
    private DurableOpRing ring = new DurableOpRing();

    PollutionLedger() {
        ring.onChange(this::setDirty);
    }

    private static final ThreadLocal<Deque<int[]>> BEFORE = ThreadLocal.withInitial(ArrayDeque::new);
    static long attributed;

    public static final Factory<PollutionLedger> FACTORY = new Factory<>(PollutionLedger::new, PollutionLedger::load);

    public static PollutionLedger get(ServerLevel any) {
        return any.getServer().overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
    }

    /** A district's totals, by type id (a copy). */
    public Map<ResourceLocation, Long> totals(long district) {
        Map<ResourceLocation, Long> m = totals.get(district);
        return m == null ? Map.of() : new TreeMap<>(m);
    }

    void add(long district, ResourceLocation type, long amount) {
        if (amount == 0)
            return;
        totals.computeIfAbsent(district, k -> new HashMap<>()).merge(type, amount, Long::sum);
        setDirty();
    }

    /** The registry id of a pollution type. */
    @SuppressWarnings({"unchecked", "rawtypes"})
    public static ResourceLocation id(PollutionType<?> type) {
        return type.chunk ? DestroyRegistries.CHUNK_POLLUTION_TYPES.getKey((PollutionType) type)
                : DestroyRegistries.LEVEL_POLLUTION_TYPES.getKey((PollutionType) type);
    }

    // ---- the mixin ------------------------------------------------------------------------------

    /** Mixin, HEAD of changePollution by position: the value before. */
    public static void before(Level level, BlockPos pos, PollutionType<?> type) {
        int v = 0;
        try {
            if (level instanceof ServerLevel)
                v = PollutionHelper.getPollution(level, pos, type);
        } catch (RuntimeException e) {
            v = Integer.MIN_VALUE;
        }
        BEFORE.get().push(new int[] {v});
    }

    /** Mixin, RETURN: the value after; the change is the district's. */
    public static void after(Level level, BlockPos pos, PollutionType<?> type) {
        Deque<int[]> stack = BEFORE.get();
        if (stack.isEmpty())
            return;
        int before = stack.pop()[0];
        if (!(level instanceof ServerLevel server) || before == Integer.MIN_VALUE)
            return;
        try {
            Industry ind = IndustryApi.current();
            if (ind == null)
                return;
            District d = ind.districtAt(level, pos);
            if (d == null)
                return;
            int after = PollutionHelper.getPollution(level, pos, type);
            ResourceLocation id = id(type);
            if (id != null && after != before) {
                get(server).add(d.id, id, (long) after - before);
                attributed++;
            }
        } catch (RuntimeException e) {
            MicRtsim.LOGGER.error("[mic_rtsim_destroy] attributing pollution at {} failed", pos, e);
        }
    }

    // ---- writing back ---------------------------------------------------------------------------

    /**
     * Applies replayed pollution of {@code type} for a district: a level type to the level, a chunk type to
     * {@code chunk}. Returns what changed (Destroy clamps at each type's maximum and at 0), and books it to
     * the district's total so the next sample agrees with the engine, unless this file already booked
     * {@code op} for {@code good} (RP-207).
     */
    @SuppressWarnings({"unchecked", "rawtypes"})
    long apply(ServerLevel level, ChunkAccess chunk, long district, ResourceLocation typeId, long delta, String good, IndustryPlugin.DepositOp op) {
        int n = (int) Math.max(Integer.MIN_VALUE, Math.min(Integer.MAX_VALUE, delta));
        PollutionType<Level> levelType = DestroyRegistries.LEVEL_POLLUTION_TYPES.get(typeId);
        long applied;
        if (levelType != null) {
            int before = PollutionHelper.getPollution(level, levelType);
            PollutionHelper.changePollution(level, levelType, n);
            applied = (long) PollutionHelper.getPollution(level, levelType) - before;
        } else {
            PollutionType<ChunkAccess> chunkType = DestroyRegistries.CHUNK_POLLUTION_TYPES.get(typeId);
            if (chunkType == null || chunk == null)
                return 0;
            int before = PollutionHelper.getPollution(chunk, chunkType);
            PollutionHelper.changePollution(chunk, chunkType, n);
            applied = (long) PollutionHelper.getPollution(chunk, chunkType) - before;
        }
        long key = op == null ? 0 : DurableOpRing.keyOf(op.id(), good);
        if (op == null || !ring.contains(key)) {
            add(district, typeId, applied);
        } else {
            rebooksRefused++;
        }
        if (op != null)
            op.keepIn(ring, key);
        return applied;
    }

    /** Bookings of an op this file already held (a crash kept it and lost the pollution): not booked again. */
    static long rebooksRefused;

    /** {@link #rebooksRefused}, for the GameTests. */
    public static long rebooksRefused() {
        return rebooksRefused;
    }

    // ---- NBT ------------------------------------------------------------------------------------

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        CompoundTag ds = new CompoundTag();
        for (Map.Entry<Long, Map<ResourceLocation, Long>> e : totals.entrySet()) {
            CompoundTag t = new CompoundTag();
            for (Map.Entry<ResourceLocation, Long> g : e.getValue().entrySet())
                t.putLong(g.getKey().toString(), g.getValue());
            ds.put(Long.toString(e.getKey()), t);
        }
        tag.put("districts", ds);
        DurableOpRings.write(ring, tag);
        return tag;
    }

    public static PollutionLedger load(CompoundTag tag, HolderLookup.Provider registries) {
        PollutionLedger l = new PollutionLedger();
        CompoundTag ds = tag.getCompound("districts");
        for (String k : ds.getAllKeys()) {
            long id;
            try {
                id = Long.parseLong(k);
            } catch (NumberFormatException e) {
                continue;
            }
            CompoundTag t = ds.getCompound(k);
            Map<ResourceLocation, Long> m = new HashMap<>();
            for (String g : t.getAllKeys()) {
                ResourceLocation rl = ResourceLocation.tryParse(g);
                if (rl != null)
                    m.put(rl, t.getLong(g));
            }
            l.totals.put(id, m);
        }
        l.ring = DurableOpRings.read(tag);
        l.ring.onChange(l::setDirty);
        return l;
    }
}
