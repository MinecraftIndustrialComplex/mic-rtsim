package com.minecraftindustrialcomplex.mic_rtsim_ribbits;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkCandidates;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkContext;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.knowledge.Sentiments;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Names;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDays;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDaysCodec;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.yungnickyoung.minecraft.ribbits.data.RibbitData;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

import java.nio.ByteBuffer;
import java.util.UUID;
import java.util.function.Supplier;

/**
 * Ribbits talk (TALK.md §4, §7; slice 3). Crouch and right-click one with an empty hand and it answers in
 * the voice of its profession, and a talk with a resident of an adopted ribbit village earns that village a
 * little sentiment toward the player (TK2).
 *
 * <ul>
 *   <li><b>Voice</b> {@code ribbit.<profession>}; a nitwit that plays in a touring band is
 *       {@code ribbit.musician}; a ribbit with no profession data is plain {@code ribbit}.</li>
 *   <li><b>Holder</b> its village for a resident, the caravan's own board for a merchant puppet, none for a
 *       band's musician (the engine keeps no board for a band, TK38) and for a ribbit with no village.</li>
 *   <li><b>Attitude</b> wary toward a player its village dislikes (the sorcerer buff filter's own test).</li>
 *   <li><b>Credit</b> one TALKED (journaled, applied by the engine once by op id) per village, player and
 *       game day, capped by an attachment on the <i>player</i> ({@link #DAYS}): the credit lands on the
 *       village's sentiment, which every ribbit of the village shares, so a cap per ribbit would let a
 *       player talk to ten ribbits for +30 (TK60).</li>
 * </ul>
 */
public final class RibbitTalker implements Talker {
    /** The sentiment-bearing modules phase 1 needs (the engine's MODULES_PHASE1, bits 0 to 3): without them it ignores TALKED. */
    private static final Module[] PHASE1 = {Module.KNOWLEDGE, Module.SETTLEMENTS, Module.TRADE, Module.TRAVEL};

    private static final DeferredRegister<AttachmentType<?>> TYPES =
            DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, MicRtsimRibbits.MODID);

    /**
     * {@code mic_rtsim_ribbits:talk_days}, on the player: for each village, the last game day a talk with one
     * of its ribbits was credited (TK60). The core's {@link TalkDays} keeps the map and its codec, with a
     * village's id as the key (see {@link #key}); at most 16 villages, the oldest dropped. Saved with the
     * player and kept when he dies.
     */
    public static final Supplier<AttachmentType<TalkDays>> DAYS = TYPES.register("talk_days",
            () -> AttachmentType.builder(() -> TalkDays.EMPTY)
                    .serialize(TalkDaysCodec.CODEC, d -> !d.isEmpty()).copyOnDeath().build());

    static void register(IEventBus modBus) {
        TYPES.register(modBus);
    }

    /** A village's key in a player's {@link #DAYS}: its id in the low bits of an otherwise empty UUID. */
    public static UUID key(long village) {
        return new UUID(0L, village);
    }

    // ---- who it is ------------------------------------------------------------------------------

    /** The profession's path ({@code merchant}), or null when the ribbit has no profession data. */
    static String profession(RibbitEntity r) {
        RibbitData d = r.getRibbitData();
        return d == null || d.getProfession() == null ? null : d.getProfession().getId().getPath();
    }

    /** True for a nitwit that is a member of a touring band: a musician on the road. */
    private static boolean musician(RibbitEntity r) {
        return "nitwit".equals(profession(r)) && RibbitBands.band(r) != null;
    }

    /**
     * The adopted village a ribbit lives in, or null. A puppet (a caravan's merchant, a band's musician) is
     * a visitor wherever it stands and has no village here: the core's {@code villageOf} would name the
     * village it is passing through.
     */
    private static Village village(Entity ribbit) {
        return RtsimApi.group(ribbit) != null ? null : RtsimApi.villageOf(ribbit);
    }

    @Override
    public String voice(Entity entity, ServerPlayer player) {
        RibbitEntity r = (RibbitEntity) entity;
        String p = profession(r);
        if (r.isBaby() || p == null)
            return "ribbit";
        return musician(r) ? "ribbit.musician" : "ribbit." + p;
    }

    @Override
    public String home(Entity entity) {
        Village v = village(entity);
        return v == null ? null : Names.village(v.id);
    }

    /** The profession as the speaker line names it; a profession the lang does not know is its path, capitalised. */
    private static Component professionName(RibbitEntity r) {
        String p = musician(r) ? "musician" : profession(r);
        if (p == null)
            return Talk.tr("mic_rtsim.talk.ribbit.profession.nitwit");
        String key = "mic_rtsim.talk.ribbit.profession." + p;
        return Language.getInstance().has(key) ? Talk.tr(key)
                : Component.literal(Character.toUpperCase(p.charAt(0)) + p.substring(1));
    }

    @Override
    public Component speaker(Entity entity, ServerPlayer player) {
        RibbitEntity r = (RibbitEntity) entity;
        if (r.hasCustomName())
            return Talk.tr("mic_rtsim.talk.speaker.named", r.getCustomName());
        Component profession = professionName(r);
        if (village(r) != null) {
            // Stable from its UUID, so every player and every session hears the same name (a name a ribbit keeps
            // for life, the way a village keeps its own).
            UUID id = r.getUUID();
            String name = Names.camp("ribbit", id.getMostSignificantBits() ^ id.getLeastSignificantBits());
            if (name != null)
                return Talk.tr("mic_rtsim.talk.speaker.ribbit", name, profession);
        }
        return Talk.tr("mic_rtsim.talk.speaker.ribbit.unplaced", profession);
    }

    // ---- what it knows ---------------------------------------------------------------------------

    /**
     * A resident asks the engine what its village knows (TK49); a merchant that is a caravan's puppet speaks
     * for the caravan, as a wandering trader does. A band's musician has no board: the engine keeps none for a
     * band (it answers a band's agent with no HOLDER, TK38), so it speaks at once. A baby has none (TK16).
     */
    @Override
    public TalkHolder holder(Entity entity) {
        if (((RibbitEntity) entity).isBaby())
            return null;
        PuppetGroup group = RtsimApi.group(entity);
        if (group != null)
            return RibbitMerchantAdapter.NAME.equals(group.kind.name()) ? new TalkHolder(TalkHolder.CARAVAN, group.agent) : null;
        Village v = RtsimApi.villageOf(entity);
        return v == null ? null : new TalkHolder(TalkHolder.VILLAGE, v.id);
    }

    /** The village's sentiment snapshot toward players, or null when it has none (no village, or the engine sent none). */
    private static Sentiments.Snapshot feeling(Entity ribbit) {
        Village v = village(ribbit);
        return v == null ? null : RtsimApi.sentiment(v.id);
    }

    @Override
    public Attitude attitude(Entity entity, ServerPlayer player) {
        Sentiments.Snapshot s = feeling(entity);
        return s != null && s.dislikes(player.getUUID()) ? Attitude.WARY : Attitude.FRIENDLY;
    }

    /**
     * A village that likes the player says so; one that dislikes him says that (TK61). The sorcerers have their
     * own lines for both ({@code ribbit.sorcerer.mood.*}); the voice chain picks them up under the same topic.
     */
    @Override
    public void candidates(TalkContext ctx, TalkCandidates out) {
        Sentiments.Snapshot s = feeling(ctx.entity());
        if (s == null)
            return;
        UUID player = ctx.player().getUUID();
        if (s.friend(player))
            out.add("mood.liked", 1.5);
        else if (s.dislikes(player))
            out.add("mood.disliked", 1.5);
    }

    /**
     * As a villager: asleep and hurt as the core decides them (Ribbits has no sleeping goal, so a ribbit is
     * asleep only if something else put it to bed). Busy is also a ribbit playing in a band and a sorcerer
     * casting its buff; a gardener watering and a fisherman fishing still stop to chat (TK62).
     */
    @Override
    public State state(Entity entity, ServerPlayer player) {
        State base = Talker.super.state(entity, player);
        if (base == State.ASLEEP || base == State.BUSY)
            return base;
        RibbitEntity r = (RibbitEntity) entity;
        return r.getPlayingInstrument() || r.getBuffing() ? State.BUSY : base;
    }

    // ---- the credit ------------------------------------------------------------------------------

    /**
     * The running session when the engine can apply a TALKED, else null: no session, a failed one, or phase 1
     * (the four modules above) off. A talk then earns nothing and spends nothing.
     */
    private static RtsimSession applying() {
        RtsimSession s = RtsimSession.current();
        if (s == null || s.failed())
            return null;
        for (Module m : PHASE1)
            if (!RtsimConfig.on(m))
                return null;
        return s;
    }

    private static long today(ServerPlayer player) {
        return player.serverLevel().getGameTime() / Talk.DAY;
    }

    /**
     * Whether this talk can earn the village a credit: a resident of an adopted village that is not asleep,
     * busy or a baby, a real player, the engine able to take it, and this player not yet credited by the
     * village today. A puppet gives none (TK63): it is far from home and its village did not see the chat.
     */
    @Override
    public boolean credits(Entity entity, ServerPlayer player) {
        RibbitEntity r = (RibbitEntity) entity;
        Village v = village(r);
        if (v == null || r.isBaby() || player.isFakePlayer() || applying() == null)
            return false;
        State s = state(entity, player);
        if (s != State.NONE && s != State.HURT)
            return false;
        return !player.getData(DAYS).creditedOn(key(v.id), today(player));
    }

    /**
     * The day first, then the record: a credit that throws after the day is written cannot credit on every
     * click (the core's own order). The core calls this at most once a game day per player and ribbit; the
     * village's own cap is the one kept here.
     */
    @Override
    public void credit(Entity entity, ServerPlayer player) {
        Village v = village(entity);
        RtsimSession session = applying();
        if (v == null || session == null)
            return;
        long day = today(player);
        TalkDays days = player.getData(DAYS);
        if (days.creditedOn(key(v.id), day))
            return;
        player.setData(DAYS, days.with(key(v.id), day));
        journal(session, v.id, player, entity);
    }

    /** One TALKED, journaled with a fresh op id like the core's other observations (docs/boundary.md §13.7). */
    static long journal(RtsimSession session, long village, ServerPlayer player, Entity ribbit) {
        int body = session.dictId(Obs.DICT_BODY, BuiltInRegistries.ENTITY_TYPE.getKey(ribbit.getType()).toString());
        ByteBuffer r = session.boundary().beginJournaled(TalkRecords.TALKED, TalkRecords.Talked.SIZE, 0, session.gameTime());
        TalkRecords.writeTalked(r, new TalkRecords.Talked(village, player.getUUID().getMostSignificantBits(),
                player.getUUID().getLeastSignificantBits(), body));
        return session.boundary().commitWithOpId(r, TalkRecords.Talked.OP_ID);
    }
}
