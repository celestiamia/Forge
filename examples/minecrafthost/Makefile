# Makefile for the minimal Minecraft server in Forge
#
# Usage (from this directory):
#   make build    - compile the Minecraft server (x86_64 by default)
#   make run      - build and run the server
#   make test     - run the integration test (requires cargo + port 25565 free)
#   make clean    - remove the compiled binary
#
# Build for 32-bit instead:
#   make TARGET=x86_32-unknown-linux-gnu build

PROJECT_ROOT := $(abspath $(CURDIR)/../..)
FORGEC       := $(PROJECT_ROOT)/target/release/forgec
TARGET       ?= x86_64-unknown-linux-gnu
SRC          := minecrafthost.dev
SRC_FILES    := $(wildcard $(SRC) net/*.dev play/*.dev world/*.dev)
BIN          := minecrafthost

.PHONY: build run test clean

build: $(SRC_FILES)
	$(FORGEC) "$(PROJECT_ROOT)/examples/minecrafthost/$(SRC)" -o "$(BIN)" --target "$(TARGET)"

run: build
	./$(BIN)

test: build
	cd $(PROJECT_ROOT) && cargo test --test integration minecrafthost_dev_compiles_and_responds

clean:
	rm -f $(BIN)
