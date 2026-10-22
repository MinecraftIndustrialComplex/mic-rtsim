package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * One subscriber's far view as the server last sent it, and the records that bring the client up to date
 * with what the filter chose this second (docs/boundary.md §12.2). An object new to the view gets a handle and
 * an INTRO; a leg is sent when it changes, a sample when it moved by an eighth of a block, the entity id when
 * it changes; an object that left the view is GONE and its handle is free again. Plain Java.
 */
public final class FarView {
    private static final class Sent {
        final int handle;
        FarWire.Intro intro;
        FarWire.Rec where;
        int entity;

        Sent(int handle) {
            this.handle = handle;
        }
    }

    private final Map<Long, Sent> sent = new HashMap<>();
    private final ArrayDeque<Integer> free = new ArrayDeque<>();
    private int nextHandle;

    /** How many objects the client holds. */
    public int size() {
        return sent.size();
    }

    /** Forgets everything (the client is told to with {@link FarWire#RESET}). */
    public void clear() {
        sent.clear();
        free.clear();
        nextHandle = 0;
    }

    /** The records that turn the client's view into {@code chosen}: GONE first, so their handles can be reused. */
    public List<FarWire.Rec> step(List<FarItem> chosen) {
        List<FarWire.Rec> out = new ArrayList<>();
        Set<Long> keep = new HashSet<>();
        for (FarItem i : chosen)
            keep.add(i.id());
        Iterator<Map.Entry<Long, Sent>> it = sent.entrySet().iterator();
        while (it.hasNext()) {
            Map.Entry<Long, Sent> e = it.next();
            if (keep.contains(e.getKey()))
                continue;
            out.add(new FarWire.Gone(e.getValue().handle));
            free.add(e.getValue().handle);
            it.remove();
        }
        for (FarItem i : chosen) {
            Sent s = sent.get(i.id());
            boolean fresh = s == null;
            if (fresh) {
                Integer h = free.poll();
                s = new Sent(h != null ? h : nextHandle++);
                s.entity = -1;
                sent.put(i.id(), s);
            }
            FarWire.Intro intro = new FarWire.Intro(s.handle, i.id(), i.kind().wire, i.style(), i.variant(), i.size(), i.rgb());
            if (fresh || !intro.equals(s.intro)) {
                out.add(intro);
                s.intro = intro;
            }
            FarWire.Rec where;
            if (i.leg() != null) {
                FarItem.Leg l = i.leg();
                where = new FarWire.Leg(s.handle, i.state().wire, l.y(), l.x0(), l.z0(), l.x1(), l.z1(), l.t0(), l.t1(), l.radius());
            } else {
                where = FarWire.Sample.of(s.handle, i.state().wire, i.seg());
            }
            if (fresh || !where.equals(s.where)) {
                out.add(where);
                s.where = where;
            }
            if (i.entity() != s.entity) {
                out.add(new FarWire.Real(s.handle, i.entity()));
                s.entity = i.entity();
            }
        }
        return out;
    }
}
