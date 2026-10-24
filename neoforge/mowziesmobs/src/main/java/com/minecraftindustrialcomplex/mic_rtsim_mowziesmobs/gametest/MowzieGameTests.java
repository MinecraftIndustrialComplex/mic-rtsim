package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthana;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaRaptor;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.MaskType;
import com.bobmowzie.mowziesmobs.server.world.feature.structure.FrostmawPieces;
import com.bobmowzie.mowziesmobs.server.world.feature.structure.UmvuthanaGrovePieces;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CensusBody;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ChunkMapInvoker;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ServerLevelAccessor;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.DenKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MowzieCamps;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Den;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Markers;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Difficulty;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Marker;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.pieces.PiecesContainer;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The camp reconciler on the jar's own bodies (MOWZIE.md §4.9, §8; docs/choices-mowzie-build.md, M5): a
 * Frostmaw's den and an Umvuthana grove, found, counted, kept home and played with the engine's
 * CAMP_RECONCILE lines, with Mowzie's Mobs itself on the GameTest server. Every test works in a far place
 * of its own (the world is reused) and declares its camps under a name no culture claims (MZB-227), so
 * the engine plays only the lines the test scripts. One batch each: the reconciler's counters are global.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class MowzieGameTests {
    private MowzieGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;
    private static final ResourceLocation FROSTMAW_TEMPLATE = ResourceLocation.fromNamespaceAndPath("mowziesmobs", "frostmaw_spawn");

    private static final int DONE = ConflictRecords.CampBodies.DONE, NOTHING = ConflictRecords.CampBodies.NOTHING,
            STALE = ConflictRecords.CampBodies.STALE;
    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN, ADOPT = ConflictRecords.CampReconcile.ADOPT,
            TAKE = ConflictRecords.CampReconcile.TAKE, SET_OUT = ConflictRecords.CampReconcile.SET_OUT,
            HAND_BACK = ConflictRecords.CampReconcile.HAND_BACK, REMOVE = ConflictRecords.CampReconcile.REMOVE;
    private static final int CHIEF = ConflictRecords.CampReconcile.LINE_CHIEF, CRYSTAL = ConflictRecords.CampReconcile.LINE_CRYSTAL;

    // ---- 1: a den ---------------------------------------------------------------------------

    /**
     * Four real Frostmaw den pieces, one per rotation, each placed as worldgen places it at the middle of
     * its own chunk (§7 test 1). The template's Frostmaw stands within 1.5 blocks of the den's spot
     * (MZB-217) and stays the jar's own: no puppet, untagged. The den is declared from a real structure
     * start; its FACTION_FOUND comes once with one body at home and the census flags the Frostmaw has (its
     * crystal as set), and its CAMP_CENSUS lists it as a stray chief with a health between what was set and
     * what it has now (it heals asleep). ADOPT makes it the camp's body (slot 0, generation 1) where it
     * stands: restricted 8 round the spot, still no puppet, still saved, its persistence as the jar left it
     * (MZB-210), and its saved NBT carries the camp tag, so a world without rtsim loads it as the jar's own.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_den", skyAccess = true)
    public static void aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth(GameTestHelper helper) {
        String test = "aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int n = 4, cz = far.z0;
        int[] cx = new int[n];
        for (int i = 0; i < n; i++)
            cx[i] = far.k + 4 * i;
        Rotation[] rotation = Rotation.values();
        boolean[] crystal = {true, false, true, false};
        float[] set = new float[n];
        EntityFrostmaw[] frost = new EntityFrostmaw[n];
        boolean[] persistent = new boolean[n];
        long[] faction = new long[n], op = new long[n];

        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++)
                        far.forceRect(cx[i] - 1, cz - 1, cx[i] + 1, cz + 1);
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    Structure structure = level.registryAccess().registryOrThrow(Registries.STRUCTURE)
                            .get(ResourceLocation.parse(MicRtsimMowziesMobs.DEN_STRUCTURE));
                    for (int i = 0; i < n; i++) {
                        int px = cx[i] * 16 + 8, pz = cz * 16 + 8;
                        MowzieKit.platform(level, (cx[i] - 1) * 16, (cx[i] + 1) * 16 + 15, (cz - 1) * 16, (cz + 1) * 16 + 15);
                        int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, px, pz);
                        FrostmawPieces.FrostmawPiece piece = new FrostmawPieces.FrostmawPiece(level.getStructureManager(), FROSTMAW_TEMPLATE,
                                new BlockPos(px, y, pz), rotation[i]);
                        ChunkPos chunk = new ChunkPos(cx[i], cz);
                        BlockPos pivot = new BlockPos(chunk.getMiddleBlockX(), piece.getBoundingBox().minY(), chunk.getMiddleBlockZ());
                        piece.postProcess(level, level.structureManager(), level.getChunkSource().getGenerator(), level.random,
                                piece.getBoundingBox(), chunk, pivot);
                        BoundingBox box = piece.getBoundingBox();
                        List<EntityFrostmaw> fs = level.getEntitiesOfClass(EntityFrostmaw.class, AABB.of(box).inflate(3));
                        check("den " + i + " (" + rotation[i] + ") made one Frostmaw from its template (" + fs.size() + ")", fs.size() == 1);
                        EntityFrostmaw f = fs.get(0);
                        Vec3 spot = MowzieCamps.denSpot(box);
                        double off = Math.hypot(f.getX() - spot.x, f.getZ() - spot.z);
                        record(test, "den " + i + " " + rotation[i], "box " + box + ", the Frostmaw " + String.format("%.2f", off) + " blocks from the spot");
                        check("den " + i + "'s Frostmaw stands within 1.5 blocks of the spot (" + off + ")", off <= 1.5);
                        check("and is the jar's own: no puppet, untagged", !RtsimApi.isPuppet(f) && !RtsimAttachments.isTagged(f) && MowzieKit.tag(f) == null);
                        f.setHasCrystal(crystal[i]);
                        f.setHealth(Math.min(60f + 30f * i, f.getMaxHealth() - 5f));
                        set[i] = f.getHealth();
                        persistent[i] = f.isPersistenceRequired();
                        frost[i] = f;
                        StructureStart start = new StructureStart(structure, chunk, 0, new PiecesContainer(List.of(piece)));
                        faction[i] = MowzieCamps.declareDen(level, start, false, MowzieKit.TEST_DEN);
                        check("den " + i + " is declared", faction[i] != 0);
                    }
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < n; i++)
                        check("den " + i + "'s census", MowzieKit.censusCount(faction[i]) >= 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++) {
                        List<ScriptedEngine.Received> found = ConflictKit.factionsFound(faction[i]);
                        check("den " + i + ": FACTION_FOUND once (" + found.size() + ")", found.size() == 1);
                        ByteBuffer fb = found.get(0).view();
                        check("home bodies 1 (" + (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) + ")",
                                (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == 1);
                        int want = ConflictRecords.FactionFound.CENSUS_CHIEF | (crystal[i] ? ConflictRecords.FactionFound.CENSUS_CRYSTAL : 0);
                        check("den " + i + ": found with the chief" + (crystal[i] ? " and its crystal" : " and no crystal"),
                                (fb.get(ConflictRecords.FactionFound.CENSUS) & 0xFF) == want);
                        ScriptedEngine.Received c = MowzieKit.census(faction[i], 1);
                        int wantFlags = ConflictRecords.CampCensus.CHIEF | (crystal[i] ? ConflictRecords.CampCensus.CRYSTAL : 0);
                        check("den " + i + ": the census's chief flags", MowzieKit.chiefFlags(c) == wantFlags);
                        List<CensusBody> bodies = ConflictKit.censusBodies(c);
                        check("den " + i + ": one body (" + bodies.size() + ")", bodies.size() == 1);
                        CensusBody b = bodies.get(0);
                        check("it is the Frostmaw", b.body() == ConflictKit.bodyId(MowzieKit.FROSTMAW) && b.uuid().equals(frost[i].getUUID()));
                        check("a stray chief (" + b.flags() + ")",
                                b.flags() == (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF));
                        float h = MowzieKit.chiefHealth(c);
                        record(test, "den " + i + " health", "set " + set[i] + ", census " + h + ", now " + frost[i].getHealth());
                        check("its health reads between what was set and what it has now (" + h + ")",
                                h >= set[i] - 0.01f && h <= frost[i].getHealth() + 0.01f);
                        check("it is still no puppet and untagged", !RtsimApi.isPuppet(frost[i]) && MowzieKit.tag(frost[i]) == null);
                        op[i] = MowzieKit.reconcile(faction[i], MowzieKit.epoch(c), List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF,
                                frost[i].getUUID())));
                    }
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < n; i++)
                        MowzieKit.answered(faction[i], op[i], 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++) {
                        MowzieKit.allOutcomes("den " + i + " ADOPT", ConflictKit.outcomes(faction[i], op[i]), DONE);
                        EntityFrostmaw f = frost[i];
                        Camps.Site site = MowzieKit.site(faction[i]);
                        check("den " + i + ": tagged (faction, 0, 1)", MowzieKit.tagged(f, faction[i], 0, 1));
                        check("a body a camp holds", RtsimApi.isCampBody(f));
                        check("restricted " + DenKind.RESTRICT + " round the spot", f.hasRestriction() && f.getRestrictRadius() == DenKind.RESTRICT
                                && f.getRestrictCenter().equals(BlockPos.containing(site.spot)));
                        check("still no puppet", !RtsimApi.isPuppet(f) && !RtsimAttachments.isTagged(f));
                        check("still saved with its chunk", f.shouldBeSaved());
                        check("its persistence is the jar's (" + persistent[i] + ")", f.isPersistenceRequired() == persistent[i]);
                        CompoundTag nbt = new CompoundTag();
                        f.saveWithoutId(nbt);
                        check("its NBT carries the camp tag", nbt.getCompound("neoforge:attachments").contains("mic_rtsim:camp_member"));
                        check("and the tag names this camp, slot 0, generation 1", nbt.getCompound("neoforge:attachments").getCompound("mic_rtsim:camp_member")
                                .getLong("faction") == faction[i]);
                    }
                    record(test, "adopted", "four Frostmaws, one per rotation, tagged and restricted; the tag is in their saved NBT");
                })
                .thenSucceed();
    }

    // ---- 2: a grove -------------------------------------------------------------------------

    /**
     * A real Umvuthana fire pit piece (rotation at random) at the middle of a chunk (§7 test 3): every
     * warrior it makes is restricted 25 round {@code MowzieCamps.groveCentre} (MZB-218). With a warrior
     * added inside, a raptor inside, a no-AI warrior 27 blocks out and a no-AI Umvuthi (direction 2), the
     * census lists exactly the warriors inside, all strays: no raptor, no warrior outside, no Umvuthi body;
     * its chief fields are the Umvuthi's (CHIEF and THRONE, its position, direction 2), and FACTION_FOUND
     * says how many are at home and that a chief is. ADOPT of all of them in one reconcile is DONE for each:
     * tagged by slot, restricted again. The footprint released and forced again is CAMP_OUT then a second
     * census listing every adopted body by UUID with its slot (not as a stray); FACTION_FOUND stays once.
     * The session reset that recounts a grove is MZB-228's gap.
     */
    @GameTest(template = ARENA, timeoutTicks = 2000, batch = "mowzie_grove", skyAccess = true)
    public static void aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt(GameTestHelper helper) {
        String test = "aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        Rotation rotation = Rotation.getRandom(level.random);
        Set<UUID> inside = new HashSet<>();
        UUID[] adopted = new UUID[0];
        List<UUID> order = new ArrayList<>();
        Entity[] extra = new Entity[4]; // warrior inside, raptor, warrior out, Umvuthi
        BlockPos[] centre = new BlockPos[1];
        long[] op = new long[2];
        long[] e1 = new long[1];
        float[] chiefHealth = new float[1];

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay();
                    Structure structure = level.registryAccess().registryOrThrow(Registries.STRUCTURE)
                            .get(ResourceLocation.parse(MicRtsimMowziesMobs.GROVE_STRUCTURE));
                    int px = g.cx * 16 + 8, pz = g.cz * 16 + 8;
                    int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, px, pz);
                    // The piece's centre is (minX + 4, maxZ - 4) of its box: shift it onto the chunk's middle.
                    UmvuthanaGrovePieces.FirepitPiece probe = new UmvuthanaGrovePieces.FirepitPiece(level.getStructureManager(), rotation,
                            new BlockPos(px, y, pz));
                    BoundingBox pb = probe.getBoundingBox();
                    int dx = px - (pb.minX() + 4), dz = pz - (pb.maxZ() - 4);
                    UmvuthanaGrovePieces.FirepitPiece piece = new UmvuthanaGrovePieces.FirepitPiece(level.getStructureManager(), rotation,
                            new BlockPos(px + dx, y, pz + dz));
                    BoundingBox box = piece.getBoundingBox();
                    check("the fire pit's centre is the chunk's middle", box.minX() + 4 == px && box.maxZ() - 4 == pz);
                    ChunkPos chunk = new ChunkPos(g.cx, g.cz);
                    BlockPos pivot = new BlockPos(chunk.getMiddleBlockX(), box.minY(), chunk.getMiddleBlockZ());
                    piece.postProcess(level, level.structureManager(), level.getChunkSource().getGenerator(), level.random, box, chunk, pivot);
                    centre[0] = MowzieCamps.groveCentre(box);
                    List<EntityUmvuthanaMinion> made = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area());
                    record(test, "fire pit", rotation + ", box " + piece.getBoundingBox() + ", centre " + centre[0] + ", " + made.size() + " warriors made");
                    check("the fire pit made warriors (" + made.size() + ")", made.size() >= 3 && made.size() <= 9);
                    for (EntityUmvuthanaMinion m : made) {
                        // the jar restricts to the surface block there; the centre is from the piece's box (MZB-242)
                        BlockPos rc = m.getRestrictCenter();
                        check("a warrior it made is restricted 25 round the grove's centre " + centre[0] + " (" + rc + ", "
                                + m.getRestrictRadius() + ")", m.hasRestriction() && m.getRestrictRadius() == GroveKind.RESTRICT
                                && rc.getX() == centre[0].getX() && rc.getZ() == centre[0].getZ() && Math.abs(rc.getY() - centre[0].getY()) <= 4);
                        m.setNoAi(true);
                    }
                    // the grove's spot is not declared yet: g.spot is null, so place from the centre block
                    extra[0] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_MINION.get(), centre[0].getX() - 7.5, y, centre[0].getZ() + 6.5, true);
                    extra[1] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_RAPTOR.get(), centre[0].getX() + 8.5, y, centre[0].getZ() - 5.5, true, false);
                    extra[2] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_MINION.get(), centre[0].getX() + 27.5, y, centre[0].getZ() + 0.5, true);
                    EntityUmvuthi u = MowzieKit.stray(level, EntityHandler.UMVUTHI.get(), centre[0].getX() + 12.5, y, centre[0].getZ() + 0.5, true, false);
                    u.setDirection(2);
                    extra[3] = u;
                    chiefHealth[0] = u.getHealth();
                    StructureStart start = new StructureStart(structure, chunk, 0, new PiecesContainer(List.of(piece)));
                    g.faction = MowzieCamps.declareGrove(level, start, false, MowzieKit.TEST_GROVE);
                    check("the grove is declared from its real structure start", g.faction != 0);
                    g.spot = g.site().spot;
                    check("its spot is the centre block's middle " + g.spot, g.spot.x == centre[0].getX() + 0.5 && g.spot.z == centre[0].getZ() + 0.5);
                    check("the 27-block warrior is outside the footprint", !g.site().contains(extra[2].position()));
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(g.faction) >= 1))
                .thenExecute(() -> {
                    Camps.Site site = g.site();
                    List<EntityUmvuthanaMinion> all = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area());
                    Set<UUID> want = new HashSet<>();
                    for (EntityUmvuthanaMinion m : all)
                        if (m.isAlive() && site.contains(m.position()))
                            want.add(m.getUUID());
                    check("the warrior added inside is counted among them", want.contains(extra[0].getUUID()));
                    ScriptedEngine.Received c = MowzieKit.census(g.faction, 1);
                    List<CensusBody> bodies = ConflictKit.censusBodies(c);
                    Set<UUID> got = new HashSet<>();
                    for (CensusBody b : bodies) {
                        got.add(b.uuid());
                        check("a warrior of the grove, a stray (" + b.flags() + ")", b.body() == ConflictKit.bodyId(MowzieKit.WARRIOR)
                                && b.flags() == ConflictRecords.CampCensus.BODY_STRAY);
                    }
                    record(test, "census 1", bodies.size() + " warriors of " + all.size() + " standing near; chief health " + MowzieKit.chiefHealth(c));
                    check("exactly the warriors inside: " + got.size() + " listed, " + want.size() + " inside", got.equals(want) && bodies.size() == want.size());
                    check("no raptor", !got.contains(extra[1].getUUID()));
                    check("no warrior 27 blocks out", !got.contains(extra[2].getUUID()));
                    check("no Umvuthi body", !got.contains(extra[3].getUUID()));
                    check("chief flags CHIEF and THRONE (" + MowzieKit.chiefFlags(c) + ")",
                            MowzieKit.chiefFlags(c) == (ConflictRecords.CampCensus.CHIEF | ConflictRecords.CampCensus.THRONE));
                    ByteBuffer cb = c.view();
                    check("the throne is the Umvuthi's position", cb.getDouble(ConflictRecords.CampCensus.THRONE_X) == extra[3].getX()
                            && cb.getDouble(ConflictRecords.CampCensus.THRONE_Y) == extra[3].getY()
                            && cb.getDouble(ConflictRecords.CampCensus.THRONE_Z) == extra[3].getZ());
                    check("its direction 2", cb.getFloat(ConflictRecords.CampCensus.DIRECTION) == 2f);
                    check("its health", Math.abs(MowzieKit.chiefHealth(c) - chiefHealth[0]) < 0.01f);
                    List<ScriptedEngine.Received> found = ConflictKit.factionsFound(g.faction);
                    check("FACTION_FOUND once (" + found.size() + ")", found.size() == 1);
                    ByteBuffer fb = found.get(0).view();
                    check("home " + want.size() + " (" + (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) + ")",
                            (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == want.size());
                    check("found with the chief, no crystal", (fb.get(ConflictRecords.FactionFound.CENSUS) & 0xFF) == ConflictRecords.FactionFound.CENSUS_CHIEF);
                    e1[0] = MowzieKit.epoch(c);
                    List<CampLine> lines = new ArrayList<>();
                    for (CensusBody b : bodies) {
                        lines.add(CampLine.of(order.size(), 1, MowzieKit.WARRIOR, ADOPT, 0, b.uuid()));
                        order.add(b.uuid());
                    }
                    op[0] = MowzieKit.reconcile(g.faction, e1[0], lines);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[0], order.size()))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    BlockPos spotBlock = BlockPos.containing(g.spot);
                    for (int i = 0; i < order.size(); i++) {
                        Entity e = level.getEntity(order.get(i));
                        check("warrior " + i + " is in the level", e instanceof EntityUmvuthanaMinion);
                        EntityUmvuthanaMinion m = (EntityUmvuthanaMinion) e;
                        check("tagged (faction, " + i + ", 1)", MowzieKit.tagged(m, g.faction, i, 1));
                        check("restricted 25 round the spot", m.hasRestriction() && m.getRestrictRadius() == GroveKind.RESTRICT
                                && m.getRestrictCenter().equals(spotBlock));
                        check("no puppet", !RtsimApi.isPuppet(m));
                    }
                    // the footprint out and in
                    far.releaseRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(g.faction).size() == 1))
                .thenWaitUntil(io(() -> {
                    for (UUID id : order)
                        check("warrior " + id + " is unloaded", level.getEntity(id) == null);
                }))
                .thenExecute(() -> {
                    check("one census so far", MowzieKit.censusCount(g.faction) == 1);
                    g.force();
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(g.faction) >= 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(g.faction, 2);
                    check("a new epoch", MowzieKit.epoch(c) != e1[0]);
                    List<CensusBody> bodies = ConflictKit.censusBodies(c);
                    check("every adopted warrior is listed and no other (" + bodies.size() + " of " + order.size() + ")", bodies.size() == order.size());
                    for (int i = 0; i < order.size(); i++) {
                        CensusBody b = MowzieKit.bodyOf(c, order.get(i));
                        check("warrior " + i + " is listed", b != null);
                        check("by its slot " + i + ", generation 1 (" + b.slot() + ", " + b.generation() + ")", b.slot() == i && b.generation() == 1);
                        check("not as a stray or a chief (" + b.flags() + ")",
                                (b.flags() & (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF)) == 0);
                    }
                    check("FACTION_FOUND is still once", ConflictKit.factionsFound(g.faction).size() == 1);
                    record(test, "out and in", "census 2 lists the " + order.size() + " adopted warriors by slot; the session reset itself has no GameTest (MZB-228)");
                })
                .thenSucceed();
    }

    // ---- review fixes: the den's footprint (MZB-230, MZB-232) --------------------------------

    /**
     * The jar's Frostmaw falls asleep where it stands inside its restriction (EntityFrostmaw.java:606-616),
     * so it rests 5 to 8 blocks from the den's spot as often as under the template's box (MZB-230). Placed
     * there, asleep and untagged, it is in the den's cylinder: the first census lists it home as a stray
     * chief with its crystal, FACTION_FOUND counts it, and ADOPT takes it. Moved 13 blocks out, beyond the
     * cylinder but in a forced chunk, and the den cycled out and in, the next census lists the tagged Frostmaw
     * apart as BODY_ELSEWHERE and BODY_CHIEF, and the chief fields still read it (it is in the faction's
     * index): CHIEF and CRYSTAL with its health (MZB-232).
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_den_asleep_away", skyAccess = true)
    public static void aFrostmawAsleepAwayFromItsSpotIsTheDensBodyAndAnIndexedOneOutsideStillAnswersAsChief(GameTestHelper helper) {
        String test = "aFrostmawAsleepAwayFromItsSpotIsTheDensBodyAndAnIndexedOneOutsideStillAnswersAsChief";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        long[] op = new long[1], e1 = new long[1];
        float[] health = new float[1];

        helper.startSequence()
                .thenExecute(() -> den.force())
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(6, 4.5), true);
                    f[0].setHealth(f[0].getMaxHealth() - 40f);
                    health[0] = f[0].getHealth();
                    double off = Math.hypot(f[0].getX() - den.spot.x, f[0].getZ() - den.spot.z);
                    record(test, "placed", String.format("the Frostmaw asleep %.2f blocks from the spot", off));
                    check("between 5 and 8 blocks from the spot (" + off + ")", off >= 5 && off <= 8);
                    check("in the den's cylinder", den.site().contains(f[0].position()));
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(den.faction, 1);
                    List<CensusBody> bodies = ConflictKit.censusBodies(c);
                    check("one body, the Frostmaw, a stray chief (" + bodies.size() + ")", bodies.size() == 1
                            && bodies.get(0).uuid().equals(f[0].getUUID())
                            && bodies.get(0).flags() == (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF));
                    check("the chief flags CHIEF and CRYSTAL (" + MowzieKit.chiefFlags(c) + ")",
                            MowzieKit.chiefFlags(c) == (ConflictRecords.CampCensus.CHIEF | ConflictRecords.CampCensus.CRYSTAL));
                    List<ScriptedEngine.Received> found = ConflictKit.factionsFound(den.faction);
                    check("FACTION_FOUND once, home 1 (" + found.size() + ")", found.size() == 1
                            && (found.get(0).view().getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == 1);
                    e1[0] = MowzieKit.epoch(c);
                    op[0] = MowzieKit.reconcile(den.faction, e1[0], List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID())));
                })
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("tagged (faction, 0, 1)", MowzieKit.tagged(f[0], den.faction, 0, 1));
                    // beyond the cylinder (13 > 11), in the forced chunk east of the den's
                    Vec3 out = den.at(13, 0);
                    f[0].moveTo(out.x, out.y, out.z, f[0].getYRot(), 0f);
                    check("now outside the footprint", !den.site().contains(f[0].position()));
                    den.release();
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenWaitUntil(io(() -> check("the Frostmaw is unloaded", level.getEntity(f[0].getUUID()) == null)))
                .thenExecute(() -> den.force())
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(den.faction, 2);
                    CensusBody b = MowzieKit.bodyOf(c, f[0].getUUID());
                    check("the Frostmaw is listed", b != null);
                    check("apart, as the chief (" + b.flags() + ")",
                            b.flags() == (ConflictRecords.CampCensus.BODY_ELSEWHERE | ConflictRecords.CampCensus.BODY_CHIEF));
                    check("by its slot 0, generation 1", b.slot() == 0 && b.generation() == 1);
                    check("the chief fields still read it: CHIEF and CRYSTAL (" + MowzieKit.chiefFlags(c) + ")",
                            MowzieKit.chiefFlags(c) == (ConflictRecords.CampCensus.CHIEF | ConflictRecords.CampCensus.CRYSTAL));
                    float h = MowzieKit.chiefHealth(c);
                    record(test, "elsewhere", "chief health " + h + " (set " + health[0] + ")");
                    check("with its health (" + h + ")", h >= health[0] - 0.01f && h <= health[0] + 20f);
                    check("FACTION_FOUND is still once", ConflictKit.factionsFound(den.faction).size() == 1);
                })
                .thenSucceed();
    }

    /**
     * A late den (found in a world from before the plugin) looks for its Frostmaw at its first census within
     * {@code DenKind.STRAY_RADIUS} blocks of the spot, loaded and untagged (MZB-232): one that wandered 30
     * blocks off is listed as a stray chief, FACTION_FOUND counts it home, and ADOPT takes it, so the den keeps
     * its one Frostmaw and the pool has nothing to spawn. A den that is not late, with its Frostmaw as far,
     * lists none: the jar's own is outside its books.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_den_late", skyAccess = true)
    public static void aLateDensFrostmawThatWanderedOffIsFoundAndAdoptedAtTheFirstCensus(GameTestHelper helper) {
        String test = "aLateDensFrostmawThatWanderedOffIsFoundAndAdoptedAtTheFirstCensus";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den late = new Den(far, far.k, far.z0), plain = new Den(far, far.k + 8, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[2];
        long[] op = new long[1], strays = new long[1];

        helper.startSequence()
                .thenExecute(() -> {
                    late.force(1);
                    plain.force(1);
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    late.lay(1);
                    plain.lay(1);
                    // both Frostmaws stand before either den is declared: nothing asked for a recount
                    Vec3 a = late.at(30, 0.5), b = plain.at(30, 0.5);
                    f[0] = MowzieKit.frostmaw(level, a, true);
                    f[1] = MowzieKit.frostmaw(level, b, false);
                    strays[0] = MowzieKit.camps().strays;
                    late.declare(true);
                    plain.declare(false);
                    check("the first is late and the second is not", late.site().late() && !plain.site().late());
                    check("each Frostmaw is beyond its den's cylinder", !late.site().contains(f[0].position()) && !plain.site().contains(f[1].position()));
                })
                .thenWaitUntil(() -> {
                    check("census of the late den", MowzieKit.censusCount(late.faction) >= 1);
                    check("census of the plain den", MowzieKit.censusCount(plain.faction) >= 1);
                })
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(late.faction, 1);
                    List<CensusBody> bodies = ConflictKit.censusBodies(c);
                    check("the late den lists its wandered Frostmaw as a stray chief (" + bodies.size() + ")", bodies.size() == 1
                            && bodies.get(0).uuid().equals(f[0].getUUID())
                            && bodies.get(0).flags() == (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF));
                    check("with the chief flags CHIEF and CRYSTAL", MowzieKit.chiefFlags(c) == (ConflictRecords.CampCensus.CHIEF | ConflictRecords.CampCensus.CRYSTAL));
                    List<ScriptedEngine.Received> found = ConflictKit.factionsFound(late.faction);
                    check("FACTION_FOUND home 1 with the chief and its crystal", found.size() == 1
                            && (found.get(0).view().getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == 1
                            && (found.get(0).view().get(ConflictRecords.FactionFound.CENSUS) & 0xFF)
                            == (ConflictRecords.FactionFound.CENSUS_CHIEF | ConflictRecords.FactionFound.CENSUS_CRYSTAL));
                    check("and says it is late (aux " + found.get(0).aux() + ")", found.get(0).aux() == ConflictRecords.FactionFound.FLAG_LATE);
                    check("one stray counted (" + (MowzieKit.camps().strays - strays[0]) + ")", MowzieKit.camps().strays - strays[0] == 1);
                    ScriptedEngine.Received p = MowzieKit.census(plain.faction, 1);
                    check("the plain den lists nothing", p.aux() == 0 && MowzieKit.chiefFlags(p) == 0);
                    List<ScriptedEngine.Received> pf = ConflictKit.factionsFound(plain.faction);
                    check("and was found with no one home", pf.size() == 1
                            && (pf.get(0).view().getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == 0
                            && (pf.get(0).view().get(ConflictRecords.FactionFound.CENSUS) & 0xFF) == 0);
                    op[0] = MowzieKit.reconcile(late.faction, MowzieKit.epoch(c),
                            List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID())));
                })
                .thenWaitUntil(() -> MowzieKit.answered(late.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(late.faction, op[0]), DONE);
                    check("tagged (faction, 0, 1) and restricted 8 round the spot", MowzieKit.tagged(f[0], late.faction, 0, 1)
                            && f[0].hasRestriction() && f[0].getRestrictRadius() == DenKind.RESTRICT);
                    check("the den holds that one Frostmaw and no second was made", MowzieKit.held(late.faction).size() == 1
                            && level.getEntitiesOfClass(EntityFrostmaw.class, late.area().expandTowards(64, 0, 0).expandTowards(-64, 0, 0)).size() == 1);
                    check("the plain den's Frostmaw is still the jar's own", MowzieKit.tag(f[1]) == null);
                    record(test, "late", "the Frostmaw 30 blocks off was listed at census 1 and adopted; the plain den's was not listed");
                })
                .thenSucceed();
    }

    /**
     * An untagged Frostmaw that joins within {@code STRAY_RADIUS} of a den with a free slot asks for a recount
     * (MZB-232): the next census lists it as a stray chief and ADOPT takes it. With the slot taken, a second
     * Frostmaw joining as near asks for nothing: no recount, no census, and it stays the jar's.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_den_joiner", skyAccess = true)
    public static void aFrostmawJoiningNearADenWithAFreeSlotAsksForARecountAndOneJoiningAtAFullDenDoesNot(GameTestHelper helper) {
        String test = "aFrostmawJoiningNearADenWithAFreeSlotAsksForARecountAndOneJoiningAtAFullDenDoesNot";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[2];
        long[] op = new long[1], watches = new long[1];

        helper.startSequence()
                .thenExecute(() -> den.force(1))
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay(1).declare();
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(den.faction, 1);
                    check("census 1 lists no one", c.aux() == 0 && MowzieKit.chiefFlags(c) == 0);
                    watches[0] = MowzieKit.camps().strayWatches;
                    f[0] = MowzieKit.frostmaw(level, den.at(30, 0.5), true);
                    check("the join was noticed (" + (MowzieKit.camps().strayWatches - watches[0]) + ")", MowzieKit.camps().strayWatches == watches[0] + 1);
                })
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(den.faction, 2);
                    List<CensusBody> bodies = ConflictKit.censusBodies(c);
                    check("census 2 lists the joiner as a stray chief (" + bodies.size() + ")", bodies.size() == 1
                            && bodies.get(0).uuid().equals(f[0].getUUID())
                            && bodies.get(0).flags() == (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF));
                    op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(c), List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID())));
                })
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    watches[0] = MowzieKit.camps().strayWatches;
                    f[1] = MowzieKit.frostmaw(level, den.at(30, 5.5), true);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("a full den watched nothing (" + (MowzieKit.camps().strayWatches - watches[0]) + ")", MowzieKit.camps().strayWatches == watches[0]);
                    check("and made no census (" + MowzieKit.censusCount(den.faction) + ")", MowzieKit.censusCount(den.faction) == 2);
                    check("the second Frostmaw is the jar's own", MowzieKit.tag(f[1]) == null && f[1].isAlive());
                    record(test, "joiners", "a join with a free slot recounted; a join at a full den did not");
                })
                .thenSucceed();
    }

    // ---- review fixes: the index, the play-time check ---------------------------------------------

    /**
     * A den's Frostmaw, adopted, whose chunk's entities go hidden and are tracked again within the tick (the
     * flicker: the leave is posted with no removal reason, and the re-promotion posts no join) keeps its place in
     * the camp's index (MZB-231): a TAKE of its slot and generation, by tag, is DONE, the body goes and its record
     * rides on a marker. The leave was seen with no removal reason and the body was never removed.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_flicker", skyAccess = true)
    public static void aBodyKeepsItsIndexEntryThroughAHiddenAndTrackedFlickerAndIsStillTaken(GameTestHelper helper) {
        String test = "aBodyKeepsItsIndexEntryThroughAHiddenAndTrackedFlickerAndIsStillTaken";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        long[] op = new long[2];
        int[][] leaves = new int[1][];
        Markers[] mk = new Markers[1];
        long[] taken = new long[1], markers = new long[1];

        helper.startSequence()
                .thenExecute(() -> den.force())
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID()))))
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("held by the camp", MowzieKit.camps().held(den.faction).contains(f[0]));
                    mk[0] = new Markers(helper, level, den.area());
                    leaves[0] = MowzieKit.watchLeaves(helper, f[0]);
                    taken[0] = MowzieKit.camps().taken;
                    markers[0] = MowzieKit.camps().markers;
                    // the flicker: hidden, then tracked and ticking again, in one tick
                    var manager = ((ServerLevelAccessor) level).mic_rtsim$entityManager();
                    ChunkPos cp = f[0].chunkPosition();
                    manager.updateChunkStatus(cp, FullChunkStatus.INACCESSIBLE);
                    check("the leave was posted with no removal reason (" + leaves[0][0] + ", " + leaves[0][1] + ")", leaves[0][0] == 1 && leaves[0][1] == 0);
                    check("the Frostmaw is alive and not removed", f[0].isAlive() && !f[0].isRemoved());
                    manager.updateChunkStatus(cp, FullChunkStatus.ENTITY_TICKING);
                    check("and the camp still holds it", MowzieKit.camps().held(den.faction).contains(f[0]));
                    check("no join came back for it (still one leave: " + leaves[0][0] + ")", leaves[0][0] == 1);
                    op[1] = MowzieKit.reconcile(den.faction, den.site().epoch(), List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, TAKE, CHIEF)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[1], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("TAKE by tag", ConflictKit.outcomes(den.faction, op[1]), DONE);
                    check("the Frostmaw is gone", f[0].isRemoved());
                    check("one taken (" + (MowzieKit.camps().taken - taken[0]) + ")", MowzieKit.camps().taken == taken[0] + 1);
                    check("a marker was made (" + (MowzieKit.camps().markers - markers[0]) + ")", MowzieKit.camps().markers == markers[0] + 1);
                    check("the camp holds nothing", MowzieKit.held(den.faction).isEmpty());
                    record(test, "flicker", "hidden then tracked in one tick: the leave carried no removal reason, the index kept the body, TAKE by tag was DONE");
                })
                .thenSucceed();
    }

    /**
     * A line that arrives in the tick its camp's footprint went down is answered STALE (MZB-233): the session
     * runs the engine's commands before the camps are looked at, so the camp is still "in" when the line lands.
     * The den's 3 x 3 chunks are released; the step that sees a footprint level above FULL sends a
     * CAMP_RECONCILE with a TAKE of the adopted Frostmaw, a SPAWN and a SET_OUT in that same tick, and all
     * three are STALE: the Frostmaw is still there and tagged, nothing was taken, spawned or set out, and no
     * marker was made. Forced again, the next census lists the Frostmaw by its slot.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_play_time", skyAccess = true)
    public static void aLineArrivingInTheTickACampGoesOutIsAnsweredStale(GameTestHelper helper) {
        String test = "aLineArrivingInTheTickACampGoesOutIsAnsweredStale";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        long[] op = new long[2];
        long[] was = new long[6];

        helper.startSequence()
                .thenExecute(() -> den.force())
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID()))))
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("the camp is in", den.site().in());
                    den.release();
                })
                .thenWaitUntil(() -> {
                    // The holder's own level from the chunk map, not the camp's cached one (MZB-592): under C2ME the
                    // camp's levels move only in the poll inside Camps.tick, which takes the camp out in that same tick.
                    ChunkMapInvoker map = (ChunkMapInvoker) level.getChunkSource().chunkMap;
                    int top = 0;
                    for (ChunkPos p : den.site().footprintChunks()) {
                        ChunkHolder h = map.mic_rtsim$updating(p.toLong());
                        top = Math.max(top, h == null ? Camps.NO_HOLDER : h.getTicketLevel());
                    }
                    check("a footprint chunk's holder is above FULL (" + top + ")", top > Camps.FULL);
                })
                .thenExecute(() -> {
                    // the same tick: the camp has not been looked at yet, so it is still in
                    check("the camp is still in when the line lands", den.site().in());
                    Camps c = MowzieKit.camps();
                    was[0] = c.taken;
                    was[1] = c.spawned;
                    was[2] = c.handedOff;
                    was[3] = c.markers;
                    was[4] = c.staleLines;
                    was[5] = c.removed;
                    op[1] = MowzieKit.reconcile(den.faction, den.site().epoch(), List.of(
                            CampLine.of(0, 1, MowzieKit.FROSTMAW, TAKE, CHIEF),
                            CampLine.of(1, 1, MowzieKit.FROSTMAW, SPAWN, CHIEF),
                            CampLine.party(0, 1, MowzieKit.FROSTMAW, SET_OUT, CHIEF, 4242L, 1)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[1], 3))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("a line in the tick the camp goes out", ConflictKit.outcomes(den.faction, op[1]), STALE);
                    Camps c = MowzieKit.camps();
                    check("nothing taken, spawned, removed or set out", c.taken == was[0] && c.spawned == was[1] && c.handedOff == was[2]
                            && c.removed == was[5]);
                    check("no marker made", c.markers == was[3]);
                    check("three stale lines counted (" + (c.staleLines - was[4]) + ")", c.staleLines - was[4] == 3);
                    check("the Frostmaw stands and is still tagged", f[0].isAlive() && !f[0].isRemoved() && MowzieKit.tagged(f[0], den.faction, 0, 1));
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenExecute(() -> den.force())
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(den.faction, 2);
                    CensusBody b = MowzieKit.bodyOf(c, f[0].getUUID());
                    check("census 2 lists the Frostmaw", b != null);
                    check("by its slot 0, generation 1, not as a stray (" + b.flags() + ")",
                            b.slot() == 0 && b.generation() == 1 && (b.flags() & ConflictRecords.CampCensus.BODY_STRAY) == 0);
                    record(test, "stale", "TAKE, SPAWN and SET_OUT in the tick the footprint went down were STALE; the Frostmaw stayed and was counted by slot");
                })
                .thenSucceed();
    }

    // ---- review fixes: the finder's queue -----------------------------------------------------------

    /** A detached chunk carrying a den start of the real structure, as a load would hand the finder (MZB-237). */
    private static LevelChunk denChunk(ServerLevel level, int cx, int cz) {
        Structure structure = level.registryAccess().registryOrThrow(Registries.STRUCTURE).get(ResourceLocation.parse(MicRtsimMowziesMobs.DEN_STRUCTURE));
        ChunkPos pos = new ChunkPos(cx, cz);
        LevelChunk chunk = new LevelChunk(level, pos);
        FrostmawPieces.FrostmawPiece piece = new FrostmawPieces.FrostmawPiece(level.getStructureManager(), FROSTMAW_TEMPLATE,
                new BlockPos(cx * 16 + 8, 64, cz * 16 + 8), Rotation.NONE);
        chunk.setStartForStructure(structure, new StructureStart(structure, pos, 0, new PiecesContainer(List.of(piece))));
        return chunk;
    }

    /** The den declared for a start chunk, or null. */
    private static Camps.Site denAt(int cx, int cz) {
        ChunkPos start = new ChunkPos(cx, cz);
        for (Camps.Site s : MowzieKit.camps().sites())
            if (MicRtsimMowziesMobs.DEN_STRUCTURE.equals(s.structure) && new ChunkPos(s.pos).equals(start))
                return s;
        return null;
    }

    /**
     * A flood of chunk loads cannot grow the finder's queue past its bound, and a den it drops is found at its
     * chunk's next load (MZB-237). The queue holds records of what the finder reads, not chunks: one den's load
     * first, then {@code QUEUE_CAP + 5} loads of a second den's chunk (the same camp each time), then a third
     * den's, all posted in one tick (the drain is at the end of it). The queue holds exactly {@code QUEUE_CAP},
     * the loads past it are counted as dropped, and the third den, dropped, is not found. Drained, the first
     * two dens are declared (a new chunk: not late). The third den's chunk loads again, this time from disk: it
     * is found, and late; the first den's load from disk changes nothing (its late flag was fixed when it was
     * found). The three camps stand in chunks nobody loads, so the real engine plays nothing for them.
     */
    @GameTest(template = ARENA, timeoutTicks = 400, batch = "mowzie_finder_queue", skyAccess = true)
    public static void aFloodOfChunkLoadsKeepsTheFindersQueueBoundedAndADroppedDenIsFoundAtItsNextLoad(GameTestHelper helper) {
        String test = "aFloodOfChunkLoadsKeepsTheFindersQueueBoundedAndADroppedDenIsFoundAtItsNextLoad";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int ex = far.k, fx = far.k + 10, dx = far.k + 20, z = far.z0;
        LevelChunk[] chunk = new LevelChunk[3];
        long[] dropped = new long[1];

        helper.startSequence()
                .thenWaitUntil(() -> check("the queue is empty (" + MowzieCamps.queued() + ")", MowzieCamps.queued() == 0))
                .thenExecute(() -> {
                    chunk[0] = denChunk(level, ex, z);
                    chunk[1] = denChunk(level, fx, z);
                    chunk[2] = denChunk(level, dx, z);
                    check("none of the three is declared yet", denAt(ex, z) == null && denAt(fx, z) == null && denAt(dx, z) == null);
                    dropped[0] = MowzieCamps.dropped;
                    MowzieCamps.onChunkLoad(new ChunkEvent.Load(chunk[0], true));
                    for (int i = 0; i < MowzieCamps.QUEUE_CAP + 5; i++)
                        MowzieCamps.onChunkLoad(new ChunkEvent.Load(chunk[1], true));
                    MowzieCamps.onChunkLoad(new ChunkEvent.Load(chunk[2], true));
                    record(test, "flood", "queued " + MowzieCamps.queued() + ", dropped " + (MowzieCamps.dropped - dropped[0]));
                    check("the queue stands at its bound (" + MowzieCamps.queued() + ")", MowzieCamps.queued() == MowzieCamps.QUEUE_CAP);
                    check("the loads past it are counted (" + (MowzieCamps.dropped - dropped[0]) + ")", MowzieCamps.dropped - dropped[0] == 7);
                })
                .thenWaitUntil(() -> check("drained (" + MowzieCamps.queued() + ")", MowzieCamps.queued() == 0))
                .thenExecute(() -> {
                    Camps.Site e = denAt(ex, z), f = denAt(fx, z);
                    check("the first den was found and is not late", e != null && !e.late());
                    check("so was the flooded one, once", f != null && !f.late());
                    check("the third, dropped, was not found", denAt(dx, z) == null);
                    MowzieCamps.onChunkLoad(new ChunkEvent.Load(chunk[2], false));
                    MowzieCamps.onChunkLoad(new ChunkEvent.Load(chunk[0], false));
                })
                .thenWaitUntil(() -> check("drained again (" + MowzieCamps.queued() + ")", MowzieCamps.queued() == 0))
                .thenExecute(() -> {
                    Camps.Site d = denAt(dx, z), e = denAt(ex, z);
                    check("the dropped den is found at its next load, and late: its chunk had loaded before", d != null && d.late());
                    check("the first den's late flag is the one it was found with", e != null && !e.late());
                    check("no more drops (" + (MowzieCamps.dropped - dropped[0]) + ")", MowzieCamps.dropped - dropped[0] == 7);
                    check("nothing played for them: no census", !d.in() && !e.in());
                })
                .thenSucceed();
    }

    // ---- review fixes: hand-back and Peaceful -----------------------------------------------------

    /** A band of {@code n} warriors of the grove's faction at its centre, holding still (the party's agent). */
    private static long bandAt(Grove g, GameTestHelper helper, int n) {
        long agent = ConflictKit.newAgent();
        ConflictKit.Band b = ConflictKit.band(agent, g.faction, RtsimTests.dimension(helper));
        b.arcX = g.spot.x;
        b.arcZ = g.spot.z;
        b.targetX = g.spot.x + 1000;
        b.targetY = g.spot.y;
        b.targetZ = g.spot.z;
        b.speed = 0f;
        ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.WARRIOR, n, false)));
        return agent;
    }

    /**
     * A player 50 blocks east of the grove's centre, removed when the test ends (MZB-591): within the recall's
     * {@value ConflictBridge#OUT_OF_VIEW} blocks of the band, so a HAND_BACK's recall does not despawn the band's
     * other puppets as unseen, and beyond a warrior's sight. It holds no ticket and loads nothing.
     */
    private static ServerPlayer watcher(GameTestHelper helper, Grove g, String name) {
        ServerPlayer p = FakePlayers.add(g.far.level, g.spot.add(50, 1, 0), name);
        ConflictKit.onEnd(helper, () -> FakePlayers.remove(p));
        return p;
    }

    /** Inside {@code thenWaitUntil}: the band's one stack is placed and every member is embodied; the puppets. */
    private static Mob[] bandStands(long agent, int n) {
        ConflictBridge.Party p = ConflictKit.party(agent);
        check("one stack", p.stacks().size() == 1);
        ConflictBridge.Stack st = p.stacks().get(0);
        check("placed (" + st.state() + ")", st.state() == ConflictBridge.StackState.PLACED);
        Mob[] out = new Mob[n];
        int stand = 0;
        for (ConflictBridge.Slot s : st.slots())
            if (s.entity() != null && !s.entity().isRemoved() && stand < n)
                out[stand++] = s.entity();
        check(n + " puppets stand (" + stand + ")", stand == n);
        return out;
    }

    /**
     * A HAND_BACK whose body another mod's join handler cancels (In Control and the like) loses nothing
     * (MZB-236): the body is added before the puppet goes, so the line is STALE and the puppet stands, still
     * the band's, with nothing taken off the party and no recall. With the handler gone, the same line
     * again is DONE: the body is the camp's, the puppet is gone, the other puppet still stands.
     */
    @GameTest(template = ARENA, timeoutTicks = 2000, batch = "mowzie_handback_cancelled", skyAccess = true)
    public static void aHandBackWhoseJoinAnotherModCancelsKeepsThePuppetAndIsStale(GameTestHelper helper) {
        String test = "aHandBackWhoseJoinAnotherModCancelsKeepsThePuppetAndIsStale";
        Far far = new Far(helper, test);
        Grove grove = new Grove(far, far.k, far.z0);
        long[] agent = new long[1], op = new long[2], handed = new long[1];
        Mob[][] puppet = new Mob[1][];

        helper.startSequence()
                .thenExecute(() -> grove.force())
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    grove.lay().declare();
                    watcher(helper, grove, "mowzie-watcher");
                    agent[0] = bandAt(grove, helper, 2);
                })
                .thenWaitUntil(() -> {
                    check("census 1", MowzieKit.censusCount(grove.faction) >= 1);
                    bandStands(agent[0], 2);
                })
                .thenExecute(() -> {
                    puppet[0] = bandStands(agent[0], 2);
                    check("the puppets are puppets and unsaved", RtsimApi.isPuppet(puppet[0][0]) && !puppet[0][0].shouldBeSaved());
                    handed[0] = MowzieKit.camps().handedBack;
                    MowzieKit.cancelCampJoins(helper, true);
                    op[0] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(
                            CampLine.party(0, 1, MowzieKit.WARRIOR, HAND_BACK, 0, agent[0], ConflictKit.party(agent[0]).epoch)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("HAND_BACK with the join cancelled", ConflictKit.outcomes(grove.faction, op[0]), STALE);
                    check("the puppet still stands", puppet[0][0].isAlive() && !puppet[0][0].isRemoved() && RtsimApi.isPuppet(puppet[0][0]));
                    check("and is still the band's slot (" + ConflictKit.party(agent[0]).stacks().get(0).slots().get(0).entity() + ")",
                            ConflictKit.party(agent[0]).stacks().get(0).slots().get(0).entity() == puppet[0][0]);
                    check("nothing handed back", MowzieKit.camps().handedBack == handed[0]);
                    check("the camp holds no body", MowzieKit.held(grove.faction).isEmpty());
                    check("the party is not sent home", !ConflictKit.party(agent[0]).recalling());
                    MowzieKit.cancelCampJoins = false;
                    op[1] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(
                            CampLine.party(0, 1, MowzieKit.WARRIOR, HAND_BACK, 0, agent[0], ConflictKit.party(agent[0]).epoch)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[1], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("HAND_BACK again", ConflictKit.outcomes(grove.faction, op[1]), DONE);
                    List<Entity> held = MowzieKit.held(grove.faction, 0, 1);
                    check("one body is the camp's (" + held.size() + ")", held.size() == 1 && MowzieKit.held(grove.faction).size() == 1);
                    check("the first puppet is gone", puppet[0][0].isRemoved());
                    check("the second still stands", puppet[0][1].isAlive() && !puppet[0][1].isRemoved());
                    check("one handed back (" + (MowzieKit.camps().handedBack - handed[0]) + ")", MowzieKit.camps().handedBack == handed[0] + 1);
                    record(test, "cancelled join", "STALE and the puppet stayed; the same line without the handler was DONE");
                })
                .thenSucceed();
    }

    /**
     * On Peaceful no new body appears and a returning one still comes home (MZB-215): the jar keeps its own
     * mobs on Peaceful, since Mowzie's entities do not despawn in it, so a SPAWN there is STALE (the pool
     * is refilled when the difficulty is raised) and a HAND_BACK, which brings a body that exists, is DONE.
     * Both lines in one CAMP_RECONCILE at Peaceful: the SPAWN of slot 1 STALE with no body made, the
     * HAND_BACK of slot 0 DONE; the body it made is still there 40 ticks later.
     */
    @GameTest(template = ARENA, timeoutTicks = 2000, batch = "mowzie_peaceful", skyAccess = true)
    public static void onPeacefulASpawnIsStaleAndAHandBackStillBringsTheBodyHome(GameTestHelper helper) {
        String test = "onPeacefulASpawnIsStaleAndAHandBackStillBringsTheBodyHome";
        Far far = new Far(helper, test);
        Grove grove = new Grove(far, far.k, far.z0);
        long[] agent = new long[1], op = new long[1], spawned = new long[1];
        Mob[][] puppet = new Mob[1][];

        helper.startSequence()
                .thenExecute(() -> grove.force())
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    grove.lay().declare();
                    watcher(helper, grove, "mowzie-watcher");
                    agent[0] = bandAt(grove, helper, 2);
                })
                .thenWaitUntil(() -> {
                    check("census 1", MowzieKit.censusCount(grove.faction) >= 1);
                    bandStands(agent[0], 2);
                })
                .thenExecute(() -> {
                    puppet[0] = bandStands(agent[0], 2);
                    far.difficulty(Difficulty.PEACEFUL);
                    spawned[0] = MowzieKit.camps().spawned;
                    op[0] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(
                            CampLine.of(1, 1, MowzieKit.WARRIOR, SPAWN, 0),
                            CampLine.party(0, 1, MowzieKit.WARRIOR, HAND_BACK, 0, agent[0], ConflictKit.party(agent[0]).epoch)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[0], 2))
                .thenExecute(() -> {
                    Map<Integer, Integer> o = ConflictKit.outcomes(grove.faction, op[0]);
                    check("the SPAWN is STALE on Peaceful, not " + MowzieKit.outcomeName(o.get(0)), o.get(0) == STALE);
                    check("the HAND_BACK is DONE on Peaceful, not " + MowzieKit.outcomeName(o.get(1)), o.get(1) == DONE);
                    check("no body was spawned", MowzieKit.camps().spawned == spawned[0]);
                    check("slot 1 has no body", MowzieKit.held(grove.faction, 1, 1).isEmpty());
                    List<Entity> held = MowzieKit.held(grove.faction, 0, 1);
                    check("slot 0 is the camp's body (" + held.size() + ")", held.size() == 1 && puppet[0][0].isRemoved());
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("the returned body is still there on Peaceful", MowzieKit.held(grove.faction, 0, 1).size() == 1);
                    check("so is the band's other puppet", puppet[0][1].isAlive() && !puppet[0][1].isRemoved());
                    record(test, "peaceful", "SPAWN STALE, HAND_BACK DONE, the returned body survived 40 ticks");
                })
                .thenSucceed();
    }

    // ---- review fixes: dimension change -----------------------------------------------------------

    /**
     * A body that changes dimension leaves its camp's index (MZB-240): the leave is posted with the removal
     * reason CHANGED_DIMENSION, not a death, and the copy that joins the Nether (it carries the camp tag, as
     * the entity's saved data is copied) is not indexed, since the camp's dimension is another. A den's
     * adopted Frostmaw is teleported to the Nether: the camp holds nothing, no CAMP_DEATH is booked, and a
     * TAKE of its slot is NOTHING (the copy stands, untouched).
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_dimension", skyAccess = true)
    public static void aBodyThatChangesDimensionLeavesItsCampsIndex(GameTestHelper helper) {
        String test = "aBodyThatChangesDimensionLeavesItsCampsIndex";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        ServerLevel nether = level.getServer().getLevel(Level.NETHER);
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        Entity[] copy = new Entity[1];
        long[] op = new long[2];
        int[][] leaves = new int[1][];
        long[] taken = new long[1];

        helper.startSequence()
                .thenExecute(() -> {
                    check("the Nether is there", nether != null);
                    ConflictKit.onEnd(helper, () -> {
                        Entity c = f[0] == null ? null : nether.getEntity(f[0].getUUID());
                        if (c != null) {
                            c.removeData(RtsimAttachments.CAMP_MEMBER); // a tagged body discarded is a death
                            c.discard();
                        }
                        ConflictKit.force(nether, far.k, far.z0, false);
                    });
                    ConflictKit.force(nether, far.k, far.z0, true);
                    den.force();
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("the Nether chunk is loaded with its entities", nether.hasChunk(far.k, far.z0)
                        && nether.areEntitiesLoaded(ChunkPos.asLong(far.k, far.z0))))
                .thenExecute(() -> {
                    den.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID()))))
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("the camp holds it", MowzieKit.camps().held(den.faction).contains(f[0]));
                    leaves[0] = MowzieKit.watchLeaves(helper, f[0]);
                    taken[0] = MowzieKit.camps().taken;
                    boolean ok = f[0].teleportTo(nether, far.k * 16 + 8.5, 80, far.z0 * 16 + 8.5, Set.of(), 0f, 0f);
                    check("teleported", ok);
                    check("the original is removed as a change of dimension (" + f[0].getRemovalReason() + ")",
                            f[0].getRemovalReason() == Entity.RemovalReason.CHANGED_DIMENSION);
                    check("and left with a removal reason (" + leaves[0][1] + ")", leaves[0][1] == 1);
                    copy[0] = nether.getEntity(f[0].getUUID());
                    check("its copy stands in the Nether", copy[0] != null && !copy[0].isRemoved());
                    check("the copy carries the camp tag", RtsimAttachments.campMember(copy[0]) != null);
                    check("the camp holds neither (" + MowzieKit.camps().held(den.faction).size() + ")", MowzieKit.camps().held(den.faction).isEmpty());
                    op[1] = MowzieKit.reconcile(den.faction, den.site().epoch(), List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, TAKE, CHIEF)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[1], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("TAKE of the slot whose body left", ConflictKit.outcomes(den.faction, op[1]), NOTHING);
                    check("nothing taken", MowzieKit.camps().taken == taken[0]);
                    check("the copy stands untouched", copy[0].isAlive() && !copy[0].isRemoved());
                    check("no CAMP_DEATH (" + ConflictKit.campDeaths(den.faction).size() + ")", ConflictKit.campDeaths(den.faction).isEmpty());
                    record(test, "dimension", "the index let go of the Frostmaw at CHANGED_DIMENSION; its Nether copy was not indexed, no death booked");
                })
                .thenSucceed();
    }

    // ---- the plan's other tests (docs/choices-mowzie-build.md, "M5 design for the implementer") ----------------

    /** A stray warrior (no AI) {@code dx}, {@code dz} from a declared grove's centre. */
    private static EntityUmvuthanaMinion warriorAt(Grove g, double dx, double dz) {
        Vec3 p = g.at(dx, dz);
        return MowzieKit.stray(g.far.level, EntityHandler.UMVUTHANA_MINION.get(), p.x, p.y, p.z, true);
    }

    /** Inside {@code thenWaitUntil}: none of the entities is in the level now (unloaded or removed). */
    private static Runnable gone(ServerLevel level, UUID... ids) {
        return io(() -> {
            for (UUID id : ids)
                check("entity " + id + " is unloaded", level.getEntity(id) == null);
        });
    }

    // ---- 3: a member comes home -------------------------------------------------------------------

    /**
     * A party's warrior handed back to its grove (§7 test 4): the puppet (a {@code mowziesmobs:umvuthana},
     * unsaved) becomes a saved vanilla body, tagged (faction, 0, 1), persistence-required, restricted 25, at the
     * puppet's last position with its health, mask and weapon; the puppet is removed; the other puppet of the
     * party is still unsaved and the party is on its way home. A REMOVE of the slot then leaves a record marker
     * (tagged {@code mic_rtsim.record}, carrying {@code mic_rtsim:pending}) that is discarded once acknowledged.
     */
    @GameTest(template = ARENA, timeoutTicks = 2000, batch = "mowzie_homecoming", skyAccess = true)
    public static void aMemberComingHomeGetsASavedVanillaBodyAndAnAwayPuppetIsNotSaved(GameTestHelper helper) {
        String test = "aMemberComingHomeGetsASavedVanillaBodyAndAnAwayPuppetIsNotSaved";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove grove = new Grove(far, far.k, far.z0);
        long[] agent = new long[1], op = new long[2];
        Mob[][] puppet = new Mob[1][];
        Markers[] markers = new Markers[1];
        Vec3[] last = new Vec3[1];
        MaskType[] mask = new MaskType[1];
        int[] weapon = new int[1];
        UUID[] bodyId = new UUID[1], markerId = new UUID[1];

        helper.startSequence()
                .thenExecute(grove::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    grove.lay().declare();
                    markers[0] = new Markers(helper, level, grove.area());
                    watcher(helper, grove, "mowzie-watcher");
                    agent[0] = bandAt(grove, helper, 2);
                })
                .thenWaitUntil(() -> {
                    check("census 1", MowzieKit.censusCount(grove.faction) >= 1);
                    bandStands(agent[0], 2);
                })
                .thenExecute(() -> {
                    puppet[0] = bandStands(agent[0], 2);
                    for (Mob p : puppet[0])
                        check("a puppet of the jar's warrior, unsaved", p instanceof EntityUmvuthanaMinion && RtsimApi.isPuppet(p) && !p.shouldBeSaved());
                    EntityUmvuthanaMinion p0 = (EntityUmvuthanaMinion) puppet[0][0];
                    p0.setHealth(7f);
                    last[0] = p0.position();
                    mask[0] = p0.getMaskType();
                    weapon[0] = p0.getWeapon();
                    op[0] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(
                            CampLine.party(0, 1, MowzieKit.WARRIOR, HAND_BACK, 0, agent[0], ConflictKit.party(agent[0]).epoch)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("HAND_BACK", ConflictKit.outcomes(grove.faction, op[0]), DONE);
                    List<Entity> held = MowzieKit.held(grove.faction);
                    check("one held body, tagged (faction, 0, 1) (" + held.size() + ")", held.size() == 1
                            && MowzieKit.tagged(held.get(0), grove.faction, 0, 1));
                    check("a warrior, no puppet", held.get(0) instanceof EntityUmvuthanaMinion && !RtsimApi.isPuppet(held.get(0)));
                    EntityUmvuthanaMinion b = (EntityUmvuthanaMinion) held.get(0);
                    bodyId[0] = b.getUUID();
                    check("persistence-required", b.isPersistenceRequired());
                    check("saved with its chunk", b.shouldBeSaved());
                    check("restricted 25 round the spot", b.hasRestriction() && b.getRestrictRadius() == GroveKind.RESTRICT);
                    check("where the puppet stood (" + b.position().distanceTo(last[0]) + " blocks on)", b.position().distanceTo(last[0]) < 4);
                    check("with its health (" + b.getHealth() + ")", Math.abs(b.getHealth() - 7f) < 1.5f);
                    check("its mask and weapon", b.getMaskType() == mask[0] && b.getWeapon() == weapon[0]);
                    CompoundTag nbt = new CompoundTag();
                    check("it saves", b.save(nbt));
                    check("puppet 0 is removed", puppet[0][0].isRemoved());
                    check("puppet 1 is still an unsaved puppet", puppet[0][1].isAlive() && !puppet[0][1].isRemoved()
                            && RtsimApi.isPuppet(puppet[0][1]) && !puppet[0][1].shouldBeSaved());
                    ConflictBridge.Party party = ConflictKit.bridge().party(agent[0]);
                    check("its party is recalling or gone", party == null || party.recalling());
                    op[1] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(CampLine.of(0, 1, MowzieKit.WARRIOR, REMOVE, 0)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[1], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("REMOVE", ConflictKit.outcomes(grove.faction, op[1]), DONE);
                    check("the body is gone", level.getEntity(bodyId[0]) == null || level.getEntity(bodyId[0]).isRemoved());
                    List<Markers.Seen> seen = markers[0].all();
                    check("a record marker was left (" + seen.size() + ")", seen.size() == 1);
                    Markers.Seen m = seen.get(0);
                    markerId[0] = m.id();
                    check("it is tagged mic_rtsim.record and carries mic_rtsim:pending", m.tagged() && m.carriesRecords());
                    record(test, "marker", "NBT tags " + m.nbt().getList("Tags", 8) + ", attachments " + m.nbt().getCompound("neoforge:attachments").getAllKeys());
                })
                .thenWaitUntil(() -> check("the marker is discarded once acknowledged", markers[0].gone(markerId[0])))
                .thenSucceed();
    }

    // ---- 4: a refilled pool ----------------------------------------------------------------------

    /**
     * A refilled pool is spawned at the next census (§7 test 5). A den and a grove 12 chunks apart, one stray
     * warrior in the grove, a player 40 blocks east of the den (so also the grove's nearest, 150 blocks from it).
     * The den's SPAWN (chief, crystal) is held (one held line, no answer, no Frostmaw after 40 ticks). The
     * grove's ADOPT and SPAWN are both DONE: the new warrior is born 20 (+-1.5) blocks from the centre on the
     * side away from the player, restricted 25 and persistence-required; a second SPAWN for its slot is
     * NOTHING. With the player gone the den's line is DONE: a sleeping Frostmaw with its crystal at the spot,
     * restricted 8, tagged (faction, 0, 1).
     */
    @GameTest(template = ARENA, timeoutTicks = 2500, batch = "mowzie_refill", skyAccess = true)
    public static void aRefilledPoolIsSpawnedAtTheNextCensus(GameTestHelper helper) {
        String test = "aRefilledPoolIsSpawnedAtTheNextCensus";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        Grove grove = new Grove(far, far.k + 12, far.z0);
        ServerPlayer[] player = new ServerPlayer[1];
        EntityUmvuthanaMinion[] stray = new EntityUmvuthanaMinion[1];
        long[] op = new long[3];
        Vec3[] born = new Vec3[1];

        helper.startSequence()
                .thenExecute(() -> {
                    den.force();
                    grove.force();
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    grove.lay().declare();
                    stray[0] = warriorAt(grove, 5, 3);
                    player[0] = FakePlayers.add(level, den.spot.add(40, 1, 0), "mowzie-refill");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    MowzieKit.everyTick(helper, () -> {
                        if (born[0] == null) {
                            List<Entity> h = MowzieKit.held(grove.faction, 1, 1);
                            if (!h.isEmpty())
                                born[0] = h.get(0).position();
                        }
                    });
                })
                .thenWaitUntil(() -> {
                    check("the den's census", MowzieKit.censusCount(den.faction) >= 1);
                    check("the grove's census", MowzieKit.censusCount(grove.faction) >= 1);
                })
                .thenExecute(() -> {
                    ScriptedEngine.Received gc = MowzieKit.census(grove.faction, 1);
                    CensusBody b = MowzieKit.bodyOf(gc, stray[0].getUUID());
                    check("the grove's census lists the stray", b != null && b.flags() == ConflictRecords.CampCensus.BODY_STRAY);
                    op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                            List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, SPAWN, CHIEF | CRYSTAL)));
                    op[1] = MowzieKit.reconcile(grove.faction, MowzieKit.epoch(gc), List.of(
                            CampLine.of(0, 1, MowzieKit.WARRIOR, ADOPT, 0, stray[0].getUUID()),
                            CampLine.of(1, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[1], 2))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the grove's ADOPT and SPAWN", ConflictKit.outcomes(grove.faction, op[1]), DONE);
                    check("the den's SPAWN is held: one held line (" + MowzieKit.camps().heldLines() + ")", MowzieKit.camps().heldLines() == 1);
                    check("with no answer yet", ConflictKit.outcomes(den.faction, op[0]).isEmpty());
                    check("and no Frostmaw", MowzieKit.held(den.faction).isEmpty()
                            && level.getEntitiesOfClass(EntityFrostmaw.class, den.area()).isEmpty());
                    List<Entity> h0 = MowzieKit.held(grove.faction, 0, 1), h1 = MowzieKit.held(grove.faction, 1, 1);
                    check("slot 0 is the stray adopted, slot 1 a new warrior", h0.size() == 1 && h0.get(0) == stray[0] && h1.size() == 1);
                    check("seen at the end of its tick", born[0] != null);
                    double dx = born[0].x - grove.spot.x, dz = born[0].z - grove.spot.z, dist = Math.sqrt(dx * dx + dz * dz);
                    record(test, "born", "offset (" + dx + ", " + dz + "), " + dist + " from the centre, the player east of it");
                    check("born 20 (+-1.5) from the centre (" + dist + ")", Math.abs(dist - GroveKind.EDGE) <= 1.5);
                    check("on the side away from the player (" + dx + ", " + dz + ")", dx > 0 && Math.abs(dz) <= 3);
                    EntityUmvuthanaMinion w = (EntityUmvuthanaMinion) h1.get(0);
                    check("restricted 25 round the spot", w.hasRestriction() && w.getRestrictRadius() == GroveKind.RESTRICT
                            && w.getRestrictCenter().equals(BlockPos.containing(grove.spot)));
                    check("persistence-required", w.isPersistenceRequired());
                    op[2] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(CampLine.of(1, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[2], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("a second SPAWN for the slot", ConflictKit.outcomes(grove.faction, op[2]), NOTHING);
                    check("still one body in slot 1", MowzieKit.held(grove.faction, 1, 1).size() == 1 && MowzieKit.held(grove.faction).size() == 2);
                })
                .thenIdle(40)
                .thenExecute(() -> {
                    check("40 ticks later the den's line is still held", MowzieKit.camps().heldLines() == 1
                            && ConflictKit.outcomes(den.faction, op[0]).isEmpty());
                    check("and there is no Frostmaw", MowzieKit.held(den.faction).isEmpty()
                            && level.getEntitiesOfClass(EntityFrostmaw.class, den.area()).isEmpty());
                    FakePlayers.remove(player[0]);
                })
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the den's SPAWN once the player is gone", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    List<Entity> h = MowzieKit.held(den.faction, 0, 1);
                    check("a Frostmaw is the den's (" + h.size() + ")", h.size() == 1 && h.get(0) instanceof EntityFrostmaw);
                    EntityFrostmaw f = (EntityFrostmaw) h.get(0);
                    check("at the spot (" + f.position().distanceTo(den.spot) + ")", f.position().distanceTo(den.spot) < 1.5);
                    check("with its crystal, asleep", f.getHasCrystal() && !f.getActive());
                    check("restricted " + DenKind.RESTRICT + " round the spot", f.hasRestriction() && f.getRestrictRadius() == DenKind.RESTRICT
                            && f.getRestrictCenter().equals(BlockPos.containing(den.spot)));
                    check("persistence-required", f.isPersistenceRequired());
                    check("no held line left", MowzieKit.camps().heldLines() == 0);
                    record(test, "refill", "the grove's spawn at once, the den's Frostmaw once the player left");
                })
                .thenSucceed();
    }

    // ---- 5: the hold's limit ---------------------------------------------------------------------

    /**
     * A Frostmaw's SPAWN with a player staying within 160 blocks of the den is placed at {@link Camps#HOLD_MAX}
     * and not before: the line is DONE no sooner than HOLD_MAX ticks after it was sent (2 ticks of slack for the
     * tick it was applied in) and within 40 ticks after that, the Frostmaw standing at the spot.
     */
    @GameTest(template = ARENA, timeoutTicks = 7000, batch = "mowzie_hold_max", skyAccess = true)
    public static void aHeldFrostmawIsPlacedAfterHoldMaxWithAPlayerStillNear(GameTestHelper helper) {
        String test = "aHeldFrostmawIsPlacedAfterHoldMaxWithAPlayerStillNear";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        ServerPlayer[] player = new ServerPlayer[1];
        long[] sent = new long[1], placed = new long[1], op = new long[1];

        helper.startSequence()
                .thenExecute(den::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    player[0] = FakePlayers.add(level, den.spot.add(40, 1, 0), "mowzie-hold");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    MowzieKit.everyTick(helper, () -> {
                        if (placed[0] == 0 && !MowzieKit.held(den.faction, 0, 1).isEmpty())
                            placed[0] = level.getGameTime();
                    });
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> {
                    sent[0] = level.getGameTime();
                    op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                            List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, SPAWN, CHIEF | CRYSTAL)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    long waited = placed[0] - sent[0];
                    record(test, "hold", "sent at " + sent[0] + ", placed at " + placed[0] + " (" + waited + " ticks), HOLD_MAX " + Camps.HOLD_MAX);
                    MowzieKit.allOutcomes("the held SPAWN", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("placed no sooner than HOLD_MAX (" + waited + ")", placed[0] != 0 && waited >= Camps.HOLD_MAX - 2);
                    check("and within 40 ticks after it (" + waited + ")", waited <= Camps.HOLD_MAX + 40);
                    check("the player was near all along", player[0].distanceToSqr(den.spot) < DenKind.HOLD_RANGE * DenKind.HOLD_RANGE);
                    List<Entity> h = MowzieKit.held(den.faction, 0, 1);
                    check("the Frostmaw stands at the spot", h.size() == 1 && h.get(0).position().distanceTo(den.spot) < 1.5);
                })
                .thenSucceed();
    }

    // ---- 6: a cold load --------------------------------------------------------------------------

    /**
     * The census waits for a camp's entities (§7 test 21). A den with a crystal Frostmaw: census 1. Its chunks
     * released until unloaded and forced again: the camp is seen waiting, no census comes before every
     * footprint chunk has its entities in, and census 2 lists the Frostmaw (a stray chief) with the crystal.
     * A grove (footprint columns c - 2 to c + 2) with only columns c - 2 and c - 1 forced: column c + 2 stands
     * at level 34, the camp stays out and sends nothing in 40 ticks; with the rest forced, its census comes.
     */
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "mowzie_cold", skyAccess = true)
    public static void theCensusWaitsForEntitiesOnACampsColdLoad(GameTestHelper helper) {
        String test = "theCensusWaitsForEntitiesOnACampsColdLoad";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        Grove grove = new Grove(far, far.k + 12, far.z0);
        UUID[] id = new UUID[1];
        boolean[] sawWaiting = new boolean[1], early = new boolean[1];
        int[] seen = new int[1];
        int[][] events = new int[1][];
        long[] declaredAt = new long[1];

        helper.startSequence()
                .thenExecute(() -> {
                    den.force();
                    far.forceRect(grove.cx - 2, grove.cz - 2, grove.cx - 1, grove.cz + 2);
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    id[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true).getUUID();
                    grove.declare();
                    declaredAt[0] = level.getGameTime();
                })
                .thenWaitUntil(() -> check("the den's census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenIdle(40)
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(den.faction, 1);
                    check("census 1 lists the Frostmaw with its crystal", MowzieKit.bodyOf(c, id[0]) != null
                            && (MowzieKit.chiefFlags(c) & ConflictRecords.CampCensus.CRYSTAL) != 0);
                    Camps.Site g = grove.site();
                    int top = 0;
                    for (int l : g.levels())
                        top = Math.max(top, l);
                    record(test, "grove levels", "highest footprint level " + top + ", " + (level.getGameTime() - declaredAt[0]) + " ticks after the declare");
                    check("a grove footprint chunk is above FULL (" + top + ")", top > Camps.FULL);
                    check("the grove sent no census, is not in and is not waiting", MowzieKit.censusCount(grove.faction) == 0 && !g.in() && !g.waiting());
                    den.release();
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenWaitUntil(io(() -> {
                    for (int dx = -1; dx <= 1; dx++)
                        for (int dz = -1; dz <= 1; dz++)
                            check("den chunk (" + (den.cx + dx) + ", " + (den.cz + dz) + ") is unloaded", !level.hasChunk(den.cx + dx, den.cz + dz));
                }))
                .thenExecute(() -> {
                    check("one census so far", MowzieKit.censusCount(den.faction) == 1);
                    seen[0] = 1;
                    events[0] = MowzieKit.watchTicketEvents(helper, den.site().footprintChunks());
                    MowzieKit.everyTick(helper, () -> {
                        Camps.Site s = den.site();
                        if (s.waiting())
                            sawWaiting[0] = true;
                        int n = MowzieKit.censusCount(den.faction);
                        if (n > seen[0]) {
                            seen[0] = n;
                            for (ChunkPos p : s.footprintChunks())
                                if (!level.areEntitiesLoaded(p.toLong()))
                                    early[0] = true;
                        }
                    });
                    den.force();
                })
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    // Waiting is seen only when the levels move ahead of the entities, which the ticket event path does; under
                    // C2ME (no event) the poll moves them with the load and the camp may never be seen waiting (MZB-593).
                    if (events[0][0] > 0)
                        check("the camp was seen waiting for its entities (" + events[0][0] + " ticket events)", sawWaiting[0]);
                    else
                        record(test, "waiting", "no ticket event fired for the den's chunks (C2ME?): the waiting is not asserted; seen waiting: " + sawWaiting[0]);
                    check("no census came before every footprint chunk had its entities", !early[0]);
                    ScriptedEngine.Received c = MowzieKit.census(den.faction, 2);
                    CensusBody b = MowzieKit.bodyOf(c, id[0]);
                    check("census 2 lists the Frostmaw, a stray chief", b != null && b.body() == ConflictKit.bodyId(MowzieKit.FROSTMAW)
                            && b.flags() == (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF));
                    check("with its crystal", (MowzieKit.chiefFlags(c) & ConflictRecords.CampCensus.CRYSTAL) != 0);
                    far.forceRect(grove.cx, grove.cz - 2, grove.cx + 2, grove.cz + 2);
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("the grove's census", MowzieKit.censusCount(grove.faction) >= 1))
                .thenExecute(() -> {
                    check("the grove is in", grove.site().in());
                    record(test, "cold load", "waiting seen, no early census; the grove came in with all its columns");
                })
                .thenSucceed();
    }

    // ---- 7: epochs and repeats -------------------------------------------------------------------

    /**
     * A second census makes the first's lines stale, and a repeated op is idempotent (§7 test 22). A grove with
     * two strays: census e1; out and in: census e2. An ADOPT at e1 is STALE and tags nothing. ADOPT of both at
     * e2 is DONE; the same op again leaves the same tags and the same two bodies. A SPAWN at e2 is DONE; the
     * same op again is NOTHING, with one body in the slot.
     */
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "mowzie_epochs", skyAccess = true)
    public static void aSecondCensusStalesTheFirstsLinesAndARepeatIsIdempotent(GameTestHelper helper) {
        String test = "aSecondCensusStalesTheFirstsLinesAndARepeatIsIdempotent";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        UUID[] id = new UUID[2];
        long[] e = new long[2], op = new long[3];
        Set<Entity> bodies = new HashSet<>();
        List<CampLine> adopt = new ArrayList<>();

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay().declare();
                    id[0] = warriorAt(g, 4, 0).getUUID();
                    id[1] = warriorAt(g, -4, 3).getUUID();
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(g.faction) >= 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(g.faction, 1);
                    e[0] = MowzieKit.epoch(c);
                    check("two strays listed", ConflictKit.censusBodies(c).size() == 2);
                    far.releaseRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(g.faction).size() == 1))
                .thenWaitUntil(gone(level, id[0], id[1]))
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(g.faction) >= 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(g.faction, 2);
                    e[1] = MowzieKit.epoch(c);
                    check("a new epoch", e[1] != e[0]);
                    check("both strays are listed again", MowzieKit.bodyOf(c, id[0]) != null && MowzieKit.bodyOf(c, id[1]) != null);
                    for (int i = 0; i < 2; i++)
                        adopt.add(CampLine.of(i, 1, MowzieKit.WARRIOR, ADOPT, 0, id[i]));
                    op[0] = MowzieKit.reconcile(g.faction, e[0], adopt);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[0], 2))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT at the first epoch", ConflictKit.outcomes(g.faction, op[0]), STALE);
                    check("it tagged nothing", MowzieKit.held(g.faction).isEmpty());
                    op[1] = MowzieKit.reconcile(g.faction, e[1], adopt);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[1], 2))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT at the second epoch", ConflictKit.outcomes(g.faction, op[1]), DONE);
                    for (int i = 0; i < 2; i++)
                        check("warrior " + i + " is tagged (faction, " + i + ", 1)", MowzieKit.tagged(level.getEntity(id[i]), g.faction, i, 1));
                    bodies.addAll(MowzieKit.held(g.faction));
                    check("two bodies", bodies.size() == 2);
                    ConflictKit.reconcile(g.faction, e[1], op[1], adopt); // the same op again
                })
                .thenWaitUntil(() -> check("the repeat is answered too (" + ConflictKit.campBodies(g.faction, op[1]).size() + " records)",
                        ConflictKit.campBodies(g.faction, op[1]).size() >= 4))
                .thenExecute(() -> {
                    for (int i = 0; i < 2; i++)
                        check("warrior " + i + " is still tagged (faction, " + i + ", 1)", MowzieKit.tagged(level.getEntity(id[i]), g.faction, i, 1));
                    check("the same two bodies", new HashSet<>(MowzieKit.held(g.faction)).equals(bodies));
                    op[2] = MowzieKit.reconcile(g.faction, e[1], List.of(CampLine.of(2, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[2], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("SPAWN at the second epoch", ConflictKit.outcomes(g.faction, op[2]), DONE);
                    check("a body in slot 2 and three in all", MowzieKit.held(g.faction, 2, 1).size() == 1 && MowzieKit.held(g.faction).size() == 3);
                    ConflictKit.reconcile(g.faction, e[1], op[2], List.of(CampLine.of(2, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(() -> check("the repeat is answered (" + ConflictKit.campBodies(g.faction, op[2]).size() + " records)",
                        ConflictKit.campBodies(g.faction, op[2]).size() >= 2))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> r = ConflictKit.campBodies(g.faction, op[2]);
                    int again = MowzieKit.outcomesOf(r.get(r.size() - 1)).get(0);
                    check("the same SPAWN again is NOTHING, not " + MowzieKit.outcomeName(again), again == NOTHING);
                    check("still one body in slot 2 and three in all", MowzieKit.held(g.faction, 2, 1).size() == 1 && MowzieKit.held(g.faction).size() == 3);
                    record(test, "epochs", "e1 lines STALE; the same ADOPT op twice kept two tags; the same SPAWN op twice made one body");
                })
                .thenSucceed();
    }

    // ---- 8: a held spawn when the den goes out ---------------------------------------------------

    /**
     * A held SPAWN is answered STALE when its den goes out (§7 test 22). A Frostmaw SPAWN held by a player,
     * then the den's chunks released: CAMP_OUT, the line STALE, nothing held. With the player gone and the den
     * forced again: census 2, no held line, and no Frostmaw (the stale line is not played later).
     */
    @GameTest(template = ARENA, timeoutTicks = 2500, batch = "mowzie_held_out", skyAccess = true)
    public static void aHeldSpawnIsAnsweredStaleWhenTheDenGoesOut(GameTestHelper helper) {
        String test = "aHeldSpawnIsAnsweredStaleWhenTheDenGoesOut";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        ServerPlayer[] player = new ServerPlayer[1];
        long[] op = new long[1];

        helper.startSequence()
                .thenExecute(den::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    player[0] = FakePlayers.add(level, den.spot.add(40, 1, 0), "mowzie-held-out");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, SPAWN, CHIEF | CRYSTAL))))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("the line is held (" + MowzieKit.camps().heldLines() + ")", MowzieKit.camps().heldLines() == 1);
                    check("with no answer", ConflictKit.outcomes(den.faction, op[0]).isEmpty());
                    den.release();
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the held SPAWN when the den went out", ConflictKit.outcomes(den.faction, op[0]), STALE);
                    check("nothing is held now", MowzieKit.camps().heldLines() == 0);
                    FakePlayers.remove(player[0]);
                    den.force();
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenIdle(20)
                .thenExecute(() -> {
                    check("no held line", MowzieKit.camps().heldLines() == 0);
                    check("no Frostmaw", MowzieKit.held(den.faction).isEmpty()
                            && level.getEntitiesOfClass(EntityFrostmaw.class, den.area()).isEmpty());
                    check("the line has one answer, STALE", ConflictKit.outcomes(den.faction, op[0]).equals(Map.of(0, STALE)));
                    record(test, "held then out", "the held SPAWN was STALE at CAMP_OUT and was not played at the next census");
                })
                .thenSucceed();
    }

    // ---- 9: deaths --------------------------------------------------------------------------------

    /**
     * A camp body's death is booked and a stray's is not (§7 test 22). A grove with four strays, three of them
     * adopted, and a den with an adopted Frostmaw. A fake player kills warrior 0: CAMP_DEATH by a player, the
     * killer's UUID, slot 0, generation 1, a Java op id, and a record marker where it fell. The stray killed
     * books nothing. Warrior 1 discarded is a death by none. {@code takeCampBody} on warrior 2 is a death by
     * "taken" with the player, and the tag is gone. The Frostmaw killed is a CAMP_DEATH flagged chief, with
     * the Frostmaw's body id.
     */
    @GameTest(template = ARENA, timeoutTicks = 2500, batch = "mowzie_deaths", skyAccess = true)
    public static void aCampBodysDeathIsBookedOnAMarkerAndAStraysIsNot(GameTestHelper helper) {
        String test = "aCampBodysDeathIsBookedOnAMarkerAndAStraysIsNot";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        Grove grove = new Grove(far, far.k + 12, far.z0);
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[4];
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        ServerPlayer[] player = new ServerPlayer[1];
        Markers[] groveMarkers = new Markers[1], denMarkers = new Markers[1];
        long[] op = new long[2];
        Vec3[] fell = new Vec3[2];

        helper.startSequence()
                .thenExecute(() -> {
                    den.force();
                    grove.force();
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    grove.lay().declare();
                    groveMarkers[0] = new Markers(helper, level, grove.area());
                    denMarkers[0] = new Markers(helper, level, den.area());
                    w[0] = warriorAt(grove, 4, 0);
                    w[1] = warriorAt(grove, -4, 0);
                    w[2] = warriorAt(grove, 0, 4);
                    w[3] = warriorAt(grove, 0, -4);
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                    player[0] = FakePlayers.add(level, grove.spot.add(12, 1, 12), "mowzie-killer");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                })
                .thenWaitUntil(() -> {
                    check("the grove's census", MowzieKit.censusCount(grove.faction) >= 1);
                    check("the den's census", MowzieKit.censusCount(den.faction) >= 1);
                })
                .thenExecute(() -> {
                    op[0] = MowzieKit.reconcile(grove.faction, MowzieKit.epoch(MowzieKit.census(grove.faction, 1)), List.of(
                            CampLine.of(0, 1, MowzieKit.WARRIOR, ADOPT, 0, w[0].getUUID()),
                            CampLine.of(1, 1, MowzieKit.WARRIOR, ADOPT, 0, w[1].getUUID()),
                            CampLine.of(2, 1, MowzieKit.WARRIOR, ADOPT, 0, w[2].getUUID())));
                    op[1] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                            List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, f[0].getUUID())));
                })
                .thenWaitUntil(() -> {
                    MowzieKit.answered(grove.faction, op[0], 3);
                    MowzieKit.answered(den.faction, op[1], 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < 3; i++)
                        check("warrior " + i + " is tagged", MowzieKit.tagged(w[i], grove.faction, i, 1));
                    check("warrior 3 is not", MowzieKit.tag(w[3]) == null);
                    check("no death yet", ConflictKit.campDeaths(grove.faction).isEmpty() && ConflictKit.campDeaths(den.faction).isEmpty());
                    fell[0] = w[0].position();
                    MowzieKit.kill(w[0], level.damageSources().playerAttack(player[0]));
                })
                .thenWaitUntil(() -> check("CAMP_DEATH", ConflictKit.campDeaths(grove.faction).size() == 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = ConflictKit.campDeaths(grove.faction).get(0);
                    check("by a player (" + d.aux() + ")", d.aux() == ConflictRecords.BY_PLAYER);
                    check("the killer is the fake player", player[0].getUUID().equals(ConflictKit.uuidAt(d, ConflictRecords.CampDeath.KILLER_MOST)));
                    check("a warrior", d.view().getInt(ConflictRecords.CampDeath.BODY) == ConflictKit.bodyId(MowzieKit.WARRIOR));
                    check("slot 0, generation 1", d.view().getInt(ConflictRecords.CampDeath.SLOT) == 0
                            && d.view().getInt(ConflictRecords.CampDeath.GENERATION) == 1);
                    check("not the chief", (d.view().getInt(ConflictRecords.CampDeath.FLAGS) & ConflictRecords.CampDeath.FLAG_CHIEF) == 0);
                    check("with a Java op id", (d.view().getLong(ConflictRecords.CampDeath.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    List<Markers.Seen> m = groveMarkers[0].near(fell[0], 3);
                    check("a record marker where it fell (" + m.size() + ")", !m.isEmpty());
                    check("it is tagged and carries the death", m.get(0).tagged() && m.get(0).carriesRecords());
                    check("the tag is off the corpse", MowzieKit.tag(w[0]) == null);
                    MowzieKit.kill(w[3], level.damageSources().playerAttack(player[0]));
                })
                .thenIdle(20)
                .thenExecute(() -> {
                    check("a stray's death books nothing (" + ConflictKit.campDeaths(grove.faction).size() + ")",
                            ConflictKit.campDeaths(grove.faction).size() == 1);
                    w[1].discard();
                })
                .thenWaitUntil(() -> check("CAMP_DEATH for the discard", ConflictKit.campDeaths(grove.faction).size() == 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = ConflictKit.campDeaths(grove.faction).get(1);
                    check("by none (" + d.aux() + ")", d.aux() == ConflictRecords.BY_NONE);
                    check("slot 1, generation 1", d.view().getInt(ConflictRecords.CampDeath.SLOT) == 1
                            && d.view().getInt(ConflictRecords.CampDeath.GENERATION) == 1);
                    check("taken by the player", RtsimApi.takeCampBody(w[2], player[0]));
                })
                .thenWaitUntil(() -> check("CAMP_DEATH for the take", ConflictKit.campDeaths(grove.faction).size() == 3))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = ConflictKit.campDeaths(grove.faction).get(2);
                    check("by taken (" + d.aux() + ")", d.aux() == ConflictRecords.BY_TAKEN);
                    check("with the player", player[0].getUUID().equals(ConflictKit.uuidAt(d, ConflictRecords.CampDeath.KILLER_MOST)));
                    check("slot 2", d.view().getInt(ConflictRecords.CampDeath.SLOT) == 2);
                    check("the tag is gone", MowzieKit.tag(w[2]) == null);
                    check("the Frostmaw has booked nothing yet", ConflictKit.campDeaths(den.faction).isEmpty());
                    fell[1] = f[0].position();
                    MowzieKit.kill(f[0], level.damageSources().generic());
                })
                .thenWaitUntil(() -> check("CAMP_DEATH for the Frostmaw", ConflictKit.campDeaths(den.faction).size() == 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = ConflictKit.campDeaths(den.faction).get(0);
                    check("flagged chief", (d.view().getInt(ConflictRecords.CampDeath.FLAGS) & ConflictRecords.CampDeath.FLAG_CHIEF) != 0);
                    check("a Frostmaw", d.view().getInt(ConflictRecords.CampDeath.BODY) == ConflictKit.bodyId(MowzieKit.FROSTMAW));
                    check("by none, a generic source (" + d.aux() + ")", d.aux() == ConflictRecords.BY_NONE);
                    check("a marker where it fell", !denMarkers[0].near(fell[1], 4).isEmpty());
                    check("the grove booked exactly three deaths", ConflictKit.campDeaths(grove.faction).size() == 3);
                    record(test, "deaths", "warrior by player, discard by none, take by taken, Frostmaw chief; the stray booked none");
                })
                .thenSucceed();
    }

    // ---- 10: a holder journals its record again (MZB-174) ----------------------------------------

    /**
     * A holder journals its record again before the camp's next census (MZB-174). A den with an adopted
     * Frostmaw, and a hand-made record marker beside it, each given a copy of the adopt's CAMP_BODIES bytes
     * directly (so no acknowledgement drops them). The den's chunks are released until both entities are
     * gone, and forced again: the new Frostmaw (same UUID, another object) is tagged, restricted again and
     * known to the session as reasserted; each re-journaled CAMP_BODIES of the op comes after the CAMP_OUT and
     * before the new CAMP_CENSUS; the census lists the Frostmaw by slot 0, generation 1, as the chief; the
     * marker is discarded once its records are acknowledged.
     */
    @GameTest(template = ARENA, timeoutTicks = 3000, batch = "mowzie_reassert", skyAccess = true)
    public static void aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus(GameTestHelper helper) {
        String test = "aHolderJournalsItsRecordAgainBeforeTheCampsNextCensus";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        UUID[] id = new UUID[2];
        long[] op = new long[1];
        Markers[] markers = new Markers[1];

        helper.startSequence()
                .thenExecute(den::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    markers[0] = new Markers(helper, level, den.area());
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                    id[0] = f[0].getUUID();
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, id[0]))))
                .thenWaitUntil(() -> {
                    MowzieKit.answered(den.faction, op[0], 1);
                    check("acknowledged: the Frostmaw carries no records", !f[0].hasData(RtsimAttachments.PENDING));
                })
                .thenExecute(() -> {
                    byte[] adopt = ConflictKit.campBodies(den.faction, op[0]).get(0).bytes();
                    f[0].setData(RtsimAttachments.PENDING, new PendingRecords(List.of(adopt.clone())));
                    Marker m = EntityType.MARKER.create(level);
                    check("a marker is made", m != null);
                    m.setPos(den.spot.x + 2, den.spot.y + 1, den.spot.z + 2);
                    m.addTag(RtsimSession.RECORD_MARKER_TAG);
                    m.setData(RtsimAttachments.PENDING, new PendingRecords(List.of(adopt.clone())));
                    check("the marker is added", level.addFreshEntity(m));
                    id[1] = m.getUUID();
                    den.release();
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenWaitUntil(gone(level, id[0], id[1]))
                .thenExecute(() -> {
                    check("no second census yet", MowzieKit.censusCount(den.faction) == 1);
                    den.force();
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    Entity e = level.getEntity(id[0]);
                    check("the Frostmaw is back, a new object", e instanceof EntityFrostmaw && e != f[0]);
                    EntityFrostmaw nf = (EntityFrostmaw) e;
                    check("tagged (faction, 0, 1)", MowzieKit.tagged(nf, den.faction, 0, 1));
                    check("restricted " + DenKind.RESTRICT + " again", nf.hasRestriction() && nf.getRestrictRadius() == DenKind.RESTRICT);
                    check("the session has journaled its records again", RtsimSession.current().reasserted(nf));
                    long out = ConflictKit.campOuts(den.faction).get(0).obsSeq();
                    ScriptedEngine.Received c2 = MowzieKit.census(den.faction, 2);
                    List<ScriptedEngine.Received> r = ConflictKit.campBodies(den.faction, op[0]);
                    check("the adopt's record was journaled again by both holders (" + r.size() + " in all)", r.size() >= 3);
                    for (int i = 1; i < r.size(); i++)
                        check("journaled again #" + i + " (obsSeq " + r.get(i).obsSeq() + ") after the CAMP_OUT (" + out + ") and before census 2 ("
                                + c2.obsSeq() + ")", r.get(i).obsSeq() > out && r.get(i).obsSeq() < c2.obsSeq());
                    CensusBody b = MowzieKit.bodyOf(c2, id[0]);
                    check("census 2 lists the Frostmaw", b != null);
                    check("by slot 0, generation 1, as the chief and no stray (" + b.flags() + ")", b.slot() == 0 && b.generation() == 1
                            && (b.flags() & ConflictRecords.CampCensus.BODY_CHIEF) != 0 && (b.flags() & ConflictRecords.CampCensus.BODY_STRAY) == 0);
                    record(test, "reassert", r.size() + " CAMP_BODIES of the op, " + (r.size() - 1) + " journaled again between obsSeq " + out + " and " + c2.obsSeq());
                })
                .thenWaitUntil(() -> check("the marker is discarded once acknowledged", markers[0].gone(id[1])))
                .thenSucceed();
    }

    // ---- 11: SET_OUT ----------------------------------------------------------------------------

    /**
     * A SET_OUT hands a camp body to its party's puppet, and one nobody claims is a take (§7 test 22). A grove
     * with three adopted warriors. In one hand-over: SET_OUT for slots 0 and 1 of agent A, then a
     * BAND_MATERIALIZE of A with one warrior. Slot 0 is handed off: the puppet stands where the body stood, with
     * its health and mask. Slot 1 is taken. Both are DONE, and the camp holds only warrior 2.
     */
    @GameTest(template = ARENA, timeoutTicks = 2500, batch = "mowzie_set_out", skyAccess = true)
    public static void aSetOutHandsTheCampBodyToItsPuppetAndAnUnclaimedSetOutIsATake(GameTestHelper helper) {
        String test = "aSetOutHandsTheCampBodyToItsPuppetAndAnUnclaimedSetOutIsATake";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove grove = new Grove(far, far.k, far.z0);
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        long[] op = new long[2], agent = new long[1], was = new long[4];
        Vec3[] stood = new Vec3[1];
        MaskType[] mask = new MaskType[1];
        Markers[] markers = new Markers[1];

        helper.startSequence()
                .thenExecute(grove::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    grove.lay().declare();
                    markers[0] = new Markers(helper, level, grove.area());
                    w[0] = warriorAt(grove, 4, 0);
                    w[1] = warriorAt(grove, -4, 0);
                    w[2] = warriorAt(grove, 0, 4);
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(grove.faction) >= 1))
                .thenExecute(() -> {
                    List<CampLine> adopt = new ArrayList<>();
                    for (int i = 0; i < 3; i++)
                        adopt.add(CampLine.of(i, 1, MowzieKit.WARRIOR, ADOPT, 0, w[i].getUUID()));
                    op[0] = MowzieKit.reconcile(grove.faction, MowzieKit.epoch(MowzieKit.census(grove.faction, 1)), adopt);
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[0], 3))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(grove.faction, op[0]), DONE);
                    w[0].setHealth(9f);
                    stood[0] = w[0].position();
                    mask[0] = w[0].getMaskType();
                    Camps c = MowzieKit.camps();
                    was[0] = c.handedOff;
                    was[1] = c.taken;
                    was[2] = c.markers;
                    agent[0] = ConflictKit.newAgent();
                    ConflictKit.Band b = ConflictKit.band(agent[0], grove.faction, RtsimTests.dimension(helper));
                    b.arcX = grove.spot.x;
                    b.arcZ = grove.spot.z;
                    b.targetX = grove.spot.x + 1000;
                    b.targetY = grove.spot.y;
                    b.targetZ = grove.spot.z;
                    b.speed = 0f;
                    ConflictKit.together(() -> {
                        op[1] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(
                                CampLine.party(0, 1, MowzieKit.WARRIOR, SET_OUT, 0, agent[0], b.epoch),
                                CampLine.party(1, 1, MowzieKit.WARRIOR, SET_OUT, 0, agent[0], b.epoch)));
                        ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(MowzieKit.WARRIOR, 1, false)));
                    });
                })
                .thenWaitUntil(() -> {
                    MowzieKit.answered(grove.faction, op[1], 2);
                    bandStands(agent[0], 1);
                })
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("SET_OUT", ConflictKit.outcomes(grove.faction, op[1]), DONE);
                    Camps c = MowzieKit.camps();
                    check("one handed off (" + (c.handedOff - was[0]) + ")", c.handedOff - was[0] == 1);
                    check("one taken (" + (c.taken - was[1]) + ")", c.taken - was[1] == 1);
                    Mob p = bandStands(agent[0], 1)[0];
                    check("a puppet of the jar's warrior", p instanceof EntityUmvuthanaMinion && RtsimApi.isPuppet(p));
                    EntityUmvuthanaMinion pu = (EntityUmvuthanaMinion) p;
                    check("it stands where the body stood (" + pu.position().distanceTo(stood[0]) + " blocks off)", pu.position().distanceTo(stood[0]) < 6);
                    check("with its health (" + pu.getHealth() + ")", Math.abs(pu.getHealth() - 9f) < 1.5f);
                    check("and its mask", pu.getMaskType() == mask[0]);
                    check("bodies 0 and 1 are gone", w[0].isRemoved() && w[1].isRemoved());
                    List<Entity> held = MowzieKit.held(grove.faction);
                    check("the camp holds only body 2 (" + held.size() + ")", held.size() == 1 && held.get(0) == w[2]
                            && MowzieKit.tagged(w[2], grove.faction, 2, 1));
                    check("records on a marker (" + (c.markers - was[2]) + " made)", c.markers - was[2] >= 1
                            && markers[0].all().stream().anyMatch(m -> m.tagged() && m.carriesRecords()));
                    record(test, "set out", "slot 0 handed to its puppet, slot 1 taken; " + (c.markers - was[2]) + " markers");
                })
                .thenSucceed();
    }

    // ---- 12: REMOVE by UUID, a body elsewhere ----------------------------------------------------

    /**
     * A REMOVE by UUID takes a stray, and a body standing elsewhere is counted apart (§7 test 22). A grove with
     * three strays: ADOPT w0 and REMOVE w1 by UUID are DONE, w1 is gone and a marker carries the records.
     * w0 (no AI) is moved 80 blocks out into a chunk forced alone; the grove is released and forced again:
     * the new census lists w0 as ELSEWHERE by slot 0, generation 1 (and w2 as a stray). With the camp in, w0's
     * chunk is released until it has gone and forced again: {@code recounts} goes up 1, and a census comes with
     * no CAMP_OUT, listing w0 as ELSEWHERE again.
     */
    @GameTest(template = ARENA, timeoutTicks = 3500, batch = "mowzie_elsewhere", skyAccess = true)
    public static void aRemoveByUuidTakesAStrayAndAnElsewhereBodyIsCountedApart(GameTestHelper helper) {
        String test = "aRemoveByUuidTakesAStrayAndAnElsewhereBodyIsCountedApart";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove grove = new Grove(far, far.k, far.z0);
        int outChunk = grove.cx + 5; // 80 blocks east: 34 once the grove's chunks alone hold it
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[3];
        UUID[] id = new UUID[3];
        long[] op = new long[1], was = new long[3];
        Vec3[] w1At = new Vec3[1];
        Markers[] markers = new Markers[1];

        helper.startSequence()
                .thenExecute(() -> {
                    grove.force();
                    far.force(outChunk, grove.cz);
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    grove.lay().declare();
                    markers[0] = new Markers(helper, level, grove.area());
                    w[0] = warriorAt(grove, 4, 0);
                    w[1] = warriorAt(grove, -4, 0);
                    w[2] = warriorAt(grove, 0, 4);
                    for (int i = 0; i < 3; i++)
                        id[i] = w[i].getUUID();
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(grove.faction) >= 1))
                .thenExecute(() -> {
                    was[0] = MowzieKit.camps().removed;
                    w1At[0] = w[1].position();
                    op[0] = MowzieKit.reconcile(grove.faction, MowzieKit.epoch(MowzieKit.census(grove.faction, 1)), List.of(
                            CampLine.of(0, 1, MowzieKit.WARRIOR, ADOPT, 0, id[0]),
                            CampLine.of(1, 1, MowzieKit.WARRIOR, REMOVE, 0, id[1])));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[0], 2))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT and REMOVE", ConflictKit.outcomes(grove.faction, op[0]), DONE);
                    check("the stray w1 is removed", w[1].isRemoved());
                    check("one removal counted", MowzieKit.camps().removed - was[0] == 1);
                    List<Markers.Seen> m = markers[0].near(w1At[0], 3);
                    check("a marker where it stood (" + m.size() + ")", !m.isEmpty() && m.get(0).tagged() && m.get(0).carriesRecords());
                    check("w0 is tagged (faction, 0, 1) and w2 is a stray", MowzieKit.tagged(w[0], grove.faction, 0, 1) && MowzieKit.tag(w[2]) == null);
                    Vec3 far0 = grove.at(80, 0);
                    w[0].teleportTo(far0.x, far0.y, far0.z);
                    check("w0 stands in the forced chunk", w[0].chunkPosition().x == outChunk && w[0].chunkPosition().z == grove.cz);
                    grove.far.releaseRect(grove.cx - 2, grove.cz - 2, grove.cx + 2, grove.cz + 2);
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(grove.faction).size() == 1))
                .thenWaitUntil(gone(level, id[2]))
                .thenExecute(grove::force)
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(grove.faction) >= 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(grove.faction, 2);
                    CensusBody b0 = MowzieKit.bodyOf(c, id[0]), b2 = MowzieKit.bodyOf(c, id[2]);
                    check("census 2 lists w0", b0 != null);
                    check("as elsewhere, slot 0, generation 1 (" + b0.flags() + ")", (b0.flags() & ConflictRecords.CampCensus.BODY_ELSEWHERE) != 0
                            && b0.slot() == 0 && b0.generation() == 1);
                    check("and w2 as a stray", b2 != null && (b2.flags() & ConflictRecords.CampCensus.BODY_STRAY) != 0);
                    check("w1 is not listed", MowzieKit.bodyOf(c, id[1]) == null);
                    was[0] = MowzieKit.camps().recounts;
                    was[1] = ConflictKit.campOuts(grove.faction).size();
                    was[2] = MowzieKit.censusCount(grove.faction);
                    check("the camp is in", grove.site().in());
                    far.release(outChunk, grove.cz);
                })
                .thenWaitUntil(gone(level, id[0]))
                .thenExecute(() -> far.force(outChunk, grove.cz))
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("a third census (" + MowzieKit.censusCount(grove.faction) + ")", MowzieKit.censusCount(grove.faction) > was[2]))
                .thenExecute(() -> {
                    check("one recount (" + (MowzieKit.camps().recounts - was[0]) + ")", MowzieKit.camps().recounts - was[0] == 1);
                    check("no new CAMP_OUT", ConflictKit.campOuts(grove.faction).size() == was[1]);
                    ScriptedEngine.Received c = MowzieKit.census(grove.faction, (int) was[2] + 1);
                    CensusBody b0 = MowzieKit.bodyOf(c, id[0]);
                    check("w0 is elsewhere again, slot 0, generation 1", b0 != null && (b0.flags() & ConflictRecords.CampCensus.BODY_ELSEWHERE) != 0
                            && b0.slot() == 0 && b0.generation() == 1);
                    record(test, "elsewhere", "w0 counted ELSEWHERE twice; the reload from disk made a recount and no CAMP_OUT");
                })
                .thenSucceed();
    }

    // ---- MZB-249, MZB-290: the poll alone --------------------------------------------------------

    /**
     * A camp follows its chunks through the level poll alone (MZB-290), as on a server where C2ME runs the
     * chunk system and no {@code ChunkTicketLevelUpdatedEvent} is ever posted (MZB-249). The reconciler is
     * told to ignore the event for 1,500 ticks. A den counted; its chunks released: CAMP_OUT within a poll
     * period, found by the poll (the change counter moves, which it would not if the event had moved the
     * levels first); the chunks unloaded and forced again: census 2 within a poll period of the chunks
     * being ready, a new epoch.
     */
    @GameTest(template = ARENA, timeoutTicks = 2500, batch = "mowzie_poll", skyAccess = true)
    public static void aCampFollowsItsChunksThroughThePollWhenTheTicketEventIsSilent(GameTestHelper helper) {
        String test = "aCampFollowsItsChunksThroughThePollWhenTheTicketEventIsSilent";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        Camps camps = MowzieKit.camps();
        long[] t = new long[5]; // released, out seen, forced again, chunks ready, in seen
        long[] n = new long[3]; // polls and level changes before the release; level changes before the reload
        long[] epoch = new long[1];

        helper.startSequence()
                .thenExecute(den::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    camps.ticketEventsSilentUntil = level.getGameTime() + 1_500;
                    ConflictKit.onEnd(helper, () -> camps.ticketEventsSilentUntil = 0);
                    den.lay().declare();
                    MowzieKit.everyTick(helper, () -> {
                        long now = level.getGameTime();
                        Camps.Site s = den.site();
                        if (t[0] != 0 && t[1] == 0 && !s.in())
                            t[1] = now;
                        if (t[2] != 0 && t[3] == 0) {
                            boolean ready = true;
                            for (ChunkPos p : s.footprintChunks())
                                ready &= level.hasChunk(p.x, p.z) && level.areEntitiesLoaded(p.toLong());
                            if (ready)
                                t[3] = now;
                        }
                        if (t[3] != 0 && t[4] == 0 && s.in())
                            t[4] = now;
                    });
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> {
                    epoch[0] = MowzieKit.epoch(MowzieKit.census(den.faction, 1));
                    n[0] = camps.polls;
                    n[1] = camps.pollChanges;
                    t[0] = level.getGameTime();
                    den.release();
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenExecute(() -> {
                    check("out within a poll period of the release (" + (t[1] - t[0]) + " ticks)",
                            t[1] != 0 && t[1] - t[0] <= Camps.POLL_PERIOD + 3);
                    check("the camp is out", !den.site().in());
                    check("the poll counted camps (" + (camps.polls - n[0]) + ")", camps.polls > n[0]);
                    check("the poll found the levels moved (" + (camps.pollChanges - n[1]) + " chunks), not the event", camps.pollChanges > n[1]);
                    record(test, "out", (t[1] - t[0]) + " ticks after the release, " + (camps.pollChanges - n[1]) + " chunk levels found by the poll");
                })
                .thenWaitUntil(io(() -> {
                    for (ChunkPos p : den.site().footprintChunks())
                        check("chunk (" + p.x + ", " + p.z + ") is unloaded", !level.hasChunk(p.x, p.z));
                }))
                .thenExecute(() -> {
                    n[2] = camps.pollChanges;
                    t[2] = level.getGameTime();
                    den.force();
                })
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    check("in within a poll period of the chunks being ready (" + (t[4] - t[3]) + " ticks)",
                            t[3] != 0 && t[4] != 0 && t[4] - t[3] <= Camps.POLL_PERIOD + 3);
                    check("the poll found the levels moved again (" + (camps.pollChanges - n[2]) + " chunks)", camps.pollChanges > n[2]);
                    check("a new epoch", MowzieKit.epoch(MowzieKit.census(den.faction, 2)) != epoch[0]);
                    check("one CAMP_OUT in all", ConflictKit.campOuts(den.faction).size() == 1);
                    record(test, "in", (t[4] - t[3]) + " ticks after the chunks were ready, " + (camps.pollChanges - n[2]) + " chunk levels found by the poll");
                })
                .thenSucceed();
    }

    // ---- P3-5: a grove with no free place --------------------------------------------------------

    /**
     * A grove with no free place for a warrior answers a SPAWN STALE and makes no body (MZB-243): the kind
     * tries 20 places, each checked for collision and liquid, as the jar does. A leaf block over every
     * column within 24 blocks of the centre, at the height a new warrior would stand (leaves are no part of
     * the NO_LEAVES height map the kind reads, so each place would put the body inside one): the kind finds
     * no place, a SPAWN is STALE with no spawn counted and no warrior in the area, with no player near and
     * with one 60 blocks away (the near ring and the far edge both tried). With the leaves cleared the same
     * line is DONE, the warrior in the open: the stale answer was not final.
     */
    @GameTest(template = ARENA, timeoutTicks = 2000, batch = "mowzie_no_room", skyAccess = true)
    public static void aGroveWithNoFreePlaceAnswersASpawnStaleAndMakesNoBody(GameTestHelper helper) {
        String test = "aGroveWithNoFreePlaceAnswersASpawnStaleAndMakesNoBody";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove grove = new Grove(far, far.k, far.z0);
        Camps camps = MowzieKit.camps();
        long[] op = new long[3];
        long[] spawned = new long[1];
        int[] y = new int[1];
        ServerPlayer[] player = new ServerPlayer[1];

        helper.startSequence()
                .thenExecute(() -> grove.force())
                .thenWaitUntil(far.ready())
                .thenExecute(() -> grove.lay().declare())
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(grove.faction) >= 1))
                .thenExecute(() -> {
                    y[0] = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) Math.floor(grove.spot.x), (int) Math.floor(grove.spot.z));
                    leaves(level, grove, y[0], true);
                    check("the leaves do not move the height map", level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES,
                            (int) Math.floor(grove.spot.x) + 5, (int) Math.floor(grove.spot.z) - 7) == y[0]);
                    spawned[0] = camps.spawned;
                    check("the kind finds no place", GroveKind.INSTANCE.spawnAt(level, grove.site(), MowzieKit.WARRIOR, 0) == null);
                    op[0] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(CampLine.of(1, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the SPAWN with no player near", ConflictKit.outcomes(grove.faction, op[0]), STALE);
                    noBody(level, grove, camps, spawned[0]);
                    player[0] = FakePlayers.add(level, grove.spot.add(60, 1, 0), "mowzie-no-room");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(player[0]));
                    check("the kind finds no place with the player near", GroveKind.INSTANCE.spawnAt(level, grove.site(), MowzieKit.WARRIOR, 0) == null);
                    op[1] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(CampLine.of(1, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[1], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the SPAWN with a player 60 blocks east", ConflictKit.outcomes(grove.faction, op[1]), STALE);
                    noBody(level, grove, camps, spawned[0]);
                    FakePlayers.remove(player[0]);
                    leaves(level, grove, y[0], false);
                    op[2] = MowzieKit.reconcile(grove.faction, grove.site().epoch(), List.of(CampLine.of(1, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(() -> MowzieKit.answered(grove.faction, op[2], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the same SPAWN with the leaves cleared", ConflictKit.outcomes(grove.faction, op[2]), DONE);
                    List<Entity> h = MowzieKit.held(grove.faction, 1, 1);
                    check("one warrior, in slot 1 (" + h.size() + ")", h.size() == 1 && camps.spawned == spawned[0] + 1);
                    check("standing in the open", level.noCollision(h.get(0)));
                    record(test, "no room", "STALE twice with no body, DONE once the leaves were cleared");
                })
                .thenSucceed();
    }

    /** A leaf block (persistent, so it does not decay) at {@code y} over every column within 24 blocks of the grove's centre, or air. */
    private static void leaves(ServerLevel level, Grove g, int y, boolean on) {
        BlockState state = on
                ? Blocks.OAK_LEAVES.defaultBlockState().setValue(LeavesBlock.PERSISTENT, true)
                : Blocks.AIR.defaultBlockState();
        int cx = (int) Math.floor(g.spot.x), cz = (int) Math.floor(g.spot.z);
        for (int x = cx - 24; x <= cx + 24; x++)
            for (int z = cz - 24; z <= cz + 24; z++)
                level.setBlock(new BlockPos(x, y, z), state, 2);
    }

    /** No spawn was counted, slot 1 has no body, and no warrior stands anywhere in the grove's area. */
    private static void noBody(ServerLevel level, Grove g, Camps camps, long spawned) {
        check("no spawn was counted", camps.spawned == spawned);
        check("slot 1 has no body", MowzieKit.held(g.faction, 1, 1).isEmpty());
        check("no warrior stands in the area", level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area()).isEmpty());
    }

    // ---- MZB-590: the index and an unload --------------------------------------------------------

    /**
     * A body whose chunk unloads leaves the camp's index within a poll period (MZB-590): vanilla posts no leave
     * event for the unload, so the removed entity would stay in the faction's set for ever. A den's adopted
     * Frostmaw, the den released and unloaded: within {@link Camps#POLL_PERIOD} + 3 ticks the raw index
     * ({@code Camps.held}) holds no removed entity. Forced again, the census comes in again and the index holds
     * the Frostmaw the load made (another object, the same UUID), tagged, with no removed entity.
     */
    @GameTest(template = ARENA, timeoutTicks = 2500, batch = "mowzie_index_unload", skyAccess = true)
    public static void anUnloadedBodyLeavesTheIndexAtTheNextPollAndALoadIndexesItAgain(GameTestHelper helper) {
        String test = "anUnloadedBodyLeavesTheIndexAtTheNextPollAndALoadIndexesItAgain";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        EntityFrostmaw[] f = new EntityFrostmaw[1];
        UUID[] id = new UUID[1];
        long[] op = new long[1];
        long[] removedAt = new long[1], heldRemovedFor = new long[1];

        helper.startSequence()
                .thenExecute(() -> den.force())
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    den.lay().declare();
                    f[0] = MowzieKit.frostmaw(level, den.at(3, 0.5), true);
                    id[0] = f[0].getUUID();
                    MowzieKit.everyTick(helper, () -> {
                        if (f[0].isRemoved()) {
                            if (removedAt[0] == 0)
                                removedAt[0] = level.getGameTime();
                            if (MowzieKit.camps().held(den.faction).contains(f[0]))
                                heldRemovedFor[0]++;
                        }
                    });
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                        List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF, id[0]))))
                .thenWaitUntil(() -> MowzieKit.answered(den.faction, op[0], 1))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    check("held by the camp", MowzieKit.camps().held(den.faction).contains(f[0]));
                    den.release();
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(den.faction).size() == 1))
                .thenWaitUntil(gone(level, id[0]))
                .thenExecute(() -> check("the Frostmaw object is removed", f[0].isRemoved()))
                .thenIdle(Camps.POLL_PERIOD + 3)
                .thenExecute(() -> {
                    check("the raw index holds no removed entity (" + MowzieKit.camps().held(den.faction).size() + " entries)",
                            MowzieKit.camps().held(den.faction).stream().noneMatch(Entity::isRemoved));
                    check("the removed Frostmaw is not in it", !MowzieKit.camps().held(den.faction).contains(f[0]));
                    record(test, "pruned", "removed at tick " + removedAt[0] + ", still in the raw index for " + heldRemovedFor[0] + " ticks after, then dropped");
                    den.force();
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(den.faction) >= 2))
                .thenExecute(() -> {
                    List<Entity> held = MowzieKit.camps().held(den.faction);
                    check("the index holds the one Frostmaw again (" + held.size() + ")", held.size() == 1);
                    Entity again = held.get(0);
                    check("a new object with the old UUID", again != f[0] && again.getUUID().equals(id[0]) && !again.isRemoved());
                    check("tagged slot 0, generation 1", MowzieKit.tagged(again, den.faction, 0, 1));
                    check("census 2 lists it home", MowzieKit.bodyOf(MowzieKit.census(den.faction, 2), id[0]) != null);
                    record(test, "indexed again", "the load's join indexed the new Frostmaw object");
                })
                .thenSucceed();
    }

    // @@NEXT@@
}
