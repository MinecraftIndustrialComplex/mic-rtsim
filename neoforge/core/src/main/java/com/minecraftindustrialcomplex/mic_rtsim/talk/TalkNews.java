package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.TalkCandidates;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkMoods.Mood;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.News;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

import java.util.List;

/**
 * The engine's answer as candidates (TALK.md §5.2, §5.3, TK49 to TK56): the moods its facts give and one
 * candidate per news item, with the arguments of the line each would be told with. The weights are
 * {@link TalkTuning}'s.
 */
final class TalkNews {
    private TalkNews() {}

    /** The ordinal arguments of a news line (§8): %1$s who, %2$s whom, %3$s when, %4$s where. */
    static final int WHO = 0, WHOM = 1, WHEN = 2, WHERE = 3;

    /** Adds the moods the answer's facts give (safety, feud, dragon, prices, hunger, the port). */
    static void moods(Refs.Env env, String voice, TalkCandidates out) {
        TalkAnswer answer = env.answer();
        for (Mood m : TalkMoods.of(answer.facts(), id -> isFood(answer.name(Obs.DICT_ITEM, id)))) {
            if (Lines.levels(voice, m.topic()).isEmpty())
                continue;
            switch (m.arg()) {
                case NONE -> out.add(m.topic(), m.weight());
                case SCARCEST -> {
                    Component good = itemName(answer.scarcest());
                    if (good != null)
                        out.add(m.topic(), m.weight(), good);
                }
                case GLUTTED -> {
                    Component good = itemName(answer.glutted());
                    if (good != null)
                        out.add(m.topic(), m.weight(), good);
                }
                case WHO -> out.add(m.topic(), m.weight(),
                        Refs.tell(env, m.source(), m.source().who(), false, false, m.source().has(TalkRecords.YOU_DID)));
            }
        }
    }

    /** Adds one candidate per news item whose topic has lines in the voice. */
    static void news(Refs.Env env, String voice, TalkCandidates out) {
        List<News> items = env.answer().facts().news();
        for (News n : items) {
            String kind = TalkTuning.kindName(n.kind());
            if (kind == null)
                continue;
            String chosen = null, topic = null;
            for (String p : TalkTuning.perspectives(n.talkFlags())) {
                String base = "news." + kind + "." + p;
                String t = n.has(TalkRecords.SEEN) && !Lines.levels(voice, base + ".seen").isEmpty() ? base + ".seen" : base;
                if (!Lines.levels(voice, t).isEmpty()) {
                    chosen = p;
                    topic = t;
                    break;
                }
            }
            if (topic == null)
                continue;
            // An item about the holder told in the third person (the topic fell back to "other"): its own place reads "us".
            boolean us = chosen.equals("other") && (n.has(TalkRecords.TO_US) || n.has(TalkRecords.BY_US));
            Object who = Refs.tell(env, n, n.who(), n.has(TalkRecords.BY_US), us, n.has(TalkRecords.YOU_DID));
            Object whom = Refs.tell(env, n, n.whom(), n.has(TalkRecords.TO_US), us, n.has(TalkRecords.TO_YOU));
            Component when = Talk.tr(Lines.ROOT + "when." + TalkTuning.when(n.ago()));
            Component where = where(env, n);
            out.add(topic, TalkTuning.newsWeight(n.talkFlags(), n.ago()), who, whom, when, where);
        }
    }

    /**
     * Where the item happened, from the talker: "a short walk north", "right here", "somewhere far from here"
     * when the engine has no place for it (another frame, TK34), and with the village it is near when the
     * talker would name it ("a short walk north, near Oakford").
     */
    static Component where(Refs.Env env, News n) {
        if (n.dx() == 0 && n.dz() == 0)
            return Talk.tr(Lines.ROOT + "where.elsewhere");
        String band = TalkTuning.where(n.dx(), n.dz());
        Component phrase = band.equals("here") ? Talk.tr(Lines.ROOT + "where.here")
                : Talk.tr(Lines.ROOT + "where." + band, Talk.tr(Lines.ROOT + "dir." + TalkTuning.compass(n.dx(), n.dz())));
        TalkRecords.Ref near = n.near();
        if (near.cls() != TalkRecords.C_VILLAGE || band.equals("here") || sameVillage(near, n.who()) || sameVillage(near, n.whom()))
            return phrase;
        boolean own = env.holder() != null && env.holder().kind() == com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder.VILLAGE
                && near.a() == env.holder().id();
        boolean named = switch (Refs.force) {
            case NAMED -> true;
            case DESCRIBED -> false;
            case NONE -> Naming.named(own || n.has(TalkRecords.SEEN), near.hasPos(), near.dx(), near.dz(), env.rnd());
        };
        return named ? Talk.tr(Lines.ROOT + "where.near", phrase, Names.village(near.a())) : phrase;
    }

    private static boolean sameVillage(TalkRecords.Ref a, TalkRecords.Ref b) {
        return a.cls() == TalkRecords.C_VILLAGE && b.cls() == TalkRecords.C_VILLAGE && a.a() == b.a();
    }

    // ---- goods ---------------------------------------------------------------------------------------

    private static Item item(String id) {
        ResourceLocation rl = id == null ? null : ResourceLocation.tryParse(id);
        return rl == null ? null : BuiltInRegistries.ITEM.getOptional(rl).orElse(null);
    }

    /** The good's name as the player's client shows the item, or null for an item this server does not have. */
    static Component itemName(String id) {
        Item item = item(id);
        return item == null ? null : new ItemStack(item).getHoverName();
    }

    /** A food has {@code DataComponents.FOOD} on its default stack (TK53). */
    static boolean isFood(String id) {
        Item item = item(id);
        return item != null && item.components().has(DataComponents.FOOD);
    }
}
