#!/usr/bin/env python3
"""Stress-world runs for mic-rtsim (DECISIONS.md ST1): paired dedicated servers on the full MIC pack.

    sw.py prep  <run> --world <src> [--rtsim] [--xmx 6G]      make ~/mic-stress-worlds/runs/<run>
    sw.py pair  <tag> --world <src> --scene <scene.json> [--arms off,on] [--xmx 6G] [--profile 60]
    sw.py boot  <run> --world-from-run <earlier run> --scene <scene.json> [--rtsim]
                                                             boot an earlier run's saved world again
                                                             (reload or uninstall check)

Another pack: --base <server dir> --runs <dir> --jars <dir> (the NeoForge version is read from the
base's libraries). Stdlib only. Every server runs under nice 10 on its own ports (26100 + slot), from a reflink copy of
~/mic-stress-worlds/srv/base (the current pack, dc49fe7, NeoForge 21.1.249, plus spark) and a reflink
copy of the world. The world sources stay untouched. Results go to <run>/sw-result.json.

A pair starts its arms 5 s apart and runs the same scene in parallel threads, so both share the
laptop's load; compare per sample. The arms wait for each other at the scene's start, before the
busy window and before the unload, so the rtsim arm's extra steps (adopts, reports, rescan) do not
shift its windows against the others'. Scenes are JSON:

    {"forceload": [[x1, z1, x2, z2], ...],       block coordinates, added 10 s apart; an optional fifth
                                                 element names the dimension (default minecraft:overworld)
     "adopt": [[x, y, z, radius], ...],          rtsim district adopt (rtsim arms only), optional dimension
     "idle": 60, "settle": 120, "busy": 300, "sample": 5,
     "unload_reload": 90,                        forceload remove all, wait, add again, wait
     "rescan": true,                             `rtsim district rescan` after the busy window (rtsim arms)
     "info_max": 12}                             districts to ask for info
"""
import argparse
import json
import os
import re
import shutil
import socket
import struct
import subprocess
import sys
import threading
import time

ROOT = os.path.expanduser("~/mic-stress-worlds")
BASE = f"{ROOT}/srv/base"
RUNS = f"{ROOT}/runs"
JARS = f"{ROOT}/jars/current"
JAVA = "/nix/store/x92dxglx7ziz2sc3wfvcyl6v2n34w4c4-openjdk-21.0.12.1+1/bin/java"
JCMD = "/nix/store/x92dxglx7ziz2sc3wfvcyl6v2n34w4c4-openjdk-21.0.12.1+1/bin/jcmd"
PW = "swstress-local-only"
CLK = os.sysconf("SC_CLK_TCK")
NO_HIPPO = """// Stress-world workaround, not part of the pack (same as the deploy rehearsal's): Ice and Fire
// 2.1-beta.1's HippocampusExplorationGoal can crash the server ("Ticking entity"). Kept out of every run.
EntityEvents.spawned('iceandfire:hippocampus', event => { event.cancel() })
"""

_print_lock = threading.Lock()


def log(*a):
    with _print_lock:
        print(time.strftime("%H:%M:%S"), *a, flush=True)


# ---------------------------------------------------------------------------- rcon
class Rcon:
    def __init__(self, port, timeout=900):
        self.s = socket.create_connection(("127.0.0.1", port), timeout=timeout)
        self.rid = 0
        self._send(3, PW)
        rid, _t, _b = self._recv()
        if rid == -1:
            raise RuntimeError("rcon auth failed")

    def _send(self, typ, body):
        self.rid += 1
        data = struct.pack("<ii", self.rid, typ) + body.encode() + b"\x00\x00"
        self.s.sendall(struct.pack("<i", len(data)) + data)
        return self.rid

    def _recvn(self, n):
        buf = b""
        while len(buf) < n:
            c = self.s.recv(n - len(buf))
            if not c:
                raise ConnectionError("rcon closed")
            buf += c
        return buf

    def _recv(self):
        (ln,) = struct.unpack("<i", self._recvn(4))
        d = self._recvn(ln)
        rid, typ = struct.unpack("<ii", d[:8])
        return rid, typ, d[8:-2].decode("utf-8", "replace")

    def cmd(self, c):
        # Vanilla splits a long response into 4096-char packets with the command's id, so a length test
        # cannot find the end (multibyte text). A second, unknown-type packet gets its own reply
        # ("Unknown request 0") only after the command's last packet: read up to it. It goes out only
        # once the first reply is in: vanilla closes the connection when one read holds two packets.
        rid = self._send(2, c)
        out = []
        r, _t, body = self._recv()
        if r == rid:
            out.append(body)
        mid = self._send(0, "")
        while True:
            r, _t, body = self._recv()
            if r == mid:
                break
            if r == rid:
                out.append(body)
        return re.sub(r"§.", "", "".join(out))


# ---------------------------------------------------------------------------- /proc
def proc_cpu(pid):
    f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return (int(f[11]) + int(f[12])) / CLK


def threads_cpu(pid):
    """CPU seconds by thread name (summed over threads with the same name)."""
    out = {}
    for tid in os.listdir(f"/proc/{pid}/task"):
        try:
            st = open(f"/proc/{pid}/task/{tid}/stat").read()
        except OSError:
            continue
        comm = st[st.index("(") + 1:st.rindex(")")]
        f = st.rsplit(")", 1)[1].split()
        out[comm] = out.get(comm, 0.0) + (int(f[11]) + int(f[12])) / CLK
    return out


def rss_mb(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmRSS:"):
            return int(line.split()[1]) / 1024
    return 0.0


def load1():
    return float(open("/proc/loadavg").read().split()[0])


def mem_avail_gb():
    for line in open("/proc/meminfo"):
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) / 1024 / 1024
    return 0.0


def du_bytes(path):
    tot = 0
    for dp, _dn, fn in os.walk(path):
        for f in fn:
            try:
                tot += os.lstat(os.path.join(dp, f)).st_size
            except OSError:
                pass
    return tot


def jcmd(pid, *args, timeout=600):
    r = subprocess.run([JCMD, str(pid), *args], capture_output=True, text=True, timeout=timeout)
    return r.stdout


def heap(pid):
    """Live heap from a class histogram (a full GC), metaspace, RSS (memory note: not heap_info)."""
    h = jcmd(pid, "GC.class_histogram")
    m = re.search(r"Total\s+(\d+)\s+(\d+)", h)
    live = int(m.group(2)) / 1048576 if m else None
    top = []
    for line in h.splitlines()[3:13]:
        p = line.split()
        if len(p) >= 4:
            top.append((p[3], int(p[2]) // 1024))
    hi = jcmd(pid, "GC.heap_info")
    meta = re.search(r"Metaspace\s+used (\d+)K, committed (\d+)K", hi)
    tot = re.search(r"total (?:reserved )?(\d+)K, (?:committed (\d+)K, )?used (\d+)K", hi)
    return {"live_heap_mb": live, "metaspace_used_mb": int(meta.group(1)) / 1024 if meta else None,
            "metaspace_committed_mb": int(meta.group(2)) / 1024 if meta else None,
            "heap_used_after_gc_mb": int(tot.group(3)) / 1024 if tot else None,
            "rss_mb": rss_mb(pid), "top_classes_kb": top}


# ---------------------------------------------------------------------------- run dirs
def set_props(path, kv):
    lines = open(path).read().splitlines()
    seen = set()
    out = []
    for line in lines:
        k = line.split("=", 1)[0]
        if k in kv:
            out.append(f"{k}={kv[k]}")
            seen.add(k)
        else:
            out.append(line)
    for k, v in kv.items():
        if k not in seen:
            out.append(f"{k}={v}")
    open(path, "w").write("\n".join(out) + "\n")


def set_prometheus(d, port):
    """Prometheus Exporter (TBSMP's pack) binds 0.0.0.0:19565 in every server, so the second server of a
    pair dies with a BindException. Give each run its own loopback port; the [web] keys are rewritten in
    place, or the file is written whole when the mod is present but has not made its config yet."""
    if not any(f.lower().startswith("prometheus") and f.endswith(".jar") for f in os.listdir(f"{d}/mods")):
        return
    p = f"{d}/config/prometheus_exporter-server.toml"
    want = {"listen_address": '"127.0.0.1"', "listen_port": str(port)}
    if not os.path.exists(p):
        os.makedirs(os.path.dirname(p), exist_ok=True)
        open(p, "w").write('[collector]\n\tjvm = true\n\tmc = true\n\tmc_dimension_tick_errors = "LOG"\n\tmc_entities = true\n\n'
                           "[web]\n" + "".join(f"\t{k} = {v}\n" for k, v in want.items()))
        return
    out, sect = [], None
    for line in open(p).read().splitlines():
        s = line.strip()
        if s.startswith("["):
            sect = s.strip("[]")
        k = s.split("=", 1)[0].strip()
        if sect == "web" and "=" in s and k in want:
            line = line[: len(line) - len(line.lstrip())] + f"{k} = {want[k]}"
        out.append(line)
    open(p, "w").write("\n".join(out) + "\n")


def neoforge_version(d):
    vs = sorted(os.listdir(f"{d}/libraries/net/neoforged/neoforge"))
    vs = [v for v in vs if os.path.isfile(f"{d}/libraries/net/neoforged/neoforge/{v}/unix_args.txt")]
    if len(vs) != 1:
        raise SystemExit(f"want one NeoForge under {d}/libraries, found {vs}")
    return vs[0]


def world_root(src):
    """The directory holding level.dat (downloads wrap it in a folder)."""
    if os.path.isfile(os.path.join(src, "level.dat")):
        return src
    for d in sorted(os.listdir(src)):
        if os.path.isfile(os.path.join(src, d, "level.dat")):
            return os.path.join(src, d)
    raise SystemExit(f"no level.dat under {src}")


def prep(run, world, rtsim, slot, xmx="6G", world_from_run=None):
    d = f"{RUNS}/{run}"
    if os.path.exists(d):
        shutil.rmtree(d)
    os.makedirs(RUNS, exist_ok=True)
    subprocess.run(["cp", "-a", "--reflink=always", BASE, d], check=True)
    if world_from_run:
        subprocess.run(["cp", "-a", "--reflink=always", f"{RUNS}/{world_from_run}/world", f"{d}/world"], check=True)
    else:
        subprocess.run(["cp", "-a", "--reflink=always", world_root(world), f"{d}/world"], check=True)
    for f in ("session.lock",):
        try:
            os.remove(f"{d}/world/{f}")
        except FileNotFoundError:
            pass
    set_props(f"{d}/server.properties", {
        "server-port": 26100 + slot, "rcon.port": 26200 + slot, "rcon.password": PW, "enable-rcon": "true",
        "level-name": "world", "motd": f"stress {run}", "max-tick-time": "-1", "online-mode": "false",
        "view-distance": 8, "simulation-distance": 8, "server-ip": "127.0.0.1", "sync-chunk-writes": "false",
        "level-seed": "", "spawn-protection": 0})
    if os.path.exists(f"{d}/config/voicechat/voicechat-server.properties"):
        set_props(f"{d}/config/voicechat/voicechat-server.properties", {"port": 26300 + slot})
    set_prometheus(d, 26400 + slot)
    open(f"{d}/user_jvm_args.txt", "w").write(
        f"-Xms2G\n-Xmx{xmx}\n-XX:+UseG1GC\n-Xlog:gc*:file=gc.log:time,uptime:filecount=2,filesize=20M\n")
    os.makedirs(f"{d}/kubejs/server_scripts", exist_ok=True)
    open(f"{d}/kubejs/server_scripts/zz_stress_no_hippocampus.js", "w").write(NO_HIPPO)
    set_rtsim(d, rtsim)
    return d


def set_rtsim(d, on):
    for f in os.listdir(f"{d}/mods"):
        if f.startswith("mic_rtsim") or f.startswith("mic_native"):
            os.remove(f"{d}/mods/{f}")
    if on:
        for f in os.listdir(JARS):
            if f.endswith(".jar"):
                shutil.copy(f"{JARS}/{f}", f"{d}/mods/{f}")


# ---------------------------------------------------------------------------- one server
class Server:
    def __init__(self, run, slot, rtsim, extra_args=()):
        self.run, self.slot, self.rtsim = run, slot, rtsim
        self.extra_args = list(extra_args)
        self.dir = f"{RUNS}/{run}"
        self.port = 26200 + slot
        self.proc = None
        self.res = {"run": run, "rtsim": rtsim, "samples": [], "events": []}
        self._rc = None

    def ev(self, what, **kw):
        kw.update({"t": time.time(), "what": what})
        self.res["events"].append(kw)

    def start(self):
        os.makedirs(f"{self.dir}/logs", exist_ok=True)
        con = open(f"{self.dir}/console.log", "w")
        args = ["nice", "-n", "10", JAVA, "@user_jvm_args.txt",
                f"@libraries/net/neoforged/neoforge/{neoforge_version(self.dir)}/unix_args.txt", "nogui"] + self.extra_args
        self.t0 = time.time()
        self.proc = subprocess.Popen(args, cwd=self.dir, stdout=con, stderr=subprocess.STDOUT,
                                     stdin=subprocess.DEVNULL)
        self.pid = self.proc.pid
        open(f"{self.dir}/sw.pid", "w").write(str(self.pid))
        log(self.run, "started pid", self.pid, "rtsim" if self.rtsim else "plain")

    def wait_done(self, timeout=1800):
        pat = re.compile(r"Done \((\d+\.\d+)s\)!")
        while time.time() - self.t0 < timeout:
            if self.proc.poll() is not None:
                raise RuntimeError(f"{self.run} exited during boot ({self.proc.returncode})")
            try:
                txt = open(f"{self.dir}/console.log", errors="replace").read()
            except FileNotFoundError:
                txt = ""
            m = pat.search(txt)
            if m:
                self.res["boot_wall_s"] = time.time() - self.t0
                self.res["boot_done_s"] = float(m.group(1))
                self.res["boot_cpu_s"] = proc_cpu(self.pid)
                log(self.run, "Done in", round(self.res["boot_wall_s"]), "s wall")
                for _ in range(60):
                    try:
                        self._rc = Rcon(self.port)
                        return
                    except OSError:
                        time.sleep(2)
                raise RuntimeError("rcon never came up")
            time.sleep(2)
        raise RuntimeError(f"{self.run} no Done within {timeout}s")

    def cmd(self, c):
        try:
            return self._rc.cmd(c)
        except Exception as e:  # reconnect once
            log(self.run, "rcon error", e, "reconnecting")
            self._rc = Rcon(self.port)
            return self._rc.cmd(c)

    def gametime(self):
        m = re.search(r"(\d+)", self.cmd("time query gametime"))
        return int(m.group(1)) if m else None

    def tps(self):
        out = self.cmd("neoforge tps")
        m = re.search(r"Overall: ([\d.]+) TPS \(([\d.]+) ms/tick\)", out)
        dims = re.findall(r"(\S+): ([\d.]+) TPS \(([\d.]+) ms/tick\)", out)
        return (float(m.group(2)) if m else None), {d: float(ms) for d, _t, ms in dims if d != "Overall:"}

    def window(self, name, seconds, sample):
        """Measure a window: neoforge tps every `sample` s, CPU per tick (process and server thread)."""
        g0, c0, th0, t0 = self.gametime(), proc_cpu(self.pid), threads_cpu(self.pid), time.time()
        ms = []
        end = t0 + seconds
        while time.time() < end:
            time.sleep(min(sample, max(0.0, end - time.time())))
            mspt, dims = self.tps()
            s = {"window": name, "t": time.time(), "mspt": mspt, "load1": load1(), "rss_mb": rss_mb(self.pid)}
            self.res["samples"].append(s)
            if mspt is not None:
                ms.append(mspt)
        g1, c1, th1, t1 = self.gametime(), proc_cpu(self.pid), threads_cpu(self.pid), time.time()
        ticks = (g1 - g0) if (g0 is not None and g1 is not None) else None
        thd = {k: round(th1.get(k, 0) - th0.get(k, 0), 2) for k in th1 if th1.get(k, 0) - th0.get(k, 0) > 0.5}
        st = th1.get("Server thread", 0) - th0.get("Server thread", 0)
        w = {"seconds": t1 - t0, "ticks": ticks, "tps": ticks / (t1 - t0) if ticks else None,
             "mspt_mean": sum(ms) / len(ms) if ms else None,
             "mspt_samples": ms,
             "cpu_ms_per_tick": 1000 * (c1 - c0) / ticks if ticks else None,
             "server_thread_ms_per_tick": 1000 * st / ticks if ticks else None,
             "threads_cpu_s": dict(sorted(thd.items(), key=lambda kv: -kv[1])[:25]),
             "load1_end": load1()}  # no "spark tps": spark answers asynchronously, into the next command's output
        self.res.setdefault("windows", {})[name] = w
        log(self.run, name, "mspt", round(w["mspt_mean"] or -1, 2), "cpu/tick", round(w["cpu_ms_per_tick"] or -1, 2),
            "srv/tick", round(w["server_thread_ms_per_tick"] or -1, 2), "tps", round(w["tps"] or -1, 2))
        return w

    def rtsim_report(self, key, info_max=12):
        if not self.rtsim:
            return
        r = {"perf": self.cmd("rtsim perf"), "district_perf": self.cmd("rtsim district perf"),
             "district_list": self.cmd("rtsim district list")}
        # rtsim's own tick handlers since the last `rtsim perf reset` (choice SG7)
        m = re.search(r"tick cost: (\d+) ticks since the reset; mean ([\d.]+) us, p50 ([\d.]+) us, p99 ([\d.]+) us, "
                      r"p99\.9 ([\d.]+) us, worst ([\d.]+) us \(game tick (-?\d+)\)", r["perf"])
        if m:
            r["tick_cost"] = {"ticks": int(m.group(1)), "mean_us": float(m.group(2)), "p50_us": float(m.group(3)),
                              "p99_us": float(m.group(4)), "p999_us": float(m.group(5)), "worst_us": float(m.group(6)),
                              "worst_at": int(m.group(7))}
            log(self.run, key, "rtsim tick cost: mean", m.group(2), "us, p99", m.group(4), "us, worst", m.group(6), "us")
        rows = re.findall(r"^\s+(\d+) '([^']*)' (\S+), (\d+) chunks, (\d+) inventories, load (-?\d+)",
                          r["district_list"], re.M)
        r["n_districts"] = len(rows)
        r["districts"] = [{"id": int(i), "name": n, "dim": dm, "chunks": int(c), "inventories": int(v), "load": int(l)}
                          for i, n, dm, c, v, l in rows]
        info = {}
        for d in sorted(r["districts"], key=lambda d: -d["inventories"])[:info_max]:
            info[str(d["id"])] = self.cmd(f"rtsim district info {d['id']}")
        r["info"] = info
        self.res.setdefault("rtsim_reports", {})[key] = r

    def stop(self):
        t = time.time()
        try:
            self.cmd("stop")
        except Exception:
            pass
        # A crash during stop (TBSMP: codmod's API server binds a fixed port, so the second server of a
        # pair has none and its ServerStoppingEvent handler throws) leaves the JVM up on mods' non-daemon
        # threads after the worlds are saved. Record the crash and end the process a minute later.
        crash_dir, crashed, deadline = f"{self.dir}/crash-reports", None, t + 900
        while self.proc.poll() is None and time.time() < deadline:
            time.sleep(2)
            if crashed is None and os.path.isdir(crash_dir):
                new = [f for f in os.listdir(crash_dir) if os.path.getmtime(f"{crash_dir}/{f}") >= t - 1]
                if new:
                    head = open(f"{crash_dir}/{sorted(new)[0]}", errors="replace").read().split("\n\n", 2)
                    crashed = head[2].strip().splitlines()[0][:300] if len(head) > 2 else "crash report"
                    self.res["stop_crash"] = crashed
                    log(self.run, "crashed while stopping:", crashed)
                    deadline = min(deadline, time.time() + 60)
        if self.proc.poll() is None:
            log(self.run, f"did not stop in {round(time.time() - t)} s, SIGTERM")
            self.proc.terminate()
            self.proc.wait(timeout=300)
        self.res["stop_s"] = time.time() - t
        self.res["exit"] = self.proc.returncode
        log(self.run, "stopped in", round(self.res["stop_s"]), "s, exit", self.proc.returncode)

    def save(self):
        json.dump(self.res, open(f"{self.dir}/sw-result.json", "w"), indent=1)


def forceload(s, boxes, add=True):
    for b in boxes:
        x1, z1, x2, z2 = b[:4]
        dim = b[4] if len(b) > 4 else "minecraft:overworld"
        out = s.cmd(f"execute in {dim} run forceload {'add' if add else 'remove'} {x1} {z1} {x2} {z2}")
        s.ev("forceload", add=add, box=list(b), out=out[:200])


def scene_run(s, sc, profile_s, barrier=None):
    smp = sc.get("sample", 5)
    if barrier:
        barrier.wait()
    s.ev("idle")
    s.window("idle", sc.get("idle", 60), smp)
    s.rtsim_report("idle")
    boxes = sc.get("forceload", [])
    for i, b in enumerate(boxes):
        forceload(s, [b])
        if i + 1 < len(boxes):
            time.sleep(10)
    s.ev("forceloaded")
    time.sleep(sc.get("settle", 120) / 2)
    if s.rtsim:
        for b in sc.get("adopt", []):
            x, y, z, r = b[:4]
            dim = b[4] if len(b) > 4 else "minecraft:overworld"
            out = s.cmd(f"execute in {dim} positioned {x} {y} {z} run rtsim district adopt {r}")
            s.ev("adopt", at=list(b), out=out)
            log(s.run, "adopt", x, z, r, out.strip()[:120])
    time.sleep(sc.get("settle", 120) / 2)
    s.rtsim_report("settled", sc.get("info_max", 12))
    if barrier:
        # the rtsim arm's adopts and reports take longer than nothing (110 s at 200 districts):
        # all arms start the busy window together (choice SG22)
        barrier.wait()
    if s.rtsim:
        s.cmd("rtsim perf reset")  # the busy window's tick cost alone (SG7)
    s.window("busy", sc.get("busy", 300), smp)
    s.rtsim_report("busy", sc.get("info_max", 12))
    if profile_s:
        for f in os.listdir(f"{s.dir}/config/spark"):
            if f.endswith(".sparkprofile"):
                os.remove(f"{s.dir}/config/spark/{f}")
        s.cmd(f"spark profiler start --timeout {profile_s} --save-to-file")
        time.sleep(profile_s + 20)
        for _ in range(30):
            got = [f for f in os.listdir(f"{s.dir}/config/spark") if f.endswith(".sparkprofile")]
            if got:
                shutil.copy(f"{s.dir}/config/spark/{got[0]}", f"{s.dir}/busy.sparkprofile")
                break
            time.sleep(5)
    if sc.get("rescan") and s.rtsim:
        # one full detection on demand, timed by the bridge (the periodic pass's work; choice SG17)
        out = s.cmd("rtsim district rescan")
        s.res["rescan_out"] = out
        log(s.run, "rescan", out.strip()[:160])
    if sc.get("unload_reload"):
        w = sc["unload_reload"]
        if barrier:
            barrier.wait()  # unload together after the rtsim arm's reports and rescan (SG22)
        forceload(s, boxes, add=False)
        s.ev("unloaded")
        time.sleep(w)
        s.rtsim_report("unloaded", sc.get("info_max", 12))
        s.window("unloaded", 30, smp)
        forceload(s, boxes, add=True)
        s.ev("reloaded")
        time.sleep(w)
        s.rtsim_report("reloaded", sc.get("info_max", 12))
        s.window("reloaded", 60, smp)
    s.res["heap"] = heap(s.pid)
    t = time.time()
    out = s.cmd("save-all flush")
    s.res["save_flush_s"] = time.time() - t
    s.res["save_out"] = out[:300]
    s.res["world_bytes"] = du_bytes(f"{s.dir}/world")
    s.res["rtsim_dir_bytes"] = du_bytes(f"{s.dir}/world/rtsim") if os.path.isdir(f"{s.dir}/world/rtsim") else 0
    s.res["data_files"] = {f: os.path.getsize(f"{s.dir}/world/data/{f}") for f in os.listdir(f"{s.dir}/world/data")
                           if f.startswith("mic_rtsim") or f.startswith("create")}


def log_scan(d):
    """Errors and warnings in the server log, grouped by a normalised message."""
    path = f"{d}/logs/latest.log"
    groups = {}
    if not os.path.exists(path):
        return {}
    for line in open(path, errors="replace"):
        m = re.match(r"\[[^\]]+\] \[([^\]]+)/(WARN|ERROR|FATAL)\] \[([^\]]*)\]: (.*)", line)
        if not m:
            continue
        msg = re.sub(r"\d+", "N", m.group(4))[:160]
        k = f"{m.group(2)} [{m.group(3)}] {msg}"
        groups[k] = groups.get(k, 0) + 1
    return dict(sorted(groups.items(), key=lambda kv: -kv[1]))


def run_arm(s, sc, profile_s, barrier, delay):
    try:
        time.sleep(delay)
        s.start()
        s.wait_done()
        scene_run(s, sc, profile_s, barrier)
        s.stop()
    except Exception as e:
        import traceback
        s.res["error"] = traceback.format_exc()
        log(s.run, "FAILED", e)
        if barrier:
            barrier.abort()
        try:
            s.stop()
        except Exception:
            pass
    s.res["log_groups"] = log_scan(s.dir)
    s.save()


def wait_mem(need_gb):
    while mem_avail_gb() < need_gb:
        log(f"waiting for memory: {mem_avail_gb():.1f} GiB available, need {need_gb}")
        time.sleep(60)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["prep", "pair", "boot", "upgrade", "rcon"])
    ap.add_argument("name")
    ap.add_argument("--world")
    ap.add_argument("--scene")
    ap.add_argument("--arms", default="off,on")
    ap.add_argument("--xmx", default="6G")
    ap.add_argument("--profile", type=int, default=0)
    ap.add_argument("--slot", type=int, default=0)
    ap.add_argument("--rtsim", action="store_true")
    ap.add_argument("--no-rtsim", action="store_true")
    ap.add_argument("--world-from-run")
    ap.add_argument("--need-gb", type=float, default=0)
    ap.add_argument("--base", help="server dir to copy (default ~/mic-stress-worlds/srv/base)")
    ap.add_argument("--runs", help="where run dirs go (default ~/mic-stress-worlds/runs)")
    ap.add_argument("--jars", help="rtsim jars for rtsim arms (default ~/mic-stress-worlds/jars/current)")
    a = ap.parse_args()
    global BASE, RUNS, JARS
    BASE = os.path.expanduser(a.base) if a.base else BASE
    RUNS = os.path.expanduser(a.runs) if a.runs else RUNS
    JARS = os.path.expanduser(a.jars) if a.jars else JARS
    if a.cmd == "upgrade":
        # A 1.19.2 world through vanilla's DataFixer on the pack server (--forceUpgrade), no rtsim; the
        # world's own serverconfig (1.19.2 mod configs) is dropped so the pack's defaultconfigs apply.
        wait_mem(a.need_gb or 8)
        d = prep(a.name, a.world, False, a.slot, a.xmx)
        shutil.rmtree(f"{d}/world/serverconfig", ignore_errors=True)
        s = Server(a.name, a.slot, False, ["--forceUpgrade"])
        try:
            s.start()
            s.wait_done(timeout=7200)
            s.res["heap"] = heap(s.pid)
            s.stop()
        except Exception:
            import traceback
            s.res["error"] = traceback.format_exc()
            log(a.name, "FAILED")
            try:
                s.stop()
            except Exception:
                pass
        s.res["log_groups"] = log_scan(d)
        s.save()
        return
    if a.cmd == "rcon":
        # sw.py rcon "<command>" --slot N: one command to a running server (a second RCON client).
        print(Rcon(26200 + a.slot, timeout=60).cmd(a.name))
        return
    if a.cmd == "prep":
        print(prep(a.name, a.world, a.rtsim, a.slot, a.xmx, a.world_from_run))
        return
    sc = json.load(open(a.scene)) if a.scene else {}
    if a.cmd == "pair":
        arms = a.arms.split(",")
        wait_mem(a.need_gb or 8 * len(arms))
        servers = []
        for i, arm in enumerate(arms):
            run = f"{a.name}-{chr(65 + i)}-{arm}"
            prep(run, a.world, arm == "on", a.slot + i, a.xmx, a.world_from_run)
            servers.append(Server(run, a.slot + i, arm == "on"))
        barrier = threading.Barrier(len(servers))
        ths = [threading.Thread(target=run_arm, args=(s, sc, a.profile, barrier, 5 * i)) for i, s in enumerate(servers)]
        for t in ths:
            t.start()
        for t in ths:
            t.join()
        summary = {}
        for s in servers:
            summary[s.run] = {k: (v.get("mspt_mean"), v.get("cpu_ms_per_tick"), v.get("server_thread_ms_per_tick"))
                              for k, v in s.res.get("windows", {}).items()}
        print(json.dumps(summary, indent=1))
    elif a.cmd == "boot":
        # A new run dir whose world is a reflink copy of an earlier run's saved world.
        wait_mem(a.need_gb or 8)
        on = a.rtsim and not a.no_rtsim
        prep(a.name, None, on, a.slot, a.xmx, a.world_from_run)
        s = Server(a.name, a.slot, on)
        s.res["boot_kind"] = f"reboot of {a.world_from_run}"
        run_arm(s, sc, a.profile, None, 0)


if __name__ == "__main__":
    main()
