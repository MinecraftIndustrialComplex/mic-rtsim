package com.minecraftindustrialcomplex.mic_rtsim.village;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import it.unimi.dsi.fastutil.objects.Object2IntMap;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.PoiTypeTags;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.entity.ai.village.poi.PoiManager;
import net.minecraft.world.entity.ai.village.poi.PoiRecord;
import net.minecraft.world.entity.ai.village.poi.PoiTypes;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.AABB;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The village census (PLAN.md §6.3): residents by profession, beds, workstations, golems and the
 * meeting point, plus vanilla's own gossip read back, for one loaded village at a time, staggered
 * (one every {@link #INTERVAL} ticks, PLACEHOLDER). It is also when residents who were not loaded get
 * the gossip their village heard meanwhile (C20).
 *
 * <p>Only loaded chunks are read: entities through the level, POIs chunk by chunk with
 * {@code PoiManager.getInChunk} (a loaded chunk's POI sections are in memory, so nothing waits on disk).
 */
public final class Census {
    /** Ticks between two village censuses (PLACEHOLDER). */
    public static final int INTERVAL = 100;

    private final RtsimSession session;
    private final VillageRegistry villages;
    private final GossipWriter gossip;
    private int cursor;
    long taken;

    public Census(RtsimSession session, VillageRegistry villages, GossipWriter gossip) {
        this.session = session;
        this.villages = villages;
        this.gossip = gossip;
    }

    public long taken() {
        return taken;
    }

    public void tick(long ticks, long now) {
        if (ticks % INTERVAL != 0 || villages.size() == 0)
            return;
        List<Village> all = new ArrayList<>(villages.all());
        for (int tries = 0; tries < all.size(); tries++) {
            Village v = all.get(Math.floorMod(cursor++, all.size()));
            ServerLevel level = session.server().getLevel(v.dimension);
            if (level != null && level.getChunkSource().getChunkNow(v.meeting.getX() >> 4, v.meeting.getZ() >> 4) != null) {
                take(level, v, now);
                return;
            }
        }
    }

    /** How far past a village's box the census looks for its residents (EQC151; the beds stay inside the box). */
    public static final int CENSUS_REACH = 48;

    /**
     * What one scan of a village found. {@code complete} when every chunk of the village's reach was FULL with its entities
     * loaded and every walker the registry tracks for the village was found, so a count below the engine's is believed.
     */
    public record Scan(int residents, int beds, int workstations, int golems, Map<String, Integer> professions,
                       List<Villager> villagers, BlockPos meeting, boolean complete) {}

    /** Scans one village now: its residents and walkers, beds, workstations, golems and meeting point (EQC151, EQC179). */
    public static Scan scan(VillageRegistry villages, ServerLevel level, Village v) {
        AABB box = AABB.of(v.box).inflate(VillageRegistry.MARGIN);
        AABB reach = AABB.of(v.box).inflate(CENSUS_REACH);
        Map<String, Integer> professions = new LinkedHashMap<>();
        List<Villager> villagers = new ArrayList<>();
        java.util.Set<UUID> counted = new java.util.HashSet<>();
        int residents = 0;
        for (Entity e : level.getEntities((Entity) null, reach, e -> e.isAlive() && Residents.kind(e) != null
                && !com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments.isTagged(e))) {
            if (Residents.villageOf(villages, e) != v)
                continue;
            if (!counted.add(e.getUUID()))
                continue;
            residents++;
            String p = Residents.profession(e);
            if (p != null)
                professions.merge(p, 1, Integer::sum);
            if (e instanceof Villager villager)
                villagers.add(villager);
        }
        boolean complete = true;
        for (int cx = ((int) reach.minX) >> 4; cx <= ((int) reach.maxX) >> 4 && complete; cx++)
            for (int cz = ((int) reach.minZ) >> 4; cz <= ((int) reach.maxZ) >> 4; cz++)
                if (level.getChunkSource().getChunkNow(cx, cz) == null || !level.areEntitiesLoaded(ChunkPos.asLong(cx, cz))) {
                    complete = false;
                    break;
                }
        // A settler still walking in counts whether or not it stands in the reach; one the level cannot find makes the
        // census incomplete (EQC178).
        for (VillageRegistry.Walker w : villages.walkers.values()) {
            if (w.village != v.id)
                continue;
            Entity e = level.getEntity(w.id);
            if (e == null) {
                complete = false;
                continue;
            }
            if (!e.isAlive() || Residents.kind(e) == null
                    || com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments.isTagged(e) || !counted.add(e.getUUID()))
                continue;
            residents++;
            String p = Residents.profession(e);
            if (p != null)
                professions.merge(p, 1, Integer::sum);
            if (e instanceof Villager villager)
                villagers.add(villager);
        }
        int golems = level.getEntitiesOfClass(IronGolem.class, box, IronGolem::isAlive).size();
        int beds = 0, workstations = 0;
        BlockPos meeting = null;
        double best = Double.MAX_VALUE;
        PoiManager pois = level.getPoiManager();
        for (int cx = ((int) box.minX) >> 4; cx <= ((int) box.maxX) >> 4; cx++) {
            for (int cz = ((int) box.minZ) >> 4; cz <= ((int) box.maxZ) >> 4; cz++) {
                if (level.getChunkSource().getChunkNow(cx, cz) == null)
                    continue;
                for (PoiRecord r : pois.getInChunk(h -> true, new ChunkPos(cx, cz), PoiManager.Occupancy.ANY).toList()) {
                    if (!box.contains(r.getPos().getCenter()))
                        continue;
                    if (r.getPoiType().is(PoiTypes.HOME))
                        beds++;
                    else if (r.getPoiType().is(PoiTypeTags.ACQUIRABLE_JOB_SITE))
                        workstations++;
                    else if (r.getPoiType().is(PoiTypes.MEETING)) {
                        double d = r.getPos().distSqr(v.meeting);
                        if (d < best) {
                            best = d;
                            meeting = r.getPos();
                        }
                    }
                }
            }
        }
        return new Scan(residents, beds, workstations, golems, professions, villagers, meeting, complete);
    }

    /** Takes one village's census now (also called by the GameTests). */
    public void take(ServerLevel level, Village v, long now) {
        Scan scan = scan(villages, level, v);
        BlockPos meeting = scan.meeting();
        if (meeting != null && (!meeting.equals(v.meeting) || !v.meetingFromPoi)) {
            v.meeting = meeting;
            v.meetingFromPoi = true;
            villages.setDirty();
        }
        gossip.catchUp(level, villages, v, scan.villagers(), now);
        journalCensus(v, level, scan.residents(), scan.beds(), scan.workstations(), scan.golems(), scan.professions(),
                scan.complete(), now);
        journalGossip(v, scan.villagers(), now);
        taken++;
    }

    private void journalCensus(Village v, ServerLevel level, int residents, int beds, int workstations, int golems,
                               Map<String, Integer> professions, boolean complete, long now) {
        int dim = session.dimId(level.dimension());
        int n = Math.min(professions.size(), (0xFFF8 - Obs.VillageCensus.ENTRIES) / Obs.VillageCensus.ENTRY);
        int[] ids = new int[n];
        int[] counts = new int[n];
        int k = 0;
        for (Map.Entry<String, Integer> e : professions.entrySet()) {
            if (k == n)
                break;
            ids[k] = session.dictId(Obs.DICT_PROFESSION, e.getKey());
            counts[k++] = e.getValue();
        }
        int at = session.boundary().direct(Obs.VILLAGE_CENSUS, Obs.VillageCensus.size(n), 0, now);
        if (at < 0)
            return;
        ByteBuffer b = session.boundary().openBuffer();
        b.putLong(at + Obs.VillageCensus.VILLAGE, v.id);
        b.putInt(at + Obs.VillageCensus.DIM, dim);
        b.putInt(at + Obs.VillageCensus.FLAGS, (v.meetingFromPoi ? Obs.VillageCensus.FLAG_MEETING_POI : 0)
                | (complete ? Obs.VillageCensus.FLAG_COMPLETE : 0));
        b.putInt(at + Obs.VillageCensus.MEET_X, v.meeting.getX());
        b.putInt(at + Obs.VillageCensus.MEET_Y, v.meeting.getY());
        b.putInt(at + Obs.VillageCensus.MEET_Z, v.meeting.getZ());
        b.putInt(at + Obs.VillageCensus.RESIDENTS, residents);
        b.putInt(at + Obs.VillageCensus.BEDS, beds);
        b.putInt(at + Obs.VillageCensus.WORKSTATIONS, workstations);
        b.putInt(at + Obs.VillageCensus.GOLEMS, golems);
        b.putInt(at + Obs.VillageCensus.PROFESSIONS, n);
        for (int i = 0; i < n; i++) {
            b.putInt(at + Obs.VillageCensus.ENTRIES + i * Obs.VillageCensus.ENTRY, ids[i]);
            b.putInt(at + Obs.VillageCensus.ENTRIES + i * Obs.VillageCensus.ENTRY + 4, counts[i]);
        }
    }

    private void journalGossip(Village v, List<Villager> villagers, long now) {
        Map<UUID, int[]> sums = new LinkedHashMap<>();
        for (Villager villager : villagers) {
            for (Map.Entry<UUID, Object2IntMap<GossipType>> e : villager.getGossips().getGossipEntries().entrySet()) {
                int[] s = sums.computeIfAbsent(e.getKey(), k -> new int[5]);
                for (Object2IntMap.Entry<GossipType> g : e.getValue().object2IntEntrySet())
                    s[g.getKey().ordinal()] += g.getIntValue();
            }
        }
        if (sums.isEmpty())
            return;
        int n = Math.min(sums.size(), (0xFFF8 - Obs.VillageGossip.FIRST) / Obs.VillageGossip.ENTRY);
        int at = session.boundary().direct(Obs.VILLAGE_GOSSIP, Obs.VillageGossip.size(n), 0, now);
        if (at < 0)
            return;
        ByteBuffer b = session.boundary().openBuffer();
        b.putLong(at + Obs.VillageGossip.VILLAGE, v.id);
        b.putInt(at + Obs.VillageGossip.ENTRIES, n);
        b.putInt(at + Obs.VillageGossip.RESIDENTS, villagers.size());
        int k = 0;
        for (Map.Entry<UUID, int[]> e : sums.entrySet()) {
            if (k == n)
                break;
            int o = at + Obs.VillageGossip.FIRST + k * Obs.VillageGossip.ENTRY;
            b.putLong(o, e.getKey().getMostSignificantBits());
            b.putLong(o + 8, e.getKey().getLeastSignificantBits());
            for (int t = 0; t < 5; t++)
                b.putInt(o + 16 + 4 * t, e.getValue()[t]);
            k++;
        }
    }
}
