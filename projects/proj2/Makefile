CARGO ?= cargo
SRC := src
BIN := $(SRC)/target/release/isbd

.PHONY: all build test sample stats clean

all: build

build:
	cd $(SRC) && $(CARGO) build --release

test:
	cd $(SRC) && $(CARGO) test --release

sample: build
	$(BIN) gen sample.isbd 10000

stats: build
	$(BIN) stats sample.isbd

clean:
	cd $(SRC) && $(CARGO) clean
