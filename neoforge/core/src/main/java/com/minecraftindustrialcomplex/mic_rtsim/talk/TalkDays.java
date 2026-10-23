package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/**
 * The attachment {@code mic_rtsim:talk_days} (TK25): for each player, the last game day a talk with
 * this entity was credited. At most {@link #CAP} players; the one with the oldest day is dropped.
 * Immutable.
 */
public record TalkDays(List<Entry> entries) {
    public static final int CAP = 16;
    public static final TalkDays EMPTY = new TalkDays(List.of());

    public record Entry(UUID player, long day) {
        static final Codec<Entry> CODEC = RecordCodecBuilder.create(i -> i.group(
                net.minecraft.core.UUIDUtil.CODEC.fieldOf("player").forGetter(Entry::player),
                Codec.LONG.fieldOf("day").forGetter(Entry::day)
        ).apply(i, Entry::new));
    }

    public static final Codec<TalkDays> CODEC = Entry.CODEC.listOf().xmap(TalkDays::new, TalkDays::entries);

    /** The last day credited for this player, or -1 for never. */
    public long lastDay(UUID player) {
        for (Entry e : entries)
            if (e.player.equals(player))
                return e.day;
        return -1;
    }

    /** True when this player was already credited on this game day. */
    public boolean creditedOn(UUID player, long day) {
        return lastDay(player) == day;
    }

    /** This record with the player's last day set, dropping the oldest entry beyond the cap. */
    public TalkDays with(UUID player, long day) {
        List<Entry> out = new ArrayList<>(entries.size() + 1);
        for (Entry e : entries)
            if (!e.player.equals(player))
                out.add(e);
        out.add(new Entry(player, day));
        while (out.size() > CAP) {
            int oldest = 0;
            for (int i = 1; i < out.size(); i++)
                if (out.get(i).day < out.get(oldest).day)
                    oldest = i;
            out.remove(oldest);
        }
        return new TalkDays(List.copyOf(out));
    }

    public boolean isEmpty() {
        return entries.isEmpty();
    }
}
