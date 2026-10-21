package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.TagKey;
import net.minecraft.world.Container;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentLinkedQueue;

/**
 * Ice and Fire's lairs (docs/boundary.md §6.2, §6.3): LAIR_FOUND for every roost and cave structure the
 * plugin meets, and HOARD_ROBBED for what players take from inside a lair's box (hoard piles broken,
 * items taken out of its containers). Server thread, except {@link #loaded}.
 *
 * <p><b>Finding lairs</b> (choice Z4). A lair is its structure start: found when the chunk holding the
 * start loads ({@code ChunkEvent.Load}, flagged LATE when the chunk came from disk rather than generation),
 * and when an untagged dragon joins inside one (its lair must be known before its DRAGON_UNTAGGED names it).
 * Each lair is reported once per session: the engine treats a lair it knows as a no-op, and an engine that
 * dropped its pages (S12) hears of every lair again as its chunks load.
 *
 * <p><b>Chests</b> (SINK.md §4.6, docs/boundary.md §6.3). Every {@link #CHEST_SCAN_TICKS} ticks the loaded
 * chunks of each lair reported this session are looked over for a chest with room, the nearest the lair's
 * home; the first found is reported (LAIR_CHEST), and the engine sends the lair's tribute into it. A lair is
 * reported again only when its chest is gone, full, or its chunk was unloaded since (the engine forgets a
 * chest whose transfer failed, and an unloaded chunk is what fails one), and every session. A lair with no chest sends
 * nothing.
 */
public final class Lairs {
    /** {@code #mic_rtsim_iceandfire:hoard}: blocks that are a lair's hoard (Ice and Fire's gold, silver and copper piles). */
    public static final TagKey<Block> HOARD = TagKey.create(Registries.BLOCK, ResourceLocation.fromNamespaceAndPath(MicRtsimIceAndFire.MODID, "hoard"));

    /** A lair as the plugin knows it: its id (the start packed as villages are), structure and box. */
    public record Lair(long id, ServerLevel level, String structure, BoundingBox box, BlockPos home) {}

    private final DragonBridge bridge;
    private final Set<Long> reported = new HashSet<>();
    /** Lairs a GameTest declared (no structure start can be made for one in a test world). */
    private final List<Lair> declared = new ArrayList<>();

    private record Loaded(LevelChunk chunk, boolean late) {}

    private final ConcurrentLinkedQueue<Loaded> loadedChunks = new ConcurrentLinkedQueue<>();

    private record RobKey(UUID player, long lair) {}

    private final Map<RobKey, Map<Item, Integer>> robbed = new LinkedHashMap<>();

    /** Open container screens inside a lair: the counts when they opened. */
    private record Opened(Lair lair, List<Container> containers, Map<Item, Integer> before) {}

    private final Map<UUID, Opened> open = new HashMap<>();

    /** LAIR_CHEST: how often the lairs' loaded chunks are looked over for a chest (PLACEHOLDER). */
    public static final int CHEST_SCAN_TICKS = 100;

    /** Lairs reported this session, by id. */
    private final Map<Long, Lair> known = new LinkedHashMap<>();
    /** The chest reported for each lair this session, with the chunk it was in when reported. */
    private final Map<Long, Reported> chests = new HashMap<>();
    private long lastScan = Long.MIN_VALUE;

    private record Reported(BlockPos pos, LevelChunk chunk) {}

    public long lairsFound, robberies, chestsReported;

    Lairs(DragonBridge bridge) {
        this.bridge = bridge;
    }

    void clear() {
        reported.clear();
        declared.clear();
        robbed.clear();
        open.clear();
        known.clear();
        chests.clear();
        lastScan = Long.MIN_VALUE;
        lairsFound = robberies = chestsReported = 0;
    }

    // ---- finding lairs --------------------------------------------------------------------------

    /** ChunkEvent.Load: a chunk holding structure starts is looked at on the server thread at the end of the tick. */
    void loaded(LevelChunk chunk, boolean newChunk) {
        if (!chunk.getAllStarts().isEmpty())
            loadedChunks.add(new Loaded(chunk, !newChunk));
    }

    void drain(long now) {
        for (Loaded l; (l = loadedChunks.poll()) != null; ) {
            if (!(l.chunk().getLevel() instanceof ServerLevel level) || level.getServer() != bridge.session().server())
                continue; // a chunk of another world (singleplayer: the queue outlived its server)
            Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
            for (Map.Entry<Structure, StructureStart> e : l.chunk().getAllStarts().entrySet()) {
                ResourceLocation id = reg.getKey(e.getKey());
                if (id != null && e.getValue().isValid() && IafDragons.lairKind(id.toString()) != null)
                    report(lair(level, id.toString(), e.getValue()), l.late(), now);
            }
        }
    }

    private Lair lair(ServerLevel level, String structure, StructureStart start) {
        RtsimSession s = bridge.session();
        int dim = s.dimId(level.dimension());
        long id = Obs.villageId(dim, start.getChunkPos().x, start.getChunkPos().z);
        return new Lair(id, level, structure, start.getBoundingBox(), home(level, structure, start.getBoundingBox()));
    }

    /**
     * Where the lair's dragon roosts (choice Z5): the box's centre; for a roost, standing on the surface
     * there when that chunk is loaded (Ice and Fire places the roost's dragon on the surface heightmap at
     * the start's pivot, the box's centre column), else the box's floor.
     */
    private static BlockPos home(ServerLevel level, String structure, BoundingBox box) {
        BlockPos c = box.getCenter();
        int[] kind = IafDragons.lairKind(structure);
        if (kind != null && kind[1] == 0) {
            int y = level.hasChunk(c.getX() >> 4, c.getZ() >> 4)
                    ? level.getHeight(Heightmap.Types.WORLD_SURFACE, c.getX(), c.getZ())
                    : box.minY();
            return new BlockPos(c.getX(), y, c.getZ());
        }
        return c;
    }

    /** LAIR_FOUND, once per lair per session. */
    void report(Lair lair, boolean late, long now) {
        if (!reported.add(lair.id()) || !RtsimConfig.on(Module.ECOLOGY))
            return;
        RtsimSession s = bridge.session();
        int dim = s.dimId(lair.level().dimension());
        int structure = s.dictId(Obs.DICT_STRUCTURE, lair.structure());
        ByteBuffer r = s.boundary().beginJournaled(DragonRecords.LAIR_FOUND, DragonRecords.LairFound.SIZE,
                late ? DragonRecords.LairFound.FLAG_LATE : 0, now);
        r.putLong(DragonRecords.LairFound.LAIR, lair.id());
        r.putInt(DragonRecords.LairFound.DIM, dim);
        r.putInt(DragonRecords.LairFound.STRUCTURE, structure);
        BoundingBox b = lair.box();
        r.putInt(DragonRecords.LairFound.MIN_X, b.minX());
        r.putInt(DragonRecords.LairFound.MIN_Y, b.minY());
        r.putInt(DragonRecords.LairFound.MIN_Z, b.minZ());
        r.putInt(DragonRecords.LairFound.MAX_X, b.maxX());
        r.putInt(DragonRecords.LairFound.MAX_Y, b.maxY());
        r.putInt(DragonRecords.LairFound.MAX_Z, b.maxZ());
        r.putInt(DragonRecords.LairFound.HOME_X, lair.home().getX());
        r.putInt(DragonRecords.LairFound.HOME_Y, lair.home().getY());
        r.putInt(DragonRecords.LairFound.HOME_Z, lair.home().getZ());
        s.boundary().commit(r);
        known.put(lair.id(), lair);
        lairsFound++;
    }

    /**
     * The lair whose box holds a position, reported first if this session has not yet (so a record that
     * names it never comes before it), or null. Looks the six lair structures up in the level's structure
     * references ({@code StructureManager.getStructureAt}: the start's box).
     */
    public Lair at(ServerLevel level, BlockPos pos, long now) {
        for (Lair l : declared)
            if (l.level() == level && l.box().isInside(pos)) {
                report(l, false, now);
                return l;
            }
        Registry<Structure> reg = level.registryAccess().registryOrThrow(Registries.STRUCTURE);
        for (String[] pair : IafDragons.LAIRS) {
            for (String name : pair) {
                Structure structure = reg.get(ResourceLocation.parse(name));
                if (structure == null)
                    continue;
                StructureStart start = level.structureManager().getStructureAt(pos, structure);
                if (start.isValid()) {
                    Lair l = lair(level, name, start);
                    report(l, false, now);
                    return l;
                }
            }
        }
        return null;
    }

    /**
     * GameTests: declares a lair over a box of the test world, as if a structure start of {@code structure}
     * stood there, under {@code id}, and reports it (LAIR_FOUND).
     */
    public Lair declareForTest(ServerLevel level, long id, String structure, BoundingBox box, BlockPos home, long now) {
        Lair l = new Lair(id, level, structure, box, home);
        declared.add(l);
        report(l, false, now);
        return l;
    }

    // ---- chests ---------------------------------------------------------------------------------

    /** End of tick: LAIR_CHEST for lairs whose chest is new to the engine (see the class comment). */
    void scanChests(long now) {
        if (lastScan != Long.MIN_VALUE && now - lastScan < CHEST_SCAN_TICKS)
            return;
        lastScan = now;
        if (!RtsimConfig.on(Module.ECOLOGY)) {
            chests.clear(); // switched on again, the engine has forgotten every chest
            return;
        }
        RtsimSession s = bridge.session();
        for (Lair l : known.values()) {
            if (l.level().getServer() != s.server())
                continue;
            Reported had = chests.get(l.id());
            if (had != null) {
                // The same chunk object: never unloaded since the report, so no transfer into it failed.
                LevelChunk chunk = l.level().getChunkSource().getChunkNow(had.pos().getX() >> 4, had.pos().getZ() >> 4);
                if (chunk == had.chunk() && chunk.getBlockEntity(had.pos()) instanceof ChestBlockEntity c && hasRoom(c))
                    continue;
                chests.remove(l.id()); // unloaded since, gone or full: another, or this one again
            }
            BlockPos found = findChest(l);
            LevelChunk in = found == null ? null : l.level().getChunkSource().getChunkNow(found.getX() >> 4, found.getZ() >> 4);
            if (in != null && reportChest(s, l, found, now))
                chests.put(l.id(), new Reported(found, in));
        }
    }

    /** The chest with room in the lair's loaded chunks nearest its home, or null. */
    private static BlockPos findChest(Lair l) {
        BoundingBox b = l.box();
        BlockPos best = null;
        double bestD = Double.MAX_VALUE;
        for (int cx = b.minX() >> 4; cx <= b.maxX() >> 4; cx++)
            for (int cz = b.minZ() >> 4; cz <= b.maxZ() >> 4; cz++) {
                LevelChunk chunk = l.level().getChunkSource().getChunkNow(cx, cz);
                if (chunk == null)
                    continue;
                for (Map.Entry<BlockPos, BlockEntity> e : chunk.getBlockEntities().entrySet()) {
                    BlockPos p = e.getKey();
                    if (!(e.getValue() instanceof ChestBlockEntity c) || !b.isInside(p) || !hasRoom(c))
                        continue;
                    double d = p.distSqr(l.home());
                    if (d < bestD || (d == bestD && p.compareTo(best) < 0)) {
                        best = p.immutable();
                        bestD = d;
                    }
                }
            }
        return best;
    }

    /**
     * Whether a chest has an empty slot (a whole tribute stack fits). A chest whose loot is not rolled yet
     * has room: reading its slots would roll it.
     */
    private static boolean hasRoom(ChestBlockEntity c) {
        if (c.getLootTable() != null)
            return true;
        for (int i = 0; i < c.getContainerSize(); i++)
            if (c.getItem(i).isEmpty())
                return true;
        return false;
    }

    private boolean reportChest(RtsimSession s, Lair l, BlockPos pos, long now) {
        int dim = s.dimId(l.level().dimension()); // before direct(): an id is defined before its first use
        int at = s.boundary().direct(DragonRecords.LAIR_CHEST, DragonRecords.LairChest.SIZE, 0, now);
        if (at < 0)
            return false; // dropped for space: the next scan
        ByteBuffer b = s.boundary().openBuffer();
        b.putLong(at + DragonRecords.LairChest.LAIR, l.id());
        b.putInt(at + DragonRecords.LairChest.DIM, dim);
        b.putInt(at + DragonRecords.LairChest.X, pos.getX());
        b.putInt(at + DragonRecords.LairChest.Y, pos.getY());
        b.putInt(at + DragonRecords.LairChest.Z, pos.getZ());
        chestsReported++;
        return true;
    }

    /** The chest reported for a lair this session, or null (GameTests). */
    public BlockPos chestOf(long lair) {
        Reported r = chests.get(lair);
        return r == null ? null : r.pos();
    }

    // ---- robbed hoards --------------------------------------------------------------------------

    /**
     * BlockEvent.BreakEvent (lowest priority, not cancelled): a hoard pile a player breaks inside a lair is
     * robbed of what the block drops for that player (its loot table: Ice and Fire's piles drop 2 nuggets a
     * layer), whatever reaches the player's inventory.
     */
    void onBreak(ServerLevel level, BlockPos pos, BlockState state, Player player, long now) {
        if (!state.is(HOARD))
            return;
        Lair lair = at(level, pos, now);
        if (lair == null)
            return;
        BlockEntity be = level.getBlockEntity(pos);
        for (ItemStack drop : Block.getDrops(state, level, pos, be, player, player.getMainHandItem()))
            add(player.getUUID(), lair.id(), drop.getItem(), drop.getCount());
    }

    /** PlayerContainerEvent.Open: a container screen on blocks inside a lair; its counts are kept until it closes. */
    void onOpen(Player player, AbstractContainerMenu menu, long now) {
        if (!(player.level() instanceof ServerLevel level))
            return;
        List<Container> containers = new ArrayList<>();
        Lair lair = null;
        for (Slot slot : menu.slots) {
            Container c = slot.container;
            if (!(c instanceof BlockEntity be) || containers.contains(c))
                continue;
            Lair l = at(level, be.getBlockPos(), now);
            if (l == null)
                continue;
            lair = l;
            containers.add(c);
        }
        if (lair != null)
            open.put(player.getUUID(), new Opened(lair, containers, count(containers)));
    }

    /** PlayerContainerEvent.Close: what left the lair's containers while the screen was open is robbed. */
    void onClose(Player player) {
        Opened o = open.remove(player.getUUID());
        if (o == null)
            return;
        Map<Item, Integer> after = count(o.containers());
        for (Map.Entry<Item, Integer> e : o.before().entrySet()) {
            int taken = e.getValue() - after.getOrDefault(e.getKey(), 0);
            if (taken > 0)
                add(player.getUUID(), o.lair().id(), e.getKey(), taken);
        }
    }

    private static Map<Item, Integer> count(List<Container> containers) {
        Map<Item, Integer> m = new HashMap<>();
        for (Container c : containers)
            for (int i = 0; i < c.getContainerSize(); i++) {
                ItemStack s = c.getItem(i);
                if (!s.isEmpty())
                    m.merge(s.getItem(), s.getCount(), Integer::sum);
            }
        return m;
    }

    private void add(UUID player, long lair, Item item, int count) {
        if (count > 0)
            robbed.computeIfAbsent(new RobKey(player, lair), k -> new LinkedHashMap<>()).merge(item, count, Integer::sum);
    }

    /** HOARD_ROBBED per (player, lair), summed over the tick, with a Java op id (the transfer out of the hoard). */
    void flushRobberies(long now) {
        if (robbed.isEmpty())
            return;
        RtsimSession s = bridge.session();
        for (Map.Entry<RobKey, Map<Item, Integer>> e : robbed.entrySet()) {
            List<Map.Entry<Item, Integer>> items = new ArrayList<>(e.getValue().entrySet());
            for (int from = 0; from < items.size(); from += DragonRecords.HoardRobbed.MAX_ITEMS) {
                List<Map.Entry<Item, Integer>> part = items.subList(from, Math.min(items.size(), from + DragonRecords.HoardRobbed.MAX_ITEMS));
                int[] ids = new int[part.size()];
                for (int i = 0; i < ids.length; i++)
                    ids[i] = s.itemId(part.get(i).getKey()); // before the record: an id is defined before its first use
                ByteBuffer r = s.boundary().beginJournaled(DragonRecords.HOARD_ROBBED, DragonRecords.HoardRobbed.size(part.size()), part.size(), now);
                r.putLong(DragonRecords.HoardRobbed.LAIR, e.getKey().lair());
                r.putLong(DragonRecords.HoardRobbed.PLAYER_MOST, e.getKey().player().getMostSignificantBits());
                r.putLong(DragonRecords.HoardRobbed.PLAYER_LEAST, e.getKey().player().getLeastSignificantBits());
                for (int i = 0; i < ids.length; i++) {
                    int at = DragonRecords.HoardRobbed.ITEMS + i * DragonRecords.HoardRobbed.ITEM;
                    r.putInt(at, ids[i]);
                    r.putInt(at + 4, part.get(i).getValue());
                }
                s.boundary().commitWithOpId(r, DragonRecords.HoardRobbed.OP_ID);
                robberies++;
            }
        }
        robbed.clear();
    }

    static String blockName(BlockState state) {
        return BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString();
    }
}
