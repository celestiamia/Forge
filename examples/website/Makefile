# Makefile for the Forge website server
#
# Usage (from this directory):
#   make build    - compile the server (x86_64)
#   make run      - build and start the server
#   make clean    - remove the compiled binary

PROJECT_ROOT := $(abspath $(CURDIR)/../..)
FORGEC       := $(PROJECT_ROOT)/target/release/forgec
TARGET       ?= x86_64-unknown-linux-gnu
SRC          := server.dev
BIN          := server

.PHONY: build run clean

build:
	$(FORGEC) "$(PROJECT_ROOT)/examples/website/$(SRC)" -o "$(BIN)" --target "$(TARGET)"

run: build
	./$(BIN)

clean:
	rm -f $(BIN)
