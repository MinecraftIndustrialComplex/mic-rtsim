package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.EntitySectionManagerAccessor;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.ServerLevelAccessor;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.Pig;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.entity.vehicle.Boat;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.entity.ChunkEntities;
import net.minecraft.world.level.entity.EntityPersistentStorage;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Puppets are never saved by vanilla (PLAN.md §6.1 item 3), tested on vanilla's own save path: the
 * level's entity manager stores every loaded entity chunk ({@code autoSave}, what the periodic world
 * save runs), and the chunk is read back from the entity storage (region files under
 * {@code entities/}, through the same IO worker), exactly as a reload would read it.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class PuppetSaveGameTests {
    private PuppetSaveGameTests() {}

    /** Stores every loaded entity chunk (vanilla's autosave) and reads the given chunks back from storage. */
    @SuppressWarnings("unchecked")
    private static CompletableFuture<List<Entity>> saveAndReadBack(ServerLevel level, ChunkPos... chunks) {
        var manager = ((ServerLevelAccessor) level).mic_rtsim$entityManager();
        manager.autoSave();
        EntityPersistentStorage<Entity> storage =
                (EntityPersistentStorage<Entity>) ((EntitySectionManagerAccessor) manager).mic_rtsim$permanentStorage();
        CompletableFuture<List<Entity>> all = CompletableFuture.completedFuture(List.of());
        for (ChunkPos chunk : new java.util.LinkedHashSet<>(List.of(chunks))) {
            CompletableFuture<ChunkEntities<Entity>> one = storage.loadEntities(chunk);
            all = all.thenCombine(one, (list, ce) -> {
                List<Entity> out = new java.util.ArrayList<>(list);
                ce.getEntities().forEach(out::add);
                return out;
            });
        }
        return all;
    }

    private static Entity find(List<Entity> entities, UUID id) {
        for (Entity e : entities) {
            for (Entity part : e.getSelfAndPassengers().toList())
                if (part.getUUID().equals(id))
                    return part;
        }
        return null;
    }

    /** A puppet is not in the saved chunk; an ordinary trader standing on the same block is. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void puppetIsNotSavedAcrossSaveAndReload(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        Vec3 at = RtsimTests.centre(helper);
        RtsimTests.engine().spawnGroup(agent, 1, "minecraft:wandering_trader", RtsimTests.dimension(helper), at.x, at.y, at.z,
                List.of(ScriptedEngine.MemberSpec.plain(0)));
        WanderingTrader control = helper.spawn(EntityType.WANDERING_TRADER, new Vec3(5.5, 1, 5.5));
        control.setPersistenceRequired();
        ChunkPos chunk = new ChunkPos(net.minecraft.core.BlockPos.containing(at));
        @SuppressWarnings("unchecked")
        CompletableFuture<List<Entity>>[] read = new CompletableFuture[1];
        UUID[] puppet = new UUID[1];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    Entity p = RtsimTests.member(RtsimTests.group(agent), 0);
                    puppet[0] = p.getUUID();
                    check("the puppet is tagged", RtsimAttachments.isTagged(p));
                    check("shouldBeSaved is false for the puppet", !p.shouldBeSaved());
                    check("save() refuses the puppet", !p.save(new CompoundTag()));
                    check("shouldBeSaved is true for the control trader", control.shouldBeSaved());
                    check("save() writes the control trader", control.save(new CompoundTag()));
                    read[0] = saveAndReadBack(helper.getLevel(), chunk);
                })
                .thenWaitUntil(() -> check("the entity chunk was read back", read[0] != null && read[0].isDone()))
                .thenExecute(() -> {
                    List<Entity> loaded = read[0].join();
                    record("puppetIsNotSavedAcrossSaveAndReload", "entities read back from chunk " + chunk, loaded.size());
                    check("the control trader was saved and reloads", find(loaded, control.getUUID()) != null);
                    check("the puppet did not reach the disk", find(loaded, puppet[0]) == null);
                    for (Entity e : loaded)
                        check("no tagged entity reloads (" + e + ")", !RtsimAttachments.isTagged(e));
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the puppet was despawned", RtsimTests.group(agent) == null))
                .thenExecute(control::discard)
                .thenSucceed();
    }

    /**
     * A puppet riding a boat is not saved with the boat. The boat saves its passengers from
     * saveWithoutId through saveAsPassenger, never asking shouldBeSaved; a pig in a second boat shows
     * that passengers do reach the disk on this path.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void puppetRidingABoatIsNotSaved(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        Vec3 at = RtsimTests.centre(helper);
        RtsimTests.engine().spawnGroup(agent, 1, "minecraft:wandering_trader", RtsimTests.dimension(helper), at.x, at.y, at.z,
                List.of(ScriptedEngine.MemberSpec.plain(0)));
        Boat boat = helper.spawn(EntityType.BOAT, new Vec3(5.5, 1, 5.5));
        Boat controlBoat = helper.spawn(EntityType.BOAT, new Vec3(5.5, 1, 3.5));
        Pig pig = helper.spawn(EntityType.PIG, new Vec3(5.5, 1, 3.5));
        ChunkPos chunk = new ChunkPos(net.minecraft.core.BlockPos.containing(at));
        @SuppressWarnings("unchecked")
        CompletableFuture<List<Entity>>[] read = new CompletableFuture[1];
        UUID[] puppet = new UUID[1];

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    Entity p = RtsimTests.member(g, 0);
                    puppet[0] = p.getUUID();
                    // A boat boards any mob it touches (Boat.tick), so either may be aboard already.
                    check("the puppet boards the boat", p.getVehicle() == boat || p.startRiding(boat, true));
                    check("the pig boards the control boat", pig.getVehicle() == controlBoat || pig.startRiding(controlBoat, true));
                    CompoundTag tag = new CompoundTag();
                    check("the boat itself is saved", boat.save(tag));
                    check("the boat's saved NBT has no passengers", !tag.contains("Passengers"));
                    CompoundTag control = new CompoundTag();
                    controlBoat.save(control);
                    check("the control boat's NBT lists its pig", control.getList("Passengers", 10).size() == 1);
                    // The arena may straddle a chunk border: read back both boats' chunks.
                    read[0] = saveAndReadBack(helper.getLevel(), chunk, controlBoat.chunkPosition());
                })
                .thenWaitUntil(() -> check("the entity chunk was read back", read[0] != null && read[0].isDone()))
                .thenExecute(() -> {
                    List<Entity> loaded = read[0].join();
                    Entity reBoat = find(loaded, boat.getUUID());
                    Entity reControl = find(loaded, controlBoat.getUUID());
                    record("puppetRidingABoatIsNotSaved", "boat passengers after reload", reBoat == null ? "boat missing" : reBoat.getPassengers());
                    check("the boat reloads", reBoat != null);
                    check("the reloaded boat is empty", reBoat.getPassengers().isEmpty());
                    check("the control boat reloads with its pig", reControl != null && reControl.getPassengers().size() == 1);
                    check("the puppet did not reach the disk", find(loaded, puppet[0]) == null);
                    RtsimTests.engine().despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the puppet was despawned", RtsimTests.group(agent) == null))
                .thenExecute(() -> {
                    pig.discard();
                    boat.discard();
                    controlBoat.discard();
                })
                .thenSucceed();
    }
}
