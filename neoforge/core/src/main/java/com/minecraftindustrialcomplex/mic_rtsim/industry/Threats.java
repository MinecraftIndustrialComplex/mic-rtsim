package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameRules;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.phys.AABB;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * Threat sampling for the industry engine (INDUSTRY.md §13.4 to §13.6; I20, I21; C16 to C18), all
 * vanilla and NeoForge, with plugin hooks for turrets and kill machines:
 * <ul>
 *   <li><b>IND_SPAWN_SAMPLE</b> (I20): natural hostile spawns ({@code FinalizeSpawnEvent}, spawn type
 *       NATURAL) in a district and its ring while a player is in the district at night, per threat class,
 *       with the night ticks observed and the dark spawnable blocks of that area;</li>
 *   <li><b>IND_FIGHT</b> (I21 calibration, I22's "never better than observed"): a real fight at a district,
 *       from the first engagement of a hostile mob there to {@code fightGapTicks} without one;</li>
 *   <li><b>IND_SURVEY</b> and <b>IND_SURVEY_BLOCKS</b> (I21): the defence vector and the exposed block set,
 *       recomputed when the district's revision changes, after player edits in it, and every
 *       {@code surveyPeriodTicks} while it is loaded ({@link SurveyJob}).</li>
 * </ul>
 * Choices Q10 to Q19 in docs/overnight-choices-indplugins.md.
 */
public final class Threats {

    private final Industry ind;
    private final Map<Long, State> states = new HashMap<>();
    private SurveyJob job;

    // counters for /rtsim district perf and the GameTests
    long surveys;
    long surveysAborted;
    long spawnSamples;
    long spawnsCounted;
    long fights;
    long lastSurveyNanos;

    Threats(Industry ind) {
        this.ind = ind;
    }

    /** One district's threat state this session. */
    static final class State {
        int surveyedRevision = -1;
        long surveyedAt = Long.MIN_VALUE / 2;
        boolean dirty;
        // the last survey's numbers, for the samples and the GameTests
        int darkDistrict = -1;
        int darkRing = -1;
        long[] lastSurvey;
        int lastExposed;
        // natural spawns (I20)
        boolean observing;
        long observedTicks;
        final int[] spawns = new int[IndRecords.ThreatClass.COUNT];
        // fights, per class
        final Fight[] fights = new Fight[IndRecords.ThreatClass.COUNT];
    }

    /** One real fight against one class at one district. */
    static final class Fight {
        final long start;
        long last;
        final Set<UUID> attackers = new HashSet<>();
        final Set<UUID> killed = new HashSet<>();
        int blocksDestroyed;
        int machinesDestroyed;
        int guardsLost;

        Fight(long now) {
            start = now;
            last = now;
        }
    }

    State state(District d) {
        return states.computeIfAbsent(d.id, k -> new State());
    }

    /** The last survey of a district, {perimeterColumns, perimeterGaps, darkSpawnable, machines, roofed, killAtGaps, guards[0..3], turret[0..3] x 1000, revision, exposed}; null if none. */
    public long[] lastSurvey(long district) {
        State s = states.get(district);
        return s == null || s.lastSurvey == null ? null : s.lastSurvey.clone();
    }

    /** Asks for a survey of the district at the next opportunity (commands, GameTests). */
    public void surveySoon(District d) {
        State s = state(d);
        s.dirty = true;
        s.surveyedAt = Long.MIN_VALUE / 2;
    }

    public long surveys() {
        return surveys;
    }

    public long fights() {
        return fights;
    }

    public long spawnSamples() {
        return spawnSamples;
    }

    // ---- the tick -------------------------------------------------------------------------------

    void tick(long now, long ticks) {
        int check = ThreatConfig.get(ThreatConfig.OBSERVE_CHECK);
        if (ticks % check == 0)
            observe(now, check);
        endFights(now);
        survey(now);
    }

    /** Who is in which district at night: the observed ticks of the spawn samples. */
    private void observe(long now, int check) {
        int sampleTicks = ThreatConfig.get(ThreatConfig.SPAWN_SAMPLE);
        for (District d : ind.districts.all()) {
            State s = states.get(d.id);
            boolean was = s != null && s.observing;
            boolean is = false;
            if (ind.districts.loadedChunks(d) > 0) {
                ServerLevel level = ind.level(d.dim);
                if (level != null && level.isNight()) {
                    for (ServerPlayer p : level.players()) {
                        if (!p.isSpectator() && d.chunks.contains(p.chunkPosition().toLong())) {
                            is = true;
                            break;
                        }
                    }
                }
            }
            if (!is && !was)
                continue;
            if (s == null)
                s = state(d);
            s.observing = is;
            if (is)
                s.observedTicks += check;
            if (s.observedTicks > 0 && (s.observedTicks >= sampleTicks || !is))
                flushSpawns(d, s);
        }
    }

    private void flushSpawns(District d, State s) {
        if (s.darkDistrict < 0) {
            surveySoon(d); // a sample needs the dark area; keep counting until a survey has it
            return;
        }
        ByteBuffer r = ind.journal(IndRecords.IND_SPAWN_SAMPLE, IndRecords.SpawnSample.SIZE, 0);
        r.putLong(IndRecords.SpawnSample.DISTRICT, d.id);
        r.putLong(IndRecords.SpawnSample.OBSERVED_TICKS, s.observedTicks);
        r.putLong(IndRecords.SpawnSample.DARK_BLOCKS, (long) s.darkDistrict + s.darkRing);
        for (int c = 0; c < IndRecords.ThreatClass.COUNT; c++)
            r.putInt(IndRecords.SpawnSample.SPAWNS + 4 * c, s.spawns[c]);
        ind.commit(r);
        spawnSamples++;
        s.observedTicks = 0;
        java.util.Arrays.fill(s.spawns, 0);
    }

    /** Ends the observation of every district now (GameTests): partial samples go out. */
    public void flushObservations() {
        for (District d : ind.districts.all()) {
            State s = states.get(d.id);
            if (s != null && s.observedTicks > 0)
                flushSpawns(d, s);
        }
    }

    /** Test helper: what a district's open spawn sample holds, {observedTicks, spawns[0..3]}. */
    public long[] openSample(long district) {
        State s = states.get(district);
        if (s == null)
            return null;
        return new long[] {s.observedTicks, s.spawns[0], s.spawns[1], s.spawns[2], s.spawns[3]};
    }

    // ---- events (via ThreatEvents) --------------------------------------------------------------

    /** The district a position counts for: its own chunk's, else one within the spawn ring. */
    District districtNear(Level level, BlockPos pos, int ring) {
        int cx = pos.getX() >> 4, cz = pos.getZ() >> 4;
        District d = ind.districtAt(level.dimension(), ChunkPos.asLong(cx, cz));
        if (d != null || ring <= 0)
            return d;
        for (int r = 1; r <= ring; r++)
            for (int dx = -r; dx <= r; dx++)
                for (int dz = -r; dz <= r; dz++) {
                    if (Math.max(Math.abs(dx), Math.abs(dz)) != r)
                        continue;
                    d = ind.districtAt(level.dimension(), ChunkPos.asLong(cx + dx, cz + dz));
                    if (d != null)
                        return d;
                }
        return null;
    }

    /** A natural spawn of a hostile mob (FinalizeSpawnEvent, NATURAL). */
    void naturalSpawn(ServerLevel level, Mob mob, BlockPos pos) {
        District d = districtNear(level, pos, ThreatConfig.get(ThreatConfig.SPAWN_RING));
        if (d == null)
            return;
        State s = states.get(d.id);
        if (s == null || !s.observing)
            return;
        s.spawns[ThreatClasses.classOf(mob)]++;
        spawnsCounted++;
    }

    /** A hostile mob in a district engages something: the fight against its class begins or goes on. */
    void engage(Entity attacker) {
        if (!(attacker.level() instanceof ServerLevel level) || !ThreatClasses.hostile(attacker))
            return;
        District d = ind.districtAt(level, attacker.blockPosition());
        if (d == null)
            return;
        int c = ThreatClasses.classOf(attacker);
        State s = state(d);
        long now = ind.gameTime();
        Fight f = s.fights[c];
        if (f == null)
            f = s.fights[c] = new Fight(now);
        f.last = now;
        f.attackers.add(attacker.getUUID());
    }

    /** Something living took damage: either side of a fight. */
    void damaged(LivingEntity victim, Entity attacker) {
        if (attacker != null && ThreatClasses.hostile(attacker) && (ThreatClasses.guard(victim) || victim instanceof net.minecraft.world.entity.player.Player))
            engage(attacker);
        if (ThreatClasses.hostile(victim) && attacker != null && !ThreatClasses.hostile(attacker))
            engage(victim);
    }

    /** A death: a killed attacker, or a lost guard. */
    void died(LivingEntity e, Entity killer) {
        if (!(e.level() instanceof ServerLevel level))
            return;
        District d = ind.districtAt(level, e.blockPosition());
        State s = d == null ? null : states.get(d.id);
        if (s == null)
            return;
        if (ThreatClasses.hostile(e)) {
            Fight f = s.fights[ThreatClasses.classOf(e)];
            if (f != null && f.attackers.contains(e.getUUID())) {
                f.killed.add(e.getUUID());
                f.last = ind.gameTime();
            }
        } else if (ThreatClasses.guard(e) && killer != null && ThreatClasses.hostile(killer)) {
            Fight f = s.fights[ThreatClasses.classOf(killer)];
            if (f != null) {
                f.guardsLost++;
                f.last = ind.gameTime();
            }
        }
    }

    /** An explosion: blocks and machines destroyed, booked to the exploder's class (a creeper: siege). */
    void exploded(ServerLevel level, Entity source, List<BlockPos> blocks) {
        if (source == null || !ThreatClasses.hostile(source))
            return;
        engage(source);
        Map<BlockPos, Boolean> cache = new HashMap<>();
        for (BlockPos p : blocks) {
            if (level.getBlockState(p).isAir())
                continue;
            District d = ind.districtAt(level, p);
            if (d == null)
                continue;
            State s = state(d);
            int c = ThreatClasses.classOf(source);
            Fight f = s.fights[c];
            if (f == null)
                f = s.fights[c] = new Fight(ind.gameTime());
            f.last = ind.gameTime();
            f.attackers.add(source.getUUID());
            f.blocksDestroyed++;
            BlockEntity be = level.getBlockEntity(p);
            if (be != null && ind.districts.industrial(level, be, cache) && d.inventoryAt(p) == null)
                f.machinesDestroyed++;
            s.dirty = true;
        }
    }

    /** A player placed or broke a block in a district: survey it again. */
    void edited(Level level, BlockPos pos) {
        District d = ind.districtAt(level, pos);
        if (d != null)
            state(d).dirty = true;
    }

    private void endFights(long now) {
        int gap = ThreatConfig.get(ThreatConfig.FIGHT_GAP);
        for (Map.Entry<Long, State> e : states.entrySet())
            for (int c = 0; c < IndRecords.ThreatClass.COUNT; c++) {
                Fight f = e.getValue().fights[c];
                if (f != null && now - f.last > gap) {
                    e.getValue().fights[c] = null;
                    emitFight(e.getKey(), c, f);
                }
            }
    }

    /** Ends every open fight now (GameTests). */
    public void endFightsNow() {
        for (Map.Entry<Long, State> e : states.entrySet())
            for (int c = 0; c < IndRecords.ThreatClass.COUNT; c++) {
                Fight f = e.getValue().fights[c];
                if (f != null) {
                    e.getValue().fights[c] = null;
                    emitFight(e.getKey(), c, f);
                }
            }
    }

    private void emitFight(long district, int cls, Fight f) {
        if (f.attackers.isEmpty() || ind.district(district) == null)
            return;
        int attackers = f.attackers.size();
        ByteBuffer r = ind.journal(IndRecords.IND_FIGHT, IndRecords.Fight.SIZE, cls);
        r.putLong(IndRecords.Fight.DISTRICT, district);
        r.putInt(IndRecords.Fight.ATTACKERS, attackers);
        // Natural mobs have strength 1 each (the engine's F17 unit, a PLACEHOLDER there; choice Q12).
        r.putFloat(IndRecords.Fight.ATTACKER_STRENGTH, attackers);
        r.putInt(IndRecords.Fight.ATTACKERS_KILLED, f.killed.size());
        r.putInt(IndRecords.Fight.BLOCKS_DESTROYED, f.blocksDestroyed);
        r.putInt(IndRecords.Fight.MACHINES_DESTROYED, f.machinesDestroyed);
        r.putInt(IndRecords.Fight.GUARDS_LOST, f.guardsLost);
        r.putLong(IndRecords.Fight.ITEMS_STOLEN, 0); // vanilla mobs take nothing from containers (Q12)
        r.putLong(IndRecords.Fight.DURATION_TICKS, Math.max(0, f.last - f.start));
        ind.commit(r);
        fights++;
    }

    // ---- the survey -----------------------------------------------------------------------------

    private void survey(long now) {
        if (job != null) {
            if (ind.district(job.district.id) != job.district || job.district.revision != job.revision) {
                job = null; // the district changed under the survey: start again
            } else {
                job.step(ind, ThreatConfig.get(ThreatConfig.SURVEY_CHUNKS_PER_TICK));
                if (job.aborted) {
                    surveysAborted++;
                    job = null;
                } else if (job.done()) {
                    SurveyJob j = job;
                    job = null;
                    finish(j, now);
                }
                return;
            }
        }
        int minInterval = ThreatConfig.get(ThreatConfig.SURVEY_MIN_INTERVAL);
        int period = ThreatConfig.get(ThreatConfig.SURVEY_PERIOD);
        for (District d : ind.districts.all()) {
            if (d.chunks.isEmpty() || ind.districts.loadedChunks(d) < d.chunks.size())
                continue;
            State s = state(d);
            boolean due = s.surveyedRevision != d.revision || s.dirty || now - s.surveyedAt >= period;
            if (!due || now - s.surveyedAt < minInterval)
                continue;
            ServerLevel level = ind.level(d.dim);
            if (level == null)
                continue;
            s.dirty = false;
            job = new SurveyJob(d, level, now, ThreatConfig.get(ThreatConfig.SPAWN_RING));
            return;
        }
    }

    private void finish(SurveyJob j, long now) {
        long t0 = System.nanoTime();
        District d = j.district;
        State s = state(d);
        SurveyJob.Walk walk = j.walk();
        // Guards: owned and player-made fighters in the district's chunks.
        int[] guards = new int[IndRecords.ThreatClass.COUNT];
        AABB box = new AABB(d.minCx * 16, j.level.getMinBuildHeight(), d.minCz * 16, d.maxCx * 16 + 16, j.level.getMaxBuildHeight(), d.maxCz * 16 + 16);
        for (Entity e : j.level.getEntities((Entity) null, box, e -> e instanceof LivingEntity && e.isAlive() && ThreatClasses.guard(e)
                && d.chunks.contains(e.chunkPosition().toLong()))) {
            int bits = ThreatClasses.guardClasses(e);
            for (int c = 0; c < IndRecords.ThreatClass.COUNT; c++)
                if ((bits & (1 << c)) != 0)
                    guards[c]++;
        }
        // Ammunition: the good most turrets hold (Q17: ammoPerEngagement is not measured, 0).
        String ammo = null;
        long most = 0;
        for (Map.Entry<String, Long> e : j.ammo.entrySet())
            if (e.getValue() > most) {
                most = e.getValue();
                ammo = e.getKey();
            }
        boolean griefing = j.level.getGameRules().getBoolean(GameRules.RULE_MOBGRIEFING);
        ByteBuffer r = ind.journal(IndRecords.IND_SURVEY, IndRecords.Survey.SIZE, griefing ? IndRecords.Survey.FLAG_MOB_GRIEFING : 0);
        r.putLong(IndRecords.Survey.DISTRICT, d.id);
        r.putInt(IndRecords.Survey.REVISION, j.revision);
        r.putInt(IndRecords.Survey.PERIMETER_COLUMNS, walk.perimeterColumns());
        r.putInt(IndRecords.Survey.PERIMETER_GAPS, walk.perimeterGaps());
        r.putInt(IndRecords.Survey.DARK_SPAWNABLE, j.darkDistrict);
        r.putInt(IndRecords.Survey.MACHINE_BLOCKS, j.machines);
        r.putInt(IndRecords.Survey.ROOFED_MACHINES, j.roofed);
        r.putInt(IndRecords.Survey.KILL_MACHINES_AT_GAPS, walk.killAtGaps());
        r.putInt(IndRecords.Survey.AMMO_GOOD, ammo == null ? 0 : ind.goodId(ammo));
        for (int c = 0; c < IndRecords.ThreatClass.COUNT; c++) {
            r.putFloat(IndRecords.Survey.TURRET_STRENGTH + 4 * c, j.turretStrength[c]);
            r.putInt(IndRecords.Survey.GUARDS + 4 * c, guards[c]);
        }
        r.putInt(IndRecords.Survey.AMMO_PER_ENGAGEMENT, 0);
        ind.commit(r);

        // The exposed set: wall faces first (ground attackers break in there), then machines, then stock.
        List<SurveyJob.Exposed> exposed = new ArrayList<>();
        int cap = ThreatConfig.get(ThreatConfig.SURVEY_MAX_BLOCKS);
        for (long c : walk.wallColumns()) {
            if (exposed.size() >= cap)
                break;
            BlockPos p = new BlockPos(SurveyJob.colX(c), j.top.get(c), SurveyJob.colZ(c));
            net.minecraft.world.level.block.state.BlockState st = j.level.getBlockState(p);
            if (!st.isAir())
                exposed.add(new SurveyJob.Exposed(p, st, IndRecords.SurveyBlocks.FLAG_PERIMETER, IndRecords.NO_INVENTORY));
        }
        for (SurveyJob.Exposed e : j.machineBlocks)
            if (exposed.size() < cap)
                exposed.add(e);
        for (SurveyJob.Exposed e : j.inventoryBlocks)
            if (exposed.size() < cap)
                exposed.add(e);
        int part = 0;
        int max = IndRecords.SurveyBlocks.MAX_BLOCKS;
        for (int from = 0; from < exposed.size() || (from == 0 && part == 0); from += max, part++) {
            int n = Math.min(max, exposed.size() - from);
            ByteBuffer b = ind.journal(IndRecords.IND_SURVEY_BLOCKS, IndRecords.SurveyBlocks.size(n), n);
            b.putLong(IndRecords.SurveyBlocks.DISTRICT, d.id);
            b.putInt(IndRecords.SurveyBlocks.REVISION, j.revision);
            b.putInt(IndRecords.SurveyBlocks.PART, part);
            for (int i = 0; i < n; i++) {
                SurveyJob.Exposed e = exposed.get(from + i);
                int at = IndRecords.SurveyBlocks.BLOCKS + i * IndRecords.SurveyBlocks.BLOCK;
                b.putInt(at + IndRecords.SurveyBlocks.B_X, e.pos().getX());
                b.putInt(at + IndRecords.SurveyBlocks.B_Y, e.pos().getY());
                b.putInt(at + IndRecords.SurveyBlocks.B_Z, e.pos().getZ());
                b.putInt(at + IndRecords.SurveyBlocks.B_STATE, ind.boundary.dictId(IndRecords.DICT_BLOCK_STATE, BlockStateParser.serialize(e.state())));
                b.putInt(at + IndRecords.SurveyBlocks.B_FLAGS, e.flags());
                b.putInt(at + IndRecords.SurveyBlocks.B_INVENTORY, e.inventory());
            }
            ind.commit(b);
            if (n == 0)
                break;
        }

        s.surveyedRevision = j.revision;
        s.surveyedAt = now;
        s.darkDistrict = j.darkDistrict;
        s.darkRing = j.darkRing;
        s.lastExposed = exposed.size();
        s.lastSurvey = new long[] {walk.perimeterColumns(), walk.perimeterGaps(), j.darkDistrict, j.machines, j.roofed, walk.killAtGaps(),
                guards[0], guards[1], guards[2], guards[3],
                Math.round(j.turretStrength[0] * 1000), Math.round(j.turretStrength[1] * 1000), Math.round(j.turretStrength[2] * 1000),
                Math.round(j.turretStrength[3] * 1000), j.revision, exposed.size(), j.darkRing, walk.cutComputed() ? 1 : 0};
        surveys++;
        lastSurveyNanos = System.nanoTime() - t0;
        if (!walk.cutComputed())
            MicRtsim.LOGGER.info("[mic_rtsim] industry: district {} is too wide for the perimeter cut ({} columns at most): perimeter sent as 0", d.id,
                    ThreatConfig.get(ThreatConfig.SURVEY_MAX_COLUMNS));
    }

    /** The survey running now, for /rtsim district perf. */
    public String jobDescription() {
        return job == null ? "none" : "district " + job.district.id + " since tick " + job.startedAt;
    }

    /** Forgets this session's threat state of a district (GameTests). */
    public void forget(long district) {
        states.remove(district);
        if (job != null && job.district.id == district)
            job = null;
    }

    /** Forgets every district's threat state (GameTests). */
    public void forgetAll() {
        states.clear();
        job = null;
    }

}
