#!/usr/bin/env python3
# Wykresy do sprawozdania: python3 scripts/plots.py  ->  results/plots/*.png
import csv, os
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "results")
OUT = os.path.join(R, "plots")
os.makedirs(OUT, exist_ok=True)


def load(name):
    with open(os.path.join(R, name)) as f:
        return list(csv.DictReader(f))


def fmt_bytes(v):
    v = int(v)
    for unit, s in ((1 << 20, "M"), (1 << 10, "K")):
        if v >= unit and v % unit == 0:
            return f"{v // unit}{s}"
    return str(v)


def latency():
    rows = [r for r in load("latency_256M.csv") if r["cache"] == "cold"]
    fig, (a, b) = plt.subplots(1, 2, figsize=(11, 4))
    for m in sorted({int(r["M"]) for r in rows}):
        rs = sorted((r for r in rows if int(r["M"]) == m), key=lambda r: int(r["threads"]))
        t = [int(r["threads"]) for r in rs]
        a.plot(t, [float(r["min_s"]) for r in rs], "o-", label=f"M={fmt_bytes(m)}")
        b.plot(t, [float(r["avg_read_us"]) for r in rs], "o-", label=f"M={fmt_bytes(m)}")
    for ax in (a, b):
        ax.set_xscale("log", base=2)
        ax.set_xlabel("liczba watkow")
        ax.grid(alpha=.3)
    a.set_ylabel("czas [s]"); a.set_title("Czas programu (cold, 256 MB)")
    b.set_yscale("log"); b.set_ylabel("sredni czas pread [us]"); b.set_title("Latencja pojedynczego odczytu")
    a.legend()
    fig.tight_layout(); fig.savefig(os.path.join(OUT, "latency.png"), dpi=120)


def throughput():
    rows = load("throughput_1G.csv")
    fig, (a, b) = plt.subplots(1, 2, figsize=(11, 4))
    for cache in ("cold", "warm"):
        for mode in ("read", "mmap"):
            rs = sorted((r for r in rows if r["cache"] == cache and r["mode"] == mode), key=lambda r: int(r["M"]))
            m = [int(r["M"]) for r in rs]
            style = "-" if mode == "mmap" else "--"
            a.plot(m, [float(r["min_s"]) for r in rs], "o" + style, label=f"{mode} {cache}")
            if cache == "warm":
                b.plot(m, [float(r["sys_s"]) for r in rs], "o" + style, label=mode)
    for ax in (a, b):
        ax.set_xscale("log", base=2)
        ax.set_xlabel("M [B]")
        ax.grid(alpha=.3); ax.legend()
    a.set_ylabel("czas [s]"); a.set_title("Odczyt sekwencyjny, 1 GB")
    b.set_ylabel("CPU sys [s]"); b.set_title("Czas w jadrze (warm)")
    fig.tight_layout(); fig.savefig(os.path.join(OUT, "throughput.png"), dpi=120)


def pipeline_k():
    fig, (a, b) = plt.subplots(1, 2, figsize=(11, 4), sharey=True)
    k = load("pipeline_k.csv")
    for cache in ("warm", "cold"):
        for mode in ("seq", "pipe"):
            rs = sorted((r for r in k if r["cache"] == cache and r["mode"] == mode), key=lambda r: int(r["K"]))
            a.plot([int(r["K"]) for r in rs], [float(r["min_s"]) for r in rs],
                   "o" + ("--" if mode == "seq" else "-"), label=f"{mode} {cache}")
    for name, label in (("pipeline_k_iomax.csv", "dysk 200 MB/s"),):
        rows = load(name)
        for mode in sorted({r["mode"] for r in rows}, reverse=True):
            rs = sorted((r for r in rows if r["mode"] == mode), key=lambda r: int(r["K"]))
            b.plot([int(r["K"]) for r in rs], [float(r["min_s"]) for r in rs],
                   "o" + ("--" if mode.startswith("seq") else "-"), label=f"{mode.split('-')[0]} {label}")
    for ax in (a, b):
        ax.set_xscale("log"); ax.set_xlabel("K"); ax.grid(alpha=.3); ax.legend()
    a.set_ylabel("czas [s]"); a.set_title("M=1M, 512 MB, page cache")
    b.set_title("M=1M, 512 MB, dysk ograniczony do 200 MB/s")
    fig.tight_layout(); fig.savefig(os.path.join(OUT, "pipeline_k.png"), dpi=120)


def pipeline_threads():
    rows = load("pipeline_threads.csv") + load("pipeline_threads_iomax.csv")

    def name(r):
        if r["mode"] == "seq":
            return "seq"
        return f"{r['readers']}R/{r['hashers']}H"

    configs = []
    for r in rows:
        if r["cap"] == "4" and name(r) not in configs:
            configs.append(name(r))
    series = [("warm, K=50000", lambda r: r["cache"] == "warm" and r["K"] == "50000" and not r.get("io_max")),
              ("cold, K=50000", lambda r: r["cache"] == "cold" and r["K"] == "50000" and not r.get("io_max")),
              ("200 MB/s, K=1", lambda r: r["K"] == "1" and r.get("io_max"))]
    fig, ax = plt.subplots(figsize=(9, 4))
    w = 0.8 / len(series)
    for i, (label, pred) in enumerate(series):
        vals = {name(r): float(r["min_s"]) for r in rows if pred(r) and r["cap"] == "4"}
        ax.bar([j + i * w for j in range(len(configs))], [vals.get(c, 0) for c in configs], w, label=label)
    ax.set_xticks([j + w for j in range(len(configs))], configs)
    ax.set_ylabel("czas [s]"); ax.set_title("Pipeline: liczba watkow czytajacych (R) i liczacych (H)")
    ax.grid(axis="y", alpha=.3); ax.legend()
    fig.tight_layout(); fig.savefig(os.path.join(OUT, "pipeline_threads.png"), dpi=120)


if __name__ == "__main__":
    for f in (latency, throughput, pipeline_k, pipeline_threads):
        f()
    print("zapisano", sorted(os.listdir(OUT)))
