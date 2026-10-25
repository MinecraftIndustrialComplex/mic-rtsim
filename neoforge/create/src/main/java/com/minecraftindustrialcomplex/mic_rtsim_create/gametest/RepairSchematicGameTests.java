package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.industry.DestroyedBlock;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.RepairFiles;
import com.minecraftindustrialcomplex.mic_rtsim_create.RepairSchematics;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllDataComponents;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.kinetics.belt.BeltBlock;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.kinetics.belt.BeltPart;
import com.simibubi.create.content.kinetics.belt.item.BeltConnectorItem;
import com.simibubi.create.content.kinetics.simpleRelays.AbstractSimpleShaftBlock;
import com.simibubi.create.content.schematics.cannon.SchematicannonBlockEntity;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.Tag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.Container;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.*;

/**
 * Repair schematics (I1, docs/choices-repair.md R10 to R20): an offline raid on a small Create district,
 * through the path an unwatched attack's damage takes (the engine's IND_CHUNK_RECONCILE destroy entries,
 * held while the chunk is unloaded and applied at its load), comes back as one Create schematic of exactly
 * the blocks the sim destroyed, with their states from before: a three-segment belt (one segment hit, the
 * whole belt goes, its pulleys left as shafts), an andesite casing with the torch on it, and a chest. A
 * casing a player broke next door is not in it. The schematic item lies at the damage, deployed and
 * anchored, and a real Schematicannon with a creative crate prints every block back. Either engine.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class RepairSchematicGameTests {
    private RepairSchematicGameTests() {}

    private static int farChunk(GameTestHelper helper, boolean x, int offset) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        return ((x ? o.getX() : o.getZ()) >> 4) + offset;
    }

    /**
     * A wait on the world's IO (a far chunk generating, loading, saving its entities): the GameTest server
     * ticks as fast as it can, so on a busy machine a timeout in ticks passes before the IO is done. While
     * the check fails, the tick waits 5 ms, so a timeout of 12,000 ticks is at least a minute of IO.
     */
    private static Runnable io(Runnable check) {
        return () -> {
            try {
                check.run();
            } catch (RuntimeException e) {
                try {
                    Thread.sleep(5);
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                }
                throw e;
            }
        };
    }

    private static ByteBuffer result(long op) {
        List<ByteBuffer> r = received(IndRecords.IND_RECONCILE_RESULT, IndRecords.ReconcileResult.OP_ID, op);
        return r.isEmpty() ? null : r.get(r.size() - 1);
    }

    /** The template's blocks as absolute position to state, read back from the file. */
    private static Map<BlockPos, BlockState> readTemplate(ServerLevel level, RepairSchematics.Delivered d, Map<BlockPos, CompoundTag> nbtOut) {
        CompoundTag t;
        try {
            t = NbtIo.readCompressed(d.path(), NbtAccounter.unlimitedHeap());
        } catch (IOException e) {
            throw new net.minecraft.gametest.framework.GameTestAssertException("reading " + d.path() + ": " + e);
        }
        ListTag palette = t.getList("palette", Tag.TAG_COMPOUND);
        ListTag blocks = t.getList("blocks", Tag.TAG_COMPOUND);
        Map<BlockPos, BlockState> out = new HashMap<>();
        for (int i = 0; i < blocks.size(); i++) {
            CompoundTag b = blocks.getCompound(i);
            ListTag p = b.getList("pos", Tag.TAG_INT);
            BlockPos abs = d.anchor().offset(p.getInt(0), p.getInt(1), p.getInt(2));
            out.put(abs, NbtUtils.readBlockState(level.holderLookup(Registries.BLOCK), palette.getCompound(b.getInt("state"))));
            if (b.contains("nbt"))
                nbtOut.put(abs, b.getCompound("nbt"));
        }
        return out;
    }

    @GameTest(template = YARD, batch = "repair_1", timeoutTicks = ChunkWait.NO_TICK_TIMEOUT)
    public static void anOfflineRaidComesBackAsARepairSchematicThatACannonPrints(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        ChunkWait waits = ChunkWait.of(helper, 16000);
        int fx = farChunk(helper, true, 3500), fz = farChunk(helper, false, 3500);
        waits.onEnd(() -> level.setChunkForced(fx, fz, false));
        int x0 = fx * 16, z0 = fz * 16, y = 100;
        BlockPos chestPos = new BlockPos(x0 + 2, y, z0 + 2);
        BlockPos beltStart = new BlockPos(x0 + 4, y, z0 + 6), beltMid = beltStart.east(), beltEnd = beltStart.east(2);
        BlockPos casing = new BlockPos(x0 + 4, y, z0 + 9), torch = casing.above(), playerCasing = casing.east();
        BlockPos cannonPos = new BlockPos(x0 + 12, y, z0 + 12), cratePos = cannonPos.below();
        long op = engineOp();
        long[] id = {0};
        int[] mark = {0};
        Map<BlockPos, BlockState> before = new HashMap<>();
        String[] expected = new String[3];
        RepairSchematics.Delivered[] got = {null};
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> check("the far chunk loads", level.getChunkSource().getChunkNow(fx, fz) != null)))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    for (BlockPos q : BlockPos.betweenClosed(x0, y, z0, x0 + 15, y + 3, z0 + 15))
                        level.setBlock(q, Blocks.AIR.defaultBlockState(), 2 | 16);
                    for (BlockPos q : BlockPos.betweenClosed(x0, y - 1, z0, x0 + 15, y - 1, z0 + 15))
                        level.setBlock(q, Blocks.STONE.defaultBlockState(), 2 | 16); // a floor: drops and the schematic stay put
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    ((Container) level.getBlockEntity(chestPos)).setItem(0, new ItemStack(Items.COPPER_INGOT, 5));
                    BlockState shaft = AllBlocks.SHAFT.getDefaultState().setValue(AbstractSimpleShaftBlock.AXIS, Direction.Axis.Z);
                    level.setBlock(beltStart, shaft, 3);
                    level.setBlock(beltEnd, shaft, 3);
                    BeltConnectorItem.createBelts(level, beltStart, beltEnd);
                    level.setBlock(casing, AllBlocks.ANDESITE_CASING.getDefaultState(), 3);
                    level.setBlock(torch, Blocks.TORCH.defaultBlockState(), 3);
                    level.setBlock(playerCasing, AllBlocks.ANDESITE_CASING.getDefaultState(), 3);
                    playerPlaced(helper, chestPos);
                    playerPlaced(helper, beltStart);
                })
                .thenWaitUntil(waits.engine(() -> {
                    check("a three-segment belt", level.getBlockEntity(beltMid) instanceof BeltBlockEntity b && b.getController() != null
                            && b.getControllerBE() != null && b.getControllerBE().beltLength == 3);
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null && d == ind().districtAt(level, beltMid));
                    id[0] = d.id;
                }))
                .thenExecute(() -> {
                    // A player breaks the casing next to the one the raid will hit: that hole is the player's.
                    com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.editor(level).gameMode.destroyBlock(playerCasing);
                    check("the player's casing is gone", level.getBlockState(playerCasing).isAir());
                    for (BlockPos p : List.of(chestPos, beltStart, beltMid, beltEnd, casing, torch))
                        before.put(p, level.getBlockState(p));
                    expected[0] = BlockStateParser.serialize(level.getBlockState(beltMid));
                    expected[1] = BlockStateParser.serialize(level.getBlockState(casing));
                    expected[2] = BlockStateParser.serialize(level.getBlockState(chestPos));
                    level.setChunkForced(fx, fz, false);
                })
                .thenWaitUntil(waits.chunks(() -> check("the district unloads", level.getChunkSource().getChunkNow(fx, fz) == null
                        && ind().districts().loadedChunks(ind().district(id[0])) == 0)))
                // The raid, unwatched: its damage reaches Java as the reconcile's destroy entries, held.
                .thenExecute(() -> {
                    mark[0] = ind().reconciler().held();
                    reconcile(op, id[0], 1, level, fx, fz, List.of(), List.of(
                            new Destroy(beltMid, expected[0]), new Destroy(casing, expected[1]), new Destroy(chestPos, expected[2])));
                })
                .thenWaitUntil(waits.engine(() -> check("held" + ReconcileAheadGameTests.pipeline(op), ind().reconciler().held() == mark[0] + 1)))
                .thenExecute(() -> {
                    level.setChunkForced(fx, fz, true);
                })
                .thenWaitUntil(waits.chunks(() -> loads(level, fx, fz)))
                .thenWaitUntil(waits.engine(() -> {
                    ByteBuffer r = result(op);
                    check("the damage applied at the load", r != null);
                    check("three blocks broken: " + r.getInt(IndRecords.ReconcileResult.DESTROYED), r.getInt(IndRecords.ReconcileResult.DESTROYED) == 3);
                    check("the belt is gone, its pulleys left as shafts", level.getBlockState(beltMid).isAir()
                            && AllBlocks.SHAFT.has(level.getBlockState(beltStart)) && AllBlocks.SHAFT.has(level.getBlockState(beltEnd)));
                    check("the torch popped off", level.getBlockState(torch).isAir() && level.getBlockState(casing).isAir());
                }))
                .thenWaitUntil(waits.engine(() -> {
                    got[0] = RepairSchematics.delivered().stream().filter(r -> r.district() == id[0]).reduce((p, q) -> q).orElse(null);
                    check("a repair schematic is delivered for the district", got[0] != null);
                }))
                .thenExecute(() -> {
                    RepairSchematics.Delivered d = got[0];
                    check("the file is in Create's uploaded folder under mic-rtsim: " + d.path(), Files.isRegularFile(d.path())
                            && d.path().getParent().getFileName().toString().equals(RepairSchematics.OWNER));
                    Map<BlockPos, CompoundTag> nbt = new HashMap<>();
                    Map<BlockPos, BlockState> t = readTemplate(level, d, nbt);
                    record("anOfflineRaidComesBackAsARepairSchematicThatACannonPrints", "template", t.size() + " blocks, anchor " + d.anchor().toShortString()
                            + ", size " + d.size().toShortString() + ", nbt at " + nbt.keySet());
                    check("exactly the blocks the sim destroyed: " + t.keySet(), t.keySet().equals(before.keySet()));
                    for (Map.Entry<BlockPos, BlockState> e : before.entrySet())
                        check("the state before at " + e.getKey().toShortString() + ": " + t.get(e.getKey()), e.getValue().equals(t.get(e.getKey())));
                    check("not the casing the player broke", !t.containsKey(playerCasing));
                    for (CompoundTag c : nbt.values())
                        check("no inventory written: " + c.getAllKeys(), !c.contains("Items") && !c.contains("Inventory"));
                    check("the belt keeps its length", nbt.containsKey(beltStart) && nbt.get(beltStart).getInt("Length") == 3);
                    check("the chest carries no data", !nbt.containsKey(chestPos));
                    Set<BlockPos> listed = new HashSet<>();
                    for (DestroyedBlock b : d.blocks())
                        listed.add(b.pos());
                    check("the delivery lists the same blocks", listed.equals(before.keySet()));
                    // The item: Create's schematic, deployed and anchored, lying at the damage, never despawning.
                    ItemEntity item = d.item();
                    ItemStack s = item.getItem();
                    check("a Create schematic item", AllItems.SCHEMATIC.isIn(s));
                    check("deployed", Boolean.TRUE.equals(s.get(AllDataComponents.SCHEMATIC_DEPLOYED)));
                    check("anchored at the damage's corner", d.anchor().equals(s.get(AllDataComponents.SCHEMATIC_ANCHOR)));
                    check("its file and owner", d.file().equals(s.get(AllDataComponents.SCHEMATIC_FILE))
                            && RepairSchematics.OWNER.equals(s.get(AllDataComponents.SCHEMATIC_OWNER)));
                    check("in the world at the damage", item.isAlive() && before.keySet().stream().anyMatch(p -> p.closerToCenterThan(item.position(), 2.5)));
                    check("it never despawns", item.getAge() < 0);
                    // The cannon: a creative crate beside it, the schematic in its slot.
                    level.setBlock(cratePos, AllBlocks.CREATIVE_CRATE.getDefaultState(), 3);
                    level.setBlock(cannonPos, AllBlocks.SCHEMATICANNON.getDefaultState(), 3);
                    SchematicannonBlockEntity cannon = (SchematicannonBlockEntity) level.getBlockEntity(cannonPos);
                    cannon.inventory.setStackInSlot(0, s.copy());
                    // A belt prints from its last pulley, which the break left as a shaft: a block entity,
                    // which a cannon replaces only with "Replace Block Entities" on (R17).
                    cannon.replaceBlockEntities = true;
                    item.discard();
                    cannon.state = SchematicannonBlockEntity.State.RUNNING;
                    cannon.statusMsg = "running";
                })
                .thenWaitUntil(waits.engine(() -> {
                    SchematicannonBlockEntity cannon = (SchematicannonBlockEntity) level.getBlockEntity(cannonPos);
                    check("the cannon finished: " + cannon.state + " " + cannon.statusMsg, cannon.state == SchematicannonBlockEntity.State.STOPPED
                            && cannon.flyingBlocks.isEmpty());
                    check("the chest is back", level.getBlockState(chestPos).is(Blocks.CHEST));
                    check("the casing is back", AllBlocks.ANDESITE_CASING.has(level.getBlockState(casing)));
                    check("the torch is back", level.getBlockState(torch).is(Blocks.TORCH));
                    // Create prints a belt from its last pulley back (LaunchedItem.ForBelt), so the belt comes back
                    // facing the other way, its START and END swapped; it carries items the same way (its movement
                    // facing follows the axis and the shaft's rotation only).
                    for (BlockPos p : List.of(beltStart, beltMid, beltEnd)) {
                        BlockState s = level.getBlockState(p);
                        check("a belt at " + p.toShortString() + ": " + s, AllBlocks.BELT.has(s)
                                && s.getValue(BeltBlock.HORIZONTAL_FACING).getAxis() == before.get(p).getValue(BeltBlock.HORIZONTAL_FACING).getAxis());
                    }
                    check("the belt's middle", level.getBlockState(beltMid).getValue(BeltBlock.PART) == BeltPart.MIDDLE);
                    check("its ends", Set.of(level.getBlockState(beltStart).getValue(BeltBlock.PART), level.getBlockState(beltEnd).getValue(BeltBlock.PART))
                            .equals(Set.of(BeltPart.START, BeltPart.END)));
                    check("three segments long", level.getBlockEntity(beltMid) instanceof BeltBlockEntity b && b.getControllerBE() != null
                            && b.getControllerBE().beltLength == 3);
                    check("the player's hole stays a hole", level.getBlockState(playerCasing).isAir());
                }))
                .thenExecute(() -> {
                    record("anOfflineRaidComesBackAsARepairSchematicThatACannonPrints", "printed", "chest, casing, torch, belt "
                            + level.getBlockState(beltStart).getValue(BeltBlock.PART) + "/" + level.getBlockState(beltMid).getValue(BeltBlock.PART)
                            + "/" + level.getBlockState(beltEnd).getValue(BeltBlock.PART));
                    for (BlockPos p : List.of(chestPos, beltStart, beltMid, beltEnd, casing, torch, cannonPos, cratePos))
                        level.setBlock(p, Blocks.AIR.defaultBlockState(), 2 | 16);
                    for (ItemEntity it : level.getEntitiesOfClass(ItemEntity.class, new net.minecraft.world.phys.AABB(chestPos).inflate(24)))
                        it.discard();
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setChunkForced(fx, fz, false);
                })
                .thenSucceed();
    }

    /** The district's {@code n}-th schematic delivered in this server run (0 first), or null. */
    private static RepairSchematics.Delivered nth(long district, int n) {
        List<RepairSchematics.Delivered> mine = RepairSchematics.delivered().stream().filter(r -> r.district() == district).toList();
        return mine.size() > n ? mine.get(n) : null;
    }

    /**
     * Retention (R25): three waves of damage on one loaded district, a casing each. The first is printed
     * back (set again, as the cannon would) before the second wave: the second's delivery deletes the first
     * file. The second is left a hole: the third's delivery keeps the second file. Only this world's listed
     * files are candidates.
     */
    @GameTest(template = YARD, batch = "repair_2", timeoutTicks = ChunkWait.NO_TICK_TIMEOUT)
    public static void aRepairSchematicIsDeletedOnceItsBlocksAreBackAndANewWaveComes(GameTestHelper helper) {
        fastConfig();
        ServerLevel level = helper.getLevel();
        ChunkWait waits = ChunkWait.of(helper, 12000);
        int fx = farChunk(helper, true, 3700), fz = farChunk(helper, false, 3700);
        waits.onEnd(() -> level.setChunkForced(fx, fz, false));
        int x0 = fx * 16, z0 = fz * 16, y = 100;
        BlockPos chestPos = new BlockPos(x0 + 2, y, z0 + 2);
        BlockPos[] casing = {new BlockPos(x0 + 6, y, z0 + 6), new BlockPos(x0 + 8, y, z0 + 6), new BlockPos(x0 + 10, y, z0 + 6)};
        long[] ops = {engineOp(), engineOp(), engineOp()};
        long[] id = {0};
        int[] base = {0};
        RepairSchematics.Delivered[] got = new RepairSchematics.Delivered[3];
        String state = BlockStateParser.serialize(AllBlocks.ANDESITE_CASING.getDefaultState());
        helper.startSequence()
                .thenExecute(() -> level.setChunkForced(fx, fz, true))
                .thenWaitUntil(waits.chunks(() -> check("the far chunk loads", level.getChunkSource().getChunkNow(fx, fz) != null)))
                .thenExecute(() -> {
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    for (BlockPos q : BlockPos.betweenClosed(x0, y, z0, x0 + 15, y + 2, z0 + 15))
                        level.setBlock(q, Blocks.AIR.defaultBlockState(), 2 | 16);
                    for (BlockPos q : BlockPos.betweenClosed(x0, y - 1, z0, x0 + 15, y - 1, z0 + 15))
                        level.setBlock(q, Blocks.STONE.defaultBlockState(), 2 | 16);
                    level.setBlock(chestPos, Blocks.CHEST.defaultBlockState(), 3);
                    for (BlockPos c : casing)
                        level.setBlock(c, AllBlocks.ANDESITE_CASING.getDefaultState(), 3);
                    playerPlaced(helper, chestPos);
                })
                .thenWaitUntil(waits.engine(() -> {
                    District d = ind().districtAt(level, chestPos);
                    check("district", d != null);
                    id[0] = d.id;
                    base[0] = (int) RepairSchematics.delivered().stream().filter(r -> r.district() == d.id).count();
                }))
                // Wave 1: casing 0.
                .thenExecute(() -> reconcile(ops[0], id[0], 1, level, fx, fz, List.of(), List.of(new Destroy(casing[0], state))))
                .thenWaitUntil(waits.engine(() -> {
                    got[0] = nth(id[0], base[0]);
                    check("wave 1 delivered" + ReconcileAheadGameTests.pipeline(ops[0]), got[0] != null && Files.isRegularFile(got[0].path()));
                    check("listed for this world", RepairFiles.get(level.getServer()).entries().stream().anyMatch(e -> e.file().equals(got[0].file())));
                }))
                .thenExecute(() -> {
                    got[0].item().discard();
                    level.setBlock(casing[0], AllBlocks.ANDESITE_CASING.getDefaultState(), 3); // printed back
                    reconcile(ops[1], id[0], 2, level, fx, fz, List.of(), List.of(new Destroy(casing[1], state)));
                })
                // Wave 2: casing 1. Wave 1's blocks are all back: its file goes.
                .thenWaitUntil(waits.engine(() -> {
                    got[1] = nth(id[0], base[0] + 1);
                    check("wave 2 delivered" + ReconcileAheadGameTests.pipeline(ops[1]), got[1] != null && Files.isRegularFile(got[1].path()));
                    check("wave 1's file is deleted: every block is back", !Files.exists(got[0].path()));
                    check("and no longer listed", RepairFiles.get(level.getServer()).entries().stream().noneMatch(e -> e.file().equals(got[0].file())));
                }))
                .thenExecute(() -> {
                    got[1].item().discard();
                    reconcile(ops[2], id[0], 3, level, fx, fz, List.of(), List.of(new Destroy(casing[2], state)));
                })
                // Wave 3: casing 2. Wave 2's casing is still a hole: its file stays.
                .thenWaitUntil(waits.engine(() -> {
                    got[2] = nth(id[0], base[0] + 2);
                    check("wave 3 delivered" + ReconcileAheadGameTests.pipeline(ops[2]), got[2] != null && Files.isRegularFile(got[2].path()));
                    check("wave 2's file stays: its casing is still missing", Files.isRegularFile(got[1].path()));
                    check("both listed", RepairFiles.get(level.getServer()).entries().stream().filter(e -> e.district() == id[0]).count() == 2);
                    record("aRepairSchematicIsDeletedOnceItsBlocksAreBackAndANewWaveComes", "files",
                            got[0].file() + " deleted, " + got[1].file() + " kept, " + got[2].file() + " new");
                }))
                .thenExecute(() -> {
                    got[2].item().discard();
                    try {
                        Files.deleteIfExists(got[1].path());
                        Files.deleteIfExists(got[2].path());
                    } catch (IOException e) {
                        throw new net.minecraft.gametest.framework.GameTestAssertException("cleaning up: " + e);
                    }
                    for (ItemEntity it : level.getEntitiesOfClass(ItemEntity.class, new net.minecraft.world.phys.AABB(chestPos).inflate(24)))
                        it.discard();
                    for (BlockPos q : BlockPos.betweenClosed(x0, y, z0, x0 + 15, y + 2, z0 + 15))
                        level.setBlock(q, Blocks.AIR.defaultBlockState(), 2 | 16);
                    ind().districts().forgetArea(level, fx - 1, fz - 1, fx + 1, fz + 1);
                    level.setChunkForced(fx, fz, false);
                })
                .thenSucceed();
    }
}
