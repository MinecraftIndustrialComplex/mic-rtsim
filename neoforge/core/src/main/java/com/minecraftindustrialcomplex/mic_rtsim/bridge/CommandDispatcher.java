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
            case Cmd.MARKET -> Cmd.Market.GOODS;
            case Cmd.MERCHANT_OFFERS -> Cmd.MerchantOffers.OFFERS;
            case Cmd.WRITE_GOSSIP -> Cmd.WriteGossip.SIZE;
            case Cmd.SENTIMENT -> Cmd.Sentiment.ENTRIES;
            case Cmd.PLACE_HOUSE -> Cmd.PlaceHouse.SIZE;
            case Cmd.SPAWN_RESIDENT -> Cmd.SpawnResident.SIZE;
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
                    g.steerTarget = new Vec3(b.getDouble(at + Cmd.SteerGroup.X), b.getDouble(at + Cmd.SteerGroup.Y), b.getDouble(at + Cmd.SteerGroup.Z));
                    g.steerSpeed = b.getFloat(at + Cmd.SteerGroup.SPEED);
                    if (leader != null) {
                        // The target's image nearest the leader, never through an edge_stop seam (TP11, TP12).
                        g.steerImage = session.steerImage(leader, g.steerTarget);
                        RtsimSession.adapter(g, g.members[0]).steer(leader, g.steerImage, g.steerSpeed);
                    }
                }
                applied++;
                return true;
            }
            case Cmd.TRANSFER_TO_CONTAINER -> {
                transferToContainer(b, at, now);
                applied++;
                return true;
            }
            case Cmd.MARKET -> {
                market(b, at, size, aux);
                applied++;
                return true;
            }
            case Cmd.MERCHANT_OFFERS -> {
                merchantOffers(b, at, size, aux);
                applied++;
                return true;
            }
            case Cmd.WRITE_GOSSIP -> {
                session.phase1.writeGossip(b.getLong(at + Cmd.CMD_SEQ), b.getLong(at + Cmd.WriteGossip.OP_ID),
                        b.getLong(at + Cmd.WriteGossip.VILLAGE),
                        new java.util.UUID(b.getLong(at + Cmd.WriteGossip.PLAYER_MOST), b.getLong(at + Cmd.WriteGossip.PLAYER_LEAST)),
                        b.getInt(at + Cmd.WriteGossip.GOSSIP), b.getInt(at + Cmd.WriteGossip.AMOUNT),
                        b.getInt(at + Cmd.WriteGossip.REPORT), b.getInt(at + Cmd.WriteGossip.FLAGS), now);
                applied++;
                return true;
            }
            case Cmd.SENTIMENT -> {
                sentiment(b, at, size, aux);
                applied++;
                return true;
            }
            case Cmd.PLACE_HOUSE -> {
                placeHouse(b, at, now);
                applied++;
                return true;
            }
            case Cmd.SPAWN_RESIDENT -> {
                spawnResident(b, at, now); // queued; the spawn itself counts against the quota when it happens
                applied++;
                return true;
            }
            default -> {
                // Industry commands (0x8400 to 0x84FF, docs/industry-boundary.md §4) go to the module.
                if (com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords.isIndustryCommand(kind) && session.industry != null)
                    return session.industry.accept(b, at, kind, size, aux, this::engineName);
                // The far view's range (0x8C00 to 0x8CFF, docs/boundary.md §12): the engine's dragon legs.
                if (com.minecraftindustrialcomplex.mic_rtsim.far.FarLegs.isFarCommand(kind)) {
                    if (session.farLegs.accept(b, at, kind, size, aux, this::engineName))
                        applied++;
                    return true;
                }
                // A plugin's own range (the dragons' 0x8500 to 0x85FF, docs/boundary.md §6.4).
                com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin plugin =
                        com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugins.forCommand(kind);
                if (plugin != null) {
                    try {
                        boolean done = plugin.accept(session, b, at, kind, size, aux, this::engineName);
                        if (done)
                            applied++;
                        return done;
                    } catch (RuntimeException ex) {
                        malformed++;
                        MicRtsim.LOGGER.error("[mic_rtsim] plugin {} failed on command 0x{}; skipped", plugin.name(), Integer.toHexString(kind), ex);
                        return true;
                    }
                }
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
        int flags = b.getInt(at + Cmd.SpawnGroup.FLAGS);
        int result = session.spawnGroup(agent, epoch, kind, level, anchor, spawns, flags, now);
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

    // ---- phase 1 --------------------------------------------------------------------------------

    private void market(ByteBuffer b, int at, int size, int goods) {
        Item currency = item(b.getInt(at + Cmd.Market.CURRENCY));
        int levels = b.get(at + Cmd.Market.LEVELS) & 0xFF, depth = b.get(at + Cmd.Market.DEPTH) & 0xFF;
        if (currency == null || goods < 0 || levels < 1 || levels > Cmd.Market.MAX_LEVELS || depth < 1
                || depth > Cmd.Market.MAX_DEPTH || Cmd.Market.size(goods, levels, depth) > size) {
            malformed++;
            return;
        }
        int[] thresholds = new int[levels];
        for (int k = 0; k < levels; k++)
            thresholds[k] = b.getInt(at + Cmd.Market.THRESHOLDS + 4 * k);
        java.util.Map<Item, com.minecraftindustrialcomplex.mic_rtsim.market.MarketBook.Good> lines = new java.util.HashMap<>();
        for (int i = 0; i < goods; i++) {
            int g = at + Cmd.Market.GOODS + i * Cmd.Market.good(levels, depth);
            Item item = item(b.getInt(g + Cmd.Market.G_ITEM));
            if (item == null) {
                malformed++;
                continue;
            }
            long[][][] ladders = new long[2][levels][];
            for (int side = 0; side < 2; side++) {
                for (int k = 0; k < levels; k++) {
                    long[] steps = new long[2 * depth];
                    int n = 0;
                    for (int j = 0; j < depth; j++) {
                        int o = g + Cmd.Market.step(levels, depth, side, k, j);
                        int items = b.getInt(o), coins = b.getInt(o + 4);
                        if (items <= 0 || coins <= 0)
                            break; // (0, 0) ends the ladder
                        steps[2 * n] = items;
                        steps[2 * n + 1] = coins;
                        n++;
                    }
                    ladders[side][k] = java.util.Arrays.copyOf(steps, 2 * n);
                }
            }
            lines.put(item, new com.minecraftindustrialcomplex.mic_rtsim.market.MarketBook.Good(b.getLong(g + Cmd.Market.G_STOCK), ladders));
        }
        session.phase1.market(b.getLong(at + Cmd.Market.VILLAGE), b.getLong(at + Cmd.Market.AS_OF_OBS_SEQ), currency,
                b.getLong(at + Cmd.Market.TREASURY), b.getLong(at + Cmd.Market.TREASURY_TARGET), thresholds, lines);
    }

    private void merchantOffers(ByteBuffer b, int at, int size, int count) {
        if (count < 0 || Cmd.MerchantOffers.size(count) > size) {
            malformed++;
            return;
        }
        java.util.List<com.minecraftindustrialcomplex.mic_rtsim.market.PuppetOffers.Spec> specs = new ArrayList<>(count);
        for (int i = 0; i < count; i++) {
            int o = at + Cmd.MerchantOffers.OFFERS + i * Cmd.MerchantOffers.OFFER;
            Item a = item(b.getInt(o + Cmd.MerchantOffers.O_COST_A_ITEM));
            int bId = b.getInt(o + Cmd.MerchantOffers.O_COST_B_ITEM);
            Item bi = bId == 0 ? null : item(bId);
            Item r = item(b.getInt(o + Cmd.MerchantOffers.O_RESULT_ITEM));
            int ac = b.getInt(o + Cmd.MerchantOffers.O_COST_A_COUNT), rc = b.getInt(o + Cmd.MerchantOffers.O_RESULT_COUNT);
            if (a == null || r == null || (bId != 0 && bi == null) || ac <= 0 || rc <= 0
                    || ac > a.getDefaultMaxStackSize() || rc > r.getDefaultMaxStackSize()) {
                malformed++;
                continue;
            }
            specs.add(new com.minecraftindustrialcomplex.mic_rtsim.market.PuppetOffers.Spec(a, ac, bi, b.getInt(o + Cmd.MerchantOffers.O_COST_B_COUNT),
                    r, rc, b.getInt(o + Cmd.MerchantOffers.O_MAX_USES), b.getInt(o + Cmd.MerchantOffers.O_XP),
                    b.getFloat(o + Cmd.MerchantOffers.O_PRICE_MULT), b.getInt(o + Cmd.MerchantOffers.O_FLAGS)));
        }
        session.phase1.merchantOffers(b.getLong(at + Cmd.MerchantOffers.AGENT), b.getInt(at + Cmd.MerchantOffers.EPOCH),
                b.getInt(at + Cmd.MerchantOffers.MEMBER), specs);
    }

    private void sentiment(ByteBuffer b, int at, int size, int count) {
        if (count < 0 || Cmd.Sentiment.size(count) > size) {
            malformed++;
            return;
        }
        java.util.List<java.util.UUID> players = new ArrayList<>(count);
        java.util.List<Integer> values = new ArrayList<>(count);
        for (int i = 0; i < count; i++) {
            int e = at + Cmd.Sentiment.ENTRIES + i * Cmd.Sentiment.ENTRY;
            players.add(new java.util.UUID(b.getLong(e), b.getLong(e + 8)));
            values.add(b.getInt(e + 16));
        }
        session.phase1.sentiment(b.getLong(at + Cmd.Sentiment.HOLDER), b.getInt(at + Cmd.Sentiment.DISLIKE),
                b.getInt(at + Cmd.Sentiment.FRIEND), (b.getInt(at + Cmd.Sentiment.FLAGS) & Cmd.Sentiment.FLAG_REPLACE) != 0, players, values);
    }

    private void placeHouse(ByteBuffer b, int at, long now) {
        com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry.Build h = new com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry.Build();
        h.op = b.getLong(at + Cmd.PlaceHouse.OP_ID);
        h.cmdSeq = b.getLong(at + Cmd.CMD_SEQ);
        h.village = b.getLong(at + Cmd.PlaceHouse.VILLAGE);
        ServerLevel level = level(b.getInt(at + Cmd.PlaceHouse.DIM));
        h.dimension = level == null ? null : level.dimension();
        h.template = location(name(Obs.DICT_TEMPLATE, b.getInt(at + Cmd.PlaceHouse.TEMPLATE)));
        int proc = b.getInt(at + Cmd.PlaceHouse.PROCESSORS);
        h.processors = proc == 0 ? null : location(name(Obs.DICT_PROCESSORS, proc));
        h.x = b.getInt(at + Cmd.PlaceHouse.X);
        h.y = b.getInt(at + Cmd.PlaceHouse.Y);
        h.z = b.getInt(at + Cmd.PlaceHouse.Z);
        h.rotation = b.get(at + Cmd.PlaceHouse.ROTATION) & 0xFF;
        h.mirror = b.get(at + Cmd.PlaceHouse.MIRROR) & 0xFF;
        h.flags = b.getShort(at + Cmd.PlaceHouse.FLAGS) & 0xFFFF;
        h.radius = b.getInt(at + Cmd.PlaceHouse.RADIUS);
        h.seed = b.getLong(at + Cmd.PlaceHouse.SEED);
        h.yOffset = b.getInt(at + Cmd.PlaceHouse.Y_OFFSET);
        if (h.dimension == null || h.template == null) {
            session.journalCommandResult(h.cmdSeq, h.op, Obs.CommandResult.FAILED, now);
            return;
        }
        session.phase1.placeHouse(h, now);
    }

    private void spawnResident(ByteBuffer b, int at, long now) {
        com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry.ResidentOrder o = new com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry.ResidentOrder();
        o.op = b.getLong(at + Cmd.SpawnResident.OP_ID);
        o.cmdSeq = b.getLong(at + Cmd.CMD_SEQ);
        o.village = b.getLong(at + Cmd.SpawnResident.VILLAGE);
        ServerLevel level = level(b.getInt(at + Cmd.SpawnResident.DIM));
        o.dimension = level == null ? null : level.dimension();
        o.body = name(Obs.DICT_BODY, b.getInt(at + Cmd.SpawnResident.BODY));
        int prof = b.getInt(at + Cmd.SpawnResident.PROFESSION);
        o.profession = prof == 0 ? null : name(Obs.DICT_PROFESSION, prof);
        o.flags = b.getInt(at + Cmd.SpawnResident.FLAGS);
        o.x = b.getDouble(at + Cmd.SpawnResident.X);
        o.y = b.getDouble(at + Cmd.SpawnResident.Y);
        o.z = b.getDouble(at + Cmd.SpawnResident.Z);
        o.house = b.getLong(at + Cmd.SpawnResident.HOUSE);
        if (o.dimension == null || o.body == null) {
            session.journalCommandResult(o.cmdSeq, o.op, Obs.CommandResult.FAILED, now);
            return;
        }
        session.phase1.spawnResident(o, now);
    }

    private String name(int dictKind, int id) {
        return dict.get(((long) dictKind << 32) | (id & 0xFFFF_FFFFL));
    }

    private static ResourceLocation location(String name) {
        return name == null ? null : ResourceLocation.tryParse(name);
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
