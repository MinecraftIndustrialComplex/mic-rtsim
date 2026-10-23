package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkCandidates;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkContext;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.tags.BiomeTags;
import net.minecraft.tags.TagKey;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.monster.Enemy;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.entity.raid.Raid;
import net.neoforged.neoforge.common.Tags;

import java.util.ArrayList;
import java.util.Deque;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The talk service (TALK.md §5, §7): gathers what a talker could say, picks one line, sends it to the
 * talking player and to nobody else, then lets the talker react and earns the credit. Server thread
 * only; its memory (rate limits, each player's last spoken keys) is never saved.
 */
public final class Talk {
    private Talk() {}

    private static final RateLimit LIMIT = new RateLimit();
    private static final Map<UUID, Deque<String>> RECENT = new HashMap<>();
    private static final int RECENT_BOUND = 256;

    /** The game day length used by the credit cap (TK1): game time, so /time set and sleeping do not reset it. */
    public static final long DAY = 24000L;

    // ---- text ----------------------------------------------------------------------------------

    /** The server's English for a key, which is also the fallback a client without the lang file shows. */
    public static String english(String key) {
        return Language.getInstance().getOrDefault(key);
    }

    /** A line of ours as a component: translated by the client, the server's English when it cannot. */
    public static Component tr(String key, Object... args) {
        return Component.translatableWithFallback(key, english(key), args);
    }

    // ---- speaking ------------------------------------------------------------------------------

    /**
     * The talk itself: the rate limits, one line to this player, the talker's reaction and the credit.
     * Returns true when a line was sent.
     */
    public static boolean speak(Talker talker, Entity entity, ServerPlayer player) {
        ServerLevel level = player.serverLevel();
        long now = level.getGameTime();
        if (!LIMIT.tryAcquire(entity.getUUID(), player.getUUID(), now))
            return false;
        Picker.Pick pick = choose(talker, entity, player, level, now);
        if (pick == null)
            return false;
        Component line = Component.translatableWithFallback(pick.key(), english(pick.key()), pick.option().args());
        player.sendSystemMessage(tr("mic_rtsim.talk.say", talker.speaker(entity, player), line));
        remember(player.getUUID(), pick.key());
        try {
            talker.react(entity, player);
        } catch (Throwable e) {
            failed(talker, "react", e);
        }
        credit(talker, entity, player, now);
        return true;
    }

    private static void credit(Talker talker, Entity entity, ServerPlayer player, long now) {
        try {
            if (!talker.credits(entity, player))
                return;
            long day = now / DAY;
            TalkDays days = entity.getData(RtsimAttachments.TALK_DAYS);
            if (days.creditedOn(player.getUUID(), day))
                return;
            talker.credit(entity, player);
            entity.setData(RtsimAttachments.TALK_DAYS, days.with(player.getUUID(), day));
        } catch (Throwable e) {
            failed(talker, "credit", e);
        }
    }

    private static final java.util.Set<Class<?>> REPORTED = java.util.concurrent.ConcurrentHashMap.newKeySet();

    /** A talker threw: logged once per talker class with the stack trace; the click stays cancelled. */
    public static void failed(Object talker, String what, Throwable e) {
        if (REPORTED.add(talker.getClass()))
            MicRtsim.LOGGER.error("TALK: {} threw in {} (reported once per class; the click stays cancelled)", talker.getClass().getName(), what, e);
    }

    private static void remember(UUID player, String key) {
        if (RECENT.size() >= RECENT_BOUND && !RECENT.containsKey(player))
            RECENT.clear();
        Picker.remember(RECENT.computeIfAbsent(player, p -> Picker.newHistory()), key);
    }

    /** Each lower level of a voice counts this much of the level above it (TK31). */
    public static final double PARENT_WEIGHT = 0.6;

    /** Gathers the candidates and picks one; null when the talker's voice has nothing to say. */
    static Picker.Pick choose(Talker talker, Entity entity, ServerPlayer player, ServerLevel level, long now) {
        Talker.State state = Talker.State.NONE;
        try {
            state = talker.state(entity, player);
        } catch (Throwable e) {
            failed(talker, "state", e);
        }
        Talker.Attitude attitude = talker.attitude(entity, player);
        TalkContext ctx = new TalkContext(entity, player, level, now, state, attitude, talker.home(entity));
        TalkCandidates out = new TalkCandidates();
        if (!talker.chats(entity, player)) {
            ask(talker, ctx, out);
        } else if (state == Talker.State.ASLEEP) {
            out.add("state.asleep", 1);
        } else if (state == Talker.State.BUSY) {
            out.add("state.busy", 1);
        } else {
            fluff(ctx, out);
            mood(ctx, out);
            if (state == Talker.State.HURT)
                out.add("state.hurt", 3);
            ask(talker, ctx, out);
        }
        String voice = talker.voice(entity, player);
        List<Picker.Option> options = new ArrayList<>();
        for (TalkCandidates.Candidate c : out.list()) {
            double w = c.weight();
            for (Lines.Found found : Lines.levels(voice, c.topic())) {
                options.add(new Picker.Option(c.topic(), found.prefix(), found.count(), w, c.args()));
                w *= PARENT_WEIGHT;
            }
        }
        List<String> recent = List.copyOf(RECENT.getOrDefault(player.getUUID(), Picker.newHistory()));
        return Picker.pick(options, recent, new java.util.SplittableRandom(level.random.nextLong()));
    }

    private static void ask(Talker talker, TalkContext ctx, TalkCandidates out) {
        try {
            talker.candidates(ctx, out);
        } catch (Throwable e) {
            failed(talker, "candidates", e);
        }
    }

    // ---- looking at the player ---------------------------------------------------------------------

    private record Look(Mob mob, ServerPlayer player, long until) {}

    private static final Map<UUID, Look> LOOKS = new HashMap<>();
    private static final int LOOK_BOUND = 64;

    /** Keeps a goal-driven mob looking at the player for this many ticks (the server tick calls {@link #tickLooks}). */
    public static void lookAt(Mob mob, ServerPlayer player, int ticks) {
        mob.getLookControl().setLookAt(player);
        if (LOOKS.size() >= LOOK_BOUND && !LOOKS.containsKey(mob.getUUID()))
            LOOKS.values().removeIf(l -> !l.mob.isAlive() || l.mob.level().getGameTime() >= l.until);
        if (LOOKS.size() < LOOK_BOUND || LOOKS.containsKey(mob.getUUID()))
            LOOKS.put(mob.getUUID(), new Look(mob, player, mob.level().getGameTime() + ticks));
    }

    public static void tickLooks() {
        if (LOOKS.isEmpty())
            return;
        LOOKS.values().removeIf(l -> {
            if (!l.mob.isAlive() || l.mob.isRemoved() || !l.player.isAlive() || l.mob.level() != l.player.level()
                    || l.mob.level().getGameTime() >= l.until)
                return true;
            l.mob.getLookControl().setLookAt(l.player);
            return false;
        });
    }

    // ---- the core's candidates (TALK.md §5.1, §5.2) ----------------------------------------------

    private static final Map<String, List<TagKey<Biome>>> BIOMES = new java.util.LinkedHashMap<>();

    static {
        biome("hot", Tags.Biomes.IS_HOT);
        biome("cold", Tags.Biomes.IS_COLD);
        biome("wet", Tags.Biomes.IS_WET);
        biome("dry", Tags.Biomes.IS_DRY);
        biome("ocean", BiomeTags.IS_OCEAN, Tags.Biomes.IS_OCEAN);
        biome("mountain", Tags.Biomes.IS_MOUNTAIN, BiomeTags.IS_MOUNTAIN);
        biome("forest", BiomeTags.IS_FOREST, Tags.Biomes.IS_FOREST);
        biome("plains", Tags.Biomes.IS_PLAINS);
        biome("swamp", Tags.Biomes.IS_SWAMP);
        biome("desert", Tags.Biomes.IS_DESERT);
        biome("snowy", Tags.Biomes.IS_SNOWY);
        biome("jungle", BiomeTags.IS_JUNGLE);
        biome("savanna", Tags.Biomes.IS_SAVANNA, BiomeTags.IS_SAVANNA);
        biome("taiga", BiomeTags.IS_TAIGA);
        biome("badlands", BiomeTags.IS_BADLANDS);
    }

    @SafeVarargs
    private static void biome(String name, TagKey<Biome>... tags) {
        BIOMES.put("fluff." + name, List.of(tags));
    }

    private static void fluff(TalkContext ctx, TalkCandidates out) {
        ServerLevel level = ctx.level();
        BlockPos pos = ctx.entity().blockPosition();
        out.add("fluff", 1.0);
        long t = level.getDayTime() % DAY;
        boolean day = false;
        if (t >= 23000 || t < 1000)
            out.add("fluff.dawn", 0.5);
        else if (t < 11500) {
            day = true;
            out.add("fluff.day", 0.5);
        } else if (t < 13000)
            out.add("fluff.dusk", 0.5);
        else
            out.add("fluff.night", 0.5);

        // Weather means something only under a sky: a dimension with sky light and no ceiling.
        if (level.dimensionType().hasSkyLight() && !level.dimensionType().hasCeiling()) {
            if (!level.isRaining()) {
                if (day)
                    out.add("fluff.clear", 0.5);
            } else if (level.canSeeSky(pos)) {
                Biome.Precipitation p = level.getBiome(pos).value().getPrecipitationAt(pos);
                if (p == Biome.Precipitation.SNOW)
                    out.add("fluff.snow", 0.5);
                else if (p == Biome.Precipitation.RAIN) {
                    out.add("fluff.rain", 0.5);
                    if (level.isThundering())
                        out.add("fluff.thunder", 0.5);
                }
            }
        }

        Holder<Biome> biome = level.getBiome(pos);
        for (Map.Entry<String, List<TagKey<Biome>>> e : BIOMES.entrySet())
            for (TagKey<Biome> tag : e.getValue())
                if (biome.is(tag)) {
                    out.add(e.getKey(), 0.5);
                    break;
                }
        if (ctx.home() != null)
            out.add("fluff.home", 0.5, ctx.home());
    }

    private static void mood(TalkContext ctx, TalkCandidates out) {
        ServerLevel level = ctx.level();
        BlockPos pos = ctx.entity().blockPosition();
        Raid raid = level.getRaidAt(pos);
        if (raid != null && raid.isActive() && !raid.isOver())
            out.add("mood.raid", 6);
        long t = level.getDayTime() % DAY;
        if (t >= 13000 && t < 23000 && !level.getEntitiesOfClass(LivingEntity.class,
                ctx.entity().getBoundingBox().inflate(16), e -> e instanceof Enemy && e.isAlive()).isEmpty())
            out.add("mood.monsters", 3);
    }

    // ---- housekeeping ----------------------------------------------------------------------------

    /** A player left: forget his last lines. */
    public static void forget(UUID player) {
        RECENT.remove(player);
    }

    /** Clears every in-memory state (server start and stop). */
    public static void clear() {
        LIMIT.clear();
        RECENT.clear();
        LOOKS.clear();
        Lines.clear();
        Names.clear();
    }
}
