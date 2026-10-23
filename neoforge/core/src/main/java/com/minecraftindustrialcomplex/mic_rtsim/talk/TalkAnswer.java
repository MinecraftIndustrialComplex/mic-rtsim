package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin.EngineNames;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkFacts;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Facts;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.News;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Ref;

import java.util.HashMap;
import java.util.Map;

/**
 * The engine's answer with the dictionary names its ids stand for, read out of the session's dictionary
 * while the answer is applied (the dictionary is the session's, so a name is copied, never looked up
 * later). Pure: no Minecraft types.
 *
 * @param facts the decoded record
 * @param names dictionary kind and id to name, for every id the record uses that the dictionary knew
 */
public record TalkAnswer(Facts facts, Map<Long, String> names) {

    private static long key(int dictKind, int id) {
        return ((long) dictKind << 32) | (id & 0xFFFF_FFFFL);
    }

    /** The dictionary kind a ref class's {@code what} is in, or 0 when it has none. */
    public static int dictKindOf(int cls) {
        return switch (cls) {
            case TalkRecords.C_VILLAGE, TalkRecords.C_CAMP, TalkRecords.C_CHIEF -> Obs.DICT_CULTURE;
            case TalkRecords.C_DRAGON, TalkRecords.C_LAIR, TalkRecords.C_SHIP -> Obs.DICT_BODY;
            default -> 0;
        };
    }

    /** Reads every name the record needs from the session's dictionary. */
    public static TalkAnswer resolve(Facts f, EngineNames dict) {
        Map<Long, String> names = new HashMap<>();
        put(names, dict, Obs.DICT_ITEM, f.scarcest());
        put(names, dict, Obs.DICT_ITEM, f.glutted());
        for (News n : f.news()) {
            ref(names, dict, n.who());
            ref(names, dict, n.whom());
            ref(names, dict, n.near());
        }
        return new TalkAnswer(f, names);
    }

    private static void ref(Map<Long, String> names, EngineNames dict, Ref r) {
        int kind = dictKindOf(r.cls());
        if (kind != 0)
            put(names, dict, kind, r.what());
    }

    private static void put(Map<Long, String> names, EngineNames dict, int kind, int id) {
        if (id == 0)
            return;
        String name = dict.name(kind, id);
        if (name != null)
            names.put(key(kind, id), name);
    }

    public String name(int dictKind, int id) {
        return names.get(key(dictKind, id));
    }

    /** The culture or body name of a ref ({@code what}), or null. */
    public String whatOf(Ref r) {
        int kind = dictKindOf(r.cls());
        return kind == 0 || r.what() == 0 ? null : name(kind, r.what());
    }

    public String scarcest() {
        return name(Obs.DICT_ITEM, facts.scarcest());
    }

    public String glutted() {
        return name(Obs.DICT_ITEM, facts.glutted());
    }

    /**
     * The villages that are the holder's own side, told as "us" and always named. A village holder's is its
     * own id. A caravan's home village is not on the wire or known to Java, but the engine counts it as "us"
     * (TK36), so the villages this answer flags as the holder's side (the whom of a TO_US item, the who of a
     * BY_US one) are it.
     */
    public java.util.Set<Long> ownVillages(int holderKind, long holderId) {
        java.util.Set<Long> out = new java.util.HashSet<>();
        if (holderKind == com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder.VILLAGE) {
            out.add(holderId);
            return out;
        }
        for (News n : facts.news()) {
            if (n.has(TalkRecords.TO_US) && n.whom().cls() == TalkRecords.C_VILLAGE)
                out.add(n.whom().a());
            if (n.has(TalkRecords.BY_US) && n.who().cls() == TalkRecords.C_VILLAGE)
                out.add(n.who().a());
        }
        return out;
    }

    /** The slice of the answer a talker's own candidates may read. */
    public TalkFacts forTalkers() {
        return new TalkFacts(facts.holder(), facts.sentiment(), facts.portClosed(), facts.blockaded(),
                scarcest(), Integer.toUnsignedLong(facts.scarcestRatio()), glutted(),
                Integer.toUnsignedLong(facts.gluttedRatio()), facts.news().size());
    }
}
