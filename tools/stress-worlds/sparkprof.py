#!/usr/bin/env python3
"""Read a spark .sparkprofile (SamplerData protobuf) and report time shares for one thread.

sparkprof.py <file> [--thread "Server thread"] [--top N] [--match REGEX ...]

Times in spark nodes are inclusive. For each --match regex (against "class.method"), prints the
inclusive time of matching frames (counted once per stack: a match under a matching ancestor is not
added again), as ms and as a share of the thread total. --top prints the top self-time methods.
"""
import argparse
import re
import struct
import sys
from collections import defaultdict


def varint(b, i):
    r = 0
    s = 0
    while True:
        c = b[i]
        i += 1
        r |= (c & 0x7F) << s
        if not c & 0x80:
            return r, i
        s += 7


def parse(b):
    """Return dict field -> list of (wiretype, raw value)."""
    out = defaultdict(list)
    i = 0
    n = len(b)
    while i < n:
        key, i = varint(b, i)
        f, wt = key >> 3, key & 7
        if wt == 0:
            v, i = varint(b, i)
        elif wt == 1:
            v = b[i:i + 8]
            i += 8
        elif wt == 2:
            ln, i = varint(b, i)
            v = b[i:i + ln]
            i += ln
        elif wt == 5:
            v = b[i:i + 4]
            i += 4
        else:
            raise ValueError("wire type %d" % wt)
        out[f].append((wt, v))
    return out


def doubles(entries):
    r = []
    for wt, v in entries:
        if wt == 1:
            r.append(struct.unpack("<d", v)[0])
        elif wt == 2:
            r.extend(struct.unpack("<%dd" % (len(v) // 8), v))
    return r


def ints(entries):
    r = []
    for wt, v in entries:
        if wt == 0:
            r.append(v)
        elif wt == 2:
            i = 0
            while i < len(v):
                x, i = varint(v, i)
                r.append(x)
    return r


def s(entries):
    return entries[0][1].decode("utf-8", "replace") if entries else ""


def load(path, thread):
    data = open(path, "rb").read()
    top = parse(data)
    for wt, tb in top[2]:
        t = parse(tb)
        name = s(t[1])
        if thread and name != thread:
            continue
        nodes = []
        for _w, nb in t[3]:
            nd = parse(nb)
            nodes.append((s(nd[3]), s(nd[4]), sum(doubles(nd[8])), ints(nd[9])))
        roots = ints(t[5])
        total = sum(doubles(t[4]))
        return name, total, nodes, roots
    raise SystemExit("thread not found: %r" % thread)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--thread", default="Server thread")
    ap.add_argument("--top", type=int, default=0)
    ap.add_argument("--match", action="append", default=[])
    a = ap.parse_args()
    name, total, nodes, roots = load(a.file, a.thread)
    if total == 0:
        total = sum(nodes[r][2] for r in roots)
    print("thread %r total %.1f ms (%d nodes)" % (name, total, len(nodes)))
    pats = [re.compile(m) for m in a.match]
    incl = [0.0] * len(pats)
    selft = defaultdict(float)
    stack = [(r, [False] * len(pats)) for r in roots]
    while stack:
        idx, matched = stack.pop()
        cls, meth, t, kids = nodes[idx]
        label = cls + "." + meth
        newm = list(matched)
        for k, p in enumerate(pats):
            if not matched[k] and p.search(label):
                incl[k] += t
                newm[k] = True
        selft[label] += t - sum(nodes[c][2] for c in kids)
        for c in kids:
            stack.append((c, newm))
    for k, m in enumerate(a.match):
        print("match %-70s %10.1f ms  %6.3f%%" % (m, incl[k], 100 * incl[k] / total if total else 0))
    if a.top:
        for label, t in sorted(selft.items(), key=lambda x: -x[1])[:a.top]:
            print("self %8.1f ms %6.2f%%  %s" % (t, 100 * t / total, label))


if __name__ == "__main__":
    main()
