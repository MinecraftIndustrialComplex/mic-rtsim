package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

/** The saved form of {@link TalkDays}: a list of (player, day). */
public final class TalkDaysCodec {
    private TalkDaysCodec() {}

    private static final Codec<TalkDays.Entry> ENTRY = RecordCodecBuilder.create(i -> i.group(
            net.minecraft.core.UUIDUtil.CODEC.fieldOf("player").forGetter(TalkDays.Entry::player),
            Codec.LONG.fieldOf("day").forGetter(TalkDays.Entry::day)
    ).apply(i, TalkDays.Entry::new));

    public static final Codec<TalkDays> CODEC = ENTRY.listOf().xmap(TalkDays::new, TalkDays::entries);
}
