#!/usr/bin/env python3
"""Benchmark programu `latency`: czas w funkcji M i liczby watkow, z cache i bez.

Zgodnie z PROJECT.md:
  * czas mierzony zegarem monotonicznym, raportowane minimum z kilku powtorzen,
  * "cold" = przed kazdym pomiarem `sudo sysctl -w vm.drop_caches=3`,
  * "warm" = plik wczytany wczesniej do page cache,
  * opcjonalnie limit pamieci przez `systemd-run --scope -p MemoryMax=...`.

Program wywolywany jest jako:  latency <plik> <watki> <M>
Jesli program wypisze na stderr linie `avg_read_us=<liczba>`, zostanie ona
zapisana w CSV (srednia latencja pojedynczego pread).

Przyklad:
  ./bench_latency.py --size 256M --threads 1,2,4,8,16,32 --block 512,4096,65536
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


def build():
    subprocess.run(["cargo", "build", "--release", "--bin", "latency"], cwd=SRC, check=True)
    return os.path.join(SRC, "target", "release", "latency")


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


def run_once(binary, path, threads, block, mem_max):
    cmd = [binary, path, str(threads), str(block)]
    if mem_max:
        cmd = ["sudo", "systemd-run", "--quiet", "--scope", "-p", f"MemoryMax={mem_max}"] + cmd
    t0 = time.monotonic()
    p = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.monotonic() - t0
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} -> {p.returncode}\n{p.stderr}")
    lat = re.search(r"avg_read_us=([0-9.]+)", p.stderr)
    return elapsed, p.stdout.split()[-1], float(lat.group(1)) if lat else None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--file", default="/tmp/latency_bench.bin")
    ap.add_argument("--size", default="256M", help="rozmiar pliku testowego (np. 256M, 4G)")
    ap.add_argument("--threads", default="1,2,4,8,16,32,64")
    ap.add_argument("--block", default="512,4096,65536", help="wartosci M w bajtach")
    ap.add_argument("--cache", default="cold,warm", help="cold, warm lub oba")
    ap.add_argument("--reps", type=int, default=3, help="powtorzenia; raportowane minimum")
    ap.add_argument("--mem-max", default=None, help="np. 512M (systemd-run MemoryMax)")
    ap.add_argument("--binary", default=None, help="gotowa binarka zamiast cargo build")
    ap.add_argument("--out", default="latency_results.csv")
    a = ap.parse_args()

    binary = a.binary or build()
    make_file(a.file, parse_size(a.size))

    expected = {}
    with open(a.out, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["cache", "M", "threads", "min_s", "median_s", "avg_read_us", "hash"])
        for cache in a.cache.split(","):
            for block in ints(a.block):
                for threads in ints(a.threads):
                    times, lats, h = [], [], None
                    for _ in range(a.reps):
                        if cache == "cold":
                            drop_caches()
                        else:
                            warm_up(a.file)
                        t, h, lat = run_once(binary, a.file, threads, block, a.mem_max)
                        times.append(t)
                        if lat is not None:
                            lats.append(lat)
                    # wynik musi byc ten sam niezaleznie od liczby watkow i cache
                    if expected.setdefault(block, h) != h:
                        print(f"BLAD: rozny hash dla M={block} T={threads}: {h} != {expected[block]}",
                              file=sys.stderr)
                    row = [cache, block, threads, f"{min(times):.4f}", f"{statistics.median(times):.4f}",
                           f"{min(lats):.2f}" if lats else "", h]
                    w.writerow(row)
                    f.flush()
                    print("\t".join(map(str, row)), flush=True)
    print(f"Zapisano {a.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
