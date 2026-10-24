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
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CensusBody;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
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
import net.minecraft.world.level.block.Rotation;
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
                    int top = 0;
                    for (int l : den.site().levels())
                        top = Math.max(top, l);
                    check("a footprint chunk's level is above FULL (" + top + ")", top > Camps.FULL);
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

    // @@NEXT@@
}
