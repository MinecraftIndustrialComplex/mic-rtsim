package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.journal.RingView;
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
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainRing.BreakForTest;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
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
import java.util.EnumMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * The train op ring (docs/choices-mowzie-build.md RP-190 to RP-198, RP-250 to RP-254, RP-257): IND_CARRIAGE_DELTA's
 * dedup across a crash that kept one of Java's two SavedData files and not the other, the acknowledgement that
 * takes an op out, the settle of a ring another session answered, and the valve.
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
        crash(helper, "aCrashThatKeptBothFilesTakesATrainOpOnce", BreakForTest.NONE, true, true, true);
    }

    /** J2, neither file saved: the restored train and its ring are from before the op, which is applied. */
    @GameTest(template = YARD, batch = "train_ring_2", timeoutTicks = 1200)
    public static void aCrashThatKeptNeitherFileTakesATrainOpOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptNeitherFileTakesATrainOpOnce", BreakForTest.NONE, false, false, true);
    }

    /** J3, the tracks saved and not the journal: the ring came back with the goods and answers DUPLICATE. */
    @GameTest(template = YARD, batch = "train_ring_3", timeoutTicks = 1200)
    public static void aCrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpOnce", BreakForTest.NONE, true, false, true);
    }

    /**
     * J4, the journal saved and not the tracks: the journal knows the op, the restored goods and ring do not,
     * and the ring alone decides: applied on the restored goods, taken once.
     */
    @GameTest(template = YARD, batch = "train_ring_4", timeoutTicks = 1200)
    public static void aCrashThatKeptTheJournalAndNotTheGoodsTakesATrainOpOnce(GameTestHelper helper) {
        crash(helper, "aCrashThatKeptTheJournalAndNotTheGoodsTakesATrainOpOnce", BreakForTest.NONE, false, true, true);
    }

    // ---- the fallback: the journal's op index, slice T's dedup (RP-193) ----------------------------

    /**
     * The ring's write injection broken ({@link BreakForTest#WRITE}), as if it had not applied: the real probe
     * fails at its write check and logs its WARN once, and the dedup is the journal's op index. J3 on it is
     * slice T's double: the journal from before the op applies it again on goods that already lost it, 20
     * taken for a raid of 10.
     */
    @GameTest(template = YARD, batch = "train_ring_5", timeoutTicks = 1200)
    public static void withoutTheRingACrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpTwice(GameTestHelper helper) {
        crash(helper, "withoutTheRingACrashThatKeptTheGoodsAndNotTheJournalTakesATrainOpTwice", BreakForTest.WRITE, true, false, false);
    }

    /**
     * J4 on the journal's op index, slice T's dupe (review P1-1), with the ring's read injection broken
     * ({@link BreakForTest#READ}): the real probe fails at its read check and logs its WARN once, and the
     * journal answers DUPLICATE on restored goods, so the train keeps its 32 iron while the raider holds 10:
     * 42 between them.
     */
    @GameTest(template = YARD, batch = "train_ring_6", timeoutTicks = 1200)
    public static void withoutTheRingACrashThatKeptTheJournalAndNotTheGoodsLeavesThemOnTheTrain(GameTestHelper helper) {
        crash(helper, "withoutTheRingACrashThatKeptTheJournalAndNotTheGoodsLeavesThemOnTheTrain", BreakForTest.READ, false, true, false);
    }

    /**
     * One crash: a train of 32 iron, a raid of 10 applied, the tracks file from after the raid
     * ({@code tracksSaved}) or before it, the journal knowing the op ({@code journalSaved}) or not, and the
     * raid sent again. The second answer and the iron left follow the dedup under test: the ring knows the op
     * when the tracks were saved, the journal's index when the journal was. {@code broken} is the ring hook a
     * fallback test breaks (none for the ring's own tests): the real probe then fails through that hook's
     * branch. {@code conserve} checks that the train and the raider hold 32 between them, with the raider's
     * share a constant {@code TAKEN} (the stub engine has no raider), so it covers Java's side only; without
     * it the totals are recorded only.
     */
    private static void crash(GameTestHelper helper, String test, BreakForTest broken, boolean tracksSaved, boolean journalSaved, boolean conserve) {
        boolean ring = broken == BreakForTest.NONE;
        fastConfig();
        // The raid is sent again after its answer may have been acknowledged, which a real engine never does
        // (RP-250): the op stays in the ring for it.
        TrainRing.holdPrunesForTest(true);
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
                    logs[0] = TrainRing.fallbackLogsForTest();
                    TrainRing.breakForTest(broken);
                    check("the ring is " + (ring ? "on: its mixin applied and the probe passed" : "off: the probe found " + broken + " broken"),
                            TrainRing.available(level.getServer()) == ring);
                    if (!ring)
                        check("the probe's WARN names the broken hook: " + TrainRing.lastFallbackForTest(),
                                TrainRing.lastFallbackForTest().startsWith(broken == BreakForTest.WRITE ? "Train.write wrote no " : "Train.read did not read "));
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
                    TrainRing.breakForTest(BreakForTest.NONE);
                    TrainRing.holdPrunesForTest(false);
                })
                .thenSucceed();
    }

    // ---- the module switch (UR-1, RP-257) --------------------------------------------------------

    /**
     * RP-257 (the SEC-1 lead's hole, RP-206): saving and loading never depend on the industry module's switch.
     * A raid is applied, the module is switched off, the tracks are saved, and the server crashes before the
     * journal's file is written (its op index forgets the op); the restart loads the tracks with the module
     * still off and switches it on, and the engine sends the raid again under the same op. The save with the
     * module off held the ring, so the op is answered DUPLICATE and taken once (before RP-257 the save left
     * the key out: APPLIED, the raid taken twice). The saved train keeps every key of Create's (UR-1).
     *
     * <p>Then the prunes with the module off: the DUPLICATE's answer is acknowledged and the op leaves the ring
     * while the module is off, and the next save writes none of the ring's keys. The prunes are held until
     * the module is off, so the prune is the session hook's (the industry's own tick does not run then).
     */
    @GameTest(template = YARD, batch = "train_ring_7", timeoutTicks = 2400)
    public static void anOpAppliedBeforeTheModuleWentOffIsTakenOnceAfterASaveWithItOffAndACrash(GameTestHelper helper) {
        String test = "anOpAppliedBeforeTheModuleWentOffIsTakenOnceAfterASaveWithItOffAndACrash";
        fastConfig();
        TrainRing.holdPrunesForTest(true); // the answer's acknowledgement must not empty the ring before the crash
        forget(helper);
        ServerLevel level = helper.getLevel();
        var server = level.getServer();
        Train[] train = {null};
        CompoundTag[] saved = {null};
        long op = engineOp();
        long[] pruned = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    check("the ring is on", TrainRing.available(server));
                    train[0] = ringTrain(helper);
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> {
                    check("the raid is applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)));
                    check("10 iron left the train", IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT) == IRON - TAKEN);
                }))
                .thenExecute(() -> {
                    RtsimConfig.Test.setModule(Module.INDUSTRY, false);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(paced(() -> check("the module is off", IndustryApi.current() == null)))
                .thenExecute(() -> {
                    // The save with the module off, then the crash before the journal's file was written.
                    saved[0] = saveTracks(level, train[0]);
                    CompoundTag tag = trainTag(saved[0]);
                    record(test, "keys saved with the module off", tag.getAllKeys().size() + ", the ring key "
                            + (tag.contains(TrainRing.KEY) ? "kept: " + Arrays.toString(tag.getLongArray(TrainRing.KEY)) : "dropped"));
                    journal().forgetAppliedForTest(op);
                    TrainWatch.forget(train[0]);
                    train[0] = loadTracks(level, saved[0]);
                    check("the tracks came back with " + (IRON - TAKEN) + " iron", IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT) == IRON - TAKEN);
                    RtsimConfig.Test.setModule(Module.INDUSTRY, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(paced(() -> check("the module is on again", IndustryApi.current() != null)))
                .thenIdle(3) // the industry's announce at its first tick on (forgetAnswers) comes before the raid's answer
                .thenExecute(() -> raid(op, train[0])) // the replay re-decides the raid under the same op
                .thenWaitUntil(paced(() -> check("the replayed raid is answered", commandResults(op).size() == 2)))
                .thenExecute(() -> {
                    int second = commandResults(op).get(1);
                    long iron = IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT);
                    long held = iron + TAKEN;
                    record(test, "second answer, iron on the train, train plus raider",
                            status(second) + ", " + iron + ", " + held + (held == IRON ? " (conserved)" : held > IRON ? " (a dupe)" : " (a double)"));
                    check("the op is not applied again: answered " + status(second), second == Obs.CommandResult.DUPLICATE_OP);
                    check("iron on the train " + (IRON - TAKEN) + ": " + iron, iron == IRON - TAKEN);
                    CompoundTag tag = trainTag(saved[0]);
                    check("the save with the module off held the ring's op: " + Arrays.toString(tag.getLongArray(TrainRing.KEY)),
                            Arrays.equals(tag.getLongArray(TrainRing.KEY), new long[] {op}));
                    check("and its answers and generation", tag.contains(AppliedOpRing.answersKey(TrainRing.KEY)) && tag.contains(AppliedOpRing.genKey(TrainRing.KEY)));
                    // UR-1: the ring's three keys beside every key of Create's, which a read and write of the same
                    // file without them gives (Train.read alone: the train is not watched).
                    CompoundTag plain = withoutRing(saved[0]);
                    Set<String> keys = new HashSet<>(Train.read(trainTag(plain), level.registryAccess(), Map.of(), DimensionPalette.read(plain))
                            .write(new DimensionPalette(), level.registryAccess()).getAllKeys());
                    keys.addAll(ringKeys());
                    check("the save is every key of Create's and the ring's three: " + tag.getAllKeys() + " vs " + keys, tag.getAllKeys().equals(keys));
                    // The prunes with the module off.
                    pruned[0] = TrainRing.pruned();
                    RtsimConfig.Test.setModule(Module.INDUSTRY, false);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(paced(() -> check("the module is off", IndustryApi.current() == null)))
                .thenExecute(() -> TrainRing.holdPrunesForTest(false))
                .thenWaitUntil(paced(() -> {
                    engine().requestSaveAtNextStep();
                    AppliedOpRing ring = TrainRing.peekForTest(train[0]);
                    check("with the module off, the engine acknowledged the DUPLICATE's answer and the op left: " + ring, ring != null && ring.isEmpty());
                }))
                .thenExecute(() -> {
                    check("the module is still off", IndustryApi.current() == null);
                    check("counted: " + (TrainRing.pruned() - pruned[0]), TrainRing.pruned() > pruned[0]);
                    CompoundTag tag = trainTag(saveTracks(level, train[0]));
                    for (String k : ringKeys())
                        check("the save with the module off holds no " + k + " once the op left", !tag.contains(k));
                    record(test, "ops pruned with the module off", TrainRing.pruned() - pruned[0]);
                    TrainWatch.forget(train[0]);
                    RtsimConfig.Test.setModule(Module.INDUSTRY, null);
                    BridgeEvents.moduleSwitchesChanged(server);
                })
                .thenWaitUntil(paced(() -> check("the module is on again", IndustryApi.current() != null)))
                .thenSucceed();
    }

    /** A tracks file with the ring's keys taken out of its train, as Create alone would have saved it. */
    private static CompoundTag withoutRing(CompoundTag tracks) {
        CompoundTag copy = tracks.copy();
        CompoundTag t = trainTag(copy);
        for (String k : ringKeys())
            t.remove(k);
        return copy;
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
        check("no ring saved reads as none, generation 0", !back.readFrom(new CompoundTag(), key) && back.isEmpty() && back.gen() == 0);
        tag.putString("mic_rtsim:other", "not a ring");
        check("a foreign tag under a key is not a ring", !AppliedOpRing.savedIn(tag, "mic_rtsim:other"));

        // The answers and the generation beside the ops (RP-250, RP-251)
        check("no answer known and no generation: neither key", !tag.contains(AppliedOpRing.answersKey(key)) && !tag.contains(AppliedOpRing.genKey(key)));
        ring.answered(3, 300);
        ring.answered(5, 500);
        ring.gen(0x5EED);
        ring.writeTo(tag, key);
        check("the answers in the ops' order: " + Arrays.toString(tag.getLongArray(AppliedOpRing.answersKey(key))),
                Arrays.equals(tag.getLongArray(AppliedOpRing.answersKey(key)), new long[] {0, 300, 0, 500}));
        // Five keys: the owner's, the foreign one, and the ring's three.
        check("the generation beside them: " + tag.getAllKeys(), tag.getLong(AppliedOpRing.genKey(key)) == 0x5EED && tag.size() == 5);
        AppliedOpRing again = new AppliedOpRing(4);
        check("read back", again.readFrom(tag, key));
        check("with its answers and generation: " + again, again.answerOf(3) == 300 && again.answerOf(5) == 500
                && again.answerOf(2) == AppliedOpRing.UNKNOWN && again.gen() == 0x5EED);
        AppliedOpRing smaller = new AppliedOpRing(2);
        smaller.readFrom(tag, key);
        check("a smaller cap keeps the newest with their answers: " + smaller, Arrays.equals(smaller.toArray(), new long[] {4, 5})
                && smaller.answerOf(5) == 500 && smaller.answerOf(4) == AppliedOpRing.UNKNOWN);
        ring.forgetAnswers();
        ring.writeTo(tag, key);
        check("every answer unknown: the answers' key goes", !tag.contains(AppliedOpRing.answersKey(key)) && tag.contains(AppliedOpRing.genKey(key)));

        ring.clear();
        ring.writeTo(tag, key);
        check("an empty ring removes its three keys", !tag.contains(key) && !tag.contains(AppliedOpRing.answersKey(key)) && !tag.contains(AppliedOpRing.genKey(key)));
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

    // ---- the probe's own branches (RP-193, RP-253) -----------------------------------------------

    /**
     * The real probe against each hook broken in turn ({@link BreakForTest}): the class mixin (the train is no
     * AppliedOpsHolder), the write injection and the read injection. Each fails through its own branch, logs
     * its WARN once and leaves the ring off; nothing broken, it passes again.
     */
    @GameTest(template = YARD, batch = "train_ring_9")
    public static void theProbeFindsEachBrokenHookAndSaysWhich(GameTestHelper helper) {
        fastConfig();
        var server = helper.getLevel().getServer();
        try {
            check("the ring is on", TrainRing.available(server));
            Map<BreakForTest, String> said = new EnumMap<>(BreakForTest.class);
            for (BreakForTest b : new BreakForTest[] {BreakForTest.HOLDER, BreakForTest.WRITE, BreakForTest.READ}) {
                int logs = TrainRing.fallbackLogsForTest();
                TrainRing.breakForTest(b);
                check(b + " broken: the ring is off", !TrainRing.available(server));
                check(b + " broken: asked again, the probe does not run again", !TrainRing.available(server));
                check(b + " broken: one WARN, " + (TrainRing.fallbackLogsForTest() - logs), TrainRing.fallbackLogsForTest() == logs + 1);
                said.put(b, TrainRing.lastFallbackForTest());
            }
            check("the class mixin: " + said.get(BreakForTest.HOLDER), said.get(BreakForTest.HOLDER).startsWith("Train does not implement AppliedOpsHolder"));
            check("the write injection: " + said.get(BreakForTest.WRITE), said.get(BreakForTest.WRITE).startsWith("Train.write wrote no " + TrainRing.KEY));
            check("the read injection: " + said.get(BreakForTest.READ), said.get(BreakForTest.READ).startsWith("Train.read did not read " + TrainRing.KEY));
            TrainRing.breakForTest(BreakForTest.NONE);
            check("nothing broken: the probe passes again", TrainRing.available(server));
            record("theProbeFindsEachBrokenHookAndSaysWhich", "the probe's reasons", String.join(" | ", said.values()));
        } finally {
            TrainRing.breakForTest(BreakForTest.NONE);
        }
        helper.succeed();
    }

    // ---- the ring past the old count, the acknowledgement, the settle and the valve (RP-250 to RP-254) ----

    /** Station exchanges after the raid: more than the old count bound (RP-192's 256). */
    private static final int EXCHANGES = 304;

    /**
     * Review2 P2's repro: a raid, then more later exchanges on the same train than the old ring held (256),
     * then the raid sent again with both files kept (J1). A count-bounded ring had pushed the raid's op out
     * and applied it a second time (APPLIED, 12 iron); the ring now holds an op until the engine acknowledges
     * its answer, so it answers DUPLICATE and 22 iron stay. The prunes are held: the raid is sent again after
     * its answer could have been acknowledged, which a real engine never does.
     */
    @GameTest(template = YARD, batch = "train_ring_10", timeoutTicks = 4000)
    public static void anOpPushedPastTheOldCountByLaterExchangesIsTakenOnce(GameTestHelper helper) {
        fastConfig();
        TrainRing.holdPrunesForTest(true);
        forget(helper);
        ServerLevel level = helper.getLevel();
        Train[] train = {null};
        long op = engineOp();
        long[] exchanges = new long[EXCHANGES];
        for (int i = 0; i < exchanges.length; i++)
            exchanges[i] = engineOp();
        GameTestSequence seq = helper.startSequence()
                .thenExecute(() -> {
                    check("the ring is on", TrainRing.available(level.getServer()));
                    train[0] = ringTrain(helper);
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> check("the raid is applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)))));
        for (int batch = 0; batch < EXCHANGES / 16; batch++) {
            int from = batch * 16;
            seq = seq.thenExecute(() -> {
                for (int i = from; i < from + 16; i++)
                    carriageDelta(exchanges[i], CreateIds.of(train[0].id), IndRecords.CarriageDelta.EXCHANGE,
                            List.<Object[]>of(new Object[] {0, "i/64/minecraft:iron_ingot", (long) (i % 2 == 0 ? -1 : 1)}));
            }).thenWaitUntil(paced(() -> check("exchanges applied", commandResults(exchanges[from + 15]).size() == 1)));
        }
        seq.thenExecute(() -> {
                    check("the exchanges left 22 iron", IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT) == IRON - TAKEN);
                    AppliedOpRing ring = TrainRing.peekForTest(train[0]);
                    check("the ring holds the raid and every exchange: " + (ring == null ? 0 : ring.size()),
                            ring != null && ring.contains(op) && ring.size() == EXCHANGES + 1);
                    CompoundTag tracks = saveTracks(level, train[0]);
                    journal().rememberAppliedForTest(op);
                    TrainWatch.forget(train[0]);
                    train[0] = loadTracks(level, tracks);
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> check("the raid sent again is answered", commandResults(op).size() == 2)))
                .thenExecute(() -> {
                    int second = commandResults(op).get(1);
                    long iron = IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT);
                    record("anOpPushedPastTheOldCountByLaterExchangesIsTakenOnce", "J1 after " + EXCHANGES + " exchanges: second answer, iron",
                            status(second) + ", " + iron);
                    journal().forgetAppliedForTest(op);
                    TrainWatch.forget(train[0]);
                    TrainRing.holdPrunesForTest(false);
                    check("J1 past " + EXCHANGES + " exchanges: DUPLICATE, answered " + status(second), second == Obs.CommandResult.DUPLICATE_OP);
                    check("iron on the train 22: " + iron, iron == IRON - TAKEN);
                })
                .thenSucceed();
    }

    /**
     * An idle train's ring empties (RP-250): once the engine's durable save holds the raid's answer, the op
     * leaves the ring and the next tracks save writes none of the ring's keys.
     */
    @GameTest(template = YARD, batch = "train_ring_11", timeoutTicks = 2400)
    public static void anAcknowledgedOpLeavesTheRing(GameTestHelper helper) {
        fastConfig();
        forget(helper);
        ServerLevel level = helper.getLevel();
        Train[] train = {null};
        long op = engineOp();
        long[] pruned = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    check("the ring is on", TrainRing.available(level.getServer()));
                    train[0] = ringTrain(helper);
                    pruned[0] = TrainRing.pruned();
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> check("the raid is applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)))))
                .thenWaitUntil(paced(() -> {
                    engine().requestSaveAtNextStep();
                    AppliedOpRing ring = TrainRing.peekForTest(train[0]);
                    check("the engine acknowledged the answer and the op left: " + ring, ring != null && ring.isEmpty());
                }))
                .thenExecute(() -> {
                    check("counted: " + (TrainRing.pruned() - pruned[0]), TrainRing.pruned() > pruned[0]);
                    CompoundTag tag = trainTag(saveTracks(level, train[0]));
                    for (String k : ringKeys())
                        check("the save holds no " + k, !tag.contains(k));
                    check("the goods stay taken", IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT) == IRON - TAKEN);
                    record("anAcknowledgedOpLeavesTheRing", "ops pruned", TrainRing.pruned() - pruned[0]);
                    TrainWatch.forget(train[0]);
                })
                .thenSucceed();
    }

    /**
     * The settle with the session's own view (RP-251): a ring saved by another generation comes back with its
     * answer unknown, whatever the view, because this GameTest's journal names no session that answered it.
     * The acknowledgement of the old answer then removes nothing; the op sent again is answered DUPLICATE
     * from the ring, and the acknowledgement of that answer removes it.
     */
    @GameTest(template = YARD, batch = "train_ring_12", timeoutTicks = 2400)
    public static void aRingAnotherSessionAnsweredKeepsItsOpUntilAnAnswerOfThisSessionIsAcknowledged(GameTestHelper helper) {
        fastConfig();
        TrainRing.holdPrunesForTest(true);
        forget(helper);
        ServerLevel level = helper.getLevel();
        Train[] train = {null};
        long op = engineOp();
        long[] first = {0}, pruned = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    check("the ring is on", TrainRing.available(level.getServer()));
                    train[0] = ringTrain(helper);
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> check("the raid is applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)))))
                .thenExecute(() -> {
                    long gen = gen();
                    AppliedOpRing ring = TrainRing.peekForTest(train[0]);
                    first[0] = ring.answerOf(op);
                    check("the ring has the answer's sequence: " + ring, first[0] > 0 && ring.gen() == gen);
                    CompoundTag tracks = saveTracks(level, train[0]);
                    // As another session saved it: its generation, its answer.
                    trainTag(tracks).putLong(AppliedOpRing.genKey(TrainRing.KEY), ~gen);
                    TrainWatch.forget(train[0]);
                    train[0] = loadTracks(level, tracks);
                    AppliedOpRing back = TrainRing.peekForTest(train[0]);
                    RingView view = TrainRing.sessionViewForTest();
                    record("aRingAnotherSessionAnsweredKeepsItsOpUntilAnAnswerOfThisSessionIsAcknowledged", "the session's view",
                            view + ", " + (view.unproven() == null ? "proves pages " + view.pages() : view.unproven()));
                    check("settled on read: the op stays, its answer unknown: " + back,
                            back != null && back.contains(op) && back.answerOf(op) == AppliedOpRing.UNKNOWN && back.gen() == gen);
                    pruned[0] = TrainRing.pruned();
                    TrainRing.holdPrunesForTest(false);
                })
                .thenWaitUntil(paced(() -> {
                    // The old ring object's answer is acknowledged (and pruned there); the read ring's op stays.
                    engine().requestSaveAtNextStep();
                    check("the old answer is acknowledged", TrainRing.pruned() > pruned[0]);
                }))
                .thenExecute(() -> check("the acknowledgement of the old answer left the op in the read ring",
                        TrainRing.peekForTest(train[0]).contains(op)))
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> check("the raid sent again is answered", commandResults(op).size() == 2)))
                .thenExecute(() -> {
                    int second = commandResults(op).get(1);
                    check("the ring answers DUPLICATE: " + status(second), second == Obs.CommandResult.DUPLICATE_OP);
                    check("iron on the train 22", IndustryGameTests.carriageCount(train[0], Items.IRON_INGOT) == IRON - TAKEN);
                    long at = TrainRing.peekForTest(train[0]).answerOf(op);
                    check("the DUPLICATE's sequence is the op's answer now: " + at, at > first[0]);
                })
                .thenWaitUntil(paced(() -> {
                    engine().requestSaveAtNextStep();
                    check("its acknowledgement removes the op", !TrainRing.peekForTest(train[0]).contains(op));
                }))
                .thenExecute(() -> TrainWatch.forget(train[0]))
                .thenSucceed();
    }

    /**
     * The settle's three cases through Create's Train.read (RP-251), against a view a test sets: the ring of
     * the journal's writer (generation W) with the pages through 100 loses an op answered at 5, keeps one
     * answered at 200 and one never answered, both unknown; another generation's ring, or W's under pages
     * ahead of the journal file (P29), keeps all three unknown; a ring this session saved keeps its answers.
     */
    @GameTest(template = YARD, batch = "train_ring_13", timeoutTicks = 1200)
    public static void theSettleTakesWhatThePagesHoldAndKeepsTheRestUnknown(GameTestHelper helper) {
        fastConfig();
        TrainRing.holdPrunesForTest(true);
        forget(helper);
        ServerLevel level = helper.getLevel();
        long w = 0x5EED_0001L, v = 0x5EED_0002L;
        long a = engineOp(), b = engineOp(), c = engineOp();
        helper.startSequence()
                .thenExecute(() -> {
                    check("the ring is on", TrainRing.available(level.getServer()));
                    Train t = ringTrain(helper);
                    long gen = gen();
                    TrainRing.viewForTest(new RingView(gen, w, 100, 150, 151));
                    try {
                        AppliedOpRing writers = settled(level, t, a, b, c, w);
                        check("W's ring under proven pages through 100: the op answered at 5 leaves, the others stay unknown: " + writers,
                                Arrays.equals(writers.toArray(), new long[] {b, c}) && writers.unknownCount() == 2 && writers.gen() == gen);
                        AppliedOpRing others = settled(level, t, a, b, c, v);
                        check("another generation's ring: all stay, unknown: " + others,
                                Arrays.equals(others.toArray(), new long[] {a, b, c}) && others.unknownCount() == 3);
                        AppliedOpRing own = settled(level, t, a, b, c, gen);
                        check("this session's ring keeps its answers: " + own, own.size() == 3 && own.answerOf(a) == 5 && own.answerOf(b) == 200);
                        TrainRing.viewForTest(new RingView(gen, w, 160, 150, 151));
                        AppliedOpRing ahead = settled(level, t, a, b, c, w);
                        check("W's ring under pages ahead of the journal file: all stay, unknown: " + ahead,
                                Arrays.equals(ahead.toArray(), new long[] {a, b, c}) && ahead.unknownCount() == 3);
                        record("theSettleTakesWhatThePagesHoldAndKeepsTheRestUnknown", "writer's, other's, own, P29",
                                writers + " | " + others + " | " + own + " | " + ahead);
                    } finally {
                        TrainRing.viewForTest(null);
                        TrainRing.holdPrunesForTest(false);
                        TrainWatch.forget(t);
                    }
                })
                .thenSucceed();
    }

    /**
     * The valve (RP-250): a train ring of {@link TrainRing#CAPACITY} unacknowledged ops pushes its oldest out
     * for the next op applied, counts it, and logs the train once; the next eviction is only counted.
     */
    @GameTest(template = YARD, batch = "train_ring_14", timeoutTicks = 1200)
    public static void aFullRingEvictsItsOldestCountsItAndWarnsOncePerTrain(GameTestHelper helper) {
        fastConfig();
        TrainRing.holdPrunesForTest(true);
        forget(helper);
        ServerLevel level = helper.getLevel();
        Train[] train = {null};
        long op = engineOp();
        long[] before = {0, 0};
        long[] filler = new long[TrainRing.CAPACITY];
        for (int i = 0; i < filler.length; i++)
            filler[i] = 0x7E00_0000_0000_0000L + i;
        helper.startSequence()
                .thenExecute(() -> {
                    check("the ring is on", TrainRing.available(level.getServer()));
                    train[0] = ringTrain(helper);
                    TrainRing.of(train[0]).load(filler);
                    before[0] = TrainRing.evictions();
                    before[1] = TrainRing.warnedForTest();
                })
                .thenExecute(() -> raid(op, train[0]))
                .thenWaitUntil(paced(() -> check("the raid is applied", commandResults(op).equals(List.of(Obs.CommandResult.APPLIED)))))
                .thenExecute(() -> {
                    AppliedOpRing ring = TrainRing.peekForTest(train[0]);
                    check("full, the raid in and the oldest out: " + ring.size(), ring.size() == TrainRing.CAPACITY && ring.contains(op)
                            && !ring.contains(filler[0]) && ring.contains(filler[1]));
                    check("counted: " + (TrainRing.evictions() - before[0]), TrainRing.evictions() == before[0] + 1);
                    check("the train is logged once", TrainRing.warnedForTest() == before[1] + 1);
                    TrainRing.appliedForTest(train[0], ring, engineOp());
                    check("the next eviction is counted", TrainRing.evictions() == before[0] + 2 && !ring.contains(filler[1]));
                    check("and not logged again", TrainRing.warnedForTest() == before[1] + 1);
                    record("aFullRingEvictsItsOldestCountsItAndWarnsOncePerTrain", "evictions, trains logged",
                            (TrainRing.evictions() - before[0]) + ", " + (TrainRing.warnedForTest() - before[1]));
                    TrainWatch.forget(train[0]);
                    TrainRing.holdPrunesForTest(false);
                })
                .thenSucceed();
    }

    /**
     * Train {@code t} saved with a ring of ops a, b, c answered at 5, 200 and never, settled last by generation
     * {@code gen}, read back through Create's Train.read: the read ring.
     */
    private static AppliedOpRing settled(ServerLevel level, Train t, long a, long b, long c, long gen) {
        CompoundTag tracks = saveTracks(level, t);
        AppliedOpRing ring = new AppliedOpRing(TrainRing.CAPACITY);
        ring.load(new long[] {a, b, c}, new long[] {5, 200, 0});
        ring.gen(gen);
        ring.writeTo(trainTag(tracks), TrainRing.KEY);
        Train read = loadTracks(level, tracks);
        AppliedOpRing back = TrainRing.peekForTest(read);
        if (back == null)
            throw new GameTestAssertException("Train.read made no ring");
        TrainWatch.forget(read);
        return back;
    }

    private static long gen() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        return s.boundary().gen();
    }

    /** The ring's three keys in a train's compound. */
    private static List<String> ringKeys() {
        return List.of(TrainRing.KEY, AppliedOpRing.answersKey(TrainRing.KEY), AppliedOpRing.genKey(TrainRing.KEY));
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
