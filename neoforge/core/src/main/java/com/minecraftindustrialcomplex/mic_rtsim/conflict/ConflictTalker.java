package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.api.TalkCandidates;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkContext;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Lines;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talkers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.monster.Ravager;

import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The core's talker for conflict bodies (TALK.md §4, §5.4, §5.5; CL-1): a body talks only while it is a
 * live member of a party the bridge holds, and only when a player crouch-clicks it (TALK's gesture) or
 * clicks the envoy ({@link ConflictBridge#interact}), so this is player-initiated chat and nothing else in
 * the conflict sends words (MZB-101).
 *
 * <p>The envoy (voice {@code envoy.<culture>}) says its demand with its reason, the same line every time
 * while it is open, then why it takes nothing (its settlement, a blow, its deadline); only to the player
 * the demand is to (anyone for the settlers'). Every other member (voice {@code <culture>}) warns, with the reason when the camp has one,
 * or insults in a raid or a rout. Beasts (an {@link Animal}, a ravager) do not talk. Holder: the faction
 * (3); no small talk, no credit (TK6).
 */
public final class ConflictTalker implements Talker {
    static final ConflictTalker INSTANCE = new ConflictTalker();

    private ConflictTalker() {}

    private static final Set<EntityType<?>> REGISTERED = ConcurrentHashMap.newKeySet();

    /**
     * Registers the talker for a body's entity type the first time the bridge resolves it (MZB-120): a
     * built-in (asked after any plugin's talker for the type), not always talking, so the client never
     * cancels a click on that type and the server decides per entity.
     */
    static void ensure(EntityType<?> type) {
        if (REGISTERED.add(type))
            Talkers.registerBuiltin(BuiltInRegistries.ENTITY_TYPE.getKey(type), INSTANCE, false);
    }

    private static final String CONFLICT_PREFIX = "mic_rtsim:conflict/";

    /** The culture of a body kind named {@code mic_rtsim:conflict/<culture>/<unit>}, or null. */
    static String culture(String body) {
        if (body == null || !body.startsWith(CONFLICT_PREFIX))
            return null;
        String rest = body.substring(CONFLICT_PREFIX.length());
        int slash = rest.indexOf('/');
        String c = slash < 0 ? rest : rest.substring(0, slash);
        return c.matches("[a-z0-9_]+") ? c : null;
    }

    private static boolean beast(Entity e) {
        return e instanceof Animal || e instanceof Ravager;
    }

    /** The live member an entity is, if it may talk to this player now; else null. */
    private static ConflictBridge.Slot member(Entity entity, ServerPlayer player) {
        ConflictBridge.Slot sl = ConflictBridge.INSTANCE.live(entity);
        if (sl == null || !sl.alive || beast(entity))
            return null;
        ConflictBridge.Party p = sl.stack.party;
        if (sl.envoy) {
            if (p.player != null && !p.player.equals(player.getUUID()))
                return null;
            if (p.demand == null && p.settled == 0)
                return null;
        }
        return sl;
    }

    @Override
    public boolean talks(Entity entity, ServerPlayer player) {
        return member(entity, player) != null;
    }

    @Override
    public String voice(Entity entity, ServerPlayer player) {
        ConflictBridge.Slot sl = ConflictBridge.INSTANCE.live(entity);
        String c = sl == null ? null : culture(sl.body);
        if (sl != null && sl.envoy)
            return c == null ? "envoy" : "envoy." + c;
        return c == null ? "conflict" : c;
    }

    /** "Goblin Envoy" ("Envoy" for a body kind of no culture): the envoy as it speaks and as it is named (MZB-101). */
    static Component envoyName(String body) {
        String c = culture(body);
        return c == null ? Talk.tr("mic_rtsim.talk.speaker.envoy.plain") : Talk.tr("mic_rtsim.talk.speaker.envoy", cultureName(c));
    }

    /** "Goblin Envoy"; "Goblin"; a named war chief by its name. */
    @Override
    public Component speaker(Entity entity, ServerPlayer player) {
        ConflictBridge.Slot sl = ConflictBridge.INSTANCE.live(entity);
        String c = sl == null ? null : culture(sl.body);
        if (sl != null && sl.envoy)
            return envoyName(sl.body);
        if (sl != null && sl.chief && entity.hasCustomName())
            return Talk.tr("mic_rtsim.talk.speaker.named", entity.getCustomName());
        return c == null ? entity.getType().getDescription() : cultureName(c);
    }

    /** The culture's word ("Goblin"); a culture with no lang key (a plugin's) by its name, capitalised. */
    private static Component cultureName(String culture) {
        String key = "mic_rtsim.talk.culture." + culture;
        if (Language.getInstance().has(key))
            return Talk.tr(key);
        String plain = culture.replace('_', ' ');
        return Component.literal(Character.toUpperCase(plain.charAt(0)) + plain.substring(1));
    }

    @Override
    public TalkHolder holder(Entity entity) {
        ConflictBridge.Slot sl = ConflictBridge.INSTANCE.live(entity);
        return sl == null ? null : new TalkHolder(TalkHolder.FACTION, sl.stack.party.faction);
    }

    @Override
    public Attitude attitude(Entity entity, ServerPlayer player) {
        ConflictBridge.Slot sl = ConflictBridge.INSTANCE.live(entity);
        if (sl == null)
            return Attitude.WARY;
        ConflictBridge.Party p = sl.stack.party;
        if (sl.envoy)
            return Attitude.WARY;
        if (insults(p) || player.getUUID().equals(p.targetPlayer)
                || ConflictBridge.INSTANCE.reasonFor(p.faction, player.getUUID()) >= 0)
            return Attitude.HOSTILE;
        return Attitude.WARY;
    }

    /** A raid is on (a band out to fight, or one attacking), or it is losing a fight (routed). */
    private static boolean insults(ConflictBridge.Party p) {
        if (p.envoy)
            return false;
        return p.attack || (p.flags & ConflictRecords.Materialize.FLAG_HOSTILE) != 0 || p.purpose >= ConflictRecords.PURPOSE_RAID
                || (p.recalling && p.recallReason == ConflictRecords.RECALL_ROUTED);
    }

    @Override
    public boolean chats(Entity entity, ServerPlayer player) {
        return false;
    }

    @Override
    public void candidates(TalkContext ctx, TalkCandidates out) {
        ConflictBridge.Slot sl = member(ctx.entity(), ctx.player());
        if (sl == null)
            return;
        ConflictBridge.Party p = sl.stack.party;
        if (sl.envoy) {
            envoy(ctx, p, voice(ctx.entity(), ctx.player()), out);
            return;
        }
        if (insults(p)) {
            out.add("hostile.insult", 1);
            return;
        }
        int reason = ConflictBridge.INSTANCE.reasonFor(p.faction, ctx.player().getUUID());
        if (reason >= 0)
            out.add("hostile.warn." + signalKey(reason), 1, ConflictBridge.signalWord(reason));
        else
            out.add("hostile.warn", 1);
    }

    /**
     * The envoy's one line (TALK.md §5.5, MZB-121): while the demand is open, {@code envoy.demand.<form>}
     * (tribute, stop, or tribute_or_stop) with (1) the reason, (2) how many are still owed, (3) the good,
     * (4) the signal to stop and (5) the deadline; once it takes nothing, {@code envoy.settled.<why>}
     * ({@link ConflictBridge#closed}: the settlement's outcome, struck, expired, or paid before the engine
     * settles).
     */
    private static void envoy(TalkContext ctx, ConflictBridge.Party p, String voice, TalkCandidates out) {
        ConflictBridge.Demand d = p.demand;
        String closed = ConflictBridge.closed(p, ctx.gameTime());
        if (closed != null) {
            out.add("envoy.settled." + closed, 1);
            return;
        }
        boolean tribute = (d.flags & ConflictRecords.Envoy.DEMAND_TRIBUTE) != 0 && d.remaining > 0 && d.itemName != null;
        boolean stop = (d.flags & ConflictRecords.Envoy.DEMAND_STOP) != 0;
        String form = tribute && stop ? "tribute_or_stop" : tribute ? "tribute" : "stop";
        Component item = !tribute ? Component.empty() : d.item != null ? d.item.getDescription() : Component.literal(d.itemName);
        out.add("envoy.demand." + form, 1, reason(voice, d.signal), d.remaining, item, ConflictBridge.signalWord(d.signal),
                deadline(ctx.level(), d.deadline, ctx.gameTime()));
    }

    /** The reason, as the envoy's voice words it: the most specific level's first line, the same every time. */
    private static Component reason(String voice, int signal) {
        Lines.Found f = Lines.find(voice, "envoy.reason." + signalKey(signal));
        if (f == null)
            f = Lines.find(voice, "envoy.reason");
        return f == null ? Component.empty() : Talk.tr(f.key(0));
    }

    static String signalKey(int signal) {
        return ConflictRecords.signalName(signal).replace(' ', '_');
    }

    static String outcomeKey(int outcome) {
        return switch (outcome) {
            case ConflictRecords.SETTLED_PAID -> "paid";
            case ConflictRecords.SETTLED_CUT -> "cut";
            case ConflictRecords.SETTLED_EXPIRED -> "expired";
            case ConflictRecords.SETTLED_REFUSED -> "refused";
            case ConflictRecords.SETTLED_WITHDRAWN -> "withdrawn";
            default -> "other";
        };
    }

    /** Dawn, noon, dusk and midnight: the day time each quarter of a day starts at. */
    private static final String[] PARTS = {"dawn", "noon", "dusk", "midnight"};
    private static final long QUARTER = 6000L, DAY = 24000L;

    /**
     * The deadline as an envoy says it (MZB-123): by the last of dawn, noon, dusk or midnight at or before
     * it ("by dusk tomorrow", "by noon, 3 days from now"), never later than it; "at once" when that mark
     * is already past. Deadlines are game time; the marks are the level's day time.
     */
    static Component deadline(ServerLevel level, long deadline, long now) {
        long left = deadline - now;
        long today = level.getDayTime();
        long at = today + left;
        long mark = Math.floorDiv(at, QUARTER) * QUARTER;
        if (left <= 0 || mark <= today)
            return Talk.tr("mic_rtsim.talk.deadline.now");
        Component part = Talk.tr("mic_rtsim.talk.deadline.part." + PARTS[(int) Math.floorMod(mark / QUARTER, 4L)]);
        long days = Math.floorDiv(mark, DAY) - Math.floorDiv(today, DAY);
        if (days <= 0)
            return Talk.tr("mic_rtsim.talk.deadline.today", part);
        if (days == 1)
            return Talk.tr("mic_rtsim.talk.deadline.tomorrow", part);
        return Talk.tr("mic_rtsim.talk.deadline.days", part, days);
    }
}
