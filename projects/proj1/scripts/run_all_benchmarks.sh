#!/bin/bash
# Pelny przebieg benchmarkow throughput i pipeline (po kolei, pod wspolna blokada).
# Latency: scripts/bench_latency.py (osobno). Wyniki w results/, wykresy: make plots.
set -euo pipefail
cd "$(dirname "$0")/.."
L="flock /tmp/isbd_bench.lock"
B=src/target/release
(cd src && cargo build --release -q)
log() { echo "[$(date +%T)] $*"; vmstat 1 2 | tail -1; }

log throughput 1G
$L python3 scripts/bench_throughput.py --binary $B/throughput --size 1G --block 512,4K,64K,1M,16M --modes read,mmap --cache cold,warm --reps 3 --keep-file --out results/throughput_1G.csv
log throughput 1G mem512M
$L python3 scripts/bench_throughput.py --binary $B/throughput --size 1G --block 4K,64K,1M --modes read,mmap --cache cold --reps 3 --mem-max 512M --out results/throughput_1G_mem512M.csv

P="python3 scripts/bench_pipeline.py --binary $B/pipeline --size 512M"
K=1,10,100,1000,2000,5000,10000,20000,50000,100000
log pipeline K
$L $P --cache warm,cold --k $K --configs seq,pipe --out results/pipeline_k.csv
log pipeline K memmax
$L $P --cache cold --mem-max 256M --k 1,1000,10000,100000 --configs seq,pipe --out results/pipeline_k_memmax.csv
log pipeline K iomax
$L $P --cache cold --io-max "/dev/sda 200M" --k $K --configs seq,pipe --reps 2 --out results/pipeline_k_iomax.csv
log pipeline threads
TH=seq,pipe:1:1,pipe:1:2,pipe:1:3,pipe:2:1,pipe:2:2,pipe:3:3,pipe:1:1:1,pipe:1:1:16
$L $P --cache warm,cold --k 1,50000 --configs $TH --out results/pipeline_threads.csv
log pipeline threads iomax
$L $P --cache cold --io-max "/dev/sda 200M" --k 1,50000 --configs $TH --reps 2 --out results/pipeline_threads_iomax.csv
log pipeline M
$L $P --cache warm,cold --block 4K,16K,64K,256K,1M,4M,16M --k 1,100,1000 --configs seq,pipe --out results/pipeline_m.csv
rm -f /tmp/throughput_bench.bin /tmp/pipeline_bench.bin
log KONIEC
