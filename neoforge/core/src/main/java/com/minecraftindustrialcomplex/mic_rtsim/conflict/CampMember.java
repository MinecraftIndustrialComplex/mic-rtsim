package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

/**
 * The payload of the {@code mic_rtsim:camp_member} attachment (MOWZIE.md §4.9, ledger 16): the camp
 * whose member a vanilla body is, by the camp's faction key, the member's slot and its generation.
 *
 * <p>Saved with the body, on purpose: a body at home stays the jar's own entity, saved by vanilla, and
 * the tag is how the next census tells it from a stray. A world without rtsim skips the attachment with
 * NeoForge's log line and the entity loads as the jar left it (UR-1).
 */
public record CampMember(long faction, int slot, int generation) {
    /** The attachment's default, never stored ({@code faction} 0 is no camp's key). */
    public static final CampMember NONE = new CampMember(0L, 0, 0);

    public static final Codec<CampMember> CODEC = RecordCodecBuilder.create(i -> i.group(
            Codec.LONG.fieldOf("faction").forGetter(CampMember::faction),
            Codec.INT.fieldOf("slot").forGetter(CampMember::slot),
            Codec.INT.fieldOf("generation").forGetter(CampMember::generation)
    ).apply(i, CampMember::new));

    public boolean isNone() {
        return faction == 0L;
    }
}
