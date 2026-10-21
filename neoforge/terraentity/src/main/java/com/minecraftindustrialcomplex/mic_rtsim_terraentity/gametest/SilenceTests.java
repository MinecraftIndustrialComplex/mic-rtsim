package com.minecraftindustrialcomplex.mic_rtsim_terraentity.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_terraentity.MicRtsimTerraEntity;
import com.minecraftindustrialcomplex.mic_rtsim_terraentity.Probes;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LocalMobCapCalculator;
import net.minecraft.world.level.NaturalSpawner;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.MobSpawnSettings;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.GenerationStep;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.world.BiomeModifier;
import net.neoforged.neoforge.registries.NeoForgeRegistries;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.TreeSet;
import java.util.stream.Collectors;

/**
 * Test 3: can Terra Entity's natural spawns be silenced, and what does it add to world generation? Run once
 * per arm (-PteSilence=none, datapack, config): the census of every biome's spawn lists, and a long run with
 * a survival player standing in the level that drives the vanilla natural spawner over the chunks around it.
 */
@GameTestHolder(MicRtsimTerraEntity.MODID)
@PrefixGameTestTemplate(false)
public final class SilenceTests {
    private SilenceTests() {}

    private static boolean te(ResourceLocation id) {
        return id != null && id.getNamespace().equals("terra_entity");
    }

    /** Every biome's spawner entries after biome modifiers, by terra_entity entity, with the biome count. */
    @GameTest(template = "arena", timeoutTicks = 200, batch = "silence_a_lists")
    public static void biomeSpawnLists(GameTestHelper helper) {
        String test = "biomeSpawnLists";
        if (!Kit.run(helper, test, true)) return;
        Registry<Biome> biomes = helper.getLevel().registryAccess().registryOrThrow(Registries.BIOME);
        Map<String, Integer> perEntity = new TreeMap<>();
        TreeSet<String> worms = new TreeSet<>();
        int biomesWith = 0;
        int total = 0;
        for (Holder.Reference<Biome> b : biomes.holders().toList()) {
            boolean any = false;
            for (MobCategory cat : MobCategory.values()) {
                for (MobSpawnSettings.SpawnerData d : b.value().getMobSettings().getMobs(cat).unwrap()) {
                    ResourceLocation id = BuiltInRegistries.ENTITY_TYPE.getKey(d.type);
                    if (te(id)) {
                        perEntity.merge(id.getPath() + " (" + cat.getName() + ")", 1, Integer::sum);
                        total++;
                        any = true;
                        if (id.getPath().contains("worm") || id.getPath().contains("serpent") || id.getPath().equals("devourer") || id.getPath().equals("leech") || id.getPath().equals("tomb_crawler"))
                            worms.add(id.getPath() + " in " + b.key().location());
                    }
                }
            }
            if (any) biomesWith++;
        }
        Kit.say(test, "biomes", biomes.size());
        Kit.say(test, "biomes with a terra_entity spawner entry", biomesWith);
        Kit.say(test, "terra_entity spawner entries (entity, category -> biomes)", total + " " + perEntity);
        Kit.say(test, "worm-family entries (count of biome/entity pairs)", worms.size());
        Kit.say(test, "worm-family entries, first 40", worms.stream().limit(40).collect(Collectors.toList()));
        Registry<BiomeModifier> mods = helper.getLevel().registryAccess().registryOrThrow(NeoForgeRegistries.Keys.BIOME_MODIFIERS);
        Map<String, Integer> types = new TreeMap<>();
        for (var e : mods.entrySet())
            if (te(e.getKey().location()))
                types.merge(String.valueOf(NeoForgeRegistries.BIOME_MODIFIER_SERIALIZERS.getKey(e.getValue().codec())), 1, Integer::sum);
        Kit.say(test, "terra_entity biome modifiers registered (codec -> count)", types);
        helper.succeed();
    }

    /** What the mod adds to world generation: structures, features, biomes, noise settings, dimensions, carvers, biome modifiers. */
    @GameTest(template = "arena", timeoutTicks = 200, batch = "silence_b_worldgen")
    public static void worldgenAdditions(GameTestHelper helper) {
        String test = "worldgenAdditions";
        if (!Kit.run(helper, test, true)) return;
        var access = helper.getLevel().registryAccess();
        List<ResourceKey<? extends Registry<?>>> keys = List.of(Registries.STRUCTURE, Registries.STRUCTURE_SET, Registries.STRUCTURE_PIECE, Registries.CONFIGURED_FEATURE,
                Registries.PLACED_FEATURE, Registries.BIOME, Registries.DIMENSION, Registries.DIMENSION_TYPE, Registries.NOISE_SETTINGS, Registries.CONFIGURED_CARVER,
                Registries.TEMPLATE_POOL, Registries.PROCESSOR_LIST, NeoForgeRegistries.Keys.BIOME_MODIFIERS);
        for (var k : keys) {
            Registry<?> r = access.registryOrThrow(k);
            List<String> mine = r.keySet().stream().filter(Kit::isTerra).map(ResourceLocation::getPath).sorted().toList();
            Kit.say(test, k.location().getPath() + " entries in terra_entity", mine.size() + (mine.isEmpty() ? "" : " " + (mine.size() > 12 ? mine.subList(0, 12) + "..." : mine)));
        }
        long blocks = BuiltInRegistries.BLOCK.keySet().stream().filter(Kit::isTerra).count();
        long items = BuiltInRegistries.ITEM.keySet().stream().filter(Kit::isTerra).count();
        long ents = BuiltInRegistries.ENTITY_TYPE.keySet().stream().filter(Kit::isTerra).count();
        long feats = BuiltInRegistries.FEATURE.keySet().stream().filter(Kit::isTerra).count();
        long structTypes = BuiltInRegistries.STRUCTURE_TYPE.keySet().stream().filter(Kit::isTerra).count();
        Kit.say(test, "blocks / items / entity types / worldgen Feature types / structure types", blocks + " / " + items + " / " + ents + " / " + feats + " / " + structTypes);
        // Features and structures placed in any biome that come from terra_entity placed features
        long inBiomes = 0;
        for (Holder.Reference<Biome> b : access.registryOrThrow(Registries.BIOME).holders().toList())
            for (GenerationStep.Decoration step : GenerationStep.Decoration.values()) {
                var list = b.value().getGenerationSettings().features();
                if (step.ordinal() < list.size())
                    for (Holder<net.minecraft.world.level.levelgen.placement.PlacedFeature> pf : list.get(step.ordinal()))
                        if (pf.unwrapKey().map(kk -> te(kk.location())).orElse(false)) inBiomes++;
            }
        Kit.say(test, "terra_entity placed features present in biome generation settings", inBiomes);
        helper.succeed();
    }

    /**
     * The long run: a survival player in the level, the vanilla spawner driven over the 15x15 chunks around
     * it (spawnForChunk, as ServerChunkCache does each tick), N ticks, counting every spawn attempt that
     * reaches FinalizeSpawn as NATURAL, by entity type, and the terra_entity entities alive at the end.
     */
    @GameTest(template = "arena", timeoutTicks = 40000, batch = "silence_c_natural")
    public static void naturalSpawns(GameTestHelper helper) {
        String test = "naturalSpawns";
        if (!Kit.run(helper, test, true)) return;
        ServerLevel level = helper.getLevel();
        Vec3 at = helper.absoluteVec(new Vec3(5.5, 1, 5.5));
        ServerPlayer player = Kit.trackedPlayer(level, at);
        ChunkPos c0 = new ChunkPos(player.blockPosition());
        int radius = 7;
        List<ChunkPos> forced = new ArrayList<>();
        for (int dx = -radius; dx <= radius; dx++)
            for (int dz = -radius; dz <= radius; dz++) {
                ChunkPos p = new ChunkPos(c0.x + dx, c0.z + dz);
                if (level.setChunkForced(p.x, p.z, true)) forced.add(p);
            }
        int ticks = Integer.getInteger("mic_rtsim_terraentity.spawnTicks", 24000);
        Probes.FINALIZE.clear();
        Probes.JOINS.clear();
        Probes.countSpawns = true;
        Kit.say(test, "setup", "player at " + Kit.v(at) + ", biome " + level.getBiome(player.blockPosition()).unwrapKey().map(k -> k.location().toString()).orElse("?")
                + ", " + ticks + " ticks, drive = NaturalSpawner.spawnForChunk over " + (2 * radius + 1) + "^2 chunks each tick");
        int[] t = {0};
        helper.onEachTick(() -> {
            if (t[0] > ticks) return;
            if (t[0] == ticks) {
                Probes.countSpawns = false;
                Map<String, Integer> alive = new TreeMap<>();
                int vanillaMonsters = 0;
                for (Entity e : level.getAllEntities()) {
                    ResourceLocation id = BuiltInRegistries.ENTITY_TYPE.getKey(e.getType());
                    if (te(id)) alive.merge(id.getPath(), 1, Integer::sum);
                    else if (e.getType().getCategory() == MobCategory.MONSTER) vanillaMonsters++;
                }
                Map<String, Integer> teFinal = Probes.sorted(Probes.FINALIZE).entrySet().stream().filter(x -> x.getKey().startsWith("terra_entity:"))
                        .collect(Collectors.toMap(Map.Entry::getKey, Map.Entry::getValue, (x, y) -> x, TreeMap::new));
                int vanillaNatural = Probes.FINALIZE.entrySet().stream().filter(x -> !x.getKey().startsWith("terra_entity:") && x.getKey().contains(" NATURAL")).mapToInt(Map.Entry::getValue).sum();
                int teNatural = teFinal.entrySet().stream().filter(x -> x.getKey().contains(" NATURAL")).mapToInt(Map.Entry::getValue).sum();
                int teNaturalKept = teFinal.entrySet().stream().filter(x -> x.getKey().endsWith(" NATURAL")).mapToInt(Map.Entry::getValue).sum();
                Kit.say(test, "terra_entity NATURAL spawn attempts reaching finalize (incl. cancelled)", teNatural + " " + teFinal);
                Kit.say(test, "terra_entity NATURAL spawns kept (not cancelled)", teNaturalKept);
                Kit.say(test, "control: vanilla/other NATURAL finalize events", vanillaNatural);
                Kit.say(test, "terra_entity entities alive at the end", alive.values().stream().mapToInt(Integer::intValue).sum() + " " + alive);
                Kit.say(test, "terra_entity worm-family joins", Probes.sorted(Probes.JOINS).entrySet().stream()
                        .filter(x -> x.getKey().startsWith("terra_entity:") && (x.getKey().contains("worm") || x.getKey().contains("serpent") || x.getKey().contains("devourer") || x.getKey().contains("leech") || x.getKey().contains("crawler"))).toList());
                Kit.say(test, "other monsters alive at the end (mob cap in use)", vanillaMonsters);
                forced.forEach(p -> level.setChunkForced(p.x, p.z, false));
                Kit.removeTrackedPlayer(player);
                t[0]++;
                helper.succeed();
                return;
            }
            // the vanilla natural spawner over the chunks around the player
            List<LevelChunk> chunks = new ArrayList<>();
            for (int dx = -radius; dx <= radius; dx++)
                for (int dz = -radius; dz <= radius; dz++)
                    chunks.add(level.getChunk(c0.x + dx, c0.z + dz));
            var state = NaturalSpawner.createState(chunks.size(), level.getAllEntities(), (pos, consumer) -> consumer.accept(level.getChunk(ChunkPos.getX(pos), ChunkPos.getZ(pos))),
                    new LocalMobCapCalculator(level.getChunkSource().chunkMap));
            for (LevelChunk ch : chunks)
                NaturalSpawner.spawnForChunk(level, ch, state, true, true, level.getGameTime() % 400 == 0);
            if (t[0] % 4000 == 0)
                Kit.say(test, "progress t=" + t[0], "terra_entity NATURAL attempts so far " + Probes.FINALIZE.entrySet().stream().filter(x -> x.getKey().startsWith("terra_entity:")).mapToInt(Map.Entry::getValue).sum());
            t[0]++;
        });
    }
}
