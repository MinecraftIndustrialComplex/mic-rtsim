package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.SessionStart;

import java.nio.ByteBuffer;
import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The Rust engine (crates/rtsim-jni) through JNI, a plugin of mic_native: its steps run on the host's
 * background pool. Every {@link com.minecraftindustrialcomplex.mic_rtsim.boundary.Engine} call maps
 * to one native call; the four buffers are engine memory seen as direct {@code ByteBuffer}s.
 *
 * <p>The test and debug surface ({@link ScriptedEngine}) goes through orders ({@link NativeCodec})
 * and the native inspection calls. With {@link #FLAG_TEST} the engine also keeps a log of the
 * observation records it received, which this class drains into a Java list of the last 10,000, and
 * every DICT among them into a map of Java's dictionary that never drops an entry (Java sends each
 * entry once a session, so a log that drops its oldest records loses names still in use).
 *
 * <p>Loading ({@link #ensureLoaded}) has mic_native load the plugin from this mod's jar, then binds
 * the natives below to this mod's class loader with {@code System.load}. There is no fallback: if the
 * library cannot be loaded, opening fails with the reason (A8).
 */
public final class NativeEngine implements ScriptedEngine {

    /** Keep the received log and the inspection data. GameTests and unit tests only. */
    public static final int FLAG_TEST = 1;
    /** Skip fsync when writing pages. GameTests only: a test world is thrown away. */
    public static final int FLAG_NO_FSYNC = 2;

    /** The plugin's library: {@code librtsim_jni.so}, {@code rtsim_jni.dll}, {@code librtsim_jni.dylib}. */
    public static final String LIBRARY = "rtsim_jni";

    // nStats indexes.
    static final int STAT_STEPS_STARTED = 0;
    static final int STAT_STEPS_DONE = 1;
    static final int STAT_STALE_SKIPPED = 2;
    static final int STAT_OP_DUPLICATES = 3;
    static final int STAT_APPLIED_THROUGH = 4;
    static final int STAT_PAGES_OBS_SEQ = 5;
    static final int STAT_ACKED_SENT = 6;
    static final int STAT_NEXT_CMD_SEQ = 7;
    static final int STAT_ORDERS_PENDING = 8;
    static final int STAT_SAVES_DONE = 9;
    static final int STAT_LAST_STEP_NANOS = 10;
    static final int STAT_AGENTS = 11;
    static final int STAT_EMBODIED = 12;
    static final int STAT_GAPS = 13;
    static final int STAT_COMMANDS_WRITTEN = 14;
    static final int STAT_SAVE_FAILURES = 15;
    static final int STATS = 16;

    // nPhase1Stats indexes.
    /** Phase 1 villages the engine knows. */
    public static final int P1_VILLAGES = 0;
    /** Phase 1 agents embodied now. */
    public static final int P1_EMBODIED = 1;
    /** Trades with a village resident the engine counted. */
    public static final int P1_RESIDENT_TRADES = 2;
    /** Trades with a caravan the engine counted. */
    public static final int P1_CARAVAN_TRADES = 3;
    /** Trades the engine refused. */
    public static final int P1_TRADES_REFUSED = 4;
    /** Robberies the engine counted. */
    public static final int P1_ROBBERIES = 5;
    /** Reports (of assaults, killings) the engine took in. */
    public static final int P1_REPORTS = 6;
    /** WRITE_GOSSIP commands the engine sent. */
    public static final int P1_GOSSIP_SENT = 7;
    /** MARKET commands the engine sent. */
    public static final int P1_MARKETS_SENT = 8;
    /** MERCHANT_OFFERS commands the engine sent. */
    public static final int P1_OFFERS_SENT = 9;
    /** PLACE_HOUSE commands the engine sent. */
    public static final int P1_HOUSES_SENT = 10;
    /** SPAWN_RESIDENT commands the engine sent. */
    public static final int P1_RESIDENTS_SENT = 11;
    /** SENTIMENT commands the engine sent. */
    public static final int P1_SENTIMENTS_SENT = 12;
    /** Phase 1 observations the engine took in. */
    public static final int P1_INPUTS = 13;
    /** SPAWN_GROUP commands the engine sent for phase 1 travellers. */
    public static final int P1_SPAWNS_SENT = 14;
    /** The phase 1 world's tick. */
    public static final int P1_WORLD_TICK = 15;
    /** Works the phase 1 world's villages bought (SINK.md). */
    public static final int P1_WORKS_BOUGHT = 16;
    /** Works Java answered as placed (APPLIED or a duplicate). */
    public static final int P1_WORKS_PLACED = 17;
    /** Works Java answered as refused (NOT_FREE or FAILED): the kind is full for that village. */
    public static final int P1_WORKS_REFUSED = 18;
    /** Tributes paid to raiding dragons. */
    public static final int P1_TRIBUTES = 19;
    /** The length of the array {@link #phase1Stats} returns. */
    public static final int PHASE1_STATS = 20;

    /** {@link #phase1Works} indexes: works bought, kinds full, purse, census golems, tribute paid, spent. */
    public static final int WORKS_BOUGHT = 0;
    public static final int WORKS_FULL = 1;
    public static final int WORKS_PURSE = 2;
    public static final int WORKS_GOLEMS = 3;
    public static final int WORKS_TRIBUTE = 4;
    public static final int WORKS_SPENT = 5;
    public static final int WORKS_VIEW = 6;

    private static final int RECEIVED_CAP = 10_000;
    private static final int CLOSE_TIMEOUT_MS = 5000;
    private static final long IDLE_TIMEOUT_NANOS = 10_000_000_000L;

    // ---- the natives (crates/rtsim-jni) ---------------------------------------------------------

    /** flags: {@link #FLAG_TEST}, {@link #FLAG_NO_FSYNC}. capacity: record bytes per buffer (0: the engine's default). Throws RuntimeException. */
    private static native long nOpen(String pagesDir, int capacity, int flags);

    /** 0, 1: observation buffers; 2, 3: command buffers. Direct, engine memory. */
    private static native ByteBuffer nBuffer(long handle, int which);

    /** out[0] = pagesObsSeq, out[1] = nextCmdSeq. 0 or a negative status (message: nLastError). */
    private static native int nStart(long handle, long lastAppliedCmdSeq, long[] out);

    /** Queues step {@code handover} on the background pool; returns at once. 0 or negative. */
    private static native int nSubmit(long handle, long handover);

    private static native void nRequestSave(long handle, long throughObsSeq);

    /** save: final save waited for; false: a crash. Waits up to timeoutMs for a step in flight. The handle is dead afterwards. */
    private static native int nClose(long handle, boolean save, int timeoutMs);

    private static native String nDescribe(long handle);

    /** This thread's last failure message, "" if none. */
    private static native String nLastError();

    /** Orders, encoded as in {@link NativeCodec}. 0 or negative (-2: bad encoding). */
    private static native int nOrder(long handle, byte[] orders);

    /** The received log since the last drain ({@link NativeCodec}); an empty array if none, never null. */
    private static native byte[] nDrainReceived(long handle);

    /** out.length >= 16; indexes STAT_*. */
    private static native void nStats(long handle, long[] out);

    /** out.length >= PHASE1_STATS; indexes P1_*. */
    private static native void nPhase1Stats(long handle, long[] out);

    /** A phase 1 village's sentiment toward a player (-128 to 127), Integer.MIN_VALUE when the engine knows no such village (TEST mode). */
    private static native int nPhase1Sentiment(long handle, long village, long most, long least);

    /** A phase 1 village's works (WORKS_* indexes) as of the last step; false when unknown (TEST mode). */
    private static native boolean nPhase1Works(long handle, long village, long[] out);

    private static native long nLedger(long handle, long store);

    private static native boolean nOpApplied(long handle, long opId);

    /** The herd behind a group's agent id: {@code HERD_INFO} values; false when there is none. */
    private static native boolean nHerdInfo(long handle, long agent, long[] out);

    /** A phase 1 village's fear to the dragons, its golems' included; -1 for a village the world does not know. */
    private static native long nDragonsVillageRisk(long handle, long village);

    /** The dragons' tribute: {@code TRIBUTE_*} values. */
    private static native void nDragonsTribute(long handle, long[] out);

    /** The oil gate's counters and one dragon's reserve: {@code OIL_*} values. */
    private static native void nDragonsOil(long handle, long agent, long[] out);

    /** One oil nest: {@code NEST_*} values; false for no such nest. */
    private static native boolean nDragonsNest(long handle, long nest, long[] out);

    /** The world model at block (x, z) of Java's dimension {@code dim}: {@code HABITAT_INFO} values. */
    private static native void nHabitatInfo(long handle, int dim, int x, int z, long[] out);

    /** Null when none. */
    private static native String nProtocolError(long handle);

    /** A district's glass report (GlassReport's layout); null without a complete compile. */
    private static native double[] nGlassReport(long handle, long district);

    /** One district's threat state ({@link #industryThreatProbe}'s layout). */
    private static native double[] nIndustryThreatProbe(long handle, long district);

    /** The industry engine's learned train lines ({@link #industryLines}'s layout). */
    private static native long[] nIndustryLines(long handle);

    /** The glass check of a record stream on a fresh industry engine; null without a complete compile. */
    private static native double[] nGlassOffline(byte[] records, long district);

    /** The chunk reconciles a fresh industry engine would send a district over a record stream: (inventory, good, delta) triples. */
    private static native long[] nGlassReplayOffline(byte[] records, long district);

    /** The industry engine's log lines about a district from a tick on, joined by newlines; "" when none. */
    private static native String nIndustryNotes(long handle, long district, long from);

    /** The same lines from a fresh industry engine over a record stream. */
    private static native String nGlassNotesOffline(byte[] records, long district, long from);

    /** A hold on the engine's buffer memory for one Java buffer; {@link #nReleaseBuffers} gives it back. */
    private static native long nHoldBuffers(long handle);

    private static native void nReleaseBuffers(long token);

    /**
     * Frees the engine's buffer memory only once no Java buffer can reach it: each buffer from
     * {@code nBuffer} (and every duplicate or view of it, which keeps it reachable) holds the memory
     * until this cleaner runs, whatever {@code nClose} did meanwhile.
     */
    private static final java.lang.ref.Cleaner BUFFERS = java.lang.ref.Cleaner.create();

    /** The cleaner's action: it must not refer to the buffer itself. */
    private record ReleaseHold(long token) implements Runnable {
        @Override
        public void run() {
            nReleaseBuffers(token);
        }
    }

    // ---- loading --------------------------------------------------------------------------------

    private static boolean loaded;

    /**
     * Loads the plugin through mic_native once, and binds this class's natives to this mod's class
     * loader. Throws IllegalStateException naming the reason when either step fails.
     */
    public static synchronized void ensureLoaded() {
        if (loaded)
            return;
        Path library;
        try {
            library = MicNativeLink.loadPlugin();
        } catch (LinkageError e) {
            throw new IllegalStateException("mic_native is not installed (" + e + ")", e);
        }
        try {
            System.load(library.toString());
        } catch (UnsatisfiedLinkError | SecurityException e) {
            throw new IllegalStateException("cannot bind the rtsim_jni natives from " + library + ": " + e.getMessage(), e);
        }
        loaded = true;
    }

    /**
     * Every reference to mic_native's classes, in a class of its own: NativeEngine itself then links
     * without them, and a missing mic_native shows as a LinkageError at the call.
     */
    private static final class MicNativeLink {
        static Path loadPlugin() {
            try {
                return com.minecraftindustrialcomplex.micnative.api.MicNative
                        .loadPlugin(MicRtsim.MODID, LIBRARY).libraryPath();
            } catch (com.minecraftindustrialcomplex.micnative.api.MicNativeException e) {
                throw new IllegalStateException(e.getMessage(), e);
            }
        }
    }

    // ---- one engine -----------------------------------------------------------------------------

    private final Path pagesDir;
    private final int capacity;
    private final int flags;
    private volatile long handle;
    private final ByteBuffer[] buffers = new ByteBuffer[4];
    private final ArrayDeque<Received> received = new ArrayDeque<>();
    /** Java's dictionary as this engine received it (DICT records, by id), kept whole: not the capped log. */
    private final Map<Integer, String> javaDict = new HashMap<>();
    private final long[] stats = new long[STATS];
    private long submits;

    private NativeEngine(Path pagesDir, int capacity, int flags, long handle) {
        this.pagesDir = pagesDir;
        this.capacity = capacity;
        this.flags = flags;
        this.handle = handle;
        for (int i = 0; i < 4; i++) {
            ByteBuffer b = nBuffer(handle, i);
            if (b == null)
                throw new IllegalStateException("rtsim_jni returned no buffer " + i + ": " + nLastError());
            BUFFERS.register(b, new ReleaseHold(nHoldBuffers(handle)));
            buffers[i] = b.order(Layout.ORDER);
        }
    }

    /**
     * Opens the engine on the pages in {@code pagesDir} (created if missing).
     *
     * @param capacity record bytes per buffer: a multiple of 8, at least {@link Layout#MIN_CAPACITY};
     *                 0 lets the engine size them
     * @param flags    {@link #FLAG_TEST}, {@link #FLAG_NO_FSYNC}, or 0
     * @throws IllegalStateException if the library cannot be loaded or the engine cannot open
     */
    public static NativeEngine open(Path pagesDir, int capacity, int flags) {
        ensureLoaded();
        long h;
        try {
            h = nOpen(pagesDir.toAbsolutePath().toString(), capacity, flags);
        } catch (RuntimeException e) {
            throw new IllegalStateException("the native engine did not open " + pagesDir + ": " + e.getMessage(), e);
        }
        if (h == 0)
            throw new IllegalStateException("the native engine did not open " + pagesDir + ": " + nLastError());
        try {
            return new NativeEngine(pagesDir, capacity, flags, h);
        } catch (RuntimeException e) {
            nClose(h, false, CLOSE_TIMEOUT_MS);
            throw e;
        }
    }

    public Path pagesDir() {
        return pagesDir;
    }

    private long live() {
        long h = handle;
        if (h == 0)
            throw new IllegalStateException("the native engine is closed");
        return h;
    }

    private static void check(int status, String call) {
        if (status < 0) {
            String why = nLastError();
            throw new IllegalStateException("rtsim_jni " + call + " failed with status " + status
                    + (why == null || why.isEmpty() ? "" : ": " + why));
        }
    }

    // ---- Engine ---------------------------------------------------------------------------------

    @Override
    public ByteBuffer observationBuffer(int index) {
        return buffers[index];
    }

    @Override
    public ByteBuffer commandBuffer(int index) {
        return buffers[2 + index];
    }

    // Every native call that takes the handle runs under this object's monitor, and so does close:
    // a call can never use a handle another thread is freeing.

    @Override
    public synchronized SessionStart start(long lastAppliedCmdSeq) {
        long[] out = new long[2];
        check(nStart(live(), lastAppliedCmdSeq, out), "start");
        return new SessionStart(out[0], out[1]);
    }

    @Override
    public synchronized void submit(long handover) {
        check(nSubmit(live(), handover), "submit(" + handover + ")");
        submits++;
        if ((flags & FLAG_TEST) != 0)
            drain();
    }

    @Override
    public synchronized void requestSave(long throughObsSeq) {
        nRequestSave(live(), throughObsSeq);
    }

    @Override
    public synchronized String describe() {
        long h = handle;
        return h == 0 ? "native engine (closed)" : nDescribe(h);
    }

    /** The final save, waited for (up to 5 s for a step in flight). Idempotent. */
    @Override
    public synchronized void close() {
        long h = takeHandle();
        if (h == 0)
            return;
        int status = nClose(h, true, CLOSE_TIMEOUT_MS);
        if (status < 0)
            MicRtsim.LOGGER.error("[mic_rtsim] the native engine's final save failed (status {}): {}; the journal replays what the pages miss",
                    status, nLastError());
    }

    /** Takes the handle once: this object is dead afterwards. Drains the received log first. */
    private synchronized long takeHandle() {
        long h = handle;
        if (h != 0 && (flags & FLAG_TEST) != 0)
            drain();
        handle = 0;
        return h;
    }

    // ---- ScriptedEngine: orders -----------------------------------------------------------------

    private synchronized void order(NativeCodec.Orders orders, String what) {
        check(nOrder(live(), orders.toBytes()), "order " + what);
    }

    @Override
    public void spawnGroup(long agent, int epoch, String groupKind, String dimension, double x, double y, double z,
                           List<MemberSpec> members) {
        spawnTogether(new SpawnSpec(agent, epoch, groupKind, dimension, x, y, z, members));
    }

    @Override
    public void spawnTogether(SpawnSpec... spawns) {
        NativeCodec.Orders orders = new NativeCodec.Orders();
        for (SpawnSpec s : spawns)
            orders.spawnGroup(s);
        order(orders, "SPAWN_GROUP");
    }

    @Override
    public void despawnGroup(long agent, int epoch) {
        order(new NativeCodec.Orders().despawnGroup(agent, epoch), "DESPAWN_GROUP");
    }

    @Override
    public void steerGroup(long agent, int epoch, double x, double y, double z, float speed) {
        order(new NativeCodec.Orders().steerGroup(agent, epoch, x, y, z, speed), "STEER_GROUP");
    }

    @Override
    public void transferToContainer(long opId, String dimension, int x, int y, int z, String item, int count) {
        order(new NativeCodec.Orders().transferToContainer(opId, dimension, x, y, z, item, count), "TRANSFER_TO_CONTAINER");
    }

    @Override
    public void rewindCommandSequence(long next) {
        order(new NativeCodec.Orders().rewindCommandSequence(next), "REWIND_CMD_SEQ");
    }

    @Override
    public void scriptVillage(long village) {
        order(new NativeCodec.Orders().scriptVillage(village), "SCRIPT_VILLAGE");
    }

    @Override
    public void market(long village, long asOfObsSeq, String currency, long treasury, long treasuryTarget, int worstNum, int worstDen,
                       List<GoodSpec> goods) {
        order(new NativeCodec.Orders().market(village, asOfObsSeq, currency, treasury, treasuryTarget, worstNum, worstDen, goods), "MARKET");
    }

    @Override
    public void merchantOffers(long agent, int epoch, int member, List<OfferSpec> offers) {
        order(new NativeCodec.Orders().merchantOffers(agent, epoch, member, offers), "MERCHANT_OFFERS");
    }

    @Override
    public void writeGossip(long opId, long village, UUID player, int gossipType, int amount, int report, int flags) {
        order(new NativeCodec.Orders().writeGossip(opId, village, player, gossipType, amount, report, flags), "WRITE_GOSSIP");
    }

    @Override
    public void sentiment(long holder, int dislike, int friend, boolean replace, Map<UUID, Integer> entries) {
        order(new NativeCodec.Orders().sentiment(holder, dislike, friend, replace, entries), "SENTIMENT");
    }

    @Override
    public void placeHouse(long opId, long village, String dimension, String template, String processors, int x, int y, int z,
                           int rotation, int mirror, int flags, int radius, long seed, int yOffset) {
        order(new NativeCodec.Orders().placeHouse(opId, village, dimension, template, processors, x, y, z, rotation, mirror,
                flags, radius, seed, yOffset), "PLACE_HOUSE");
    }

    @Override
    public void spawnResident(long opId, long village, String dimension, String body, String profession, int flags,
                              double x, double y, double z, long house) {
        order(new NativeCodec.Orders().spawnResident(opId, village, dimension, body, profession, flags, x, y, z, house), "SPAWN_RESIDENT");
    }

    /**
     * Test only (an engine opened with {@link #FLAG_TEST}): materializes the phase 1 caravan with wares
     * nearest (searchX, searchZ) within {@code radius} blocks, as group {@code agent}/{@code epoch}, at
     * (x, y, z). Nothing spawns if no caravan carrying at least one whole item is that close (the
     * engine logs it): advance the world and ask again.
     */
    public void phase1SpawnNear(long agent, int epoch, String dimension, double x, double y, double z, int radius,
                                double searchX, double searchZ) {
        order(new NativeCodec.Orders().phase1SpawnNear(agent, epoch, dimension, x, y, z, radius, searchX, searchZ),
                "P1_SPAWN_NEAR");
    }

    /**
     * Test only (an engine opened with {@link #FLAG_TEST}): the phase 1 world runs this many more ticks
     * ahead of game time from now on.
     */
    public void phase1Advance(long ticks) {
        order(new NativeCodec.Orders().phase1Advance(ticks), "P1_ADVANCE");
    }

    /** INDUSTRY_THREAT's source and class codes (rtsim-jni's orders.rs SOURCES and ThreatClass::ALL). */
    public static final int THREAT_NATURAL = 0, THREAT_DRAGON = 1, THREAT_BANDIT = 2, THREAT_BOSS = 3, THREAT_SYNTHETIC = 4,
            THREAT_NEST = 5;
    public static final int CLASS_GROUND_MELEE = 0, CLASS_GROUND_RANGED = 1, CLASS_FLYING = 2, CLASS_SIEGE = 3,
            CLASS_BURROWING = 4;

    /**
     * An attack on a district (INDUSTRY_THREAT, target kind 1), resolved by the industry engine at its next
     * step as an unwatched attack: {@code count} attackers of one class and source, {@code strength} in all, at
     * {@code gameTime} (GameTests: the roof gate, OILGATE.md OG-11).
     */
    public void industryThreat(long id, int source, int threatClass, int count, double strength, long district, long gameTime) {
        order(new NativeCodec.Orders().industryThreat(id, source, threatClass, count, 1, strength, district, gameTime), "INDUSTRY_THREAT");
    }

    /**
     * Test only (an engine opened with {@link #FLAG_TEST}): coin, in thousandths of the village's own
     * currency, put straight into a phase 1 village's works purse (SINK.md), at the next step.
     */
    public void worksGrant(long village, long thousandths) {
        order(new NativeCodec.Orders().worksGrant(village, thousandths), "WORKS_GRANT");
    }

    /**
     * Test only (an engine opened with {@link #FLAG_TEST}): a dragon raided a phase 1 village (SINK.md
     * §4.6); with {@code tribute} the village pays it from its works purse.
     */
    public void worksRaid(long village, boolean tribute) {
        order(new NativeCodec.Orders().worksRaid(village, tribute), "WORKS_RAID");
    }

    /**
     * Test only (an engine opened with {@link #FLAG_TEST}): phase 1 village {@code village} becomes the
     * tributary of the dragons' lair {@code lair} (LAIR_FOUND's id), as if a dragon of the lair had raided
     * it (SINK.md §4.6): it pays a season's tribute at once, which waits in the lair until the plugin
     * reports its chest (LAIR_CHEST). At the next step.
     */
    public void tributeStart(long village, long lair) {
        order(new NativeCodec.Orders().tributeStart(village, lair), "TRIBUTE_START");
    }

    /**
     * Test only (an engine opened with {@link #FLAG_TEST}): the oil nests' day, their watch period and the
     * unit of their regrowth waits (OILGATE.md §4.6), is {@code ticks} from the next step on; 0 restores the
     * default (24,000).
     */
    public void nestDay(long ticks) {
        order(new NativeCodec.Orders().nestDay(ticks), "NEST_DAY");
    }

    /** An actor a test report names: a TALK ref's class ({@code TalkRecords.C_*}) and its two ids (docs/boundary.md §13.2). */
    public record TalkActor(int cls, long a, long b) {
        public static final TalkActor NONE = new TalkActor(0, 0, 0);

        /** A player by UUID. */
        public static TalkActor player(java.util.UUID id) {
            return new TalkActor(1, id.getMostSignificantBits(), id.getLeastSignificantBits());
        }

        /** A village by Java's id. */
        public static TalkActor village(long id) {
            return new TalkActor(2, id, 0);
        }
    }

    /**
     * Test only (an engine opened with {@link #FLAG_TEST}): puts one report on {@code village}'s noticeboard
     * (TK40): kind (an rtsim-knowledge {@code ReportKind} id), severity, the report's flags, who and whom, the
     * place (x, z, in the village's frame) and how many ticks ago it happened. The engine refuses it (logged,
     * nothing changes) when the village is not adopted, an actor does not resolve, or the report would already
     * be expired. At the next step.
     */
    public void talkReport(long village, int kind, int severity, int flags, TalkActor who, TalkActor whom, int x, int z, long ago) {
        order(new NativeCodec.Orders().talkReport(village, kind, severity, flags, who, whom, x, z, ago), "TALK_REPORT");
    }

    @Override
    public synchronized String javaDictName(int id) {
        drain();
        return javaDict.get(id);
    }

    @Override
    public void failForTest() {
        order(new NativeCodec.Orders().fail(), "FAIL");
    }

    @Override
    public void requestSaveAtNextStep() {
        order(new NativeCodec.Orders().saveAtNextStep(), "SAVE_AT_NEXT_STEP");
    }

    /** One entry of Java's dictionary that an ordered industry command uses. */
    public record DictEntry(int kind, int id, String name) {}

    /**
     * Orders an industry command (docs/industry-boundary.md §4): {@code record} is the whole record,
     * its cmdSeq ignored, with Java's own dictionary ids (GOOD, NAME, BLOCK_STATE, dimension), and
     * {@code names} the entries it uses. The engine writes it as its industry engine's own: its
     * sequence, the DICT records it needs, and, for a carriage, channel, deposit or package command,
     * its outbox until Java answers. Tests drive the industry module with it; the engine decides its
     * own commands.
     */
    public void industryCommand(List<DictEntry> names, byte[] record) {
        order(new NativeCodec.Orders().industryCommand(names, record), "INDUSTRY_COMMAND");
    }

    /**
     * Orders a dragon command (docs/boundary.md §6.4): {@code record} is the whole record, its cmdSeq
     * ignored, with Java's own species ids (body dictionary) and dimension id, and {@code names} the
     * species entries it uses. The engine writes it as the dragons' own: their sequence and the DICT
     * records it needs. TEST mode only: the Ice and Fire plugin's GameTests drive the plugin with it;
     * the dragons decide their own commands.
     */
    public void dragonCommand(List<DictEntry> names, byte[] record) {
        order(new NativeCodec.Orders().dragonCommand(names, record), "DRAGON_COMMAND");
    }

    /**
     * Orders a herd command (docs/boundary.md §8.4): {@code record} is the whole record, its cmdSeq
     * ignored, with Java's own species ids (body dictionary) and dimension id, and {@code names} the
     * species entries it uses. The engine writes it as the herds' own: their sequence and the DICT
     * records it needs. TEST mode only: the vanilla herd adapter's GameTests drive the adapter with it;
     * the herds decide their own commands.
     */
    public void herdCommand(List<DictEntry> names, byte[] record) {
        order(new NativeCodec.Orders().herdCommand(names, record), "HERD_COMMAND");
    }

    /**
     * Orders a conflict command (docs/boundary.md §9): {@code record} is the whole record, its cmdSeq
     * ignored, with Java's own body, item and dimension ids, and {@code names} the body and item entries
     * it uses. The engine writes it as the conflict's own: its sequence and the DICT records it needs.
     * TEST mode only, and dropped unless the conflict module is on: the conflict bridge's GameTests drive
     * the bridge with it; the conflict decides its own commands.
     */
    public void conflictCommand(List<DictEntry> names, byte[] record) {
        order(new NativeCodec.Orders().conflictCommand(names, record), "CONFLICT_COMMAND");
    }

    /**
     * Test only: founds a herd of {@code count} animals of {@code species} ({@code minecraft:sheep} and
     * the rest) at (x, z) in a dimension (Java's dictionary id) when the engine takes the order; seed 0
     * draws one. Nothing in the game founds herds yet (no habitat provider).
     */
    public void herdFound(String name, String species, int dimension, int x, int z, long count, long seed) {
        order(new NativeCodec.Orders().herdFound(name, species, dimension, x, z, count, seed), "HERD_FOUND");
    }

    /** {@link #herdInfo}'s fields. */
    public static final int HERD_EXISTS = 0, HERD_ALIVE = 1, HERD_KILLED = 2, HERD_TAKEN = 3, HERD_LOST = 4,
            HERD_EMBODIED_EPOCH = 5, HERD_EMBODIED_MEMBERS = 6, HERD_DEAD = 7, HERD_LAST_EPOCH = 8, HERD_INFO = 9;

    /**
     * One herd by its group's agent id, as of the engine's last step (indexed by {@code HERD_*}): alive,
     * killed by players, taken, lost, the embodied group's epoch and members, the dead-index set's
     * size, the last materialization's epoch. Null when the agent is no herd's.
     */
    public synchronized long[] herdInfo(long agent) {
        long[] out = new long[HERD_INFO];
        return nHerdInfo(live(), agent, out) ? out : null;
    }

    /**
     * A phase 1 village's fear to the dragons before a dragon's boldness bends it (fullness points, its
     * iron golems' included: SINK.md §4.6), as of the engine's last step; -1 for a village it does not know.
     */
    public synchronized long dragonsVillageRisk(long village) {
        return nDragonsVillageRisk(live(), village);
    }

    /** {@link #dragonsTribute}'s fields. */
    public static final int TRIBUTE_PAID = 0, TRIBUTE_PLACED = 1, TRIBUTE_WAITING = 2, TRIBUTE_SENDING = 3, TRIBUTE_INFO = 4;

    /**
     * The dragons' tribute in the whole world, as of the engine's last step (indexed by {@code TRIBUTE_*}):
     * seasons paid, whole coins put in lairs' chests, whole coins waiting for a chest, and stacks sent to a
     * chest and not yet answered.
     */
    public synchronized long[] dragonsTribute() {
        long[] out = new long[TRIBUTE_INFO];
        nDragonsTribute(live(), out);
        return out;
    }

    /** {@link #dragonsOil}'s fields. */
    public static final int OIL_CHUNKS = 0, OIL_FIELDS = 1, OIL_FIELD_ROLLS = 2, OIL_FIELD_DRAWS = 3, OIL_REFUELS = 4,
            OIL_RAIDS = 5, OIL_DRUNK_MB = 6, OIL_BREATHS = 7, OIL_DRY_RAIDS = 8, OIL_ENCROACHMENTS = 9,
            OIL_DISTRICT_RAIDS = 10, OIL_LAIRS_PLACED = 11, OIL_VISITORS_LEFT = 12, OIL_SURVEYS = 13, OIL_LEVEL = 14,
            OIL_FLAGS = 15, OIL_NOW = 16, OIL_WHEEL_NOW = 17, OIL_PRESENCES = 18, OIL_PRESENCES_FRESH = 19,
            OIL_PRESENCES_AHEAD = 20, OIL_ROLLS_ACTIVE = 21, OIL_ROLLS_BOOKED = 22, OIL_ROLL_NEXT = 23,
            OIL_VISITORS = 24, OIL_VISITORS_MOST = 25, OIL_INFO = 26;

    /**
     * The oil gate's counters in the whole world, as of the engine's last step (indexed by {@code OIL_*};
     * OILGATE.md §4): oil chunks indexed, fields, field-flight rolls and draws, refuels, drinking raids, mB
     * drunk, breaths, dry raids, Encroachment reports, district raids, lairs placed, visitors gone, surveys;
     * then dragon {@code agent}'s reserve now (-1 for no such dragon) and the OIL_CONFIG switches it holds;
     * then the field flights' state, for tests: the engine's clock and its event wheel's, players' presences
     * kept, fresh and stamped ahead of the clock, rolls active, FieldRoll events booked and the earliest one's
     * tick (-1 with none), field flights' dragons alive, and the most on one field.
     */
    public synchronized long[] dragonsOil(long agent) {
        long[] out = new long[OIL_INFO];
        nDragonsOil(live(), agent, out);
        return out;
    }

    /** {@link #nestInfo}'s fields. NEST_STATE: 0 asked, 1 placed, 2 heart broken, 3 placement failed. */
    public static final int NEST_STATE = 0, NEST_STAGE = 1, NEST_WORMS = 2, NEST_K = 3, NEST_AWAKE = 4,
            NEST_PRESSURE_MILLI = 5, NEST_WAIT_TICKS = 6, NEST_RENEWED = 7, NEST_LOST_PLAYER = 8, NEST_RAIDS = 9,
            NEST_HATCHED = 10, NEST_COUNT = 11, NEST_INFO = 12;

    /**
     * Oil nest {@code nest} (PLACE_NEST's id) as of the engine's last step (OILGATE.md §4.6, indexed by
     * {@code NEST_*}): its state, its worms' size stage (1 to 3), worms alive, its K, whether a drilled well
     * kept it awake at its last watch (0 or 1), its pressure in thousandths, the regrowth wait left in ticks
     * (-1 with none running), worms renewed, worms players killed, raids sent, whether its founding brood
     * hatched (0 or 1), and the number of nests in the world. Null for no such nest.
     */
    public synchronized long[] nestInfo(long nest) {
        long[] out = new long[NEST_INFO];
        return nDragonsNest(live(), nest, out) ? out : null;
    }

    /**
     * A district's glass report as of the engine's last step (G1, read only): the solve's bounds per good
     * and the checksum's verdict ({@code industry.glass.GlassReport} decodes it). Null while the district
     * has no complete compile.
     */
    public synchronized double[] glassReport(long district) {
        return nGlassReport(live(), district);
    }

    /**
     * One district's threat state as of the engine's last step, for tests (the roof gate, OG-11): whether the
     * engine has it (1 or 0), watched, presence (0 unloaded, 1 partial, 2 live), loaded chunks, exposed blocks
     * and the roofless ones, blocks waiting to be destroyed, attack reports; then the last report's tick,
     * skipped (0 no, 1 watched, 2 cooldown, 3 other), strength through, defender, survived, survivors and
     * blocks destroyed; then the engine's threats taken, attacks and attacks skipped; then the link's
     * commands queued and kept, written, dropped, failed by Java, the front command's name waits, and the
     * module switch (1 on).
     */
    public synchronized double[] industryThreatProbe(long district) {
        return nIndustryThreatProbe(live(), district);
    }

    /**
     * The industry engine's learned train lines (choices SG50, SG51), flat: the unwatched stops it took, the
     * self edges it refused, the loaded calls it saw (IND_TRAIN_ARRIVED at a loaded district's station), then
     * every edge as {@code (from station, to station, trips, travel ticks)}, the stations as the wire carries
     * them ({@code CreateIds.of}). Between steps.
     */
    public synchronized long[] industryLines() {
        return nIndustryLines(live());
    }

    /**
     * The same report from a record stream (whole records back to back, as Java sent them) run through a
     * fresh industry engine: the stub engine's GameTests check their run with the native code (choice
     * GB80). Loads the natives; throws IllegalStateException when they cannot load.
     */
    public static double[] glassOffline(byte[] records, long district) {
        ensureLoaded();
        return nGlassOffline(records, district);
    }

    /**
     * The replay of a record stream (as {@link #glassOffline} takes it) on a fresh industry engine: every
     * IND_CHUNK_RECONCILE that engine would send {@code district}, once per op id, flattened to triples
     * {@code (inventory, good, delta)} at indices {@code 3i, 3i + 1, 3i + 2}: the inventory's index in the
     * district, the good's dictionary id (Java's DICT_GOOD id, as {@code Industry.goodName} reads it) and the
     * signed count. Empty when there are none. Nothing applies them: the stub engine's G2 exit GameTests add
     * them to what the world did. Loads the natives; throws IllegalStateException when they cannot load.
     */
    public static long[] glassReplayOffline(byte[] records, long district) {
        ensureLoaded();
        return nGlassReplayOffline(records, district);
    }

    /**
     * The industry engine's own log lines about {@code district} from game time {@code from} on, oldest
     * first, each {@code "<tick> <words>"} (replay starts, breakpoints, "glass replay falls back: ..."):
     * the G2 exit GameTests log why a district replayed the way it did. Empty unless the engine keeps its
     * log (test sessions). Between steps.
     */
    public synchronized List<String> industryNotes(long district, long from) {
        return lines(nIndustryNotes(live(), district, from));
    }

    /**
     * {@link #industryNotes} of the fresh industry engine of {@link #glassOffline} over a record stream (the
     * stub engine's G2 exit GameTests). Loads the natives; throws IllegalStateException when they cannot load.
     */
    public static List<String> glassNotesOffline(byte[] records, long district, long from) {
        ensureLoaded();
        return lines(nGlassNotesOffline(records, district, from));
    }

    private static List<String> lines(String joined) {
        return joined == null || joined.isEmpty() ? List.of() : List.of(joined.split("\n"));
    }

    /** {@link #habitatInfo}'s fields (docs/boundary.md §10, {@code Engine::habitat_info}). */
    public static final int HABITAT_KNOWN_CELL = 0, HABITAT_KNOWN_MASK = 1, HABITAT_WATER = 2, HABITAT_ICE = 3, HABITAT_LAND = 4,
            HABITAT_SURFACE = 5, HABITAT_FINGERPRINT = 6, HABITAT_CELLS = 7, HABITAT_REGIONS = 8, HABITAT_REGION_HERD = 9,
            HABITAT_REGION_MARKS = 10, HABITAT_RECORDS = 11, HABITAT_HERDS_FOUNDED = 12, HABITAT_KNOWN_WRITTEN = 13,
            HABITAT_HERD_X = 14, HABITAT_HERD_Z = 15, HABITAT_INFO = 16;

    /**
     * The engine's world model at block (x, z) of Java's dimension {@code dim}, between steps (tests): whether
     * the cell is known, its masks, mean surface and fingerprint, the cells and regions known, the herd founded
     * for the region (its agent, 0 none, and where it is now), and counters.
     */
    public synchronized long[] habitatInfo(int dim, int x, int z) {
        long[] out = new long[HABITAT_INFO];
        nHabitatInfo(live(), dim, x, z, out);
        return out;
    }

    // ---- ScriptedEngine: inspection -------------------------------------------------------------

    /** Moves the native received log into the Java list (capped, oldest dropped first). */
    private synchronized void drain() {
        long h = handle;
        if (h == 0)
            return;
        byte[] log = nDrainReceived(h);
        if (log == null || log.length == 0)
            return;
        List<Received> fresh = new ArrayList<>();
        NativeCodec.decodeReceived(log, fresh);
        for (Received r : fresh) {
            if (r.kind() == com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.DICT)
                learnDict(r.view());
            if (received.size() >= RECEIVED_CAP)
                received.removeFirst();
            received.addLast(r);
        }
    }

    /** One DICT record into {@link #javaDict} (a later entry for an id replaces it, as the engine reads it). */
    private void learnDict(ByteBuffer b) {
        int len = b.getInt(com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Dict.NAME_LEN);
        if (len < 0 || len > b.capacity() - com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Dict.NAME)
            return;
        byte[] bytes = new byte[len];
        b.get(com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Dict.NAME, bytes, 0, len);
        javaDict.put(b.getInt(com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Dict.ID),
                new String(bytes, java.nio.charset.StandardCharsets.UTF_8));
    }

    @Override
    public synchronized List<Received> received() {
        drain();
        return Collections.unmodifiableList(new ArrayList<>(received));
    }

    @Override
    public synchronized List<Received> received(int kind) {
        drain();
        List<Received> out = new ArrayList<>();
        for (Received r : received)
            if (r.kind() == kind)
                out.add(r);
        return out;
    }

    private synchronized long stat(int index) {
        nStats(live(), stats);
        return stats[index];
    }

    @Override
    public synchronized long ledger(long store) {
        return nLedger(live(), store);
    }

    @Override
    public synchronized boolean opApplied(long opId) {
        return nOpApplied(live(), opId);
    }

    /** The phase 1 world's counters, indexed by {@code P1_*}; {@link #PHASE1_STATS} long. */
    public synchronized long[] phase1Stats() {
        long[] out = new long[PHASE1_STATS];
        nPhase1Stats(live(), out);
        return out;
    }

    /**
     * A phase 1 village's sentiment toward a player, -128 to 127; {@link Integer#MIN_VALUE} when the
     * engine knows no such village. An engine opened with {@link #FLAG_TEST} only.
     */
    public synchronized int phase1Sentiment(long village, UUID player) {
        return nPhase1Sentiment(live(), village, player.getMostSignificantBits(), player.getLeastSignificantBits());
    }

    /**
     * A phase 1 village's works as of the last step ({@link #WORKS_VIEW} long, WORKS_* indexes), or
     * null when the engine knows no such village or it has no works yet. An engine opened with
     * {@link #FLAG_TEST} only.
     */
    public synchronized long[] phase1Works(long village) {
        long[] out = new long[WORKS_VIEW];
        return nPhase1Works(live(), village, out) ? out : null;
    }

    @Override
    public long steps() {
        return stat(STAT_STEPS_DONE);
    }

    @Override
    public long staleSkipped() {
        return stat(STAT_STALE_SKIPPED);
    }

    @Override
    public long opDuplicates() {
        return stat(STAT_OP_DUPLICATES);
    }

    @Override
    public long maxObsSeq() {
        return stat(STAT_APPLIED_THROUGH);
    }

    @Override
    public long pagesObsSeq() {
        return stat(STAT_PAGES_OBS_SEQ);
    }

    /** Nanoseconds the last step took on its pool thread (from its start to its publish). */
    public long lastStepNanos() {
        return stat(STAT_LAST_STEP_NANOS);
    }

    @Override
    public int scriptedPending() {
        return (int) stat(STAT_ORDERS_PENDING);
    }

    @Override
    public synchronized String protocolError() {
        return nProtocolError(live());
    }

    @Override
    public void awaitIdle() {
        long want;
        synchronized (this) {
            want = submits;
        }
        long deadline = System.nanoTime() + IDLE_TIMEOUT_NANOS;
        while (stat(STAT_STEPS_DONE) < want) {
            if (System.nanoTime() - deadline > 0)
                throw new IllegalStateException("native engine stuck: " + stat(STAT_STEPS_DONE) + " of " + want + " steps done after 10 s");
            try {
                Thread.sleep(1);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException("interrupted waiting for the native engine", e);
            }
        }
    }

    @Override
    public synchronized NativeEngine crashAndReopen() {
        long h = takeHandle();
        if (h == 0)
            throw new IllegalStateException("the native engine is closed");
        int status = nClose(h, false, CLOSE_TIMEOUT_MS);
        if (status < 0)
            throw new IllegalStateException("rtsim_jni close (crash) failed with status " + status + ": " + nLastError());
        return open(pagesDir, capacity, flags);
    }
}
