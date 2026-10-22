#!/usr/bin/env python3
"""Rebuild the TBSMP Season 2 pack from its level.dat mod list (private local testing only).

The lookups (lookup-b*.json, one entry per level.dat mod id) were made by hand against
Modrinth and CurseForge; each entry names the exact jar, its url and its hashes, or the jar
that carries it as jar-in-jar. Nothing here is committed with jars or world files: the
manifest written by `manifest` holds ids, versions, urls and hashes only.

    tbsmp_pack.py manifest <pack-dir> <out.json> [bump.json]
                                                    one JSON manifest of the pack; bump.json adds the
                                                    instance bumped to Create 6.0.10 (its jars, the
                                                    ones it replaced and removed, and why)
    tbsmp_pack.py packwiz  <pack-dir>               pack.toml + mods/*.pw.toml (then `packwiz refresh`)
    tbsmp_pack.py server   <pack-dir> <srv-dir>     copy the jars (sha1 checked) into <srv-dir>/mods
"""
import hashlib
import json
import os
import shutil
import sys

MC = "1.21.1"
NEOFORGE = "21.1.188"  # the world's own NeoForge (level.dat)


def entries(pack):
    out = []
    for b in (1, 2, 3):
        p = os.path.join(pack, f"lookup-b{b}.json")
        for e in json.load(open(p)):
            e["batch"] = b
            out.append(e)
    return sorted(out, key=lambda e: e["modid"])


def jar_path(pack, e):
    return os.path.join(pack, "dl", f"b{e['batch']}", e["filename"])


def manifest(pack, out, bump=None):
    keep = ("modid", "version", "found", "exact", "jij_of", "source", "project_id", "slug", "version_id",
            "file_id", "filename", "url", "sha1", "sha512", "client_only", "notes")
    rows = [{k: e.get(k) for k in keep if e.get(k) is not None} for e in entries(pack)]
    doc = {"minecraft": MC, "neoforge": NEOFORGE, "mods": rows}
    if bump:
        doc["bumped_instance"] = json.load(open(bump))
    json.dump(doc, open(out, "w"), indent=1)
    print(f"{len(rows)} entries -> {out}")


def toml_str(s):
    return json.dumps(s)  # a JSON string is a valid TOML basic string


def packwiz(pack):
    os.makedirs(os.path.join(pack, "mods"), exist_ok=True)
    with open(os.path.join(pack, "pack.toml"), "w") as f:
        f.write('name = "TBSMP Season 2 (rebuilt, private testing)"\npack-format = "packwiz:1.1.0"\n\n'
                '[index]\nfile = "index.toml"\nhash-format = "sha256"\nhash = ""\n\n'
                f'[versions]\nminecraft = "{MC}"\nneoforge = "{NEOFORGE}"\n')
    open(os.path.join(pack, "index.toml"), "a").close()
    n = 0
    for e in entries(pack):
        if e.get("jij_of") or not e.get("found"):
            continue
        side = "client" if e.get("client_only") else "both"
        name = e.get("slug") or e["modid"]
        lines = [f"name = {toml_str(name)}", f"filename = {toml_str(e['filename'])}", f'side = "{side}"', "",
                 "[download]", f"url = {toml_str(e['url'])}", 'hash-format = "sha1"', f"hash = {toml_str(e['sha1'])}", ""]
        if e["source"] == "modrinth" and e.get("project_id") and e.get("version_id"):
            lines += ["[update]", "[update.modrinth]", f"mod-id = {toml_str(e['project_id'])}", f"version = {toml_str(e['version_id'])}", ""]
        elif e["source"] == "curseforge" and e.get("project_id") and e.get("file_id"):
            lines += ["[update]", "[update.curseforge]", f"project-id = {int(e['project_id'])}", f"file-id = {int(e['file_id'])}", ""]
        with open(os.path.join(pack, "mods", f"{e['modid']}.pw.toml"), "w") as f:
            f.write("\n".join(lines))
        n += 1
    print(f"{n} mod files written; run `packwiz refresh` in {pack}")


def server(pack, srv):
    mods = os.path.join(srv, "mods")
    os.makedirs(mods, exist_ok=True)
    n = 0
    for e in entries(pack):
        if e.get("jij_of") or not e.get("found"):
            continue
        src = jar_path(pack, e)
        h = hashlib.sha1(open(src, "rb").read()).hexdigest()
        if h != e["sha1"]:
            sys.exit(f"sha1 mismatch: {src}")
        dst = os.path.join(mods, e["filename"])
        if not os.path.exists(dst):
            shutil.copyfile(src, dst)
        n += 1
    print(f"{n} jars in {mods}")


if __name__ == "__main__":
    cmd, *args = sys.argv[1:]
    {"manifest": manifest, "packwiz": packwiz, "server": server}[cmd](*args)
