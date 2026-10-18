#!/usr/bin/env python3
"""Benchmark programu `throughput`: read (pread blokami M) vs mmap (zero-copy).

Zgodnie z PROJECT.md:
  * czas mierzony zegarem monotonicznym, raportowane minimum z kilku powtorzen,
  * "cold" = przed kazdym pomiarem `sync; sudo sysctl -w vm.drop_caches=3`,
  * "warm" = plik wczytany wczesniej do page cache,
  * opcjonalnie limit pamieci (takze file cache) przez
    `sudo systemd-run --scope -p MemoryMax=...` -> symulacja pliku wiekszego niz RAM.

Oprocz czasu zapisywane jest zuzycie CPU procesu (user/sys z wait4/rusage)
oraz liczba page faultow (major/minor), bo pytanie ze sprawozdania dotyczy
obciazenia procesora.

Program wywolywany jest jako:  throughput <plik> <M> <tryb>
gdzie tryb to read | mmap | read-rand | mmap-rand. Program wypisuje na stderr
`elapsed_s=<czas>` (czas samego liczenia, bez startu procesu / sudo).

Przyklad:
  flock /tmp/isbd_bench.lock ./bench_throughput.py --size 1G \
      --block 512,4K,64K,1M,16M --modes read,mmap --cache cold,warm \
      --out ../results/throughput_1G.csv
"""
import argparse
import csv
import os
import re
import statistics
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "src")


def parse_size(s):
    units = {"K": 1 << 10, "M": 1 << 20, "G": 1 << 30}
    s = s.strip().upper()
    return int(s[:-1]) * units[s[-1]] if s[-1] in units else int(s)


def sizes(s):
    return [parse_size(x) for x in s.split(",")]


def build():
    subprocess.run(["cargo", "build", "--release", "--bin", "throughput"], cwd=SRC, check=True)
    return os.path.join(SRC, "target", "release", "throughput")


def make_file(path, size):
    if os.path.exists(path) and os.path.getsize(path) == size:
        return
    print(f"Tworze {path} ({size} B)...", file=sys.stderr)
    with open(path, "wb") as f:
        left = size
        while left:
            chunk = min(left, 64 << 20)
            f.write(os.urandom(chunk))
            left -= chunk
        f.flush()
        os.fsync(f.fileno())


def drop_caches():
    subprocess.run(["sync"], check=True)
    subprocess.run(["sudo", "sysctl", "-q", "-w", "vm.drop_caches=3"], check=True)


def warm_up(path):
    with open(path, "rb") as f:
        while f.read(16 << 20):
            pass


def run_once(binary, path, block, mode, mem_max):
    """Zwraca (wall_s, prog_s, user_s, sys_s, majflt, minflt, hash)."""
    cmd = [binary, path, str(block), mode]
    if mem_max:
        cmd = ["sudo", "systemd-run", "--quiet", "--scope", "-p", f"MemoryMax={mem_max}",
               "-p", "MemorySwapMax=0"] + cmd
    t0 = time.monotonic()
    with tempfile.TemporaryFile("w+") as fo, tempfile.TemporaryFile("w+") as fe:
        p = subprocess.Popen(cmd, stdout=fo, stderr=fe)
        # wait4 zamiast p.wait(), zeby dostac rusage (user/sys, page faulty) dziecka
        _, status, ru = os.wait4(p.pid, 0)
        wall = time.monotonic() - t0
        p.returncode = os.waitstatus_to_exitcode(status)
        fo.seek(0)
        fe.seek(0)
        out, err = fo.read(), fe.read()
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} -> {p.returncode}\n{err}")
    m = re.search(r"elapsed_s=([0-9.]+)", err)
    prog = float(m.group(1)) if m else wall
    return wall, prog, ru.ru_utime, ru.ru_stime, ru.ru_majflt, ru.ru_minflt, out.split()[-1]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--file", default="/tmp/throughput_bench.bin")
    ap.add_argument("--size", default="1G", help="rozmiar pliku testowego (np. 1G)")
    ap.add_argument("--block", default="512,4K,64K,1M,16M", help="wartosci M")
    ap.add_argument("--modes", default="read,mmap", help="read,mmap,read-rand,mmap-rand")
    ap.add_argument("--cache", default="cold,warm", help="cold, warm lub oba")
    ap.add_argument("--reps", type=int, default=3, help="powtorzenia; raportowane minimum")
    ap.add_argument("--mem-max", default=None, help="np. 512M (systemd-run MemoryMax)")
    ap.add_argument("--scenario", default=None, help="etykieta w CSV (domyslnie z --size/--mem-max)")
    ap.add_argument("--binary", default=None, help="gotowa binarka zamiast cargo build")
    ap.add_argument("--keep-file", action="store_true", help="nie usuwaj pliku testowego na koncu")
    ap.add_argument("--out", default="throughput_results.csv")
    a = ap.parse_args()

    binary = a.binary or build()
    size = parse_size(a.size)
    make_file(a.file, size)
    scenario = a.scenario or (f"{a.size}" + (f"_mem{a.mem_max}" if a.mem_max else ""))

    expected = {}
    hdr = ["scenario", "cache", "mode", "M", "size_B", "reps", "min_s", "median_s", "wall_min_s",
           "mb_s", "user_s", "sys_s", "majflt", "minflt", "hash"]
    with open(a.out, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(hdr)
        print("\t".join(hdr), flush=True)
        for cache in a.cache.split(","):
            for block in sizes(a.block):
                for mode in a.modes.split(","):
                    runs = []
                    for _ in range(a.reps):
                        if cache == "cold":
                            drop_caches()
                        else:
                            warm_up(a.file)
                        runs.append(run_once(binary, a.file, block, mode, a.mem_max))
                    best = min(runs, key=lambda r: r[1])
                    h = best[6]
                    if any(r[6] != h for r in runs):
                        print(f"BLAD: niedeterministyczny hash M={block} {mode}", file=sys.stderr)
                    # wynik R(F) nie zalezy od trybu ani cache (ale zalezy od M)
                    if expected.setdefault(block, h) != h:
                        print(f"BLAD: rozny hash dla M={block} {mode}: {h} != {expected[block]}",
                              file=sys.stderr)
                    row = [scenario, cache, mode, block, size, a.reps, f"{best[1]:.4f}",
                           f"{statistics.median(r[1] for r in runs):.4f}", f"{min(r[0] for r in runs):.4f}",
                           f"{size / best[1] / 1e6:.1f}", f"{best[2]:.3f}", f"{best[3]:.3f}",
                           best[4], best[5], h]
                    w.writerow(row)
                    f.flush()
                    print("\t".join(map(str, row)), flush=True)
    if not a.keep_file:
        os.remove(a.file)
    print(f"Zapisano {a.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
