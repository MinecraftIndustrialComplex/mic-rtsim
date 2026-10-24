#!/usr/bin/env python3
"""Puts a GameTest's waits on ChunkWait's two clocks (core's gametest/ChunkWait.java).

A wait on chunks (generation, a load, an unload, the world's IO) is real work on other threads, and the
GameTest server ticks unthrottled, so a tick bound on it runs out in seconds under load. For every
@GameTest method with at least one chunk wait, this script:

  - sets timeoutTicks to ChunkWait.NO_TICK_TIMEOUT and starts the body with
    `ChunkWait waits = ChunkWait.of(helper, <the old timeout>);`
  - wraps each chunk wait as `waits.chunks(...)`: bounded by 120 s of wall-clock time, counting no ticks;
  - wraps every other wait as `waits.logic(...)`: bounded by the old timeout in ticks, counted from the
    tick the last chunk wait ended (or the test's start).

A wait is any `.thenWaitUntil(<one argument>)`. Which ones are chunk waits is decided by regular
expressions matched against the argument's text (`--chunk`). The defaults are the Mowzie kit's
(MowzieKit.java): `far.ready()`, `far.unloaded(...)`, `unloadedWithout(...)`, `gone(...)`, and every
`io(...)` wait, since in those tests each one waits for a chunk to load or unload. Another kit gives
its own with `--chunk` (and `--no-default-chunks` to drop these).

The logic waits' budget is the method's old timeoutTicks, unless `--budget NAME=TICKS` (or a line
`NAME=TICKS` in a `--budgets` file) gives another: for a test whose timeout was raised only to cover its
chunk waits, give the value from before the raise. A budget names a method in one of the files, or the
script refuses it.

The script can be run again on the same files. A converted method (one with `ChunkWait waits =`) keeps
its budget (unless a `--budget` names it), and only its waits that are not wrapped yet are wrapped. So
a branch that adds tests can rerun it over the whole file after merging main.

The script refuses a method that would break under NO_TICK_TIMEOUT:
  - `helper.onEachTick`, which schedules one task for each tick up to the timeout (ten million);
    use `waits.eachTick` (or a server-tick listener such as MowzieKit.everyTick);
  - `succeedWhen` / `succeedIf`, which wait for the test's own timeout;
  - a two-argument `thenWaitUntil(delay, ...)`.

Usage (from the repository root, `python3 -I` so nothing beside the script shadows the stdlib):
  python3 -I neoforge/tools/chunk_waits.py FILE.java [FILE.java ...]
  python3 -I neoforge/tools/chunk_waits.py --dry-run FILE.java
  python3 -I neoforge/tools/chunk_waits.py --only aTestName FILE.java
  python3 -I neoforge/tools/chunk_waits.py --chunk '^kit\\.loaded\\(' FILE.java
  python3 -I neoforge/tools/chunk_waits.py --budget aTestName=1500 --budgets old-timeouts.txt FILE.java
Exit 0 when every method was converted or skipped, 1 when one was refused or a budget named no method
(nothing is written then, in any file).
"""
import argparse
import re
import sys

DEFAULT_CHUNKS = [
    r"^far\.ready\(",
    r"^far\.unloaded\(",
    r"^(MowzieKit\.)?unloadedWithout\(",
    r"^gone\(",
    r"^(RtsimTests\.)?io\(",
]
IMPORT = "import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;"
WAIT = re.compile(r"\.thenWaitUntil\(")
HEAD = re.compile(
    r"@GameTest\((?P<before>[^)]*?)timeoutTicks\s*=\s*(?P<ticks>[\w.]+)(?P<after>[^)]*)\)\s*\n"
    r"(?P<sig>\s*public static void (?P<name>\w+)\(GameTestHelper (?P<helper>\w+)\)\s*\{)"
)


def skip(s, j):
    """If s[j] opens a literal or a comment, the index just past it; else j."""
    if s.startswith('"""', j):
        k = s.index('"""', j + 3)
        while s[k - 1] == "\\":
            k = s.index('"""', k + 1)
        return k + 3
    c = s[j]
    if c in "\"'":
        k = j + 1
        while s[k] != c:
            k += 2 if s[k] == "\\" else 1
        return k + 1
    if s.startswith("//", j):
        k = s.find("\n", j)
        return len(s) if k < 0 else k
    if s.startswith("/*", j):
        return s.index("*/", j + 2) + 2
    return j


def match(s, i, open_c, close_c):
    """s[i] == open_c; the index of its match, skipping literals and comments."""
    depth, j = 0, i
    while j < len(s):
        k = skip(s, j)
        if k != j:
            j = k
            continue
        if s[j] == open_c:
            depth += 1
        elif s[j] == close_c:
            depth -= 1
            if depth == 0:
                return j
        j += 1
    raise ValueError("unbalanced " + open_c)


def top_level_commas(s):
    """How many commas sit at depth 0 in s (an argument list's text)."""
    n, depth, j = 0, 0, 0
    while j < len(s):
        k = skip(s, j)
        if k != j:
            j = k
            continue
        c = s[j]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == "," and depth == 0:
            n += 1
        j += 1
    return n


def convert_body(body, chunks, name):
    """The body with every wait not yet wrapped wrapped; (body, chunk count, logic count, refusals)."""
    out, k, nc, nl, refused = [], 0, 0, 0, []
    for m in WAIT.finditer(body):
        if m.start() < k:
            continue
        o = m.end() - 1
        c = match(body, o, "(", ")")
        arg = body[o + 1:c]
        text = arg.strip()
        if top_level_commas(arg):
            refused.append(f"{name}: a two-argument thenWaitUntil: {text[:70]!r}")
            continue
        if text.startswith("waits."):
            continue
        kind = "chunks" if any(re.search(p, text) for p in chunks) else "logic"
        if kind == "chunks":
            nc += 1
        else:
            nl += 1
        out.append(body[k:o + 1] + f"waits.{kind}(" + arg + ")")
        k = c
    out.append(body[k:])
    return "".join(out), nc, nl, refused


def convert_file(path, chunks, only, budgets, seen):
    """(refusals, report lines, the new text or None when unchanged); every method name goes in seen."""
    s = open(path, encoding="utf-8").read()
    pieces, k, refused, report, changed = [], 0, [], [], False
    for h in HEAD.finditer(s):
        name, helper = h.group("name"), h.group("helper")
        seen.add(name)
        if only and name not in only:
            continue
        o = h.end() - 1  # the body's '{'
        c = match(s, o, "{", "}")
        body = s[o + 1:c]
        converted = re.search(r"\bChunkWait\s+waits\s*=", body) is not None
        if not converted:
            if re.search(r"\bwaits\b", body):
                refused.append(f"{name}: already has a `waits` that is no ChunkWait")
                continue
            if not any(re.search(p, m_arg.strip()) for p in chunks for m_arg in wait_args(body)):
                if name in budgets:
                    refused.append(f"{name}: --budget given, but it has no chunk wait (its timeoutTicks is its bound)")
                continue  # no chunk wait: its tick timeout is right as it is
        bad = [w for w in ("onEachTick(", "succeedWhen(", "succeedIf(") if helper + "." + w in body]
        if bad:
            refused.append(f"{name}: {', '.join(helper + '.' + w + ')' for w in bad)} under NO_TICK_TIMEOUT")
            continue
        new_body, nc, nl, r = convert_body(body, chunks, name)
        if r:
            refused.extend(r)
            continue
        head = s[h.start():o + 1]
        budget = budgets.get(name)
        if not converted:
            ticks = h.group("ticks")
            if not re.fullmatch(r"[\d_]+", ticks):
                refused.append(f"{name}: timeoutTicks is {ticks}, not a number")
                continue
            head = re.sub(r"timeoutTicks\s*=\s*" + re.escape(ticks), "timeoutTicks = ChunkWait.NO_TICK_TIMEOUT", head, count=1)
            first = re.match(r"\n([ \t]*)\S", new_body)
            indent = first.group(1) if first else "        "
            new_body = f"\n{indent}ChunkWait waits = ChunkWait.of({helper}, {budget or ticks});" + new_body
        elif budget:
            of = re.compile(r"(\bChunkWait\s+waits\s*=\s*ChunkWait\.of\(\s*" + re.escape(helper) + r"\s*,\s*)([\w.]+)(\s*\))")
            if len(of.findall(new_body)) != 1:
                refused.append(f"{name}: --budget given, but its ChunkWait.of(...) is not one plain call")
                continue
            new_body = of.sub(lambda m: m.group(1) + budget + m.group(3), new_body)
        if new_body == body and converted:
            continue
        pieces.append(s[k:h.start()] + head + new_body)
        k = c
        changed = True
        what = "updated" if converted else "converted"
        report.append(f"  {name}: {what}, wrapped {nc} chunk wait(s) and {nl} logic wait(s)"
                      + (f", logic budget {budget} ticks" if budget else ""))
    pieces.append(s[k:])
    if refused:
        return refused, report, None
    if not changed:
        return [], report, None
    out = "".join(pieces)
    pkg = re.search(r"^package ([\w.]+);", out, re.M)
    if IMPORT not in out and not (pkg and pkg.group(1) == "com.minecraftindustrialcomplex.mic_rtsim.gametest"):
        imports = [m for m in re.finditer(r"^import [^\n]+;\n", out, re.M) if not m.group(0).startswith("import static")]
        after = [m for m in imports if m.group(0).rstrip("\n") > IMPORT]
        at = after[0].start() if after else (imports[-1].end() if imports else out.index("\n", pkg.end()) + 1)
        out = out[:at] + IMPORT + "\n" + out[at:]
    return [], report, out


def wait_args(body):
    for m in WAIT.finditer(body):
        o = m.end() - 1
        yield body[o + 1:match(body, o, "(", ")")]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("files", nargs="+")
    ap.add_argument("--chunk", action="append", default=[], help="a regex: a wait whose argument matches is a chunk wait")
    ap.add_argument("--no-default-chunks", action="store_true", help="drop the Mowzie kit's chunk waits")
    ap.add_argument("--only", action="append", default=[], help="convert only this method (repeatable)")
    ap.add_argument("--dry-run", action="store_true", help="report, write nothing")
    ap.add_argument("--budget", action="append", default=[], metavar="NAME=TICKS",
                    help="this method's logic budget, in place of its timeoutTicks (repeatable)")
    ap.add_argument("--budgets", action="append", default=[], metavar="FILE",
                    help="a file of NAME=TICKS lines (# starts a comment), as --budget")
    a = ap.parse_args()
    chunks = ([] if a.no_default_chunks else DEFAULT_CHUNKS) + a.chunk
    entries = list(a.budget)
    for bf in a.budgets:
        for line in open(bf, encoding="utf-8"):
            line = line.split("#", 1)[0].strip()
            if line:
                entries.append(line)
    budgets, failed = {}, False
    for e in entries:
        m = re.fullmatch(r"\s*(\w+)\s*=\s*([\d_]+)\s*", e)
        if not m:
            sys.exit(f"a budget is NAME=TICKS, not {e!r}")
        budgets[m.group(1)] = m.group(2)
    seen, outs = set(), []
    for f in a.files:
        refused, report, out = convert_file(f, chunks, set(a.only), budgets, seen)
        print(f"{f}: {'refused' if refused else ('unchanged' if out is None else ('would change' if a.dry_run else 'changed'))}")
        for line in report:
            print(line)
        for line in refused:
            print("  REFUSED " + line)
        failed |= bool(refused)
        if out is not None:
            outs.append((f, out))
    for name in sorted(set(budgets) - seen):
        print(f"REFUSED --budget {name}: no @GameTest method of that name in the files")
        failed = True
    if failed:
        print("nothing written")
        sys.exit(1)
    if not a.dry_run:
        for f, out in outs:
            open(f, "w", encoding="utf-8").write(out)
    sys.exit(0)


if __name__ == "__main__":
    main()
