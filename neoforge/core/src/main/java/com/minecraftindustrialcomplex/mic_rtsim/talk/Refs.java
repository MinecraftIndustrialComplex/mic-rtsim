package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.News;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Ref;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.QuartPos;
import net.minecraft.locale.Language;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.players.GameProfileCache;
import net.minecraft.tags.BiomeTags;
import net.minecraft.tags.TagKey;
import net.minecraft.util.Mth;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.common.Tags;

import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.random.RandomGenerator;

/**
 * Names and descriptions (TALK.md §6, TQ-3, TK29): a ref of the engine's answer told as the talker would,
 * by name or by what it is and where. Server thread only; every text is composed from the server's English
 * table and sent as plain text, so every player hears the same name.
 *
 * <p>Always named: the listener ("you"), another player (a name, "someone" when unknown), the holder's own
 * village or camp, anything in a SEEN item, anything the engine placed under 600 blocks away. Named by
 * chance: under 3,000 blocks half the time, farther a quarter ({@link Naming}). Otherwise described:
 * {@code desc.<class>[.<culture>]} and, when it has a place that is not the report's own, a locator from the
 * biome at the place and the compass ({@code desc.<class>.at}).
 */
public final class Refs {
    private Refs() {}

    /**
     * Test hook (TK52): force every ref that has a name to be named, or every ref that has a description to be
     * described. It overrides {@code own} too (the holder's own place is then named or described, not "us"
     * or always named), so it is for tests only.
     */
    public static volatile Naming.Force force = Naming.Force.NONE;

    /**
     * What telling a ref needs: the talker's level and block, the listener, the holder (or null), the answer
     * (for the dictionary names), the talk's random and the villages that are the holder's own side
     * ({@link TalkAnswer#ownVillages}; the short constructor works them out).
     */
    public record Env(ServerLevel level, BlockPos talker, UUID listener, TalkHolder holder, TalkAnswer answer,
                      RandomGenerator rnd, java.util.Set<Long> ownVillages) {
        public Env(ServerLevel level, BlockPos talker, UUID listener, TalkHolder holder, TalkAnswer answer, RandomGenerator rnd) {
            this(level, talker, listener, holder, answer, rnd,
                    answer.ownVillages(holder == null ? 0 : holder.kind(), holder == null ? 0 : holder.id()));
        }
    }

    // ---- language ------------------------------------------------------------------------------

    private static String english(String key) {
        return Language.getInstance().getOrDefault(key);
    }

    private static String fmt(String key, Object... args) {
        return String.format(english(key), args);
    }

    private static boolean has(String key) {
        return Language.getInstance().has(key);
    }

    private static final String ROOT = Lines.ROOT;

    // ---- the ref ---------------------------------------------------------------------------------

    /**
     * Tells one ref of a news item.
     *
     * @param holderSide the ref is the holder's own place in this item (the {@code whom} of a TO_US item, the
     *                   {@code who} of a BY_US one): always named
     * @param us         the line says the holder's own place as "us" ({@code ref.us}), for an item told in the
     *                   third person that is about the holder
     * @param listener   the engine says this ref is the listener (YOU_DID for {@code who}, TO_YOU for {@code whom})
     */
    public static String tell(Env env, News n, Ref r, boolean holderSide, boolean us, boolean listener) {
        switch (r.cls()) {
            case TalkRecords.C_UNKNOWN:
                return english(ROOT + (r.dead() ? "ref.notable" : "ref.unknown"));
            case TalkRecords.C_PLAYER:
                return player(env, r, listener);
            case TalkRecords.C_DRAGON: {
                String species = env.answer().whatOf(r);
                String path = species == null ? null : species.substring(species.indexOf(':') + 1);
                String key = ROOT + "ref.dragon." + path;
                return english(path != null && has(key) ? key : ROOT + "ref.dragon");
            }
            default:
                break;
        }
        boolean own = holderSide || (r.cls() == TalkRecords.C_VILLAGE && env.ownVillages().contains(r.a()));
        if (us && own)
            return english(ROOT + "ref.us");
        boolean named = switch (force) {
            case NAMED -> true;
            case DESCRIBED -> false;
            case NONE -> Naming.named(own || n.has(TalkRecords.SEEN), r.hasPos(), r.dx(), r.dz(), env.rnd());
        };
        if (named) {
            String name = name(env, r);
            if (name != null)
                return name;
        }
        return describe(env, n, r);
    }

    private static String player(Env env, Ref r, boolean listener) {
        UUID id = new UUID(r.a(), r.b());
        if (listener || id.equals(env.listener()))
            return english(ROOT + "you");
        MinecraftServer server = env.level().getServer();
        // The level's own players first: they are who the talker can see, and a test's player stands only there.
        ServerPlayer online = env.level().getServer().getPlayerList().getPlayer(id);
        if (online == null && env.level().getPlayerByUUID(id) instanceof ServerPlayer here)
            online = here;
        if (online != null)
            return online.getGameProfile().getName();
        GameProfileCache cache = server.getProfileCache();
        if (cache != null) {
            java.util.Optional<com.mojang.authlib.GameProfile> p = cache.get(id);
            if (p.isPresent())
                return p.get().getName();
        }
        return english(ROOT + "someone");
    }

    /** A ref's name, or null when it has none (no word lists for its people). */
    static String name(Env env, Ref r) {
        String culture = env.answer().whatOf(r);
        return switch (r.cls()) {
            case TalkRecords.C_VILLAGE -> Names.village(r.a());
            case TalkRecords.C_BASE -> english(ROOT + "ref.base");
            case TalkRecords.C_CARAVAN -> r.b() != 0 ? fmt(ROOT + "ref.caravan.from", Names.village(r.b())) : english(ROOT + "ref.caravan");
            case TalkRecords.C_CAMP -> Names.camp(culture, r.a());
            case TalkRecords.C_CHIEF -> {
                String chief = Names.chief(culture, r.a());
                String camp = Names.camp(culture, r.b());
                yield chief != null ? (camp != null ? fmt(ROOT + "ref.chief", chief, camp) : chief)
                        : camp != null ? fmt(ROOT + "ref.chief.unnamed", camp) : null;
            }
            case TalkRecords.C_LAIR -> english(ROOT + "ref.lair");
            case TalkRecords.C_HAVEN -> {
                String haven = Names.haven(r.a());
                yield haven != null ? fmt(ROOT + "ref.haven", haven) : english(ROOT + "ref.haven.unnamed");
            }
            case TalkRecords.C_SHIP -> english(ROOT + "ref.ship");
            default -> null;
        };
    }

    /** A ref by what it is and, when it has a place that is not the report's own, where. */
    static String describe(Env env, News n, Ref r) {
        String kind = switch (r.cls()) {
            case TalkRecords.C_VILLAGE -> "village";
            case TalkRecords.C_BASE -> "base";
            case TalkRecords.C_CARAVAN -> "caravan";
            case TalkRecords.C_CAMP -> "camp";
            case TalkRecords.C_CHIEF -> "chief";
            case TalkRecords.C_HAVEN -> "haven";
            case TalkRecords.C_LAIR -> "lair";
            case TalkRecords.C_SHIP -> "ship";
            default -> null;
        };
        if (kind == null)
            return english(ROOT + "ref.unknown");
        String base = ROOT + "desc." + kind;
        String culture = env.answer().whatOf(r);
        String key = culture != null && has(base + "." + culture) ? base + "." + culture : base;
        boolean reportPlaced = n.dx() != 0 || n.dz() != 0;
        if (r.hasPos() && !(reportPlaced && Naming.atReport(r.dx(), r.dz(), n.dx(), n.dz())) && has(key + ".at")) {
            String where = locator(env, r.dx(), r.dz());
            if (where != null)
                return fmt(key + ".at", where);
        }
        return english(key);
    }

    // ---- the locator -------------------------------------------------------------------------------

    private static final Map<String, List<TagKey<Biome>>> TERRAIN = new LinkedHashMap<>();

    @SafeVarargs
    private static void terrain(String name, TagKey<Biome>... tags) {
        TERRAIN.put(name, List.of(tags));
    }

    static {
        // The first that matches wins, so the narrow places come before the wide ones: a frozen ocean is
        // "out on the water", a snowy mountain "in the hills".
        terrain("river", BiomeTags.IS_RIVER);
        terrain("beach", BiomeTags.IS_BEACH);
        terrain("ocean", BiomeTags.IS_OCEAN, Tags.Biomes.IS_OCEAN);
        terrain("mountain", Tags.Biomes.IS_MOUNTAIN, BiomeTags.IS_MOUNTAIN);
        terrain("badlands", BiomeTags.IS_BADLANDS);
        terrain("jungle", BiomeTags.IS_JUNGLE);
        terrain("desert", Tags.Biomes.IS_DESERT);
        terrain("swamp", Tags.Biomes.IS_SWAMP);
        terrain("snowy", Tags.Biomes.IS_SNOWY);
        terrain("taiga", BiomeTags.IS_TAIGA);
        terrain("forest", BiomeTags.IS_FOREST, Tags.Biomes.IS_FOREST);
        terrain("savanna", Tags.Biomes.IS_SAVANNA, BiomeTags.IS_SAVANNA);
        terrain("plains", Tags.Biomes.IS_PLAINS);
    }

    /**
     * The biome at a block without loading a chunk or waiting for one. {@code ServerLevel.getBiome} asks
     * {@code getChunk(..., BIOMES, false)}, which waits on a generation future when a chunk is half made; so
     * a chunk that is loaded (full) gives its own biome, and anything else the generator's biome source
     * ({@code getUncachedNoiseBiome}, which reads no chunk). Checked in the 1.21.1 sources.
     */
    static Holder<Biome> biomeAt(ServerLevel level, BlockPos pos) {
        int qx = QuartPos.fromBlock(pos.getX()), qy = QuartPos.fromBlock(pos.getY()), qz = QuartPos.fromBlock(pos.getZ());
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        return chunk != null ? chunk.getNoiseBiome(qx, qy, qz) : level.getUncachedNoiseBiome(qx, qy, qz);
    }

    /** The terrain name ({@code loc.<name>}) of the biome at a block, or null. */
    static String terrainAt(ServerLevel level, BlockPos pos) {
        Holder<Biome> biome = biomeAt(level, pos);
        for (Map.Entry<String, List<TagKey<Biome>>> e : TERRAIN.entrySet())
            for (TagKey<Biome> tag : e.getValue())
                if (biome.is(tag))
                    return e.getKey();
        return null;
    }

    /**
     * The locator of a place {@code dx}, {@code dz} from the talker: "in the hills to the east" (terrain and
     * compass), "to the east" (no terrain matched), or just "in the hills" when it is within a block's
     * stone throw ({@link TalkTuning#HERE}); null when it is right here and no terrain matched.
     */
    static String locator(Env env, int dx, int dz) {
        BlockPos at = env.talker();
        // At sea level, not the talker's height: a talker in a mine would describe every far place as a cave.
        int y = Mth.clamp(env.level().getSeaLevel(), env.level().getMinBuildHeight(), env.level().getMaxBuildHeight() - 1);
        String terrain = terrainAt(env.level(), new BlockPos(at.getX() + dx, y, at.getZ() + dz));
        String land = terrain == null || !has(ROOT + "loc." + terrain) ? null : english(ROOT + "loc." + terrain);
        if (TalkTuning.distance(dx, dz) < TalkTuning.HERE)
            return land;
        String dir = english(ROOT + "dir." + TalkTuning.compass(dx, dz));
        return land != null ? fmt(ROOT + "loc.terrain_dir", land, dir) : fmt(ROOT + "loc.dir", dir);
    }
}
