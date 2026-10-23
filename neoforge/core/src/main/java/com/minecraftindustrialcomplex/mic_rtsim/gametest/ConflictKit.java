package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import net.minecraft.gametest.framework.GameTestAssertException;
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

    private static void script(Consumer<StubEngine.CommandOut> command) {
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

    /** One stack of a BAND_MATERIALIZE: a body kind, how many, and whether the war chief walks in it. */
    public record StackSpec(String body, int count, boolean chief) {}

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
                b.putInt(o + ConflictRecords.Materialize.S_FLAGS, stacks.get(i).chief() ? ConflictRecords.Materialize.STACK_CHIEF : 0);
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
    public static void notice(long faction, UUID player, String dimension, int signal, int x, int z, float grievance, int step) {
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
        });
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
        int x = -3_000_000 - ThreadLocalRandom.current().nextInt(0, 4_000) * 1_024;
        int z = 3_000_000 + ThreadLocalRandom.current().nextInt(0, 4_000) * 1_024;
        return new int[]{x, z};
    }

    /** Forces or releases a chunk; true when the call changed it. */
    public static boolean force(ServerLevel level, int cx, int cz, boolean on) {
        return level.setChunkForced(cx, cz, on);
    }

    public static String chunk(long c) {
        return "(" + ChunkPos.getX(c) + ", " + ChunkPos.getZ(c) + ")";
    }
}
