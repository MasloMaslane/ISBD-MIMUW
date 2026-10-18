CARGO ?= cargo
SRC := src
BIN := $(SRC)/target/release

.PHONY: all build bench plots clean

all: build

build:
	cd $(SRC) && $(CARGO) build --release

# Benchmarki (wymagaja sudo do drop_caches / systemd-run); wyniki w results/
bench: build
	python3 scripts/bench_latency.py --size 256M --threads 1,2,3,4,8,16,32 --block 512,4096,16384,65536,262144 --reps 5 --out results/latency_256M.csv
	scripts/run_all_benchmarks.sh

plots:
	python3 scripts/plots.py

clean:
	cd $(SRC) && $(CARGO) clean
