package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.BitSet;

/**
 * Where a template puts an argument at the start of a sentence, so a name or description there can be
 * capitalised ("%1$s." after a full stop, "the goblins in the hills" becoming "The goblins in the hills").
 * Pure: it reads the English template only.
 */
public final class Sentences {
    private Sentences() {}

    /**
     * The 0-based argument indexes the template writes as {@code %n$s} at its start or right after a
     * sentence end (". ", "! ", "? ", or an opening quote after one).
     */
    public static BitSet startsAt(String template) {
        BitSet out = new BitSet();
        int i = 0, n = template.length();
        boolean start = true;
        while (i < n) {
            char c = template.charAt(i);
            if (c == '%' && i + 3 < n && Character.isDigit(template.charAt(i + 1)) && template.charAt(i + 2) == '$'
                    && template.charAt(i + 3) == 's') {
                if (start)
                    out.set(template.charAt(i + 1) - '1');
                start = false;
                i += 4;
                continue;
            }
            if (c == '.' || c == '!' || c == '?') {
                // A sentence ends here when a space follows (or the line does).
                start = i + 1 >= n || template.charAt(i + 1) == ' ';
            } else if (c == ' ' || c == '"' || c == '\'') {
                // Spaces and quotes between the end and the next word keep the sentence start.
            } else {
                start = false;
            }
            i++;
        }
        return out;
    }

    /** The text with its first letter capitalised. */
    public static String capitalise(String s) {
        return s.isEmpty() ? s : Character.toUpperCase(s.charAt(0)) + s.substring(1);
    }
}
