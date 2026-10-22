#!/usr/bin/env python3
"""Find the calls a mod jar makes into another mod that the other mod's jar no longer has.

    linkcheck.py [--prefix com/simibubi/create/] <provider.jar> <mod.jar>...

For every class reference and every field or method reference in each <mod.jar> whose owner is under
--prefix, the provider jar must have the class, and the member (name and descriptor) in that class or
in a superclass or interface the provider also holds. Run with `python3 -I`; it only reads the jars.

Reports per jar:
  missing class X              the provider has no such class
  missing member X.name desc   the class and its provider-held supers lack it (a NoSuchMethodError or
                               NoSuchFieldError the first time that code runs)
  missing ctor X desc          no constructor with that descriptor
  changed? X.name desc         not found, the class has the name with another descriptor, but a super
                               outside the provider could still hold it (often a false positive: a
                               vanilla interface's default method)

Outer jars only: jar-in-jar libraries are not opened. Written for the TBSMP bump to Create 6.0.10
(docs/stress-tbsmp-results.md), where it found five addons that call members 6.0.7+ removed.
"""
import os
import struct
import sys
import zipfile

ROOTS = {"java/lang/Object", "java/lang/Enum", "java/lang/Record"}
ROOT_METHODS = {"equals", "hashCode", "toString", "getClass", "clone", "finalize", "notify", "notifyAll", "wait",
                "values", "valueOf", "ordinal", "name", "compareTo", "getDeclaringClass", "describeConstable"}


def cp_parse(b):
    """The constant pool: utf8 ('u', s), class ('c', name), member refs ('m', tag, class, nt), ('nt', name, desc)."""
    n = struct.unpack(">H", b[8:10])[0]
    i, k, cp = 10, 1, [None] * n
    while k < n:
        t = b[i]
        if t == 1:
            ln = struct.unpack(">H", b[i + 1:i + 3])[0]
            cp[k] = ("u", b[i + 3:i + 3 + ln].decode("utf8", "replace"))
            i += 3 + ln
        elif t in (3, 4):
            i += 5
        elif t in (5, 6):
            i += 9
            k += 1
        elif t == 7:
            cp[k] = ("c", struct.unpack(">H", b[i + 1:i + 3])[0])
            i += 3
        elif t in (8, 16, 19, 20):
            i += 3
        elif t in (9, 10, 11):
            cp[k] = ("m", t) + struct.unpack(">HH", b[i + 1:i + 5])
            i += 5
        elif t == 12:
            cp[k] = ("nt",) + struct.unpack(">HH", b[i + 1:i + 5])
            i += 5
        elif t in (17, 18):
            i += 5
        elif t == 15:
            i += 4
        else:
            raise ValueError(f"constant pool tag {t}")
        k += 1
    return cp, i


def utf(cp, k):
    return cp[k][1]


def cls(cp, k):
    return utf(cp, cp[k][1])


def members(b):
    """(superclass, interfaces, {(name, descriptor)}) of one class file."""
    cp, i = cp_parse(b)
    sup = struct.unpack(">H", b[i + 4:i + 6])[0]
    ni = struct.unpack(">H", b[i + 6:i + 8])[0]
    ifs = [cls(cp, struct.unpack(">H", b[i + 8 + 2 * q:i + 10 + 2 * q])[0]) for q in range(ni)]
    j, out = i + 8 + 2 * ni, set()
    for _kind in ("fields", "methods"):
        cnt = struct.unpack(">H", b[j:j + 2])[0]
        j += 2
        for _ in range(cnt):
            _acc, nm, ds, ac = struct.unpack(">HHHH", b[j:j + 8])
            j += 8
            out.add((utf(cp, nm), utf(cp, ds)))
            for _ in range(ac):
                j += 6 + struct.unpack(">I", b[j + 2:j + 6])[0]
    return (cls(cp, sup) if sup else None), ifs, out


class Provider:
    def __init__(self, path, prefix):
        self.z = zipfile.ZipFile(path)
        self.prefix = prefix
        self.names = {n[:-6] for n in self.z.namelist() if n.endswith(".class")}
        self.cache = {}

    def info(self, c):
        if c not in self.cache:
            self.cache[c] = members(self.z.read(c + ".class")) if c in self.names else None
        return self.cache[c]

    def has(self, c, nm, ds, seen):
        """True found, False surely absent, None unknown (a super outside the provider)."""
        if c in seen:
            return False
        seen.add(c)
        if c in ROOTS:
            return nm in ROOT_METHODS
        if not c.startswith(self.prefix):
            return None
        i = self.info(c)
        if i is None:
            return False
        sup, ifs, mem = i
        if (nm, ds) in mem:
            return True
        res = False
        for s in ([sup] if sup else []) + ifs:
            r = self.has(s, nm, ds, seen)
            if r:
                return True
            if r is None:
                res = None
        return res


def check(prov, jar):
    z = zipfile.ZipFile(jar)
    bad = {}
    for n in z.namelist():
        if not n.endswith(".class"):
            continue
        try:
            cp, _ = cp_parse(z.read(n))
        except (ValueError, IndexError, struct.error):
            continue
        for e in cp:
            if not e:
                continue
            if e[0] == "c":
                c = utf(cp, e[1])
                if c.startswith(prov.prefix) and c not in prov.names:
                    bad.setdefault(f"missing class {c}", set()).add(n)
            elif e[0] == "m":
                c = cls(cp, e[2])
                if not c.startswith(prov.prefix) or c not in prov.names:
                    continue
                nt = cp[e[3]]
                nm, ds = utf(cp, nt[1]), utf(cp, nt[2])
                own = prov.info(c)[2]
                if nm == "<init>":
                    if (nm, ds) not in own:
                        bad.setdefault(f"missing ctor {c}{ds}", set()).add(n)
                    continue
                r = prov.has(c, nm, ds, set())
                if r is False:
                    bad.setdefault(f"missing member {c}.{nm} {ds}", set()).add(n)
                elif r is None and any(m == nm and d != ds for m, d in own):
                    bad.setdefault(f"changed? {c}.{nm} {ds}", set()).add(n)
    nested = sum(1 for n in z.namelist() if n.endswith(".jar"))
    print(f"== {os.path.basename(jar)}: {len(bad)} problems" + (f" (jar-in-jar not checked: {nested})" if nested else ""))
    for k in sorted(bad):
        users = sorted(bad[k])
        print("   ", k, "<-", users[0].split("/")[-1], f"+{len(users) - 1}" if len(users) > 1 else "")
    return len(bad)


def main():
    a = sys.argv[1:]
    prefix = "com/simibubi/create/"
    if a[:1] == ["--prefix"]:
        prefix, a = a[1], a[2:]
    if len(a) < 2:
        sys.exit(__doc__)
    prov = Provider(a[0], prefix)
    sys.exit(1 if sum(check(prov, j) for j in a[1:]) else 0)


if __name__ == "__main__":
    main()
