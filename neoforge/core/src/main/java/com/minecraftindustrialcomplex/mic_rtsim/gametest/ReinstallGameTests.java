package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallGuard;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallRules;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.AfterBatch;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.storage.LevelResource;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.neoforged.fml.ModList;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.common.CommonHooks;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.stream.Stream;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/**
 * The reinstall guard (UR-1, docs/choices-reinstall-guard.md): a world that ran without rtsim is reset whole at the
 * next start, and the districts whose seeds the gap stripped are adopted again. The test emulates an uninstall in
 * the running GameTest server: {@code level.dat} is read through NeoForge's own hook with a mod list that lacks
 * {@code mic_rtsim} (as a server without rtsim would have written it), rtsim's chunk attachments are stripped from
 * the chunks that "loaded during the gap", the world clock moves on, and the session restarts.
 *
 * <p>Three far chunks, each with one machine: A and C side by side (district X), B seventeen chunks on (district Y).
 * A stays loaded and is stripped (re-adopted at the session's start), C stays loaded and keeps its seeds (it "did
 * not load during the gap"), and B is stripped and unloads, so it is re-adopted when it loads again. A fourth chunk,
 * D, carries a pending COMMAND_RESULT of the old engine across the gap without loading in it: at its load after
 * the reset the record is dropped, not journaled into the new engine (RG14). B and D lie more than 13 chunks from
 * every forced chunk, so unforcing them really unloads them (a nearer chunk stays in memory below FULL and comes
 * back without a load event). The test runs alone in its batch: a restart is the whole server's.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class ReinstallGameTests {
    private ReinstallGameTests() {}

    private static final String YARD = "industry_yard";
    /** Chunk offsets from the yard's first whole chunk: B is far enough from A and C to unload while they stay forced. */
    private static final int A = 6, C = 7, B = 24, D_DZ = 16;
    private static final long GAP_TICKS = 6000;

    private static Path worldDir(MinecraftServer server) {
        return server.getWorldPath(LevelResource.ROOT).toAbsolutePath().normalize();
    }

    /** The reset's {@code rtsim-reinstall-*} directories in the world directory. */
    private static List<Path> asides(MinecraftServer server) {
        List<Path> out = new ArrayList<>();
        try (DirectoryStream<Path> s = Files.newDirectoryStream(worldDir(server), ReinstallRules.ASIDE_PREFIX + "*")) {
            s.forEach(out::add);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        return out;
    }

    private static void deleteTree(Path p) {
        try (Stream<Path> walk = Files.walk(p)) {
            for (Path q : walk.sorted(Comparator.reverseOrder()).toList())
                Files.deleteIfExists(q);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    /** What a NeoForge server without rtsim writes into {@code level.dat}: the loaded mods but {@code mic_rtsim}, and the game time. */
    private static CompoundTag levelDatWithoutRtsim(MinecraftServer server) {
        ListTag mods = new ListTag();
        ModList.get().getMods().forEach(m -> {
            if (m.getModId().equals(MicRtsim.MODID))
                return;
            CompoundTag t = new CompoundTag();
            t.putString("ModId", m.getModId());
            t.putString("ModVersion", m.getVersion().toString());
            mods.add(t);
        });
        CompoundTag fml = new CompoundTag();
        fml.put("LoadingModList", mods);
        CompoundTag data = new CompoundTag();
        data.putLong("Time", server.overworld().getGameTime());
        CompoundTag root = new CompoundTag();
        root.put("fml", fml);
        root.put("Data", data);
        return root;
    }

    /** Drops every rtsim attachment from the chunk, as a save without rtsim does (NeoForge drops unknown attachment types). */
    private static int strip(LevelChunk chunk) {
        int n = 0;
        for (Map.Entry<net.minecraft.resources.ResourceKey<AttachmentType<?>>, AttachmentType<?>> e : NeoForgeRegistries.ATTACHMENT_TYPES.entrySet())
            if (e.getKey().location().getNamespace().startsWith(MicRtsim.MODID) && chunk.removeData(e.getValue()) != null)
                n++;
        chunk.setUnsaved(true);
        return n;
    }

    private static long[] seedsOf(LevelChunk chunk) {
        IndustryAttachments.Seeds s = chunk.getExistingDataOrNull(IndustryAttachments.SEEDS.get());
        return s == null ? new long[0] : s.positions().clone();
    }

    private static IndustryAttachments.Epochs epochsAt(ServerLevel level, ChunkPos p) {
        LevelChunk ch = level.getChunkSource().getChunkNow(p.x, p.z);
        IndustryAttachments.Epochs e = ch == null ? null : ch.getExistingDataOrNull(IndustryAttachments.RECONCILE.get());
        return e == null ? IndustryAttachments.Epochs.EMPTY : e;
    }

    private static String epochs(ServerLevel level, ChunkPos p) {
        List<String> out = new ArrayList<>();
        for (IndustryAttachments.EpochEntry e : epochsAt(level, p).entries())
            out.add("district " + e.district() + " epoch " + e.epoch() + " life " + Long.toHexString(e.life()));
        return out.toString();
    }

    private static String describe(District d) {
        if (d == null)
            return "no district";
        List<Long> chunks = new ArrayList<>(d.chunks);
        chunks.sort(null);
        List<String> seeds = new ArrayList<>();
        d.seedCounts.forEach((c, n) -> seeds.add(c + "=" + n));
        seeds.sort(null);
        return "district " + d.id + " chunks " + chunks + " seeds " + seeds;
    }

    /**
     * The gap emulated, then the restart: the sim resets loudly (pages moved aside, a reset in {@code rtsim perf}),
     * the modules announce again, X and Y are adopted again (A at the start, B at its load, C kept), and the
     * districts are what they were. The epoch the old engine applied in C is void, so the new engine's epoch 1
     * applies there (RG13). The first save deletes the marker, and the next restart is a normal one.
     */
    @GameTest(template = YARD, batch = "reinstall_1", timeoutTicks = 20000)
    public static void aReinstallResetsTheSimAndAdoptsTheStrippedDistrictsAgain(GameTestHelper helper) {
        String test = "aReinstallResetsTheSimAndAdoptsTheStrippedDistrictsAgain";
        fastConfig();
        forgetAll(helper);
        ServerLevel level = helper.getLevel();
        MinecraftServer server = level.getServer();
        BlockPos base = inChunk(helper, 0, 0);
        int bx = base.getX() >> 4, bz = base.getZ() >> 4;
        ChunkPos a = new ChunkPos(bx + A, bz), c = new ChunkPos(bx + C, bz), b = new ChunkPos(bx + B, bz), d = new ChunkPos(bx + B, bz + D_DZ);
        BlockPos[] ms = new BlockPos[3];
        long[] ids = new long[2];
        String[] before = new String[2];
        Path[] aside = new Path[1];
        long[] life = new long[1], dups = new long[1], staleOp = new long[1], counts = new long[2];
        helper.startSequence()
                .thenExecute(() -> {
                    // The gap's premise (RG13): rtsim saved this world before it, so the journal no longer holds
                    // what the old pages included, and the reset's empty pages lose observations. The run's first
                    // autosave (6,000 ticks after the start) makes that save when the tests before this one took
                    // longer; after fewer ticks the journal still starts at observation 1, the reset's engine
                    // replays all of it and does not start over. So the test asks for the save itself.
                    if (!rtsimSavedBefore())
                        RtsimTests.engine().requestSaveAtNextStep();
                })
                .thenWaitUntil(RtsimTests.io(() -> check("an engine save was acknowledged before the gap: the journal holds "
                        + journalSpan(), rtsimSavedBefore())))
                .thenExecute(() -> {
                    cleanUp(server);
                    for (ChunkPos p : new ChunkPos[] {a, c, b}) {
                        level.setChunkForced(p.x, p.z, true);
                        level.getChunk(p.x, p.z);
                    }
                    // Earlier tests leave block entities near: a leftover machine next to X or Y would join it at a detection.
                    for (int x = bx + A - 2; x <= bx + B + 2; x++)
                        for (int z = bz - 2; z <= bz + 2; z++) {
                            LevelChunk ch = level.getChunkSource().getChunkNow(x, z);
                            if (ch != null) {
                                for (BlockPos p : new ArrayList<>(ch.getBlockEntities().keySet()))
                                    level.removeBlock(p, false);
                                ch.removeData(IndustryAttachments.SEEDS.get());
                                ch.setUnsaved(true);
                            }
                        }
                    int i = 0;
                    for (ChunkPos p : new ChunkPos[] {a, c, b}) {
                        BlockPos m = new BlockPos(p.getMinBlockX() + 4, floorY(helper), p.getMinBlockZ() + 4);
                        set(helper, m, TestMachines.MACHINE.get().defaultBlockState());
                        playerPlaced(helper, m);
                        ms[i++] = m;
                    }
                })
                .thenWaitUntil(() -> {
                    District x = districtAt(helper, ms[0]), y = districtAt(helper, ms[2]);
                    check("X holds A and C with a seed each: " + describe(x), x != null && x.chunks.size() == 2 && x == districtAt(helper, ms[1]) && x.seeds() == 2);
                    check("Y holds B: " + describe(y), y != null && y != x && y.chunks.size() == 1 && y.seeds() == 1);
                    check("no detection is running", !ind().districts().rescanning());
                    ids[0] = x.id;
                    ids[1] = y.id;
                    before[0] = describe(x);
                    before[1] = describe(y);
                })
                .thenExecute(() -> {
                    // An epoch the old engine applied in C (I8, RG13). The reset's engine numbers X's epochs from 1 again.
                    life[0] = ind().engineLife();
                    reconcile(engineOp(), ids[0], 40, level, c.x, c.z, List.of());
                    // D: a transfer of the old engine applied into it, not acknowledged before D unloads (RG14).
                    level.setChunkForced(d.x, d.z, true);
                    LevelChunk dc = level.getChunk(d.x, d.z);
                    dc.removeData(RtsimAttachments.PENDING);
                    staleOp[0] = engineOp();
                    ind().commandResultInChunk(dc, 0, staleOp[0], Obs.CommandResult.APPLIED);
                    check("D carries the old engine's result", RtsimSession.chunkCarriesOp(dc, staleOp[0])
                            && dc.getData(RtsimAttachments.PENDING).life() == life[0]);
                    level.setChunkForced(d.x, d.z, false);
                })
                .thenWaitUntil(() -> check("C holds X's epoch 40: " + epochs(level, c), epochsAt(level, c).epoch(ids[0], life[0]) == 40))
                .thenExecute(() -> {
                    // The gap begins for B: it loads without rtsim (its attachments go) and unloads. No periodic detection meanwhile.
                    IndustryConfig.Test.set(IndustryConfig.PERIODIC_RESCAN, 1_728_000);
                    check("B had rtsim attachments", strip(level.getChunk(b.x, b.z)) > 0);
                    level.setChunkForced(b.x, b.z, false);
                })
                .thenWaitUntil(() -> check("B and D have unloaded", level.getChunkSource().getChunkNow(b.x, b.z) == null
                        && level.getChunkSource().getChunkNow(d.x, d.z) == null && ind().districts().loadedChunks(ind().district(ids[1])) == 0))
                .thenExecute(() -> {
                    // The rest of the gap, in one tick: level.dat without rtsim, A loaded without it, the clock on, the restart.
                    CommonHooks.readAdditionalLevelSaveData(levelDatWithoutRtsim(server), new LevelStorageSource.LevelDirectory(worldDir(server)));
                    ReinstallGuard.Capture cap = ReinstallGuard.pendingCapture(server);
                    check("the read saw the gap: " + cap, cap != null && cap.verdict() == ReinstallRules.Verdict.GAP);
                    check("the marker is written", Files.exists(worldDir(server).resolve(ReinstallRules.MARKER)));
                    check("A had rtsim attachments", strip(level.getChunk(a.x, a.z)) > 0);
                    long t = server.overworld().getGameTime();
                    server.getWorldData().overworldData().setGameTime(t + GAP_TICKS);
                    RtsimSession.restartGlobalForTest(server);

                    check("a session runs after the reset: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                    ReinstallGuard.Reset reset = ReinstallGuard.reset(server);
                    check("the start reset the sim", reset != null);
                    aside[0] = reset.aside();
                    check("the old pages are moved aside: " + reset.aside(), reset.aside() != null && Files.isDirectory(reset.aside()) && asides(server).size() == 1);
                    String line = ReinstallGuard.describe(server);
                    record(test, "rtsim perf after the restart", line);
                    check("rtsim perf says the sim was reset: " + line, line.contains("SIM RESET"));
                    check("the restart's engine lost its observations", RtsimSession.current().boundary().observationsLost());
                    check("the reset's engine has a new life", ind().engineLife() != life[0]);
                    check("A, B and C are to be adopted again: " + ind().districts().readoptLeft(), ind().districts().readoptLeft() == 3);
                    check("the marker stays after the restart's own save", !ReinstallGuard.markerCleared(server)
                            && Files.exists(worldDir(server).resolve(ReinstallRules.MARKER)));
                })
                .thenWaitUntil(() -> {
                    Districts ds = ind().districts();
                    check("the session started: A adopted again, C kept, B not loaded: " + ds.readoptSummary(),
                            ds.readoptChunks() == 1 && ds.readoptKept() == 1 && ds.readoptLeft() == 1);
                    check("A's machine is a seed again", Arrays.equals(seedsOf(level.getChunk(a.x, a.z)), new long[] {ms[0].asLong()}));
                    check("C kept its seed", Arrays.equals(seedsOf(level.getChunk(c.x, c.z)), new long[] {ms[1].asLong()}));
                    check("C's epoch from the old engine is void: " + epochs(level, c) + ", " + ind().reconciler().staleEpochs() + " dropped",
                            epochsAt(level, c).get(ids[0]) == null && ind().reconciler().staleEpochs() == 1);
                })
                .thenExecute(() -> {
                    // The new engine's first reconcile of X in C applies; with the old epoch kept, it was a duplicate (RG13).
                    dups[0] = ind().reconciler().duplicates();
                    reconcile(engineOp(), ids[0], 1, level, c.x, c.z, List.of());
                })
                .thenWaitUntil(() -> check("the reconcile was answered: " + epochs(level, c),
                        epochsAt(level, c).get(ids[0]) != null || ind().reconciler().duplicates() > dups[0]))
                .thenExecute(() -> {
                    check("the new engine's epoch 1 is applied in C, no duplicate: " + epochs(level, c) + ", duplicates "
                                    + (ind().reconciler().duplicates() - dups[0]),
                            epochsAt(level, c).epoch(ids[0], ind().engineLife()) == 1 && ind().reconciler().duplicates() == dups[0]);
                    record(test, "epochs", "C held X's epoch 40 of life " + Long.toHexString(life[0]) + "; after the reset, life "
                            + Long.toHexString(ind().engineLife()) + ": " + epochs(level, c));
                })
                .thenExecute(() -> {
                    // D loads: its record is of the old life, no op of the new engine, and dropped at the tick's end unjournaled.
                    RtsimSession s = RtsimSession.current();
                    counts[0] = s.stalePending();
                    counts[1] = s.reasserted();
                    level.setChunkForced(d.x, d.z, true);
                    LevelChunk dc = level.getChunk(d.x, d.z);
                    PendingRecords p = dc.getExistingDataOrNull(RtsimAttachments.PENDING);
                    check("D kept the old engine's record through the gap: " + p, p != null && p.life() == life[0] && p.records().size() == 1);
                    check("an op id of the old engine is no op of the new one", !RtsimSession.chunkCarriesOp(dc, staleOp[0]));
                })
                .thenWaitUntil(() -> {
                    RtsimSession s = RtsimSession.current();
                    check("D's load dropped the record unjournaled: stale " + (s.stalePending() - counts[0]) + ", journaled again "
                                    + (s.reasserted() - counts[1]),
                            !level.getChunk(d.x, d.z).hasData(RtsimAttachments.PENDING) && s.stalePending() == counts[0] + 1
                                    && s.reasserted() == counts[1]);
                    record(test, "pending records", "D held 1 record of life " + Long.toHexString(life[0]) + "; dropped at its load into life "
                            + Long.toHexString(ind().engineLife()));
                    level.setChunkForced(d.x, d.z, false);
                })
                .thenExecute(() -> {
                    level.setChunkForced(b.x, b.z, true);
                    level.getChunk(b.x, b.z);
                })
                .thenWaitUntil(() -> {
                    Districts ds = ind().districts();
                    check("B adopted again at its load: " + ds.readoptSummary(), ds.readoptChunks() == 2 && ds.readoptLeft() == 0 && ds.readoptSeeds() == 2);
                    check("B's machine is a seed again", Arrays.equals(seedsOf(level.getChunk(b.x, b.z)), new long[] {ms[2].asLong()}));
                    record(test, "re-adoption", ds.readoptSummary());
                    // The start's periodic detection, as after any start.
                    IndustryConfig.Test.set(IndustryConfig.PERIODIC_RESCAN, null);
                })
                .thenIdle(2)
                .thenWaitUntil(() -> check("the first detection is done", !ind().districts().rescanning()))
                .thenExecute(() -> {
                    ind().districts().rescanNow(level);
                    String x = describe(ind().district(ids[0])), y = describe(ind().district(ids[1]));
                    record(test, "X before / after", before[0] + " / " + x);
                    record(test, "Y before / after", before[1] + " / " + y);
                    check("X is what it was: " + x + " against " + before[0], x.equals(before[0]));
                    check("Y is what it was: " + y + " against " + before[1], y.equals(before[1]));
                    for (long id : ids)
                        check("the new engine heard of district " + id, !received(IndRecords.IND_DISTRICT, IndRecords.District.DISTRICT, id).isEmpty());
                    check("no reconcile is held for the new engine", ind().reconciler().held() == 0);
                    // An autosave may have come first; else this is the first save.
                    server.saveEverything(true, true, true);
                })
                .thenWaitUntil(() -> check("the first save deleted the marker", ReinstallGuard.markerCleared(server)
                        && !Files.exists(worldDir(server).resolve(ReinstallRules.MARKER))))
                .thenExecute(() -> {
                    check("perf says the marker went", ReinstallGuard.describe(server).contains("marker deleted"));
                    // The next restart is a normal one: no read of level.dat, no marker, no reset.
                    RtsimSession.restartGlobalForTest(server);
                    check("a session runs: " + RtsimSession.startFailure(), RtsimSession.current() != null);
                    check("the next start did not reset", ReinstallGuard.reset(server) == null && asides(server).size() == 1);
                    check("nothing left to adopt again", ind().districts().readoptLeft() == 0 && ind().districts().readoptSummary() == null);
                    record(test, "rtsim perf after a normal restart", ReinstallGuard.describe(server));
                    record(test, "engine", engineName());
                    for (ChunkPos p : new ChunkPos[] {a, c, b, d})
                        level.setChunkForced(p.x, p.z, false);
                    cleanUp(server);
                })
                .thenSucceed();
    }

    /** A {@code level.dat} that lists rtsim, as every save with rtsim writes it: no gap, no marker. */
    @GameTest(template = YARD, batch = "reinstall_2", timeoutTicks = 200)
    public static void aLevelDatThatListsRtsimIsNoGap(GameTestHelper helper) {
        MinecraftServer server = helper.getLevel().getServer();
        cleanUp(server);
        CompoundTag root = levelDatWithoutRtsim(server);
        CompoundTag self = new CompoundTag();
        self.putString("ModId", MicRtsim.MODID);
        self.putString("ModVersion", ModList.get().getModContainerById(MicRtsim.MODID).orElseThrow().getModInfo().getVersion().toString());
        root.getCompound("fml").getList("LoadingModList", 10).add(self);
        CommonHooks.readAdditionalLevelSaveData(root, new LevelStorageSource.LevelDirectory(worldDir(server)));
        ReinstallGuard.Capture cap = ReinstallGuard.pendingCapture(server);
        check("the read saw no gap: " + cap, cap != null && cap.verdict() == ReinstallRules.Verdict.NO_GAP);
        check("no marker", !Files.exists(worldDir(server).resolve(ReinstallRules.MARKER)));
        cleanUp(server);
        helper.succeed();
    }

    /**
     * {@code mic_rtsim:pending} keeps its life and each record's generation through NBT; {life, records}, written
     * before generations, reads with generation 0, and a plain list, written before the life existed, as life 0 (RG14).
     */
    @GameTest(template = YARD, batch = "reinstall_2", timeoutTicks = 200)
    public static void pendingRecordsKeepTheirEngineLife(GameTestHelper helper) {
        byte[] a = {1, 2, 3}, b = {4, 5};
        PendingRecords p = PendingRecords.EMPTY.plus(0x5eedL, 11, List.of(a)).plus(0x5eedL, 12, List.of(b));
        net.minecraft.nbt.Tag tag = PendingRecords.CODEC.encodeStart(net.minecraft.nbt.NbtOps.INSTANCE, p).getOrThrow();
        PendingRecords back = PendingRecords.CODEC.parse(net.minecraft.nbt.NbtOps.INSTANCE, tag).getOrThrow();
        check("a round trip keeps the life and the records: " + tag, back.life() == 0x5eedL && back.records().size() == 2
                && Arrays.equals(back.records().get(0), a) && Arrays.equals(back.records().get(1), b)
                && back.entries().get(0).gen() == 11 && back.entries().get(1).gen() == 12);
        check("records of another life are not carried on", p.plus(7, 13, List.of(a)).records().size() == 1 && p.plus(7, 13, List.of(a)).life() == 7);
        // A write into a holder that still carries another life's records (a chunk written in the tick it loaded,
        // before its end-of-tick reassert) drops them and counts them stale, as the reassert would.
        RtsimSession s = RtsimSession.current();
        check("a session runs", s != null);
        long stale = s.stalePending();
        PendingRecords now = s.carried(PendingRecords.EMPTY.plus(s.engineLife() ^ 1, 11, List.of(a, b)), List.of(a));
        check("a write over another life's two records counts them stale: " + (s.stalePending() - stale),
                s.stalePending() == stale + 2 && now.life() == s.engineLife() && now.records().size() == 1);
        check("a write over this life's records keeps them, nothing stale",
                s.carried(now, List.of(b)).records().size() == 2 && s.stalePending() == stale + 2);
        net.minecraft.nbt.CompoundTag ungenerated = new net.minecraft.nbt.CompoundTag();
        ungenerated.putLong("life", 0x5eedL);
        ungenerated.put("records", com.mojang.serialization.Codec.BYTE_BUFFER.listOf()
                .encodeStart(net.minecraft.nbt.NbtOps.INSTANCE, List.of(java.nio.ByteBuffer.wrap(b))).getOrThrow());
        PendingRecords u = PendingRecords.CODEC.parse(net.minecraft.nbt.NbtOps.INSTANCE, ungenerated).getOrThrow();
        check("{life, records} reads with generation 0: " + ungenerated, u.life() == 0x5eedL && u.entries().size() == 1
                && u.entries().get(0).gen() == 0 && Arrays.equals(u.records().get(0), b));
        net.minecraft.nbt.Tag old = com.mojang.serialization.Codec.BYTE_BUFFER.listOf()
                .encodeStart(net.minecraft.nbt.NbtOps.INSTANCE, List.of(java.nio.ByteBuffer.wrap(a))).getOrThrow();
        PendingRecords o = PendingRecords.CODEC.parse(net.minecraft.nbt.NbtOps.INSTANCE, old).getOrThrow();
        check("an old plain list reads as life 0: " + old, o.life() == 0 && o.records().size() == 1 && Arrays.equals(o.records().get(0), a));
        helper.succeed();
    }

    /**
     * Whether an acknowledged engine save trimmed the journal (RG13), so pages with no observation would lack some
     * the journal no longer holds ({@code BoundarySession.observationsLost}).
     */
    private static boolean rtsimSavedBefore() {
        Journal j = RtsimSession.current().boundary().journal();
        return BoundarySession.observationsLost(0, j.firstSeq(), j.nextObsSeq());
    }

    private static String journalSpan() {
        Journal j = RtsimSession.current().boundary().journal();
        return j.size() == 0 ? "no observation, the next " + j.nextObsSeq() : "observations " + j.firstSeq() + " to " + (j.nextObsSeq() - 1);
    }

    /** Leaves no marker, no capture and no moved pages behind, whatever a test did. */
    private static void cleanUp(MinecraftServer server) {
        try {
            ReinstallRules.deleteMarker(worldDir(server));
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        ReinstallGuard.forgetCaptureForTest(server);
        for (Path p : asides(server))
            deleteTree(p);
    }

    @AfterBatch(batch = "reinstall_1")
    public static void resetAfter1(ServerLevel level) {
        IndustryConfig.Test.set(IndustryConfig.PERIODIC_RESCAN, null);
        cleanUp(level.getServer());
    }

    @AfterBatch(batch = "reinstall_2")
    public static void resetAfter2(ServerLevel level) {
        cleanUp(level.getServer());
    }
}
