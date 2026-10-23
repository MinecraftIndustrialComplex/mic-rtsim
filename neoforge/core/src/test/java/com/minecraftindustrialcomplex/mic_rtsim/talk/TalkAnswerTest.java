package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin.EngineNames;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Facts;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.News;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Ref;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

/** The dictionary ids of an answer are read once, from the session's dictionary, by the ref's class (TK49). */
class TalkAnswerTest {
    @Test
    void goodsAndRefsAreNamedFromTheirOwnDictionaries() {
        List<String> asked = new ArrayList<>();
        EngineNames dict = (kind, id) -> {
            asked.add(kind + ":" + id);
            if (kind == Obs.DICT_ITEM && id == 7)
                return "minecraft:bread";
            if (kind == Obs.DICT_ITEM && id == 8)
                return "minecraft:coal";
            if (kind == Obs.DICT_CULTURE && id == 3)
                return "goblin";
            if (kind == Obs.DICT_BODY && id == 4)
                return "iceandfire:fire_dragon";
            return null;
        };
        Ref camp = new Ref(TalkRecords.C_CAMP, TalkRecords.REF_POS, 3, 10, 0, 5, 5);
        Ref dragon = new Ref(TalkRecords.C_DRAGON, 0, 4, 11, 12, 0, 0);
        Ref village = new Ref(TalkRecords.C_VILLAGE, 0, 3, 9, 0, 0, 0);
        Ref player = new Ref(TalkRecords.C_PLAYER, 0, 99, 1, 2, 0, 0);
        News n = new News(1, 1, 0, 0, 5, 1, 1, camp, dragon, village);
        News p = new News(1, 1, 0, 0, 5, 1, 1, player, Ref.NONE, Ref.NONE);
        Facts f = new Facts(1, TalkRecords.F_HOLDER, 0, 7, 100, 8, 3000, List.of(n, p));
        TalkAnswer a = TalkAnswer.resolve(f, dict);

        assertEquals("minecraft:bread", a.scarcest());
        assertEquals("minecraft:coal", a.glutted());
        assertEquals("goblin", a.whatOf(camp));
        assertEquals("iceandfire:fire_dragon", a.whatOf(dragon));
        assertEquals("goblin", a.whatOf(village));
        // A player has no dictionary entry, whatever the bytes in `what` say.
        assertNull(a.whatOf(player));
        assertNull(a.whatOf(Ref.NONE));
        assertEquals(false, asked.contains(Obs.DICT_CULTURE + ":99"));
        assertEquals(false, asked.contains(Obs.DICT_BODY + ":99"));
    }

    @Test
    void anUnknownIdIsNullAndTheTalkersSliceCarriesTheRatiosUnsigned() {
        Facts f = new Facts(2, TalkRecords.F_HOLDER | TalkRecords.F_PORT_CLOSED, -5, 0, 0, 9, -1, List.of());
        TalkAnswer a = TalkAnswer.resolve(f, (k, id) -> null);
        assertNull(a.scarcest());
        assertNull(a.glutted());
        var t = a.forTalkers();
        assertEquals(true, t.holderKnown());
        assertEquals(true, t.portClosed());
        assertEquals(false, t.blockaded());
        assertEquals(-5, t.sentiment());
        assertEquals(0xFFFF_FFFFL, t.gluttedRatio());
        assertEquals(0, t.news());
    }
}
