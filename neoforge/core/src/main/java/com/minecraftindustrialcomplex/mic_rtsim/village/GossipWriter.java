package com.minecraftindustrialcomplex.mic_rtsim.village;

import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.phys.AABB;

import java.util.Iterator;
import java.util.List;
import java.util.UUID;

/**
 * Writes the sim's slights and fair trades into vanilla villager gossip (C20, PLAN.md §8.7), so
 * vanilla's own reputation pricing and iron golems carry the consequence.
 *
 * <p><b>Each villager takes an op id at most once.</b> A villager carries the op ids it was written
 * with ({@code mic_rtsim:gossip_ops}, saved with it), so a command repeated after a crash, or resent
 * by an engine that a crash rolled back, adds nothing to a villager that already has it, whatever the
 * journal remembers. The command's own op id is also deduplicated by the journal, as transfers are.
 *
 * <p><b>Residents not loaded</b> get the entry at their village's next census (it is kept in the
 * village, saved with the registry), aged by vanilla's own daily decay for that gossip type, and
 * dropped once it would have decayed away: the entry a villager gets late is the one it would have
 * had if it had been there.
 */
public final class GossipWriter {

    /** rtsim-knowledge's {@code GossipType} numbering (1 to 5) to vanilla's. */
    public static GossipType type(int wire) {
        return switch (wire) {
            case 1 -> GossipType.MAJOR_NEGATIVE;
            case 2 -> GossipType.MINOR_NEGATIVE;
            case 3 -> GossipType.MINOR_POSITIVE;
            case 4 -> GossipType.MAJOR_POSITIVE;
            case 5 -> GossipType.TRADING;
            default -> null;
        };
    }

    /** rtsim-knowledge ReportKind values vanilla already records when its own villagers see them. */
    public static boolean vanillaSeesFirstHand(int reportKind) {
        return reportKind == 5 || reportKind == 7 || reportKind == 8; // trade, resident assault, resident killing
    }

    long written;
    long skippedHasOp;

    public long written() {
        return written;
    }

    public long skippedHasOp() {
        return skippedHasOp;
    }

    /** Writes into one villager unless it already took this op. Returns true when written. */
    public boolean writeInto(Villager villager, long op, UUID player, GossipType type, int amount) {
        if (amount <= 0)
            return false;
        GossipOps ops = villager.getData(RtsimAttachments.GOSSIP_OPS);
        if (ops.has(op)) {
            skippedHasOp++;
            return false;
        }
        villager.getGossips().add(player, type, amount);
        villager.setData(RtsimAttachments.GOSSIP_OPS, ops.plus(op));
        written++;
        return true;
    }

    /**
     * Writes a new entry into every loaded resident villager of the village, and keeps it for the rest.
     * A villager that has had every earlier entry of the village moves its high-water mark on.
     */
    public int apply(ServerLevel level, VillageRegistry villages, Village v, long op, UUID player, GossipType type, int amount, long now) {
        long seq = ++v.gossipSeq;
        int n = 0;
        for (Villager villager : loadedVillagers(level, villages, v)) {
            if (writeInto(villager, op, player, type, amount))
                n++;
            GossipOps ops = villager.getData(RtsimAttachments.GOSSIP_OPS);
            if (ops.mark(v.id) == seq - 1)
                villager.setData(RtsimAttachments.GOSSIP_OPS, ops.marked(v.id, seq));
        }
        v.gossip.add(new Village.PendingGossip(op, player, type, amount, now, seq));
        villages.setDirty();
        return n;
    }

    /**
     * At a census: writes the kept entries, aged by vanilla's decay, into the loaded residents. A
     * villager skips every entry at or below its high-water mark for the village, so however many entries
     * are kept, each villager takes each one once (the op ring alone holds only the last 64).
     */
    public void catchUp(ServerLevel level, VillageRegistry villages, Village v, List<Villager> residents, long now) {
        boolean changed = false;
        for (Iterator<Village.PendingGossip> it = v.gossip.iterator(); it.hasNext(); ) {
            if (aged(it.next(), now) <= 0) {
                it.remove();
                changed = true;
            }
        }
        if (changed)
            villages.setDirty();
        if (v.gossip.isEmpty())
            return;
        long last = v.gossip.get(v.gossip.size() - 1).seq();
        for (Villager villager : residents) {
            long mark = villager.getData(RtsimAttachments.GOSSIP_OPS).mark(v.id);
            if (mark >= last)
                continue;
            for (Village.PendingGossip g : v.gossip)
                if (g.seq() > mark)
                    writeInto(villager, g.op(), g.player(), g.type(), aged(g, now));
            villager.setData(RtsimAttachments.GOSSIP_OPS, villager.getData(RtsimAttachments.GOSSIP_OPS).marked(v.id, last));
        }
    }

    /**
     * The amount an entry written at {@code writtenAt} would have now under vanilla's daily decay
     * ({@code GossipType.decayPerDay}, applied once per in-game day by {@code GossipContainer.decay}).
     */
    public static int aged(Village.PendingGossip g, long now) {
        long days = Math.max(0, now - g.writtenAt()) / 24_000L;
        return (int) Math.max(0, g.amount() - days * g.type().decayPerDay);
    }

    static List<Villager> loadedVillagers(ServerLevel level, VillageRegistry villages, Village v) {
        AABB box = AABB.of(v.box).inflate(VillageRegistry.MARGIN);
        return level.getEntitiesOfClass(Villager.class, box, e -> e.isAlive() && !RtsimAttachments.isTagged(e) && Residents.villageOf(villages, e) == v);
    }

    /**
     * A villager's gossip record: the op ids it was last written with (bounded, oldest dropped first,
     * PLACEHOLDER 64; what stops a resend after a crash), and its high-water mark in its village's
     * sequence of kept entries (what stops a census writing an entry twice).
     */
    public record GossipOps(long[] ops, long village, long mark) {
        public static final int CAP = 64;
        public static final GossipOps EMPTY = new GossipOps(new long[0], 0, 0);
        public static final com.mojang.serialization.Codec<GossipOps> CODEC =
                com.mojang.serialization.codecs.RecordCodecBuilder.create(i -> i.group(
                        com.mojang.serialization.Codec.LONG_STREAM.xmap(java.util.stream.LongStream::toArray, java.util.Arrays::stream)
                                .fieldOf("ops").forGetter(GossipOps::ops),
                        com.mojang.serialization.Codec.LONG.optionalFieldOf("village", 0L).forGetter(GossipOps::village),
                        com.mojang.serialization.Codec.LONG.optionalFieldOf("mark", 0L).forGetter(GossipOps::mark)
                ).apply(i, GossipOps::new));

        public boolean has(long op) {
            for (long o : ops)
                if (o == op)
                    return true;
            return false;
        }

        public GossipOps plus(long op) {
            int keep = Math.min(ops.length, CAP - 1);
            long[] out = new long[keep + 1];
            System.arraycopy(ops, ops.length - keep, out, 0, keep);
            out[keep] = op;
            return new GossipOps(out, village, mark);
        }

        /** The high-water mark for a village: 0 when the record is about another village. */
        public long mark(long villageId) {
            return village == villageId ? mark : 0;
        }

        public GossipOps marked(long villageId, long newMark) {
            return new GossipOps(ops, villageId, newMark);
        }

        public boolean isEmpty() {
            return ops.length == 0 && mark == 0;
        }
    }

    static boolean isVillager(Entity e) {
        return e instanceof Villager;
    }
}
