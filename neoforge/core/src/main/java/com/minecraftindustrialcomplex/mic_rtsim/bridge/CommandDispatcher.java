package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.body.GroupKind;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberSpawn;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Handover;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Applies engine commands on the server thread (PLAN.md §6.4). Spawns and despawns count against the
 * per-tick quota a whole group at a time (C25, PLAN.md §4.1): a group that does not fit in what is
 * left of this tick's quota waits, with every command after it, for the next tick. A group bigger than
 * the whole quota goes alone at the start of a tick, so it is never starved.
 */
final class CommandDispatcher implements Handover.CommandSink {
    private final RtsimSession session;
    /** The engine's dictionary for this session: (kind << 32 | id) to name. */
    private final Map<Long, String> dict = new HashMap<>();

    long applied;
    long unknown;
    long malformed;

    /** The smallest valid size of each command kind; a shorter record is skipped as malformed. */
    private static int minSize(int kind) {
        return switch (kind) {
            case Cmd.DICT -> Cmd.Dict.NAME;
            case Cmd.SPAWN_GROUP -> Cmd.SpawnGroup.MEMBERS;
            case Cmd.DESPAWN_GROUP -> Cmd.DespawnGroup.SIZE;
            case Cmd.STEER_GROUP -> Cmd.SteerGroup.SIZE;
            case Cmd.TRANSFER_TO_CONTAINER -> Cmd.TransferToContainer.SIZE;
            default -> Cmd.BODY;
        };
    }

    CommandDispatcher(RtsimSession session) {
        this.session = session;
    }

    @Override
    public void onHeader(long handover, long ackedObsSeq, long gameTime, int flags) {
    }

    @Override
    public void onMalformed(long handover, String error) {
        MicRtsim.LOGGER.warn("[mic_rtsim] malformed command buffer at hand-over {}: {}", handover, error);
    }

    @Override
    public boolean accept(ByteBuffer b, int at, int kind, int size, int aux) {
        long now = session.gameTime();
        if (size < minSize(kind)) {
            malformed++;
            MicRtsim.LOGGER.warn("[mic_rtsim] command 0x{} of {} bytes is shorter than its layout; skipped", Integer.toHexString(kind), size);
            return true;
        }
        switch (kind) {
            case Cmd.DICT -> {
                int len = b.getInt(at + Cmd.Dict.NAME_LEN);
                if (len < 0 || len > size - Cmd.Dict.NAME) {
                    malformed++;
                    return true;
                }
                byte[] name = new byte[len];
                b.get(at + Cmd.Dict.NAME, name, 0, len);
                dict.put(((long) aux << 32) | (b.getInt(at + Cmd.Dict.ID) & 0xFFFF_FFFFL), new String(name, StandardCharsets.UTF_8));
                return true;
            }
            case Cmd.SPAWN_GROUP -> {
                return spawn(b, at, size, aux, now);
            }
            case Cmd.DESPAWN_GROUP -> {
                long agent = b.getLong(at + Cmd.DespawnGroup.AGENT);
                int epoch = b.getInt(at + Cmd.DespawnGroup.EPOCH);
                PuppetGroup g = session.registry.byAgent(agent);
                if (g == null || g.epoch != epoch) {
                    // Not embodied (already captured, or never: a replay after a crash can leave the engine
                    // thinking it is). Say so, so the engine is never left waiting for a capture.
                    session.journalDematerialized(agent, epoch, Obs.Dematerialized.NOT_EMBODIED, 0, now);
                    return true;
                }
                if (!fitsQuota(g.liveMembers()))
                    return false;
                session.spawnBudget -= g.liveMembers();
                session.captureGroup(g, Obs.Dematerialized.COMMAND, now);
                applied++;
                return true;
            }
            case Cmd.STEER_GROUP -> {
                PuppetGroup g = session.registry.byAgent(b.getLong(at + Cmd.SteerGroup.AGENT));
                if (g != null && g.epoch == b.getInt(at + Cmd.SteerGroup.EPOCH)) {
                    Entity leader = g.leader();
                    if (leader != null)
                        RtsimSession.adapter(g, g.members[0]).steer(leader,
                                new Vec3(b.getDouble(at + Cmd.SteerGroup.X), b.getDouble(at + Cmd.SteerGroup.Y), b.getDouble(at + Cmd.SteerGroup.Z)),
                                b.getFloat(at + Cmd.SteerGroup.SPEED));
                }
                applied++;
                return true;
            }
            case Cmd.TRANSFER_TO_CONTAINER -> {
                transferToContainer(b, at, now);
                applied++;
                return true;
            }
            default -> {
                // Industry commands (0x8400 to 0x84FF, docs/industry-boundary.md §4) go to the module.
                if (com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords.isIndustryCommand(kind) && session.industry != null)
                    return session.industry.accept(b, at, kind, size, aux, this::engineName);
                unknown++;
                return true;
            }
        }
    }

    /** A name in the engine's dictionary for this session, or null. */
    String engineName(int dictKind, int id) {
        return dict.get(((long) dictKind << 32) | (id & 0xFFFF_FFFFL));
    }

    private boolean fitsQuota(int members) {
        return members <= session.spawnBudget || session.spawnBudget == RtsimConfig.spawnQuota();
    }

    private boolean spawn(ByteBuffer b, int at, int size, int memberCount, long now) {
        long cmdSeq = b.getLong(at + Cmd.CMD_SEQ);
        long agent = b.getLong(at + Cmd.SpawnGroup.AGENT);
        int epoch = b.getInt(at + Cmd.SpawnGroup.EPOCH);
        String kindName = dict.get(((long) Obs.DICT_BODY << 32) | (b.getInt(at + Cmd.SpawnGroup.GROUP_KIND) & 0xFFFF_FFFFL));
        GroupKind kind = kindName == null ? null : BodyKinds.group(kindName);
        if (kind == null) {
            session.journalRejected(agent, epoch, cmdSeq, Obs.SpawnRejected.UNKNOWN_KIND, now);
            return true;
        }
        if (!fitsQuota(kind.size()))
            return false; // the whole group waits for the next tick, with every command after it
        ServerLevel level = level(b.getInt(at + Cmd.SpawnGroup.DIM));
        if (level == null) {
            session.journalRejected(agent, epoch, cmdSeq, Obs.SpawnRejected.UNKNOWN_DIMENSION, now);
            return true;
        }
        // Every member of the kind exactly once, each block inside the record.
        if (memberCount != kind.size()) {
            session.journalRejected(agent, epoch, cmdSeq, Obs.SpawnRejected.BAD_RECORD, now);
            return true;
        }
        MemberSpawn[] spawns = new MemberSpawn[kind.size()];
        int m0 = at + Cmd.SpawnGroup.MEMBERS;
        int end = at + size;
        for (int i = 0; i < memberCount; i++) {
            if (end - m0 < Cmd.SpawnGroup.Member.ITEMS) {
                session.journalRejected(agent, epoch, cmdSeq, Obs.SpawnRejected.BAD_RECORD, now);
                return true;
            }
            int index = b.getInt(m0 + Cmd.SpawnGroup.Member.INDEX);
            int items = b.getInt(m0 + Cmd.SpawnGroup.Member.ITEM_COUNT);
            if (index < 0 || index >= spawns.length || spawns[index] != null
                    || items < 0 || items > (end - m0 - Cmd.SpawnGroup.Member.ITEMS) / Obs.Item.SIZE) {
                session.journalRejected(agent, epoch, cmdSeq, Obs.SpawnRejected.BAD_RECORD, now);
                return true;
            }
            List<MemberCapture.Stack> stacks = new ArrayList<>(items);
            for (int k = 0; k < items; k++) {
                int it = m0 + Cmd.SpawnGroup.Member.ITEMS + k * Obs.Item.SIZE;
                Item item = item(b.getInt(it + Obs.Item.ITEM));
                int count = b.getInt(it + Obs.Item.COUNT);
                if (item == null || item == Items.AIR || count <= 0) {
                    // Cargo that names no item would vanish; refuse the spawn instead.
                    session.journalRejected(agent, epoch, cmdSeq, Obs.SpawnRejected.BAD_CARGO, now);
                    return true;
                }
                stacks.add(new MemberCapture.Stack(b.getInt(it + Obs.Item.SLOT), item, count));
            }
            spawns[index] = new MemberSpawn(index, b.getFloat(m0 + Cmd.SpawnGroup.Member.HEALTH),
                    b.getInt(m0 + Cmd.SpawnGroup.Member.BODY_A), b.getInt(m0 + Cmd.SpawnGroup.Member.BODY_B), stacks);
            m0 += Cmd.SpawnGroup.Member.size(items);
        }
        Vec3 anchor = new Vec3(b.getDouble(at + Cmd.SpawnGroup.X), b.getDouble(at + Cmd.SpawnGroup.Y), b.getDouble(at + Cmd.SpawnGroup.Z));
        int result = session.spawnGroup(agent, epoch, kind, level, anchor, spawns, now);
        if (result != 0) {
            session.journalRejected(agent, epoch, cmdSeq, result, now);
            return true;
        }
        session.spawnBudget -= kind.size();
        applied++;
        return true;
    }

    /**
     * An engine-to-Minecraft cross-store transfer. Applied at most once per op id: an op id whose
     * COMMAND_RESULT(APPLIED) is still in the journal is skipped as a duplicate (an engine that a crash
     * rolled back re-derives the same op under a new command sequence). All or nothing: the items go in
     * only when the whole stack fits.
     */
    private void transferToContainer(ByteBuffer b, int at, long now) {
        long cmdSeq = b.getLong(at + Cmd.CMD_SEQ);
        long opId = b.getLong(at + Cmd.TransferToContainer.OP_ID);
        if (session.boundary.journal().commandOpApplied(opId)) {
            session.journalCommandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP, now);
            return;
        }
        ServerLevel level = level(b.getInt(at + Cmd.TransferToContainer.DIM));
        Item item = item(b.getInt(at + Cmd.TransferToContainer.ITEM));
        int count = b.getInt(at + Cmd.TransferToContainer.COUNT);
        BlockPos pos = new BlockPos(b.getInt(at + Cmd.TransferToContainer.X), b.getInt(at + Cmd.TransferToContainer.Y), b.getInt(at + Cmd.TransferToContainer.Z));
        // getChunkNow, not isLoaded: isLoaded reads the ticket level only, and the capability lookup
        // below would then wait on the server thread for the chunk to finish loading.
        net.minecraft.world.level.chunk.LevelChunk chunk = level == null ? null : level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (level == null || item == null || count <= 0 || chunk == null) {
            session.journalCommandResult(cmdSeq, opId, Obs.CommandResult.FAILED, now);
            return;
        }
        // The chunk itself remembers what was applied into it (PendingRecords): it may have been saved
        // on unload after the transfer and before the journal, so after a crash only it knows.
        if (RtsimSession.chunkCarriesOp(chunk, opId)) {
            ByteBuffer dup = session.journalCommandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP, now);
            session.dropFromChunkWhenAcked(chunk, opId, dup.getLong(Obs.OBS_SEQ));
            return;
        }
        IItemHandler handler = level.getCapability(Capabilities.ItemHandler.BLOCK, pos, null);
        if (handler == null) {
            session.journalCommandResult(cmdSeq, opId, Obs.CommandResult.FAILED, now);
            return;
        }
        if (count > item.getDefaultMaxStackSize()) {
            // One stack per command, so the simulated insert below is exact; the engine splits larger ones.
            session.journalCommandResult(cmdSeq, opId, Obs.CommandResult.FAILED, now);
            return;
        }
        ItemStack stack = new ItemStack(item, count);
        if (!ItemHandlerHelper.insertItemStacked(handler, stack.copy(), true).isEmpty()) {
            session.journalCommandResult(cmdSeq, opId, Obs.CommandResult.FAILED, now);
            return;
        }
        ItemStack rest = ItemHandlerHelper.insertItemStacked(handler, stack, false);
        if (!rest.isEmpty()) {
            // Only a handler whose real insert disagrees with its own simulation gets here. Nothing may
            // vanish, so the rest goes on the ground at the container.
            MicRtsim.LOGGER.warn("[mic_rtsim] transfer {} left {} x {} over at {}; dropped there", opId, rest.getCount(), item, pos);
            net.minecraft.world.Containers.dropItemStack(level, pos.getX() + 0.5, pos.getY() + 1, pos.getZ() + 0.5, rest);
        }
        session.carryInChunk(chunk, session.journalCommandResult(cmdSeq, opId, Obs.CommandResult.APPLIED, now));
    }

    private ServerLevel level(int dimId) {
        String name = dict.get(((long) Obs.DICT_DIMENSION << 32) | (dimId & 0xFFFF_FFFFL));
        if (name == null)
            return null;
        ResourceLocation loc = ResourceLocation.tryParse(name);
        return loc == null ? null : session.server.getLevel(ResourceKey.create(Registries.DIMENSION, loc));
    }

    private Item item(int itemId) {
        String name = dict.get(((long) Obs.DICT_ITEM << 32) | (itemId & 0xFFFF_FFFFL));
        if (name == null)
            return null;
        ResourceLocation loc = ResourceLocation.tryParse(name);
        return loc == null ? null : BuiltInRegistries.ITEM.getOptional(loc).orElse(null);
    }
}
