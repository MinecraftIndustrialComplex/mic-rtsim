package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.ChunkPos;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;
import java.util.function.Consumer;

/**
 * Helpers for the conflict GameTests ({@link ConflictGameTests}). A test scripts conflict commands as the
 * engine's conflict would send them (docs/boundary.md §9): the stub writes them with its own dictionary,
 * the native engine takes them as CONFLICT_COMMAND orders with Java's dictionary ids and writes them as
 * its own (the conflict knows nothing of a scripted agent or faction and ignores what Java answers about
 * them). Ticks in a command (an arrival, a deadline) are game time.
 */
public final class ConflictKit {
    private ConflictKit() {}

    /** The goblin culture's bodies, which stand in as vanilla mobs (MZB-102). */
    public static final String WARRIOR = "mic_rtsim:conflict/goblin/warrior"; // a vindicator
    public static final String ARCHER = "mic_rtsim:conflict/goblin/archer"; // a pillager
    public static final String EMISSARY = "mic_rtsim:conflict/goblin/emissary"; // a pillager

    /** Test agents: namespace 0x43 over random 47-bit handles with bit 47 set, apart from the engine's own parties. */
    public static long newAgent() {
        return (ConflictRecords.AGENT_NAMESPACE << 56) | (1L << 47) | (ThreadLocalRandom.current().nextLong() & 0x7FFF_FFFF_FFFFL);
    }

    /** A test faction key no structure start can have (a village id's top byte is never 0x0D). */
    public static long newFaction() {
        return 0x0D00_0000_0000_0000L | (ThreadLocalRandom.current().nextLong() & 0xFFFF_FFFF_FFFFL);
    }

    public static ConflictBridge bridge() {
        return ConflictBridge.INSTANCE;
    }

    /** The party of an agent, waiting for it (inside thenWaitUntil). */
    public static ConflictBridge.Party party(long agent) {
        ConflictBridge.Party p = bridge().party(agent);
        if (p == null)
            throw new GameTestAssertException("conflict agent " + Long.toHexString(agent) + " holds no party yet");
        return p;
    }

    // ---- scripting the engine side --------------------------------------------------------------

    /** Commands {@link #together} collects instead of sending; null when none is collecting. */
    private static final ThreadLocal<List<Consumer<StubEngine.CommandOut>>> COLLECTING = new ThreadLocal<>();

    private static void script(Consumer<StubEngine.CommandOut> command) {
        List<Consumer<StubEngine.CommandOut>> collecting = COLLECTING.get();
        if (collecting != null) {
            collecting.add(command);
            return;
        }
        send(command);
    }

    private static void send(Consumer<StubEngine.CommandOut> command) {
        ScriptedEngine e = RtsimTests.engine();
        if (e instanceof StubEngine stub) {
            stub.script(command);
            return;
        }
        if (!(e instanceof NativeEngine n))
            throw new GameTestAssertException("no scripted conflict commands on " + e.describe());
        RecordOut out = new RecordOut();
        command.accept(out);
        if (out.b == null)
            throw new GameTestAssertException("the script wrote no record");
        n.conflictCommand(out.names, out.b.array());
    }

    /**
     * The commands {@code r} scripts, in one hand-over: the stub writes them together; the native engine
     * takes them as CONFLICT_COMMAND orders in a row, which its next step writes together.
     */
    public static void together(Runnable r) {
        List<Consumer<StubEngine.CommandOut>> commands = new ArrayList<>();
        COLLECTING.set(commands);
        try {
            r.run();
        } finally {
            COLLECTING.remove();
        }
        if (commands.isEmpty())
            return;
        if (RtsimTests.engine() instanceof StubEngine stub) {
            stub.script(out -> {
                for (Consumer<StubEngine.CommandOut> c : commands)
                    c.accept(out);
            });
            return;
        }
        for (Consumer<StubEngine.CommandOut> c : commands)
            send(c);
    }

    /** One record with Java's dictionary ids, for the native engine's CONFLICT_COMMAND order. */
    private static final class RecordOut implements StubEngine.CommandOut {
        private final List<NativeEngine.DictEntry> names = new ArrayList<>();
        private ByteBuffer b;

        @Override
        public int dict(int dictKind, String name) {
            int id = RtsimTests.session().dictId(dictKind, name);
            if (dictKind != Obs.DICT_DIMENSION) // the engine names dimensions by Java's own dictionary
                names.add(new NativeEngine.DictEntry(dictKind, id, name));
            return id;
        }

        @Override
        public int reserve(int kind, int size, int aux) {
            if (b != null)
                throw new IllegalStateException("one record per CONFLICT_COMMAND order");
            int aligned = (size + 7) & ~7;
            b = ByteBuffer.allocate(aligned).order(Layout.ORDER);
            b.putShort(Layout.R_KIND, (short) kind);
            b.putShort(Layout.R_SIZE, (short) aligned);
            b.putInt(Layout.R_AUX, aux);
            return 0;
        }

        @Override
        public ByteBuffer buffer() {
            return b;
        }
    }

    private static void putUuid(ByteBuffer b, int most, int least, UUID u) {
        if (u == null)
            return;
        b.putLong(most, u.getMostSignificantBits());
        b.putLong(least, u.getLeastSignificantBits());
    }

    /**
     * One stack of a BAND_MATERIALIZE: a body kind, how many, whether the war chief walks in it, and whether
     * its units are bare (no crystal on them, STACK_BARE).
     */
    public record StackSpec(String body, int count, boolean chief, boolean bare) {
        public StackSpec(String body, int count, boolean chief) {
            this(body, count, chief, false);
        }
    }

    /** BAND_MATERIALIZE's fields but its stacks; build with {@link #band}. */
    public static final class Band {
        public long agent, faction;
        public int epoch = 1, wave, purpose = ConflictRecords.PURPOSE_RAID, flags = ConflictRecords.Materialize.FLAG_HOSTILE;
        public String dimension;
        public double arcX, arcZ, targetX, targetY = 64, targetZ;
        public int arcRadius;
        public float bearing, halfWidth, speed;
        public UUID targetPlayer;
    }

    public static Band band(long agent, long faction, String dimension) {
        Band b = new Band();
        b.agent = agent;
        b.faction = faction;
        b.dimension = dimension;
        return b;
    }

    public static void materialize(Band band, List<StackSpec> stacks) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, band.dimension);
            int[] bodies = new int[stacks.size()];
            for (int i = 0; i < stacks.size(); i++)
                bodies[i] = out.dict(Obs.DICT_BODY, stacks.get(i).body());
            int m = stacks.size();
            int at = out.reserve(ConflictRecords.BAND_MATERIALIZE, ConflictRecords.Materialize.size(m), m);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Materialize.AGENT, band.agent);
            b.putInt(at + ConflictRecords.Materialize.EPOCH, band.epoch);
            b.putInt(at + ConflictRecords.Materialize.DIM, dim);
            b.putLong(at + ConflictRecords.Materialize.FACTION, band.faction);
            b.put(at + ConflictRecords.Materialize.PURPOSE, (byte) band.purpose);
            b.put(at + ConflictRecords.Materialize.FLAGS, (byte) band.flags);
            b.putShort(at + ConflictRecords.Materialize.WAVE, (short) band.wave);
            b.putFloat(at + ConflictRecords.Materialize.BEARING, band.bearing);
            b.putDouble(at + ConflictRecords.Materialize.ARC_X, band.arcX);
            b.putDouble(at + ConflictRecords.Materialize.ARC_Z, band.arcZ);
            b.putInt(at + ConflictRecords.Materialize.ARC_RADIUS, band.arcRadius);
            b.putFloat(at + ConflictRecords.Materialize.HALF_WIDTH, band.halfWidth);
            b.putDouble(at + ConflictRecords.Materialize.TARGET_X, band.targetX);
            b.putDouble(at + ConflictRecords.Materialize.TARGET_Y, band.targetY);
            b.putDouble(at + ConflictRecords.Materialize.TARGET_Z, band.targetZ);
            putUuid(b, at + ConflictRecords.Materialize.TARGET_PLAYER_MOST, at + ConflictRecords.Materialize.TARGET_PLAYER_LEAST,
                    band.targetPlayer);
            b.putFloat(at + ConflictRecords.Materialize.SPEED, band.speed);
            for (int i = 0; i < m; i++) {
                int o = at + ConflictRecords.Materialize.STACKS + i * ConflictRecords.Materialize.STACK;
                b.putInt(o + ConflictRecords.Materialize.S_BODY, bodies[i]);
                b.putInt(o + ConflictRecords.Materialize.S_COUNT, stacks.get(i).count());
                b.putInt(o + ConflictRecords.Materialize.S_FLAGS, (stacks.get(i).chief() ? ConflictRecords.Materialize.STACK_CHIEF : 0)
                        | (stacks.get(i).bare() ? ConflictRecords.Materialize.STACK_BARE : 0));
            }
        });
    }

    /** One member kind of an ENVOY_MATERIALIZE: the first member made is the envoy. */
    public record MemberSpec(String body, int count) {}

    /** ENVOY_MATERIALIZE at (x, y, z): a demand to {@code player} (null: the settlers) of {@code count} {@code item} and/or a signal stopped. */
    public static void envoy(long agent, int epoch, long faction, String dimension, double x, double y, double z, UUID player,
                             int demand, int signal, String item, int count, long deadline, List<MemberSpec> members) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int[] bodies = new int[members.size()];
            for (int i = 0; i < members.size(); i++)
                bodies[i] = out.dict(Obs.DICT_BODY, members.get(i).body());
            int itemId = item == null ? 0 : out.dict(Obs.DICT_ITEM, item);
            int m = members.size();
            int at = out.reserve(ConflictRecords.ENVOY_MATERIALIZE, ConflictRecords.Envoy.size(m), m);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Envoy.AGENT, agent);
            b.putInt(at + ConflictRecords.Envoy.EPOCH, epoch);
            b.putInt(at + ConflictRecords.Envoy.DIM, dim);
            b.putLong(at + ConflictRecords.Envoy.FACTION, faction);
            b.putDouble(at + ConflictRecords.Envoy.X, x);
            b.putDouble(at + ConflictRecords.Envoy.Y, y);
            b.putDouble(at + ConflictRecords.Envoy.Z, z);
            putUuid(b, at + ConflictRecords.Envoy.PLAYER_MOST, at + ConflictRecords.Envoy.PLAYER_LEAST, player);
            b.putInt(at + ConflictRecords.Envoy.DEMAND, demand);
            b.putInt(at + ConflictRecords.Envoy.SIGNAL, signal);
            b.putInt(at + ConflictRecords.Envoy.ITEM, itemId);
            b.putInt(at + ConflictRecords.Envoy.COUNT, count);
            b.putLong(at + ConflictRecords.Envoy.DEADLINE, deadline);
            for (int i = 0; i < m; i++) {
                int o = at + ConflictRecords.Envoy.MEMBERS + i * ConflictRecords.Envoy.MEMBER;
                b.putInt(o + ConflictRecords.Envoy.M_BODY, bodies[i]);
                b.putInt(o + ConflictRecords.Envoy.M_COUNT, members.get(i).count());
            }
        });
    }

    /** BAND_STEER: every stack walks to its place beside (x, z), arriving at game time {@code arrive} (0: at {@code speed}). */
    public static void steer(long agent, int epoch, float speed, double x, double z, long arrive, boolean attack) {
        script(out -> {
            int at = out.reserve(ConflictRecords.BAND_STEER, ConflictRecords.Steer.SIZE, attack ? ConflictRecords.Steer.FLAG_ATTACK : 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Steer.AGENT, agent);
            b.putInt(at + ConflictRecords.Steer.EPOCH, epoch);
            b.putFloat(at + ConflictRecords.Steer.SPEED, speed);
            b.putDouble(at + ConflictRecords.Steer.X, x);
            b.putDouble(at + ConflictRecords.Steer.Y, 64);
            b.putDouble(at + ConflictRecords.Steer.Z, z);
            b.putLong(at + ConflictRecords.Steer.ARRIVE, arrive);
        });
    }

    /** BAND_RECALL with a reason, a timeout in ticks (0: the default) and the way home. */
    public static void recall(long agent, int epoch, int reason, int timeout, double towardX, double towardZ) {
        script(out -> {
            int at = out.reserve(ConflictRecords.BAND_RECALL, ConflictRecords.Recall.SIZE, reason);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Recall.AGENT, agent);
            b.putInt(at + ConflictRecords.Recall.EPOCH, epoch);
            b.putInt(at + ConflictRecords.Recall.TIMEOUT, timeout);
            b.putDouble(at + ConflictRecords.Recall.TOWARD_X, towardX);
            b.putDouble(at + ConflictRecords.Recall.TOWARD_Z, towardZ);
        });
    }

    /** DEMAND_SETTLED with an outcome. */
    public static void settled(long faction, long agent, UUID player, int outcome, int signal) {
        script(out -> {
            int at = out.reserve(ConflictRecords.DEMAND_SETTLED, ConflictRecords.Settled.SIZE, outcome);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Settled.FACTION, faction);
            b.putLong(at + ConflictRecords.Settled.AGENT, agent);
            putUuid(b, at + ConflictRecords.Settled.PLAYER_MOST, at + ConflictRecords.Settled.PLAYER_LEAST, player);
            b.putInt(at + ConflictRecords.Settled.SIGNAL, signal);
        });
    }

    /** TERRITORY: a faction's circle minding the signals in {@code mask} (bit n: signal n). */
    public static void territory(long faction, String dimension, int mask, int x, int z, int radius) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(ConflictRecords.TERRITORY, ConflictRecords.Territory.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Territory.FACTION, faction);
            b.putInt(at + ConflictRecords.Territory.DIM, dim);
            b.putInt(at + ConflictRecords.Territory.MASK, mask);
            b.putInt(at + ConflictRecords.Territory.X, x);
            b.putInt(at + ConflictRecords.Territory.Z, z);
            b.putInt(at + ConflictRecords.Territory.RADIUS, radius);
        });
    }

    /** TERRITORY with the removed flag. */
    public static void territoryRemoved(long faction, String dimension) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(ConflictRecords.TERRITORY, ConflictRecords.Territory.SIZE, ConflictRecords.Territory.FLAG_REMOVED);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Territory.FACTION, faction);
            b.putInt(at + ConflictRecords.Territory.DIM, dim);
        });
    }

    /** FACTION_NOTICE to {@code player} (null: the settlers) at step {@code step}. */
    /** FACTION_NOTICE (aux = the rung) with the notice's serial, which NOTICE_SHOWN echoes in its aux (M1). */
    public static void notice(long faction, UUID player, String dimension, int signal, int x, int z, float grievance, int step, int serial) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(ConflictRecords.FACTION_NOTICE, ConflictRecords.Notice.SIZE, step);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.Notice.FACTION, faction);
            putUuid(b, at + ConflictRecords.Notice.PLAYER_MOST, at + ConflictRecords.Notice.PLAYER_LEAST, player);
            b.putInt(at + ConflictRecords.Notice.DIM, dim);
            b.putInt(at + ConflictRecords.Notice.SIGNAL, signal);
            b.putInt(at + ConflictRecords.Notice.X, x);
            b.putInt(at + ConflictRecords.Notice.Z, z);
            b.putFloat(at + ConflictRecords.Notice.GRIEVANCE, grievance);
            b.putInt(at + ConflictRecords.Notice.SERIAL, serial);
        });
    }

    // ---- the camp reconciler (MOWZIE.md §4.9, M5) ------------------------------------------------

    /**
     * One CAMP_RECONCILE line: a slot and generation, its body, its action and flags, and {@code a},
     * {@code b}: a stray's UUID halves, or a party's agent and epoch (SET_OUT, HAND_BACK).
     */
    public record CampLine(int slot, int generation, String body, int action, int flags, long a, long b) {
        public static CampLine of(int slot, int generation, String body, int action, int flags) {
            return new CampLine(slot, generation, body, action, flags, 0, 0);
        }

        /** A line naming a body by UUID (an adoption, a removal of a stray). */
        public static CampLine of(int slot, int generation, String body, int action, int flags, UUID u) {
            return new CampLine(slot, generation, body, action, flags, u.getMostSignificantBits(), u.getLeastSignificantBits());
        }

        /** A line for a party's member (SET_OUT, HAND_BACK). */
        public static CampLine party(int slot, int generation, String body, int action, int flags, long agent, int epoch) {
            return new CampLine(slot, generation, body, action, flags, agent, epoch);
        }
    }

    /** CAMP_RECONCILE for a camp at an epoch, with an op id. */
    public static void reconcile(long faction, long epoch, long op, List<CampLine> lines) {
        script(out -> {
            int[] bodies = new int[lines.size()];
            for (int i = 0; i < lines.size(); i++)
                bodies[i] = out.dict(Obs.DICT_BODY, lines.get(i).body());
            int n = lines.size();
            int at = out.reserve(ConflictRecords.CAMP_RECONCILE, ConflictRecords.CampReconcile.size(n), n);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + ConflictRecords.CampReconcile.FACTION, faction);
            b.putLong(at + ConflictRecords.CampReconcile.EPOCH, epoch);
            b.putLong(at + ConflictRecords.CampReconcile.OP_ID, op);
            for (int i = 0; i < n; i++) {
                CampLine l = lines.get(i);
                int o = at + ConflictRecords.CampReconcile.LINES + i * ConflictRecords.CampReconcile.LINE;
                b.putInt(o + ConflictRecords.CampReconcile.L_SLOT, l.slot());
                b.putInt(o + ConflictRecords.CampReconcile.L_GENERATION, l.generation());
                b.putInt(o + ConflictRecords.CampReconcile.L_BODY, bodies[i]);
                b.put(o + ConflictRecords.CampReconcile.L_ACTION, (byte) l.action());
                b.put(o + ConflictRecords.CampReconcile.L_FLAGS, (byte) l.flags());
                b.putLong(o + ConflictRecords.CampReconcile.L_A, l.a());
                b.putLong(o + ConflictRecords.CampReconcile.L_B, l.b());
            }
        });
    }

    /** A test op id: namespace 0x4D over random bits, apart from the engine's and Java's own. */
    public static long newOp() {
        return (0x4DL << 56) | (ThreadLocalRandom.current().nextLong() & 0x00FF_FFFF_FFFF_FFFFL);
    }

    /** CAMP_CENSUS records of one camp, in the order received. */
    public static List<ScriptedEngine.Received> censuses(long faction) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.CAMP_CENSUS, ConflictRecords.CampCensus.FACTION, faction);
    }

    /**
     * CAMP_BODIES records of one camp, in the order received. A holder's record journaled again is one more:
     * a copy (REASSERT, docs/boundary.md §3.2) whether the engine applied it or dropped it as a record it had
     * ({@code skipped}), listed under the copy's sequence.
     */
    public static List<ScriptedEngine.Received> campBodies(long faction) {
        return withCopies(ConflictRecords.CAMP_BODIES, ConflictRecords.CampBodies.FACTION, faction);
    }

    /**
     * The records of {@code kind} about one camp (the faction at {@code factionAt}), in the order received: those
     * the engine did not skip, and the copies (REASSERT) of such records, applied or dropped, under the copy's sequence.
     */
    private static List<ScriptedEngine.Received> withCopies(int kind, int factionAt, long faction) {
        ScriptedEngine engine = RtsimTests.engine();
        List<ScriptedEngine.Received> out = new ArrayList<>(RtsimTests.recordsWith(engine, kind, factionAt, faction));
        for (ScriptedEngine.Received c : engine.copies(kind))
            if (c.bytes().length >= factionAt + 8 && c.view().getLong(factionAt) == faction)
                out.add(c);
        out.sort(java.util.Comparator.comparingLong(ScriptedEngine.Received::obsSeq));
        return out;
    }

    /** A copy (REASSERT) as the engine received it: the generation it named, and the record it carried. */
    public record Copy(long gen, ScriptedEngine.Received record) {
        /** The engine judged it a record it had, and dropped it. */
        public boolean dropped() {
            return record.skipped();
        }
    }

    /**
     * The copies of records of {@code kind} about one camp (the faction at {@code factionAt}), in the order received,
     * each with the generation the holder named for it. Its record is the copy's ({@link ScriptedEngine.Received#inner}).
     */
    public static List<Copy> copiesOf(int kind, int factionAt, long faction) {
        List<Copy> out = new ArrayList<>();
        for (ScriptedEngine.Received r : RtsimTests.engine().received(Obs.REASSERT)) {
            ScriptedEngine.Received in = r.inner();
            if (in != null && in.kind() == kind && in.bytes().length >= factionAt + 8 && in.view().getLong(factionAt) == faction)
                out.add(new Copy(r.view().getLong(Obs.Reassert.GEN), in));
        }
        out.sort(java.util.Comparator.comparingLong(c -> c.record().obsSeq()));
        return out;
    }

    /** The copies of CAMP_BODIES records of one camp. */
    public static List<Copy> bodiesCopies(long faction) {
        return copiesOf(ConflictRecords.CAMP_BODIES, ConflictRecords.CampBodies.FACTION, faction);
    }

    /** CAMP_BODIES records answering one op. */
    public static List<ScriptedEngine.Received> campBodies(long faction, long op) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : campBodies(faction))
            if (r.view().getLong(ConflictRecords.CampBodies.OP_ID) == op)
                out.add(r);
        return out;
    }

    /** The outcome each line of an op was answered with (by line index; a line answered twice keeps its first). */
    public static Map<Integer, Integer> outcomes(long faction, long op) {
        Map<Integer, Integer> out = new LinkedHashMap<>();
        for (ScriptedEngine.Received r : campBodies(faction, op)) {
            ByteBuffer b = r.view();
            for (int i = 0; i < r.aux(); i++) {
                int o = ConflictRecords.CampBodies.LINES + i * ConflictRecords.CampBodies.LINE;
                out.putIfAbsent(b.getInt(o + ConflictRecords.CampBodies.L_LINE), b.get(o + ConflictRecords.CampBodies.L_OUTCOME) & 0xFF);
            }
        }
        return out;
    }

    public static List<ScriptedEngine.Received> campOuts(long faction) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.CAMP_OUT, ConflictRecords.CampOut.FACTION, faction);
    }

    /**
     * CAMP_DEATH records of one camp, in the order received. As {@link #campBodies}, a copy of one (a marker that
     * carried it loaded again) is one more, applied or dropped.
     */
    public static List<ScriptedEngine.Received> campDeaths(long faction) {
        return withCopies(ConflictRecords.CAMP_DEATH, ConflictRecords.CampDeath.FACTION, faction);
    }

    /** CHIEF_STATE records of one faction (the faction at offset 8), in the order received (MZB-273). */
    public static List<ScriptedEngine.Received> chiefStates(long faction) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : RtsimTests.engine().received(ConflictRecords.CHIEF_STATE))
            if (!r.skipped() && r.view().getLong(ConflictRecords.ChiefState.FACTION) == faction)
                out.add(r);
        return out;
    }

    /** A census's bodies: body id, flags, slot, generation and UUID. */
    public record CensusBody(int body, int flags, int slot, int generation, UUID uuid) {}

    public static List<CensusBody> censusBodies(ScriptedEngine.Received r) {
        List<CensusBody> out = new ArrayList<>();
        ByteBuffer b = r.view();
        for (int i = 0; i < r.aux(); i++) {
            int o = ConflictRecords.CampCensus.BODIES + i * ConflictRecords.CampCensus.BODY;
            out.add(new CensusBody(b.getInt(o + ConflictRecords.CampCensus.B_BODY), b.getInt(o + ConflictRecords.CampCensus.B_FLAGS),
                    b.getInt(o + ConflictRecords.CampCensus.B_SLOT), b.getInt(o + ConflictRecords.CampCensus.B_GENERATION),
                    new UUID(b.getLong(o + ConflictRecords.CampCensus.B_UUID_MOST), b.getLong(o + ConflictRecords.CampCensus.B_UUID_LEAST))));
        }
        return out;
    }

    // ---- reading what the engine received -------------------------------------------------------

    /** Journaled conflict records about one agent (the agent at offset 24), not skipped. */
    public static List<ScriptedEngine.Received> about(int kind, long agent) {
        return RtsimTests.recordsFor(RtsimTests.engine(), kind, agent);
    }

    /** TRIBUTE_PAID records to one envoy party (the agent at offset 32). */
    public static List<ScriptedEngine.Received> tributes(long agent) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.TRIBUTE_PAID, ConflictRecords.TributePaid.AGENT, agent);
    }

    /** FACTION_FOUND records of one faction key. */
    public static List<ScriptedEngine.Received> factionsFound(long faction) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.FACTION_FOUND, ConflictRecords.FactionFound.FACTION, faction);
    }

    /** RAIL_PASS records of one faction (the faction at offset 24), in the order received (M7, MZB-325). */
    public static List<ScriptedEngine.Received> railPasses(long faction) {
        return RtsimTests.recordsWith(RtsimTests.engine(), ConflictRecords.RAIL_PASS, ConflictRecords.RailPass.FACTION, faction);
    }

    /** A RAIL_PASS decoded: train, owner (null: none), dimension id, the point, and entry and exit in game time. */
    public record RailPassRecord(long train, UUID owner, int dimension, double x, double y, double z, long entry, long exit) {}

    public static RailPassRecord railPass(ScriptedEngine.Received r) {
        ByteBuffer b = r.view();
        return new RailPassRecord(b.getLong(ConflictRecords.RailPass.TRAIN), uuidAt(r, ConflictRecords.RailPass.OWNER_MOST),
                b.getInt(ConflictRecords.RailPass.DIM), b.getDouble(ConflictRecords.RailPass.X), b.getDouble(ConflictRecords.RailPass.Y),
                b.getDouble(ConflictRecords.RailPass.Z), b.getLong(ConflictRecords.RailPass.ENTRY), b.getLong(ConflictRecords.RailPass.EXIT));
    }

    /** TERRITORY_SIGNAL records of one kind by one player. */
    public static List<ScriptedEngine.Received> signals(int signal, UUID player) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : RtsimTests.engine().received(ConflictRecords.TERRITORY_SIGNAL))
            if (!r.skipped() && r.aux() == signal && player.equals(uuidAt(r, ConflictRecords.TerritorySignal.PLAYER_MOST)))
                out.add(r);
        return out;
    }

    /** BAND_STATE records of one agent (the agent at offset 8). */
    public static List<ScriptedEngine.Received> states(long agent) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : RtsimTests.engine().received(ConflictRecords.BAND_STATE))
            if (r.view().getLong(ConflictRecords.BandState.AGENT) == agent)
                out.add(r);
        return out;
    }

    /** ENVOY_ANSWER records of one agent with one answer. */
    public static int answers(long agent, int answer) {
        int n = 0;
        for (ScriptedEngine.Received r : about(ConflictRecords.ENVOY_ANSWER, agent))
            if (r.aux() == answer)
                n++;
        return n;
    }

    /** A UUID at (most, most + 8); null for (0, 0). */
    public static UUID uuidAt(ScriptedEngine.Received r, int most) {
        long a = r.view().getLong(most), b = r.view().getLong(most + 8);
        return a == 0 && b == 0 ? null : new UUID(a, b);
    }

    /** A BAND_DEMATERIALIZED's survivors by body name (Java's dictionary). */
    public static Map<String, Integer> survivors(ScriptedEngine.Received r) {
        Map<String, Integer> out = new LinkedHashMap<>();
        ByteBuffer b = r.view();
        int n = b.getInt(ConflictRecords.Dematerialized.COUNT);
        for (int i = 0; i < n; i++) {
            int o = ConflictRecords.Dematerialized.SURVIVORS + i * ConflictRecords.Dematerialized.ENTRY;
            int body = b.getInt(o + ConflictRecords.Dematerialized.E_BODY);
            out.merge(bodyName(body), b.getInt(o + ConflictRecords.Dematerialized.E_COUNT), Integer::sum);
        }
        return out;
    }

    /** The name of a body id of Java's dictionary (one of this kit's). */
    public static String bodyName(int id) {
        for (String n : List.of(WARRIOR, ARCHER, EMISSARY))
            if (RtsimTests.session().dictId(Obs.DICT_BODY, n) == id)
                return n;
        return "body " + id;
    }

    public static int bodyId(String name) {
        return RtsimTests.session().dictId(Obs.DICT_BODY, name);
    }

    /**
     * A PLAYER record for a test's fake player, which the session does not observe (it is in the level's
     * list only): where it stands, as the session reports a real player every 20 ticks.
     */
    public static void reportPlayer(ServerPlayer p) {
        var s = RtsimTests.session();
        int dim = s.dimId(p.level().dimension()); // before direct(): an id is defined before its first use
        int at = s.boundary().direct(Obs.PLAYER, Obs.Player.SIZE, dim, s.gameTime());
        if (at < 0)
            return;
        ByteBuffer b = s.boundary().openBuffer();
        b.putLong(at + Obs.Player.UUID_MOST, p.getUUID().getMostSignificantBits());
        b.putLong(at + Obs.Player.UUID_LEAST, p.getUUID().getLeastSignificantBits());
        b.putDouble(at + Obs.Player.X, p.getX());
        b.putDouble(at + Obs.Player.Y, p.getY());
        b.putDouble(at + Obs.Player.Z, p.getZ());
    }

    // ---- far places -----------------------------------------------------------------------------

    /**
     * A chunk-aligned block far from every earlier run's (the GameTest world is reused, B37), in an area
     * no other test uses: x below -3,000,000, z above 3,000,000.
     */
    public static int[] farBase() {
        int x = FAR_X - ThreadLocalRandom.current().nextInt(0, FAR_SLOTS) * FAR_STEP;
        int z = FAR_Z + ThreadLocalRandom.current().nextInt(0, FAR_SLOTS) * FAR_STEP;
        return new int[]{x, z};
    }

    private static final int FAR_X = -3_000_000, FAR_Z = 3_000_000, FAR_SLOTS = 4_000, FAR_STEP = 1_024;

    /**
     * Releases every chunk forced in the far-base area, with a slot's margin each side: forced chunks
     * persist in the reused world, so a run that failed before its own release leaves them ticking for
     * every later run. Call at the start of a test that forces chunks there (batches run one at a time).
     * The count released.
     */
    public static int releaseFarChunks(ServerLevel level) {
        int x0 = (FAR_X - FAR_SLOTS * FAR_STEP) >> 4, x1 = (FAR_X + FAR_STEP) >> 4;
        int z0 = (FAR_Z - FAR_STEP) >> 4, z1 = (FAR_Z + FAR_SLOTS * FAR_STEP + FAR_STEP) >> 4;
        int n = 0;
        for (long c : level.getForcedChunks().toLongArray()) {
            int cx = ChunkPos.getX(c), cz = ChunkPos.getZ(c);
            if (cx >= x0 && cx <= x1 && cz >= z0 && cz <= z1 && level.setChunkForced(cx, cz, false))
                n++;
        }
        return n;
    }

    /** Runs {@code r} when the test ends, passed or failed (a failed test skips its sequence's last steps). */
    public static void onEnd(GameTestHelper helper, Runnable r) {
        helper.testInfo.addListener(new GameTestListener() {
            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                r.run();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                r.run();
            }

            @Override
            public void testAddedForRerun(GameTestInfo old, GameTestInfo next, GameTestRunner runner) {}
        });
    }

    /** Forces or releases a chunk; true when the call changed it. */
    public static boolean force(ServerLevel level, int cx, int cz, boolean on) {
        return level.setChunkForced(cx, cz, on);
    }

    public static String chunk(long c) {
        return "(" + ChunkPos.getX(c) + ", " + ChunkPos.getZ(c) + ")";
    }
}
