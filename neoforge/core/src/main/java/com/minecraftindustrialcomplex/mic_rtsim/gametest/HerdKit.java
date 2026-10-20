package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.herd.HerdBridge;
import com.minecraftindustrialcomplex.mic_rtsim.herd.HerdRecords;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.entity.Entity;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ThreadLocalRandom;
import java.util.function.Consumer;

/**
 * Helpers for the herd GameTests ({@link HerdGameTests}). A test scripts herd commands as the engine's
 * herds would send them (docs/boundary.md §8.4): the stub writes them with its own dictionary, the native
 * engine takes them as HERD_COMMAND orders with Java's dictionary ids and writes them as its herds' own
 * (the herds know nothing of a scripted agent and ignore what Java answers about it).
 */
public final class HerdKit {
    private HerdKit() {}

    public static final String SHEEP = "minecraft:sheep";
    public static final String COW = "minecraft:cow";
    public static final String PIG = "minecraft:pig";
    public static final String HORSE = "minecraft:horse";
    public static final String GOAT = "minecraft:goat";

    /** Test agents: namespace 0x48 over random 48-bit handles with bit 47 set, apart from the engine's own rows. */
    public static long newAgent() {
        return (HerdRecords.AGENT_NAMESPACE << 56) | (1L << 47) | (ThreadLocalRandom.current().nextLong() & 0x7FFF_FFFF_FFFFL);
    }

    public static HerdBridge bridge() {
        return HerdBridge.INSTANCE;
    }

    /** One member of a scripted HERD_MATERIALIZE; (x, z) absolute. Cohort 0 young, 1 adult, 2 old. */
    public record Member(long member, long seed, int variant, int vanillaAge, int cohort, boolean sheared, int x, int z) {
        public static Member adult(long member, long seed, int variant, int x, int z) {
            return new Member(member, seed, variant, 0, 1, false, x, z);
        }
    }

    // ---- scripting the engine side --------------------------------------------------------------

    /** Writes one herd command record: {@code out.dict} gives a dictionary id, {@code reserve} the record. */
    private static void script(Consumer<StubEngine.CommandOut> command) {
        ScriptedEngine e = RtsimTests.engine();
        if (e instanceof StubEngine stub) {
            stub.script(command);
            return;
        }
        if (!(e instanceof NativeEngine n))
            throw new GameTestAssertException("no scripted herd commands on " + e.describe());
        RecordOut out = new RecordOut();
        command.accept(out);
        if (out.b == null)
            throw new GameTestAssertException("the script wrote no record");
        n.herdCommand(out.names, out.b.array());
    }

    /** One record with Java's dictionary ids, for the native engine's HERD_COMMAND order. */
    private static final class RecordOut implements StubEngine.CommandOut {
        private final List<NativeEngine.DictEntry> names = new ArrayList<>();
        private ByteBuffer b;

        @Override
        public int dict(int dictKind, String name) {
            int id = RtsimTests.session().dictId(dictKind, name);
            if (dictKind != Obs.DICT_DIMENSION)
                names.add(new NativeEngine.DictEntry(dictKind, id, name));
            return id;
        }

        @Override
        public int reserve(int kind, int size, int aux) {
            if (b != null)
                throw new IllegalStateException("one record per HERD_COMMAND order");
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

    /** Orders a HERD_MATERIALIZE of one species (flag SURFACE), heading to (tx, tz) at {@code speed} blocks a tick. */
    public static void materialize(long agent, int epoch, String species, String dimension, int cx, int cz, int tx, int tz,
                                   float speed, List<Member> members) {
        script(out -> {
            int sp = out.dict(Obs.DICT_BODY, species);
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int at = out.reserve(HerdRecords.HERD_MATERIALIZE, HerdRecords.Materialize.size(members.size()), members.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + HerdRecords.Materialize.AGENT, agent);
            b.putInt(at + HerdRecords.Materialize.EPOCH, epoch);
            b.putInt(at + HerdRecords.Materialize.SPECIES, sp);
            b.putInt(at + HerdRecords.Materialize.DIM, dim);
            b.putInt(at + HerdRecords.Materialize.FLAGS, HerdRecords.Materialize.FLAG_SURFACE);
            b.putInt(at + HerdRecords.Materialize.CENTRE_X, cx);
            b.putInt(at + HerdRecords.Materialize.CENTRE_Z, cz);
            b.putInt(at + HerdRecords.Materialize.TARGET_X, tx);
            b.putInt(at + HerdRecords.Materialize.TARGET_Z, tz);
            b.putFloat(at + HerdRecords.Materialize.SPEED, speed);
            b.putInt(at + HerdRecords.Materialize.RADIUS, 16);
            for (int i = 0; i < members.size(); i++) {
                Member m = members.get(i);
                int o = at + HerdRecords.Materialize.MEMBERS + i * HerdRecords.Materialize.MEMBER;
                b.putLong(o + HerdRecords.Materialize.M_MEMBER, m.member());
                b.putLong(o + HerdRecords.Materialize.M_SEED, m.seed());
                b.putInt(o + HerdRecords.Materialize.M_VARIANT, m.variant());
                b.putInt(o + HerdRecords.Materialize.M_VANILLA_AGE, m.vanillaAge());
                b.putInt(o + HerdRecords.Materialize.M_AGE_DAYS, m.vanillaAge() < 0 ? 10 : 400);
                b.put(o + HerdRecords.Materialize.M_COHORT, (byte) m.cohort());
                b.put(o + HerdRecords.Materialize.M_FLAGS, (byte) (m.sheared() ? HerdRecords.MEMBER_SHEARED : 0));
                b.putInt(o + HerdRecords.Materialize.M_X, m.x());
                b.putInt(o + HerdRecords.Materialize.M_Z, m.z());
            }
        });
    }

    public static void steer(long agent, int epoch, String species, float speed, int tx, int tz, long arrive) {
        script(out -> {
            out.dict(Obs.DICT_BODY, species); // the native order needs one species name; the stub sends its DICT once
            int at = out.reserve(HerdRecords.HERD_STEER, HerdRecords.Steer.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + HerdRecords.Steer.AGENT, agent);
            b.putInt(at + HerdRecords.Steer.EPOCH, epoch);
            b.putFloat(at + HerdRecords.Steer.SPEED, speed);
            b.putInt(at + HerdRecords.Steer.TARGET_X, tx);
            b.putInt(at + HerdRecords.Steer.TARGET_Z, tz);
            b.putLong(at + HerdRecords.Steer.ARRIVE, arrive);
        });
    }

    public static void despawn(long agent, int epoch, String species) {
        script(out -> {
            out.dict(Obs.DICT_BODY, species);
            int at = out.reserve(HerdRecords.HERD_DESPAWN, HerdRecords.Despawn.SIZE, 1);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + HerdRecords.Despawn.AGENT, agent);
            b.putInt(at + HerdRecords.Despawn.EPOCH, epoch);
        });
    }

    // ---- reading what the engine received -------------------------------------------------------

    /** Journaled herd records about one agent (the agent at offset 24), not skipped. */
    public static List<ScriptedEngine.Received> about(int kind, long agent) {
        return RtsimTests.recordsFor(RtsimTests.engine(), kind, agent);
    }

    /** HERD_GROUP_STATE records of one agent (the agent at offset 8). */
    public static List<ScriptedEngine.Received> states(long agent) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : RtsimTests.engine().received(HerdRecords.HERD_GROUP_STATE))
            if (r.view().getLong(HerdRecords.GroupState.AGENT) == agent)
                out.add(r);
        return out;
    }

    /** The live bodies of an agent's group, waiting for {@code n} of them (inside thenWaitUntil). */
    public static List<Entity> bodies(long agent, int n) {
        List<Entity> b = bridge().bodies(agent);
        if (b.size() != n)
            throw new GameTestAssertException("herd agent " + Long.toHexString(agent) + " has " + b.size() + " bodies, not " + n);
        return b;
    }

    /** An absolute block x, z in the arena from relative ones. */
    public static int[] abs(GameTestHelper helper, int rx, int rz) {
        var p = helper.absolutePos(new net.minecraft.core.BlockPos(rx, 2, rz));
        return new int[]{p.getX(), p.getZ()};
    }
}
