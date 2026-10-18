#!/usr/bin/env python3
"""Benchmark programu `pipeline`: czas w funkcji M i K, seq vs pipe, cold/warm.

Zgodnie z PROJECT.md:
  * czas mierzony zegarem monotonicznym, raportowane minimum z kilku powtorzen,
  * "cold" = przed kazdym pomiarem `sync; sudo sysctl -w vm.drop_caches=3`,
  * "warm" = plik wczytany wczesniej do page cache,
  * opcjonalnie limit pamieci (proces + page cache) przez
    `sudo systemd-run --scope -p MemoryMax=...` (plik wiekszy niz limit jest
    wtedy stale czytany z dysku; przy --mem-max cache jest i tak czyszczony
    przed kazdym pomiarem, bo strony z poprzedniego scope'u zostaja w pamieci),
  * opcjonalnie symulacja wolniejszego dysku (HDD/SATA) przez cgroup io:
    `--io-max "/dev/sda 200M"` -> `systemd-run -p IOReadBandwidthMax=...`
    (dysk QEMU jest szybszy niz MD5, wiec bez tego IO nigdy nie jest waskim gardlem).

Program wywolywany jest jako:  pipeline <plik> <M> <K> <seq|pipe> <readers> <hashers> <cap>
Na stderr wypisuje `read_s=... compute_s=... wall_s=...` (sumy po watkach),
co jest zapisywane w CSV (dla najszybszego powtorzenia).

Konfiguracje (--configs) to lista rozdzielona przecinkami:
  seq                    - baseline (read i compute w jednej petli)
  pipe                   - 1 reader, 1 hasher, cap=4
  pipe:R:H[:CAP]         - R readerow, H hasherow, pojemnosc kanalu CAP
  seq-direct, pipe-direct[:R:H[:CAP]] - jw., ale plik otwierany z O_DIRECT
                         (bez page cache i readahead jadra)

Przyklady:
  ./bench_pipeline.py --block 1M --k 1,10,100,1000,10000,100000 --cache warm,cold \\
      --out results/pipeline_k.csv
  ./bench_pipeline.py --block 4K,64K,1M,16M --k 1,1000 --out results/pipeline_m.csv
  ./bench_pipeline.py --k 1,100000 --configs seq,pipe:1:1,pipe:1:2,pipe:2:1,pipe:2:2 \\
      --out results/pipeline_threads.csv
"""
import argparse
import csv
import os
import re
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "..", "src")


def parse_size(s):
    units = {"K": 1 << 10, "M": 1 << 20, "G": 1 << 30}
    s = s.strip().upper()
    return int(s[:-1]) * units[s[-1]] if s[-1] in units else int(s)


def ints(s):
    return [parse_size(x) for x in s.split(",")]


def parse_config(spec):
    parts = spec.strip().split(":")
    if parts[0] in ("seq", "seq-direct"):
        return (parts[0], 1, 1, 4)
    if parts[0] not in ("pipe", "pipe-direct"):
        raise ValueError(f"nieznana konfiguracja: {spec}")
    r = int(parts[1]) if len(parts) > 1 else 1
    h = int(parts[2]) if len(parts) > 2 else 1
    cap = int(parts[3]) if len(parts) > 3 else 4
    return (parts[0], r, h, cap)


def build():
    subprocess.run(["cargo", "build", "--release", "--bin", "pipeline"], cwd=SRC, check=True)
    return os.path.join(SRC, "target", "release", "pipeline")


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


STAT_RE = {k: re.compile(rf"{k}=([0-9.]+)") for k in ("read_s", "compute_s", "wall_s")}


def run_once(binary, path, block, k, cfg, mem_max, io_max=None):
    mode, r, h, cap = cfg
    cmd = [binary, path, str(block), str(k), mode, str(r), str(h), str(cap)]
    props = []
    if mem_max:
        props += ["-p", f"MemoryMax={mem_max}"]
    if io_max:
        props += ["-p", f"IOReadBandwidthMax={io_max}"]
    if props:
        cmd = ["sudo", "systemd-run", "--quiet", "--scope"] + props + cmd
    t0 = time.monotonic()
    p = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.monotonic() - t0
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} -> {p.returncode}\n{p.stderr}")
    stats = {}
    for key, rx in STAT_RE.items():
        m = rx.search(p.stderr)
        stats[key] = float(m.group(1)) if m else None
    return elapsed, p.stdout.split()[-1], stats


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--file", default="/tmp/pipeline_bench.bin")
    ap.add_argument("--size", default="512M", help="rozmiar pliku testowego (np. 256M, 1G)")
    ap.add_argument("--block", default="1M", help="wartosci M (sufiksy K/M/G)")
    ap.add_argument("--k", default="1,10,100,1000,10000,100000", help="wartosci K")
    ap.add_argument("--configs", default="seq,pipe", help="np. seq,pipe,pipe:2:2:8")
    ap.add_argument("--cache", default="warm,cold", help="cold, warm lub oba")
    ap.add_argument("--reps", type=int, default=3, help="powtorzenia; raportowane minimum")
    ap.add_argument("--mem-max", default=None, help="np. 256M (systemd-run MemoryMax)")
    ap.add_argument("--io-max", default=None,
                    help='symulacja wolniejszego dysku, np. "/dev/sda 200M" (systemd-run IOReadBandwidthMax)')
    ap.add_argument("--binary", default=None, help="gotowa binarka zamiast cargo build")
    ap.add_argument("--out", default="pipeline_results.csv")
    ap.add_argument("--append", action="store_true", help="dopisz do istniejacego CSV")
    a = ap.parse_args()

    binary = a.binary or build()
    make_file(a.file, parse_size(a.size))
    configs = [parse_config(c) for c in a.configs.split(",")]
    size = os.path.getsize(a.file)

    expected = {}
    new = not (a.append and os.path.exists(a.out))
    with open(a.out, "a" if not new else "w", newline="") as f:
        w = csv.writer(f)
        if new:
            w.writerow(["cache", "mem_max", "io_max", "size", "M", "K", "mode", "readers", "hashers", "cap",
                        "min_s", "median_s", "read_s", "compute_s", "prog_wall_s", "hash"])
        for cache in a.cache.split(","):
            for block in ints(a.block):
                for k in ints(a.k):
                    for cfg in configs:
                        runs, h = [], None
                        for _ in range(a.reps):
                            if cache == "cold" or a.mem_max or a.io_max:
                                drop_caches()
                            else:
                                warm_up(a.file)
                            t, h, st = run_once(binary, a.file, block, k, cfg, a.mem_max, a.io_max)
                            runs.append((t, st))
                            if expected.setdefault((block, k), h) != h:
                                print(f"BLAD: rozny hash dla M={block} K={k} {cfg}: {h} != "
                                      f"{expected[(block, k)]}", file=sys.stderr)
                        best_t, best_st = min(runs, key=lambda x: x[0])
                        fmt = lambda v: f"{v:.4f}" if v is not None else ""
                        mode, r, hh, cap = cfg
                        row = [cache, a.mem_max or "", (a.io_max or "").split()[-1] if a.io_max else "", size, block, k, mode, r, hh, cap,
                               f"{best_t:.4f}", f"{statistics.median(x[0] for x in runs):.4f}",
                               fmt(best_st["read_s"]), fmt(best_st["compute_s"]),
                               fmt(best_st["wall_s"]), h]
                        w.writerow(row)
                        f.flush()
                        print("\t".join(map(str, row)), flush=True)
    print(f"Zapisano {a.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
