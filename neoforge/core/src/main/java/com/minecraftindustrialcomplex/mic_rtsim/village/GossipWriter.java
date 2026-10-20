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

    /** Writes a new entry into every loaded resident villager of the village, and keeps it for the rest. */
    public int apply(ServerLevel level, VillageRegistry villages, Village v, long op, UUID player, GossipType type, int amount, long now) {
        int n = 0;
        for (Villager villager : loadedVillagers(level, villages, v))
            if (writeInto(villager, op, player, type, amount))
                n++;
        v.gossip.add(new Village.PendingGossip(op, player, type, amount, now));
        villages.setDirty();
        return n;
    }

    /** At a census: writes the kept entries, aged by vanilla's decay, into the loaded residents. */
    public void catchUp(ServerLevel level, VillageRegistry villages, Village v, List<Villager> residents, long now) {
        boolean changed = false;
        for (Iterator<Village.PendingGossip> it = v.gossip.iterator(); it.hasNext(); ) {
            Village.PendingGossip g = it.next();
            int aged = aged(g, now);
            if (aged <= 0) {
                it.remove();
                changed = true;
                continue;
            }
            for (Villager villager : residents)
                writeInto(villager, g.op(), g.player(), g.type(), aged);
        }
        if (changed)
            villages.setDirty();
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

    /** A villager's applied gossip op ids: bounded, oldest dropped first (PLACEHOLDER 64). */
    public record GossipOps(long[] ops) {
        public static final int CAP = 64;
        public static final GossipOps EMPTY = new GossipOps(new long[0]);
        public static final com.mojang.serialization.Codec<GossipOps> CODEC =
                com.mojang.serialization.Codec.LONG_STREAM.xmap(s -> new GossipOps(s.toArray()), g -> java.util.Arrays.stream(g.ops()));

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
            return new GossipOps(out);
        }

        public boolean isEmpty() {
            return ops.length == 0;
        }
    }

    static boolean isVillager(Entity e) {
        return e instanceof Villager;
    }
}
