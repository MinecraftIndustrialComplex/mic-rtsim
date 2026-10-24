package com.minecraftindustrialcomplex.mic_rtsim.talk;

import org.junit.jupiter.api.Test;

import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The core's English lines against the code that uses them (TALK.md §8): every news line uses only the four
 * arguments it is given, starts with a capital, and sits under a kind and perspective the code knows; every
 * key the code builds exists; no line has an em dash.
 */
class TalkLinesTest {
    private static final String ROOT = "mic_rtsim.talk.";
    private static final Pattern ARG = Pattern.compile("%(\\d+)\\$s");
    private static final Pattern NEWS_KEY = Pattern.compile(
            "mic_rtsim\\.talk\\.([a-z_.]+?)\\.news\\.([a-z_]+)\\.(you|to_you|us|by_us|other)(\\.seen|\\.held|\\.beaten)?\\.(\\d+)");
    private static final Set<String> KINDS = Set.of("assault", "killing", "robbery", "theft", "trade", "rescue",
            "resident_assault", "resident_killing", "encroachment", "feud", "pact_sworn", "pact_broken", "raid", "fell",
            "port_raided", "blockade", "port_closed", "port_reopened", "haven_fell");

    /** The lang file is one flat object of string values: read it without a JSON library (the unit tests have none). */
    private static Map<String, String> lang() throws Exception {
        try (InputStream in = TalkLinesTest.class.getResourceAsStream("/assets/mic_rtsim/lang/en_us.json")) {
            assertNotNull(in, "the core's en_us.json is on the test classpath");
            String text = new String(in.readAllBytes(), StandardCharsets.UTF_8);
            Map<String, String> out = new LinkedHashMap<>();
            int[] at = {text.indexOf('{') + 1};
            while (true) {
                String key = string(text, at);
                if (key == null)
                    break;
                int colon = text.indexOf(':', at[0]);
                at[0] = colon + 1;
                String value = string(text, at);
                assertNotNull(value, "a string value for " + key);
                assertFalse(out.containsKey(key), "a key twice: " + key);
                out.put(key, value);
            }
            return out;
        }
    }

    /** The next JSON string at or after {@code at[0]}, unescaped; null at the closing brace. */
    private static String string(String text, int[] at) {
        int i = at[0];
        while (i < text.length() && text.charAt(i) != '"' && text.charAt(i) != '}')
            i++;
        if (i >= text.length() || text.charAt(i) == '}')
            return null;
        StringBuilder sb = new StringBuilder();
        for (i++; text.charAt(i) != '"'; i++) {
            char c = text.charAt(i);
            if (c != '\\') {
                sb.append(c);
                continue;
            }
            char e = text.charAt(++i);
            switch (e) {
                case 'n' -> sb.append('\n');
                case 't' -> sb.append('\t');
                case 'u' -> {
                    sb.append((char) Integer.parseInt(text.substring(i + 1, i + 5), 16));
                    i += 4;
                }
                default -> sb.append(e);
            }
        }
        at[0] = i + 1;
        return sb.toString();
    }

    @Test
    void everyNewsLineUsesOnlyTheFourArgumentsAndStartsWithACapital() throws Exception {
        List<String> bad = new ArrayList<>();
        int news = 0;
        for (Map.Entry<String, String> e : lang().entrySet()) {
            Matcher m = NEWS_KEY.matcher(e.getKey());
            if (!e.getKey().contains(".news.") || !m.matches()) {
                if (e.getKey().contains(".news."))
                    bad.add("not a news key the code knows: " + e.getKey());
                continue;
            }
            news++;
            if (!KINDS.contains(m.group(2)))
                bad.add("unknown kind: " + e.getKey());
            // The outcome topics are a raid's alone (TK83), and a held or beaten raid has no seen variant.
            if ((".held".equals(m.group(4)) || ".beaten".equals(m.group(4))) && !m.group(2).equals("raid"))
                bad.add("an outcome topic on a kind with no outcomes: " + e.getKey());
            String text = e.getValue();
            Matcher a = ARG.matcher(text);
            while (a.find()) {
                int n = Integer.parseInt(a.group(1));
                if (n < 1 || n > 4)
                    bad.add("argument " + n + " is not one of the four: " + e.getKey() + " = " + text);
            }
            if (text.isEmpty() || !Character.isUpperCase(text.charAt(0)))
                bad.add("does not start with a capital: " + e.getKey() + " = " + text);
        }
        assertTrue(news > 100, "the news lines are there: " + news);
        assertEquals(List.of(), bad);
    }

    @Test
    void aHeldOrBeatenRaidHasLinesInEveryPerspectiveItCanBeToldFromAndNeverFallsToAPlunder() throws Exception {
        Map<String, String> lang = lang();
        // The any voice tells it from every side (TK83); a raid told from `you` falls to `other`.
        for (String outcome : new String[]{"held", "beaten"})
            for (String p : new String[]{"us", "by_us", "to_you", "other"})
                assertTrue(lang.containsKey(ROOT + "any.news.raid." + p + "." + outcome + ".0"), p + " " + outcome);
        // The clans tell their own side (a camp's raid that failed) in their own voices.
        for (String voice : new String[]{"goblin", "raider", "bandit"})
            for (String outcome : new String[]{"held", "beaten"})
                assertTrue(lang.containsKey(ROOT + voice + ".news.raid.by_us." + outcome + ".0"), voice + " " + outcome);
        // And the code's topics for them are exactly those keys' topics.
        assertEquals("news.raid.by_us.held", TalkTuning.topics("raid", 13, 1, "by_us", false).get(0));
        assertEquals("news.raid.to_you.beaten", TalkTuning.topics("raid", 13, 2, "to_you", true).get(0));
    }

    @Test
    void everyKindHasAnyVoiceLinesToFallBackOn() throws Exception {
        Map<String, String> lang = lang();
        for (String kind : KINDS)
            assertTrue(lang.containsKey(ROOT + "any.news." + kind + ".other.0"), "any voice, " + kind + ", other");
        // The five kinds the player can cause or suffer have the personal tellings.
        for (String kind : new String[]{"assault", "killing", "robbery"})
            for (String p : new String[]{"you", "to_you", "us"})
                assertTrue(lang.containsKey(ROOT + "any.news." + kind + "." + p + ".0"), kind + " " + p);
    }

    @Test
    void theKindNamesTheCodeUsesAreTheKindsTheLinesHave() {
        for (int k = 0; k < 64; k++) {
            String name = TalkTuning.kindName(k);
            if (name != null)
                assertTrue(KINDS.contains(name), "kind " + k + " " + name);
        }
        int named = 0;
        for (int k = 0; k < 64; k++)
            if (TalkTuning.kindName(k) != null)
                named++;
        assertEquals(KINDS.size(), named);
    }

    @Test
    void everyKeyTheNamingCodeBuildsExists() throws Exception {
        Map<String, String> lang = lang();
        List<String> missing = new ArrayList<>();
        for (String band : new String[]{"today", "yesterday", "days", "week", "weeks", "long"})
            need(lang, missing, ROOT + "when." + band);
        for (String band : new String[]{"here", "close", "walk", "ride", "far", "near", "elsewhere"})
            need(lang, missing, ROOT + "where." + band);
        for (String dir : new String[]{"n", "ne", "e", "se", "s", "sw", "w", "nw"})
            need(lang, missing, ROOT + "dir." + dir);
        for (String t : new String[]{"mountain", "forest", "swamp", "desert", "plains", "snowy", "jungle", "ocean", "beach",
                "badlands", "taiga", "savanna", "river", "terrain_dir", "dir"})
            need(lang, missing, ROOT + "loc." + t);
        for (String k : new String[]{"you", "someone", "ref.unknown", "ref.notable", "ref.village", "ref.base", "ref.caravan",
                "ref.caravan.from", "ref.camp", "ref.chief", "ref.chief.unnamed", "ref.dragon", "ref.dragon.fire_dragon",
                "ref.dragon.ice_dragon", "ref.dragon.lightning_dragon", "ref.lair", "ref.haven", "ref.haven.unnamed",
                "ref.ship", "ref.us", "name.village.format", "name.haven.format", "name.haven.a.0", "name.haven.b.0"})
            need(lang, missing, ROOT + k);
        for (String culture : new String[]{"goblin", "bandit", "raider"}) {
            need(lang, missing, ROOT + "name." + culture + ".format");
            need(lang, missing, ROOT + "name." + culture + ".a.0");
            need(lang, missing, ROOT + "name." + culture + ".b.0");
            need(lang, missing, ROOT + "name.chief." + culture + ".0");
            need(lang, missing, ROOT + "desc.camp." + culture);
            need(lang, missing, ROOT + "desc.camp." + culture + ".at");
            need(lang, missing, ROOT + "desc.chief." + culture);
            need(lang, missing, ROOT + "desc.chief." + culture + ".at");
        }
        for (String k : new String[]{"village", "base", "caravan", "camp", "chief", "haven", "lair", "ship"}) {
            need(lang, missing, ROOT + "desc." + k);
            need(lang, missing, ROOT + "desc." + k + ".at");
        }
        for (String mood : new String[]{"afraid", "uneasy", "feud", "dragon", "dear", "cheap", "hungry", "port_closed", "blockade"})
            need(lang, missing, ROOT + "villager.mood." + mood + ".0");
        assertEquals(List.of(), missing);
    }

    private static void need(Map<String, String> lang, List<String> missing, String key) {
        if (!lang.containsKey(key))
            missing.add(key);
    }

    @Test
    void theMoodLinesTakeAtMostTheOneArgumentTheyAreGiven() throws Exception {
        List<String> bad = new ArrayList<>();
        for (Map.Entry<String, String> e : lang().entrySet()) {
            if (!e.getKey().contains(".mood."))
                continue;
            Matcher a = ARG.matcher(e.getValue());
            while (a.find())
                if (!a.group(1).equals("1"))
                    bad.add(e.getKey() + " = " + e.getValue());
            // A mood line may open with its argument: the code capitalises it (TK55).
            if (e.getValue().isEmpty() || !(Character.isUpperCase(e.getValue().charAt(0)) || e.getValue().startsWith("%1$s")))
                bad.add("no capital: " + e.getKey());
        }
        assertEquals(List.of(), bad);
    }

    @Test
    void theArgumentsAMoodLineTakesMatchWhatTheMoodGives() throws Exception {
        Map<String, String> lang = lang();
        Set<String> withArg = Set.of("dear", "cheap", "feud", "dragon");
        Set<String> without = Set.of("afraid", "uneasy", "hungry", "port_closed", "blockade");
        // The lines that take an argument may also have lines that do not (dragon.0 does not); the ones that never take
        // one must not.
        for (String m : without)
            for (Map.Entry<String, String> e : lang.entrySet())
                if (e.getKey().matches("mic_rtsim\\.talk\\.[a-z.]+\\.mood\\." + m + "\\.\\d+"))
                    assertFalse(ARG.matcher(e.getValue()).find(), e.getKey() + " takes an argument it is not given");
        for (String m : withArg) {
            boolean any = lang.entrySet().stream()
                    .anyMatch(e -> e.getKey().matches("mic_rtsim\\.talk\\.[a-z.]+\\.mood\\." + m + "\\.\\d+") && e.getValue().contains("%1$s"));
            assertTrue(any, "a " + m + " line uses its argument");
        }
    }

    @Test
    void noLineHasAnEmDash() throws Exception {
        List<String> bad = new ArrayList<>();
        for (Map.Entry<String, String> e : lang().entrySet())
            if (e.getValue().indexOf('—') >= 0 || e.getValue().indexOf('–') >= 0)
                bad.add(e.getKey());
        assertEquals(List.of(), bad);
    }

    @Test
    void aSentenceStartingWithAnArgumentIsCapitalisedByTheCodeNotTheLine() throws Exception {
        // The lines may start a later sentence with an argument ("We were robbed blind today. %1$s."); the code
        // capitalises the described ones. Check the finder sees every such place in the real lines.
        int found = 0;
        for (Map.Entry<String, String> e : lang().entrySet()) {
            if (!e.getKey().contains(".news."))
                continue;
            if (Sentences.startsAt(e.getValue()).cardinality() > 0)
                found++;
        }
        assertTrue(found > 10, "lines with an argument opening a sentence: " + found);
    }
}
