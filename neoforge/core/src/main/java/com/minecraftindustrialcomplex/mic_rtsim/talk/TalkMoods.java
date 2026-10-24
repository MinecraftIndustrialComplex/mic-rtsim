package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Faction;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Facts;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.News;

import java.util.ArrayList;
import java.util.List;
import java.util.function.IntPredicate;

/**
 * The engine-side mood (TALK.md §5.2) read from an answer, a camp's from its faction block too: pure, so the
 * thresholds are unit tests. What each mood's argument is, is {@link Arg}; the caller turns it into text.
 */
public final class TalkMoods {
    private TalkMoods() {}

    /** What a mood line's %1$s is. */
    public enum Arg {
        NONE,
        /** The scarcest good's name. */
        SCARCEST,
        /** The most glutted good's name. */
        GLUTTED,
        /** The news item's who: the camp of a feud, the dragon of an assault. */
        WHO
    }

    /** A mood topic with its weight and argument; {@code source} is the news item that made it, or null. */
    public record Mood(String topic, double weight, Arg arg, News source) {}

    /**
     * The moods an answer gives. {@code scarcestIsFood} says whether the engine's good id is a food (the
     * item's default stack carries {@code DataComponents.FOOD}); it is asked only when the scarcest good is
     * under {@link TalkTuning#DEAR_BELOW}.
     */
    public static List<Mood> of(Facts f, IntPredicate scarcestIsFood) {
        List<Mood> out = new ArrayList<>();
        if (f == null || !f.holder())
            return out;

        // Safety: the freshest assault, killing, robbery, raid or port raid on the holder decides.
        long freshest = Long.MAX_VALUE;
        News feud = null, dragon = null;
        for (News n : f.news()) {
            if (n.has(TalkRecords.TO_US) && TalkTuning.isSafetyKind(n.kind()))
                freshest = Math.min(freshest, n.ago());
            if (feud == null && n.kind() == TalkTuning.KIND_FEUD && n.has(TalkRecords.TO_US))
                feud = n;
            if (dragon == null && n.kind() == TalkTuning.KIND_ASSAULT && n.who().cls() == TalkRecords.C_DRAGON
                    && (n.has(TalkRecords.TO_US) || n.has(TalkRecords.SEEN)) && n.ago() < TalkTuning.DRAGON_TICKS)
                dragon = n;
        }
        if (freshest < TalkTuning.AFRAID_TICKS)
            out.add(new Mood("mood.afraid", TalkTuning.MOOD_AFRAID, Arg.NONE, null));
        else if (freshest < TalkTuning.UNEASY_TICKS)
            out.add(new Mood("mood.uneasy", TalkTuning.MOOD_UNEASY, Arg.NONE, null));
        if (feud != null)
            out.add(new Mood("mood.feud", TalkTuning.MOOD_FEUD, Arg.WHO, feud));
        if (dragon != null)
            out.add(new Mood("mood.dragon", TalkTuning.MOOD_DRAGON, Arg.WHO, dragon));

        // Prices and hunger: the scarcest good, a food under a third of its target is hunger instead of dear.
        long scarce = Integer.toUnsignedLong(f.scarcestRatio());
        if (f.scarcest() != 0 && scarce < TalkTuning.DEAR_BELOW) {
            if (scarce < TalkTuning.HUNGRY_BELOW && scarcestIsFood.test(f.scarcest()))
                out.add(new Mood("mood.hungry", TalkTuning.MOOD_PRICES, Arg.NONE, null));
            else
                out.add(new Mood("mood.dear", TalkTuning.MOOD_PRICES, Arg.SCARCEST, null));
        }
        if (f.glutted() != 0 && Integer.toUnsignedLong(f.gluttedRatio()) > TalkTuning.CHEAP_ABOVE)
            out.add(new Mood("mood.cheap", TalkTuning.MOOD_PRICES, Arg.GLUTTED, null));

        if (f.portClosed())
            out.add(new Mood("mood.port_closed", TalkTuning.MOOD_PORT, Arg.NONE, null));
        if (f.blockaded())
            out.add(new Mood("mood.blockade", TalkTuning.MOOD_PORT, Arg.NONE, null));

        // A camp (slice 4, TK78): the faction block's moods, none of them with an argument. The player's own
        // standing with the camp is the grudge: its feud has reached the first rung.
        Faction camp = f.faction();
        if (camp != null) {
            if (camp.has(TalkRecords.MOOD_COWED))
                out.add(new Mood("mood.cowed", TalkTuning.MOOD_COWED, Arg.NONE, null));
            if (camp.has(TalkRecords.MOOD_NEW_CHIEF))
                out.add(new Mood("mood.new_chief", TalkTuning.MOOD_NEW_CHIEF, Arg.NONE, null));
            if (camp.has(TalkRecords.MOOD_CHIEF_OUT))
                out.add(new Mood("mood.chief_out", TalkTuning.MOOD_CHIEF_OUT, Arg.NONE, null));
            if (camp.has(TalkRecords.MOOD_LEAN))
                out.add(new Mood("mood.lean", TalkTuning.MOOD_LEAN, Arg.NONE, null));
            if (camp.has(TalkRecords.MOOD_STRONG))
                out.add(new Mood("mood.strong", TalkTuning.MOOD_STRONG, Arg.NONE, null));
            if (camp.rung() >= 1)
                out.add(new Mood("mood.grudge", TalkTuning.MOOD_GRUDGE, Arg.NONE, null));
        }
        return out;
    }
}
