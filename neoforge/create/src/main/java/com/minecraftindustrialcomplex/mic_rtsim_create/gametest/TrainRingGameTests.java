package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeEvents;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;
import com.minecraftindustrialcomplex.mic_rtsim_create.AppliedOpRing;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainRing;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.Tag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Items;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * The train op ring (docs/choices-mowzie-build.md RP-190 to RP-198): IND_CARRIAGE_DELTA's dedup across a
 * crash that kept one of Java's two SavedData files and not the other.
 *
 * <p>A crash is emulated on one synthetic train. Its tracks file is what RailwaySavedData.save writes for
 * it (Create's own Train.write, with a dimension palette, through the bytes of a compressed NBT file) and
 * the restart is Create's Train.read of those bytes, the train taking its old one's id. The journal's file
 * is its op index: {@link Journal#forgetAppliedForTest} for a journal saved before the op was applied,
 * {@link Journal#rememberAppliedForTest} for one saved while its COMMAND_RESULT was unacknowledged. The
 * engine then re-decides the raid under the same op (slice T's key, RP-180) and sends it again.
 *
 * <p>The crash is at the data level: each file is taken (or not) on its own, so no test runs a real save
 * or depends on the order the server writes its SavedData in. Slice U moves the journal's write after
 * every dimension's SavedData, after which a real crash gives J3 and never J4; J4 stays pinned here.
 *
 * <p>Each raid takes 10 iron from a train of 32, and the raider is credited 10 by the decision that
 * survives the crash, so the goods are conserved when the train and the raider hold 32 between them.
 * Every test has its own batch: the fallback switch and the module switch are server-wide.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class TrainRingGameTests {
    private TrainRingGameTests() {}

    private static final int IRON = 32, TAKEN = 10;

    // ---- the four durability cases on the ring (slice T2) ----------------------------------------

    /** J1, both files saved: the ring holds the op and answers DUPLICATE; taken once. */
    @GameTest(template = YARD, batch = "train_ring_1", timeoutTicks = 1200)
    public static void aCrashThatKeptBothFilesTakesATrainOpOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptBothFilesTakesATrainOpOnce", true, true, true, true);
    }

    /** J2, neither file saved: the restored train and its ring are from before the op, which is applied. */
    @GameTest(template = YARD, batch = "train_ring_2", timeoutTicks = 1200)
    public static void aCrashThatKeptNeitherFileTakesATrainOpOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptNeitherFileTakesATrainOpOnce", true, false, false, true);
    }

    /** J3, the tracks saved and not the journal: the ring came back with the goods and answers DUPLICATE. */
    @GameTest(template = YARD, batch = "train_ring_3", timeoutTicks = 1200)
    public static void aCrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpOnce", true, true, false, true);
    }

    /**
     * J4, the journal saved and not the tracks: the journal knows the op, the restored goods and ring do not,
     * and the ring alone decides: applied on the restored goods, taken once.
     */
    @GameTest(template = YARD, batch = "train_ring_4", timeoutTicks = 1200)
    public static void aCrashThatKeptTheJournalAndNotTheGoodsTakesATrainOpOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptTheJournalAndNotTheGoodsTakesATrainOpOnce", true, false, true, true);
    }

    // ---- the fallback: the journal's op index, slice T's dedup (RP-193) ----------------------------

    /**
     * The ring switched off as if its mixin had not applied: the fallback is logged once and the dedup is
     * the journal's op index. J3 on it is slice T's double: the journal from before the op applies it again
     * on goods that already lost it, 20 taken for a raid of 10.
     */
    @GameTest(template = YARD, batch = "train_ring_5", timeoutTicks = 1200)
    public static void withoutTheRingACrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpTwice(GameTestHelper helper) {
        crash(helper, "withoutTheRingACrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpTwice", false, true, false, false);
    }

    /**
     * J4 on the journal's op index, slice T's dupe (review P1-1): the journal answers DUPLICATE on restored
     * goods, so the train keeps its 32 iron while the raider holds 10: 42 between them.
     */
    @GameTest(template = YARD, batch = "train_ring_6", timeoutTicks = 1200)
    public static void withoutTheRingACrashThatKeptTheJournalAndNotTheGoodsLeavesThemOnTheTrain(GameTestHelper helper) {
        crash(helper, "withoutTheRingACrashThatKeptTheJournalAndNotTheGoodsLeavesThemOnTheTrain", false, false, true, false);
    }

    /**
     * One crash: a train of 32 iron, a raid of 10 applied, the tracks file from after the raid
     * ({@code tracksSaved}) or before it, the journal knowing the op ({@code journalSaved}) or not, and the
     * raid sent again. The second answer and the iron left follow the dedup under test: the ring knows the op
     * when the tracks were saved, the journal's index when the journal was. {@code conserve} checks that the
     * train and the raider hold 32 between them; without it the totals are recorded only.
     */
    private static void crash(GameTestHelper helper, String test, boolean ring, boolean tracksSaved, boolean journalSaved, boolean conserve) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        Train[] train = {null};
        CompoundTag[] before = {null}, after = {null};
        int[] logs = {0};
        long op = engineOp();
        String dedup = ring ? "ring" : "journal";
        String kase = "J" + (tracksSaved ? journalSaved ? 1 : 3 : journalSaved ? 4 : 2);
        helper.startSequence()
                .thenExecute(() -> {
                    TrainRing.fallbackForTest(!ring);
                    logs[0] = TrainRing.fallbackLogsForTest();
                    check("the ring is " + (ring ? "on: its mixin applied and the probe passed" : "switched off"),
                            TrainRing.available(level.getServer()) == ring);
                    train[0] = ringTrain(helper);
                    before[0] = saveTracks(level, train[0]);
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> {
                    check("the raid is applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)));
                    check("10 iron left the train", IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT) == IRON - TAKEN);
                }))
                .thenExecute(() -> {
                    after[0] = saveTracks(level, train[0]);
                    long[] saved = trainTag(after[0]).getLongArray(TrainRing.KEY);
                    if (ring)
                        check("the tracks save holds the op in the train's ring: " + Arrays.toString(saved), Arrays.equals(saved, new long[] {op}));
                    else
                        check("no ring is saved when it is off", !trainTag(after[0]).contains(TrainRing.KEY));
                    // The crash and the restart: Create reads its tracks file, the journal its own.
                    Journal journal = journal();
                    if (journalSaved)
                        journal.rememberAppliedForTest(op);
                    else
                        journal.forgetAppliedForTest(op);
                    check("the journal " + (journalSaved ? "knows" : "does not know") + " the op", ind().opApplied(op) == journalSaved);
                    TrainWatch.forget(train[0]);
                    train[0] = loadTracks(level, tracksSaved ? after[0] : before[0]);
                    long back = IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT);
                    check("the tracks came back with " + (tracksSaved ? IRON - TAKEN : IRON) + " iron: " + back, back == (tracksSaved ? IRON - TAKEN : IRON));
                })
                .thenExecute(() -> raid(op, train[0])) // the replay re-decides the raid under the same op
                .thenWaitUntil(paced(() -> check("the replayed raid is answered", commandResults(op).size() == 2)))
                .thenExecute(() -> {
                    int second = commandResults(op).get(1);
                    long iron = IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT);
                    long held = iron + TAKEN; // the raider holds what the surviving decision credited it
                    record(test, kase + " on the " + dedup + ": second answer, iron on the train, train plus raider",
                            status(second) + ", " + iron + ", " + held + (held == IRON ? " (conserved)" : held > IRON ? " (a dupe)" : " (a double)"));
                    boolean knows = ring ? tracksSaved : journalSaved;
                    check("the " + dedup + " " + (knows ? "knows the op: DUPLICATE" : "lacks the op: APPLIED") + ", answered " + status(second),
                            second == (knows ? Obs.CommandResult.DUPLICATE_OP : Obs.CommandResult.APPLIED));
                    long expected = (tracksSaved ? IRON - TAKEN : IRON) - (knows ? 0 : TAKEN);
                    check("iron on the train " + expected + ": " + iron, iron == expected);
                    if (conserve)
                        check("the goods are conserved: train " + iron + " plus raider " + TAKEN + " = " + held + ", not " + IRON, held == IRON);
                    if (!ring)
                        check("the fallback was logged once: " + (TrainRing.fallbackLogsForTest() - logs[0]), TrainRing.fallbackLogsForTest() == logs[0] + 1);
                    journal().forgetAppliedForTest(op);
                    TrainWatch.forget(train[0]);
                    TrainRing.fallbackForTest(false);
                })
                .thenSucceed();
    }

    // ---- UR-1 -----------------------------------------------------------------------------------

    /**
     * UR-1: a tracks file with a ring, read with the industry module off, loads its train as Create would, and
     * the next save drops the key and keeps every key of Create's. The ring stays on the train in memory: on
     * again, a save writes it once more. (With the plugin removed the mixin is gone and Create's Train.write
     * builds its compound from scratch, so the key goes the same way.)
     */
    @GameTest(template = YARD, batch = "train_ring_7", timeoutTicks = 1200)
    public static void withIndustryOffASavedTrainLoadsAndItsNextSaveDropsTheRing(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        var server = level.getServer();
        Train[] train = {null};
        CompoundTag[] saved = {null};
        long op = engineOp();
        helper.startSequence()
                .thenExecute(() -> {
                    check("the ring is on", TrainRing.available(server));
                    train[0] = ringTrain(helper);
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> check("the raid is applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)))))
                .thenExecute(() -> {
                    saved[0] = saveTracks(level, train[0]);
                    check("the save holds the ring", Arrays.equals(trainTag(saved[0]).getLongArray(TrainRing.KEY), new long[] {op}));
                    RtsimConfig.Test.setModule(Module.INDUSTRY, false);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenIdle(3)
                .thenExecute(() -> {
                    check("the module is off", IndustryApi.current() == null);
                    TrainWatch.forget(train[0]);
                    Train loaded = loadTracks(level, saved[0]);
                    check("the train loads: its id", loaded.id.equals(train[0].id));
                    check("and its goods", IndustryGameTests.carriageCount(loaded, Items.IRON_INGOT) == IRON - TAKEN);
                    train[0] = loaded;
                    CompoundTag next = trainTag(saveTracks(level, loaded));
                    check("the next save drops " + TrainRing.KEY, !next.contains(TrainRing.KEY));
                    Set<String> create = new HashSet<>(trainTag(saved[0]).getAllKeys());
                    create.remove(TrainRing.KEY);
                    check("and keeps every key of Create's: " + next.getAllKeys() + " vs " + create, next.getAllKeys().equals(create));
                    record("withIndustryOffASavedTrainLoadsAndItsNextSaveDropsTheRing", "keys saved with the module off", next.getAllKeys().size()
                            + ", the ring key " + (next.contains(TrainRing.KEY) ? "kept" : "dropped"));
                    RtsimConfig.Test.setModule(Module.INDUSTRY, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(paced(() -> check("the module is on again", IndustryApi.current() != null)))
                .thenExecute(() -> {
                    AppliedOpRing ring = TrainRing.peekForTest(train[0]);
                    check("the ring stayed on the train", ring != null && ring.contains(op));
                    check("on again, the next save writes it", Arrays.equals(trainTag(saveTracks(level, train[0])).getLongArray(TrainRing.KEY), new long[] {op}));
                    TrainWatch.forget(train[0]);
                })
                .thenSucceed();
    }

    // ---- the reusable ring's NBT form (RP-198) ----------------------------------------------------

    /**
     * AppliedOpRing in any compound (the API slice U reuses for GlobalPackagePort): one long array under a
     * namespaced key beside the owner's keys, oldest first, read back under a smaller cap keeping the newest;
     * an empty ring removes its key, a missing or foreign key reads as no ring, and an unnamespaced key is
     * refused. Here because the unit tests' classpath has no Minecraft.
     */
    @GameTest(template = YARD, batch = "train_ring_8")
    public static void anAppliedOpRingSavesUnderItsKeyBesideTheOwnersKeys(GameTestHelper helper) {
        String key = "mic_rtsim:test_ops";
        CompoundTag tag = new CompoundTag();
        tag.putString("Owner", "kept");
        AppliedOpRing ring = new AppliedOpRing(4);
        for (long op = 1; op <= 5; op++)
            ring.add(op);
        check("a resend is not added", !ring.add(5));
        ring.writeTo(tag, key);
        check("saved oldest first, the oldest past the cap gone: " + Arrays.toString(tag.getLongArray(key)),
                Arrays.equals(tag.getLongArray(key), new long[] {2, 3, 4, 5}));
        check("beside the owner's keys", tag.getString("Owner").equals("kept") && tag.size() == 2);
        check("savedIn finds it", AppliedOpRing.savedIn(tag, key));

        AppliedOpRing back = new AppliedOpRing(2);
        check("read back", back.readFrom(tag, key));
        check("a smaller cap keeps the newest", Arrays.equals(back.toArray(), new long[] {4, 5}) && back.capacity() == 2);
        check("no ring saved reads as none", !back.readFrom(new CompoundTag(), key) && back.isEmpty());
        tag.putString("mic_rtsim:other", "not a ring");
        check("a foreign tag under a key is not a ring", !AppliedOpRing.savedIn(tag, "mic_rtsim:other"));

        ring.clear();
        ring.writeTo(tag, key);
        check("an empty ring removes its key", !tag.contains(key));
        for (String bad : new String[] {"applied_ops", ":x", "x:"}) {
            try {
                ring.writeTo(tag, bad);
                throw new GameTestAssertException("an unnamespaced key was taken: " + bad);
            } catch (IllegalArgumentException expected) {
                // refused
            }
        }
        helper.succeed();
    }

    // ---- the emulation --------------------------------------------------------------------------

    /** A synthetic train of 32 iron that IND_CARRIAGE_DELTA finds, whose carriage saves its storage. */
    private static Train ringTrain(GameTestHelper helper) {
        Train t = IndustryGameTests.syntheticTrain(helper, helper.absolutePos(new BlockPos(5, 1, 5)), IRON);
        // Carriage storage is saved in the carriage's per-dimension entries (Carriage.write, EntityPositioning):
        // a train Create runs has one for each dimension it is in, a synthetic train none until asked.
        t.carriages.get(0).getDimensional(helper.getLevel());
        TrainWatch.seenForTest(t);
        return t;
    }

    /** The raid: 10 iron off carriage 0, one IND_CARRIAGE_DELTA (loot) under the op. */
    private static void raid(long op, Train train) {
        carriageDelta(op, CreateIds.of(train.id), IndRecords.CarriageDelta.LOOT,
                List.<Object[]>of(new Object[] {0, "i/64/minecraft:iron_ingot", (long) -TAKEN}));
    }

    /** What RailwaySavedData.save writes for this train (its Trains list and palette), through a file's bytes. */
    private static CompoundTag saveTracks(ServerLevel level, Train train) {
        DimensionPalette dimensions = new DimensionPalette();
        CompoundTag nbt = new CompoundTag();
        ListTag trains = new ListTag();
        trains.add(train.write(dimensions, level.registryAccess()));
        nbt.put("Trains", trains);
        dimensions.write(nbt);
        try {
            ByteArrayOutputStream file = new ByteArrayOutputStream();
            NbtIo.writeCompressed(nbt, file);
            return NbtIo.readCompressed(new ByteArrayInputStream(file.toByteArray()), NbtAccounter.unlimitedHeap());
        } catch (IOException e) {
            throw new GameTestAssertException("the tracks file did not round-trip: " + e);
        }
    }

    /** What RailwaySavedData.load makes of it: Create's Train.read, the train found under its old id. */
    private static Train loadTracks(ServerLevel level, CompoundTag nbt) {
        Train t = Train.read(trainTag(nbt), level.registryAccess(), Map.of(), DimensionPalette.read(nbt));
        TrainWatch.seenForTest(t);
        return t;
    }

    private static CompoundTag trainTag(CompoundTag tracks) {
        return tracks.getList("Trains", Tag.TAG_COMPOUND).getCompound(0);
    }

    private static Journal journal() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s.boundary().journal();
    }

    private static String status(int s) {
        return switch (s) {
            case Obs.CommandResult.APPLIED -> "APPLIED";
            case Obs.CommandResult.DUPLICATE_OP -> "DUPLICATE";
            case Obs.CommandResult.FAILED -> "FAILED";
            default -> "status " + s;
        };
    }
}
