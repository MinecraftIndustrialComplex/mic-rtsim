package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.config.IafCommonConfig;
import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.DragonRecords;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.IafDragons;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.ExperienceOrb;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.ai.targeting.TargetingConditions;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.monster.Blaze;
import net.minecraft.world.entity.monster.Enemy;
import net.minecraft.world.entity.monster.Husk;
import net.minecraft.world.entity.projectile.Projectile;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.Biomes;
import net.minecraft.world.level.block.BaseFireBlock;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.fml.loading.FMLPaths;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.tick.LevelTickEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.DragonKit.record;

/**
 * OG-10, the danger A/B of OILGATE.md: is a desert over oil at least as dangerous with the gate's field
 * flights (arm B: the pack's In Control rules without their fire dragon entries, flights on) as with the
 * pack's In Control dragons (arm A: the pack's own spawner.json and spawn.json, the gate off)? Three sites,
 * each its own far patch of the flat world dressed as a desert (desert biome, a sandstone floor at y 64,
 * In Control's 100 blocks and more): 1 on the surface over oil, 2 over oil in a sealed pocket below y 15,
 * 3 on the surface with no oil. At each, a probe player stands for {@link #ARM} ticks per arm, A then B,
 * at the same hour of the day.
 *
 * <p>The probe is a mock player (the one In Control spawns around and every hostile targets) with a
 * stand-in at its feet: the mock player is in the level's player list but not its entity sections, so
 * projectiles and area fire never find it; the stand-in (a still, invisible, silent husk of the player's
 * size) is what they hit. Both are guarded, so every blow is written down and cancelled. Damage is summed
 * as vanilla would take it (a hit within 10 ticks of the last counts only its excess), from blows with an
 * attacker (fire left burning underfoot is counted apart).
 *
 * <p>Native only (the stub decides no flights), and only with {@code -Pog10}: it takes a quarter of an
 * hour of server time. Each site is its own batch, so nothing else stands in the level while the arms
 * run (In Control spawns around a random player of the level).
 */
@GameTestHolder(MicRtsimIceAndFire.MODID)
@PrefixGameTestTemplate(false)
public final class OilDangerGameTests {
    private OilDangerGameTests() {}

    /** Ticks per arm: three game minutes. */
    static final int ARM = 3600;
    /** Forced chunks around the site: the engine's view radius (160) and a chunk, as the flights test. */
    static final int FORCE = 11;
    /** The sandstone floor: In Control's maxdist (100) and a margin, at a desert's height. */
    static final int FLOOR_R = 104, FLOOR_Y = 64;
    /** The pocket of site 2: the probe's feet at y 10, below the gate's flight_min_y (15). */
    static final int POCKET_Y = 10;
    /** The hour both arms start at: morning (fire dragons sleep at night). */
    static final long MORNING = 1000;
    /** A hit within this many ticks of the last one counts only its excess (LivingEntity.hurt). */
    static final int HURT_WINDOW = 10;
    static final String STAND_IN = "mic_rtsim_og10_stand_in";

    // ---- In Control -------------------------------------------------------------------------------

    private static volatile boolean rulesActive;
    private static volatile long beat = -1;
    private static volatile ServerPlayer liveProbe;
    private static volatile LivingEntity liveStandIn;
    private static boolean watching;

    /**
     * Puts arm {@code arm}'s rules ({@code "a"}, {@code "b"}, or null for none) into the server's
     * config/incontrol and reloads In Control as its reload command does. The arms' files are copied
     * beside the server by the Gradle task og10Rules.
     */
    static void inControl(String arm) throws Exception {
        Path dir = FMLPaths.CONFIGDIR.get().resolve("incontrol");
        Files.createDirectories(dir);
        for (String f : List.of("spawner.json", "spawn.json")) {
            if (arm == null)
                Files.writeString(dir.resolve(f), "[]\n");
            else
                Files.copy(FMLPaths.GAMEDIR.get().resolve("og10").resolve(arm).resolve(f), dir.resolve(f),
                        StandardCopyOption.REPLACE_EXISTING);
        }
        // CmdReload's calls (the command itself wants a player source).
        String[][] calls = {
                {"mcjty.incontrol.ErrorHandler", "clearErrors"},
                {"mcjty.incontrol.areas.AreaSystem", "reloadRules"},
                {"mcjty.incontrol.rules.RulesManager", "reloadRules"},
                {"mcjty.incontrol.spawner.SpawnerSystem", "reloadRules"},
                {"mcjty.incontrol.events.EventsSystem", "reloadRules"}};
        for (String[] c : calls)
            Class.forName(c[0]).getMethod(c[1]).invoke(null);
        rulesActive = arm != null;
    }

    static boolean inControlPresent() {
        try {
            Class.forName("mcjty.incontrol.spawner.SpawnerSystem");
            return true;
        } catch (ClassNotFoundException e) {
            return false;
        }
    }

    /**
     * A watchdog ahead of In Control's own tick: rules left on by a test that died (no heartbeat for 40
     * ticks) are taken off, and its probe let go. In Control's spawner picks a random player of the level
     * and throws when there is none.
     */
    private static synchronized void watch() {
        if (watching)
            return;
        NeoForge.EVENT_BUS.addListener(EventPriority.HIGHEST, false, LevelTickEvent.Pre.class, OilDangerGameTests::onLevelTick);
        watching = true;
    }

    private static void onLevelTick(LevelTickEvent.Pre event) {
        if (!rulesActive || !(event.getLevel() instanceof ServerLevel level) || level.dimension() != Level.OVERWORLD)
            return;
        if (level.getGameTime() - beat <= 40 && !level.players().isEmpty())
            return;
        try {
            inControl(null);
        } catch (Exception e) {
            rulesActive = false;
            record("OilDangerGameTests", "watchdog", "could not clear the rules: " + e);
        }
        if (liveProbe != null)
            OilGameTests.release(liveProbe);
        if (liveStandIn != null)
            liveStandIn.discard();
        liveProbe = null;
        liveStandIn = null;
        record("OilDangerGameTests", "watchdog", "a test stopped beating: In Control's rules cleared, its probe let go");
    }

    // ---- the tests --------------------------------------------------------------------------------

    @GameTest(template = DragonKit.ARENA, timeoutTicks = 12_000, batch = "og10_site1")
    public static void dangerSite1DesertSurfaceOverOil(GameTestHelper helper) {
        run(helper, 1);
    }

    @GameTest(template = DragonKit.ARENA, timeoutTicks = 12_000, batch = "og10_site2")
    public static void dangerSite2DesertOverOilBelowY15(GameTestHelper helper) {
        run(helper, 2);
    }

    @GameTest(template = DragonKit.ARENA, timeoutTicks = 12_000, batch = "og10_site3")
    public static void dangerSite3DesertWithoutOil(GameTestHelper helper) {
        run(helper, 3);
    }

    /** One arm's tally. */
    static final class Arm {
        final String name;
        long start = -1, end = -1;
        long drawsAt, refuelsAt, leftAt, breathsAt;
        long draws, refuels, left, breaths;
        int players;
        final Set<UUID> dragonsTargeting = new HashSet<>();
        final Set<UUID> dragonsNear = new HashSet<>();
        final Set<UUID> blazes = new HashSet<>();
        long firstTargetedByHostile = -1, firstTargetedByDragon = -1;
        // From the blows, at the arm's end.
        double hostileRaw, hostileEff, dragonRaw, dragonEff, underfoot;
        int hostileHits, dragonHits;
        long firstHostileHit = -1, firstDragonHit = -1;
        final Set<UUID> dragonsHitting = new HashSet<>();
        // Ice and Fire's target goal run for the probe (seek).
        final Set<UUID> seekers = new HashSet<>();
        int seeks;

        Arm(String name) {
            this.name = name;
        }

        long ticks() {
            return end - start;
        }

        double perMinute(double v) {
            return v * 1200.0 / ticks();
        }

        double perHour(int n) {
            return n * 72_000.0 / ticks();
        }

        Set<UUID> dragonsEngaged() {
            Set<UUID> s = new HashSet<>(dragonsTargeting);
            s.addAll(dragonsHitting);
            return s;
        }

        static String at(long t) {
            return t < 0 ? "none" : t + " ticks";
        }

        String line() {
            return String.format(Locale.ROOT,
                    "%d ticks; dragon damage %.1f (%.1f raw, %d hits) = %.1f/min; all hostile damage %.1f (%.1f raw, %d hits) = %.1f/min; "
                            + "first dragon hit %s (targeted from %s); first hostile hit %s (targeted from %s); dragons hitting %d (%.0f/h), "
                            + "targeting or hitting %d (%.0f/h), within 100 blocks %d; blazes %d; fire underfoot %.1f; players in the level %d; "
                            + "Ice and Fire's target goal found the probe %d times for %d dragons",
                    ticks(), dragonEff, dragonRaw, dragonHits, perMinute(dragonEff), hostileEff, hostileRaw, hostileHits, perMinute(hostileEff),
                    at(firstDragonHit), at(firstTargetedByDragon), at(firstHostileHit), at(firstTargetedByHostile),
                    dragonsHitting.size(), perHour(dragonsHitting.size()), dragonsEngaged().size(), perHour(dragonsEngaged().size()),
                    dragonsNear.size(), blazes.size(), underfoot, players, seeks, seekers.size());
        }
    }

    /**
     * Ice and Fire's wild-dragon target goal (DragonAITargetNonTamedGoal, priority 5; the priority 6 goal
     * never finds a first target in this port) looks for its prey with getEntitiesOfClass(LivingEntity), so
     * it never finds a mock player, which is not in the entity sections; a real player it would find. This
     * runs the goal's test for the probe as the goal would, on every dragon of either arm that has no
     * target: one tick in three (its randomInterval 5, halved), within the dragon's follow range (the
     * pack's targetSearchLength) around its box, no sight needed, a sleeping dragon only within 4 blocks,
     * none tame or looking for a roost, and only while Ice and Fire's neutralToPlayer is off. The probe's
     * blows are the same either way: what a dragon does with its target is Ice and Fire's own.
     */
    static void seek(ServerLevel level, ServerPlayer probe, Arm arm) {
        if (IafCommonConfig.INSTANCE.dragon.neutralToPlayer.getValue())
            return;
        for (DragonBaseEntity d : level.getEntitiesOfClass(DragonBaseEntity.class, probe.getBoundingBox().inflate(256))) {
            if (d.isTame() || d.lookingForRoostAIFlag || d.getTarget() != null || IafDragons.dead(d) || d.getRandom().nextInt(3) != 0)
                continue;
            double range = d.getAttributeValue(Attributes.FOLLOW_RANGE);
            if (!d.getBoundingBox().inflate(range).intersects(probe.getBoundingBox()))
                continue;
            if (!TargetingConditions.forCombat().range(range).ignoreLineOfSight().test(d, probe))
                continue;
            if (d.isSleeping() && d.distanceToSqr(probe) > 16.0)
                continue;
            d.setTarget(probe);
            arm.seeks++;
            arm.seekers.add(d.getUUID());
        }
    }

    private static void run(GameTestHelper helper, int siteNo) {
        String test = "og10Site" + siteNo;
        if (!Boolean.getBoolean("mic_rtsim.og10")) {
            record(test, "skipped", "OG-10 runs with -Pog10 (a quarter of an hour of server time)");
            helper.succeed();
            return;
        }
        if (!DragonKit.nativeEngine()) {
            record(test, "skipped", "on the stub: arm B is the engine's field flights");
            helper.succeed();
            return;
        }
        check("In Control is on the GameTest server", inControlPresent());
        watch();
        boolean oil = siteNo != 3;
        boolean pocket = siteNo == 2;
        ServerLevel level = helper.getLevel();
        BlockPos site = DragonKit.farSite();
        ChunkPos sc = new ChunkPos(site);
        Runnable unforce = DragonKit.forceChunks(level, sc, FORCE);
        Vec3 spot = new Vec3(site.getX() + 0.5, pocket ? POCKET_Y : FLOOR_Y + 1, site.getZ() + 0.5);
        ServerPlayer[] probe = {null};
        Husk[] standIn = {null};
        Arm a = new Arm("A"), b = new Arm("B");
        Arm[] current = {null};
        int[] ticks = {0};

        helper.onEachTick(() -> {
            beat = level.getGameTime();
            Husk h = standIn[0];
            if (h != null && !h.isRemoved() && h.position().distanceToSqr(spot) > 1e-4) {
                h.teleportTo(spot.x, spot.y, spot.z);
                h.setDeltaMovement(Vec3.ZERO);
            }
            Arm arm = current[0];
            if (arm == null || probe[0] == null)
                return;
            int t = ticks[0]++;
            if (t % 20 == 0)
                DragonKit.reportPlayer(probe[0]);
            if (t % 10 == 0)
                sample(level, spot, probe[0], h, arm);
            seek(level, probe[0], arm);
        });

        helper.startSequence()
                .thenWaitUntil(() -> check("the site's chunks loaded", DragonKit.ready(level, sc, FORCE)))
                .thenExecute(() -> {
                    long t0 = System.nanoTime();
                    desert(level, sc, FORCE);
                    floor(level, site);
                    if (pocket)
                        pocket(level, site);
                    if (oil) {
                        List<int[]> field = new ArrayList<>();
                        for (int dx = -1; dx <= 1; dx++)
                            for (int dz = -1; dz <= 1; dz++)
                                field.add(new int[] {sc.x + dx, sc.z + dz, 40_000});
                        DragonKit.oilChunks(level, field);
                    }
                    // Mock players a failed test left behind would share In Control's spawns.
                    List<ServerPlayer> stale = new ArrayList<>(level.players());
                    stale.forEach(OilGameTests::release);
                    probe[0] = OilGameTests.guardedPlayer(helper, spot, "og10-probe");
                    standIn[0] = standIn(level, spot);
                    liveProbe = probe[0];
                    liveStandIn = standIn[0];
                    record(test, "site", String.format(Locale.ROOT, "%s at %d, %d: probe at y %.0f, %s; dressed in %d ms; %d stale mock players let go",
                            siteNo == 1 ? "desert surface over oil" : siteNo == 2 ? "desert over oil, a sealed pocket below y 15" : "desert without oil",
                            site.getX(), site.getZ(), spot.y, oil ? "3 x 3 oil chunks of 40,000 mB reported" : "no oil reported",
                            (System.nanoTime() - t0) / 1_000_000, stale.size()));
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    startArm(level, a, "a", 0);
                    current[0] = a;
                })
                .thenIdle(ARM)
                .thenExecute(() -> {
                    endArm(level, a, probe[0], standIn[0]);
                    current[0] = null;
                    record(test, "arm A (the pack's In Control rules, the gate off)", a.line());
                    clearRules();
                    int swept = sweep(level, site, standIn[0]);
                    int fires = putOutFires(level, site);
                    record(test, "between arms", swept + " entities and " + fires + " fires cleared");
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    startArm(level, b, "b", DragonRecords.OilConfig.FLAG_FIELD_FLIGHTS);
                    current[0] = b;
                })
                .thenIdle(ARM)
                .thenExecute(() -> {
                    endArm(level, b, probe[0], standIn[0]);
                    current[0] = null;
                    record(test, "arm B (In Control without its fire dragons, field flights on)", b.line()
                            + String.format(Locale.ROOT, "; engine: %d field draws, %d refuels, %d visitors left, %d breaths",
                            b.draws, b.refuels, b.left, b.breaths));
                    clearRules();
                    DragonKit.restoreOilConfig();
                    sweep(level, site, null);
                    OilGameTests.release(probe[0]);
                    liveProbe = null;
                    liveStandIn = null;
                    unforce.run();
                    check("In Control spawned at the site in arm A (" + a.blazes.size() + " blazes)", !a.blazes.isEmpty());
                    check("In Control spawned at the site in arm B (" + b.blazes.size() + " blazes)", !b.blazes.isEmpty());
                    check("the probe was the level's only player (" + a.players + ", " + b.players + ")", a.players == 1 && b.players == 1);
                    record(test, "table", table(siteNo, a, b));
                    if (siteNo != 3)
                        record(test, "verdict", verdict(a, b));
                })
                .thenSucceed();
    }

    private static void startArm(ServerLevel level, Arm arm, String rules, int oilFlags) {
        try {
            inControl(rules);
        } catch (Exception e) {
            throw new IllegalStateException("could not load In Control's arm " + rules + " rules: " + e, e);
        }
        DragonKit.oilConfig(oilFlags);
        level.setDayTime(MORNING);
        long[] o = DragonKit.nativeOil(0);
        arm.drawsAt = o[NativeEngine.OIL_FIELD_DRAWS];
        arm.refuelsAt = o[NativeEngine.OIL_REFUELS];
        arm.leftAt = o[NativeEngine.OIL_VISITORS_LEFT];
        arm.breathsAt = o[NativeEngine.OIL_BREATHS];
        arm.start = level.getGameTime();
        arm.players = level.players().size();
    }

    private static void endArm(ServerLevel level, Arm arm, ServerPlayer probe, LivingEntity standIn) {
        arm.end = level.getGameTime();
        arm.players = Math.max(arm.players, level.players().size());
        long[] o = DragonKit.nativeOil(0);
        arm.draws = o[NativeEngine.OIL_FIELD_DRAWS] - arm.drawsAt;
        arm.refuels = o[NativeEngine.OIL_REFUELS] - arm.refuelsAt;
        arm.left = o[NativeEngine.OIL_VISITORS_LEFT] - arm.leftAt;
        arm.breaths = o[NativeEngine.OIL_BREATHS] - arm.breathsAt;
        List<Guard.Blow> blows = new ArrayList<>(Guard.blows(probe));
        blows.addAll(Guard.blows(standIn));
        blows.removeIf(x -> x.tick() < arm.start || x.tick() >= arm.end);
        blows.sort(Comparator.comparingLong(Guard.Blow::tick));
        double[] all = taken(blows.stream().filter(x -> x.attacker() != null).toList());
        double[] dragon = taken(blows.stream().filter(Guard.Blow::dragon).toList());
        arm.hostileRaw = all[0];
        arm.hostileEff = all[1];
        arm.dragonRaw = dragon[0];
        arm.dragonEff = dragon[1];
        for (Guard.Blow x : blows) {
            if (x.attacker() == null) {
                arm.underfoot += x.amount();
                continue;
            }
            arm.hostileHits++;
            if (arm.firstHostileHit < 0)
                arm.firstHostileHit = x.tick() - arm.start;
            if (x.dragon()) {
                arm.dragonHits++;
                arm.dragonsHitting.add(x.attacker());
                if (arm.firstDragonHit < 0)
                    arm.firstDragonHit = x.tick() - arm.start;
            }
        }
    }

    /** {raw, as taken}: a hit within {@link #HURT_WINDOW} ticks of the last one counts only what it adds. */
    static double[] taken(List<Guard.Blow> blows) {
        double raw = 0, eff = 0;
        long lastAt = Long.MIN_VALUE / 2;
        float last = 0;
        for (Guard.Blow x : blows) {
            raw += x.amount();
            if (x.tick() - lastAt >= HURT_WINDOW) {
                eff += x.amount();
                last = x.amount();
                lastAt = x.tick();
            } else if (x.amount() > last) {
                eff += x.amount() - last;
                last = x.amount();
            }
        }
        return new double[] {raw, eff};
    }

    private static void sample(ServerLevel level, Vec3 spot, ServerPlayer probe, LivingEntity standIn, Arm arm) {
        long t = level.getGameTime() - arm.start;
        AABB box = new AABB(spot, spot).inflate(FORCE * 16, 256, FORCE * 16);
        for (Mob m : level.getEntitiesOfClass(Mob.class, box)) {
            boolean dragon = m instanceof DragonBaseEntity;
            if (m instanceof Blaze)
                arm.blazes.add(m.getUUID());
            if (dragon && Math.hypot(m.getX() - spot.x, m.getZ() - spot.z) <= 100)
                arm.dragonsNear.add(m.getUUID());
            LivingEntity target = m.getTarget();
            boolean onProbe = target != null && (target == probe || target == standIn);
            if (!onProbe)
                continue;
            if (dragon) {
                arm.dragonsTargeting.add(m.getUUID());
                if (arm.firstTargetedByDragon < 0)
                    arm.firstTargetedByDragon = t;
            }
            if ((dragon || m instanceof Enemy) && arm.firstTargetedByHostile < 0)
                arm.firstTargetedByHostile = t;
        }
    }

    private static void clearRules() {
        try {
            inControl(null);
        } catch (Exception e) {
            throw new IllegalStateException("could not clear In Control's rules: " + e, e);
        }
    }

    /** Every chunk within {@code radius} of {@code centre} made desert, as /fillbiome does. */
    static void desert(ServerLevel level, ChunkPos centre, int radius) {
        Holder<Biome> desert = level.registryAccess().registryOrThrow(Registries.BIOME).getHolderOrThrow(Biomes.DESERT);
        for (int dx = -radius; dx <= radius; dx++)
            for (int dz = -radius; dz <= radius; dz++) {
                LevelChunk c = level.getChunk(centre.x + dx, centre.z + dz);
                c.fillBiomesFromNoise((x, y, z, sampler) -> desert, level.getChunkSource().randomState().sampler());
                c.setUnsaved(true);
            }
    }

    /** The sandstone floor at y 64 (sand would fall: the flat world is air below). */
    static void floor(ServerLevel level, BlockPos site) {
        BlockState s = Blocks.SANDSTONE.defaultBlockState();
        BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
        for (int dx = -FLOOR_R; dx <= FLOOR_R; dx++)
            for (int dz = -FLOOR_R; dz <= FLOOR_R; dz++)
                level.setBlock(p.set(site.getX() + dx, FLOOR_Y, site.getZ() + dz), s, 2);
    }

    /** Site 2's pocket: a 3 x 3 x 3 room in a 9 x 11 x 9 block of sandstone, the probe's feet at y 10. */
    static void pocket(ServerLevel level, BlockPos site) {
        BlockState s = Blocks.SANDSTONE.defaultBlockState();
        BlockState air = Blocks.AIR.defaultBlockState();
        BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
        for (int dx = -4; dx <= 4; dx++)
            for (int dy = -4; dy <= 6; dy++)
                for (int dz = -4; dz <= 4; dz++) {
                    boolean room = Math.abs(dx) <= 1 && Math.abs(dz) <= 1 && dy >= 0 && dy <= 2;
                    level.setBlock(p.set(site.getX() + dx, POCKET_Y + dy, site.getZ() + dz), room ? air : s, 2);
                }
    }

    /** The stand-in at the probe's feet: what projectiles and area fire hit. */
    static Husk standIn(ServerLevel level, Vec3 spot) {
        Husk h = EntityType.HUSK.create(level);
        h.moveTo(spot.x, spot.y, spot.z, 0, 0);
        h.setNoAi(true);
        h.setNoGravity(true);
        h.setSilent(true);
        h.setInvisible(true);
        h.setPersistenceRequired();
        h.addTag(STAND_IN);
        level.addFreshEntity(h);
        Guard.guard(h);
        return h;
    }

    /**
     * Every mob, projectile, item and orb near the site, except the stand-in and the engine's own dragons
     * (those leave with their chunks, as in the flights test). Returns how many went.
     */
    static int sweep(ServerLevel level, BlockPos site, Entity keep) {
        AABB box = new AABB(site).inflate(FORCE * 16 + 16, 400, FORCE * 16 + 16);
        List<Entity> gone = new ArrayList<>();
        for (Entity e : level.getEntities((Entity) null, box,
                e -> e instanceof Mob || e instanceof Projectile || e instanceof ItemEntity || e instanceof ExperienceOrb)) {
            if (e == keep)
                continue;
            if (keep == null && e.getTags().contains(STAND_IN)) {
                gone.add(e);
                continue;
            }
            if (e.getTags().contains(STAND_IN))
                continue;
            if (e instanceof DragonBaseEntity d && DragonKit.bridge().agentOf(d) != 0)
                continue;
            gone.add(e);
        }
        gone.forEach(Entity::discard);
        return gone.size();
    }

    /** Fire on the floor (fire ticks are off in GameTest worlds, so it never burns out). */
    static int putOutFires(ServerLevel level, BlockPos site) {
        int n = 0;
        BlockPos.MutableBlockPos p = new BlockPos.MutableBlockPos();
        for (int dx = -FLOOR_R; dx <= FLOOR_R; dx++)
            for (int dz = -FLOOR_R; dz <= FLOOR_R; dz++)
                for (int y = FLOOR_Y + 1; y <= FLOOR_Y + 2; y++)
                    if (level.getBlockState(p.set(site.getX() + dx, y, site.getZ() + dz)).getBlock() instanceof BaseFireBlock) {
                        level.setBlock(p, Blocks.AIR.defaultBlockState(), 2);
                        n++;
                    }
        return n;
    }

    static String table(int siteNo, Arm a, Arm b) {
        StringBuilder s = new StringBuilder("site " + siteNo + " | arm | dragon dmg/min | hostile dmg/min | first dragon hit | first hostile hit | "
                + "dragons hitting/h | dragons targeting or hitting/h | dragons within 100 | blazes");
        for (Arm x : List.of(a, b))
            s.append(String.format(Locale.ROOT, " || %s | %.1f | %.1f | %s | %s | %.0f | %.0f | %d | %d", x.name,
                    x.perMinute(x.dragonEff), x.perMinute(x.hostileEff), Arm.at(x.firstDragonHit), Arm.at(x.firstHostileHit),
                    x.perHour(x.dragonsHitting.size()), x.perHour(x.dragonsEngaged().size()), x.dragonsNear.size(), x.blazes.size()));
        return s.toString();
    }

    /** B at least as dangerous as A: on dragon damage, the time to the first dragon hit, and the dragons that hit. */
    static String verdict(Arm a, Arm b) {
        double da = a.perMinute(a.dragonEff), db = b.perMinute(b.dragonEff);
        boolean damage = db >= da;
        long ta = a.firstDragonHit < 0 ? Long.MAX_VALUE : a.firstDragonHit;
        long tb = b.firstDragonHit < 0 ? Long.MAX_VALUE : b.firstDragonHit;
        boolean contact = tb <= ta;
        boolean distinct = b.dragonsHitting.size() >= a.dragonsHitting.size();
        boolean byTargeting = b.dragonsEngaged().size() >= a.dragonsEngaged().size();
        return String.format(Locale.ROOT,
                "B >= A: %s (dragon damage %.1f vs %.1f per minute: %s; first dragon hit %s vs %s: %s; dragons hitting %d vs %d: %s); "
                        + "dragons targeting or hitting %d vs %d: %s",
                damage && contact && distinct ? "yes" : "no", db, da, damage ? "yes" : "no", Arm.at(b.firstDragonHit), Arm.at(a.firstDragonHit),
                contact ? "yes" : "no", b.dragonsHitting.size(), a.dragonsHitting.size(), distinct ? "yes" : "no",
                b.dragonsEngaged().size(), a.dragonsEngaged().size(), byTargeting ? "yes" : "no");
    }
}
