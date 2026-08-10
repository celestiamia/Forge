# Makefile for the Forge Brainfuck interpreter
#
# Usage (from this directory):
#   make build    - compile the interpreter (x86_64 by default)
#   make run      - build and run it on hello.bf ("Hello World!")
#   make clean    - remove the compiled binary
#
# Build for 32-bit instead:
#   make TARGET=x86_32-unknown-linux-gnu build

PROJECT_ROOT := $(abspath $(CURDIR)/../..)
FORGEC       := $(PROJECT_ROOT)/target/release/forgec
TARGET       ?= x86_64-unknown-linux-gnu
SRC          := brainfuck.dev
BIN          := bf

.PHONY: build run clean

build:
	$(FORGEC) "$(PROJECT_ROOT)/examples/brainfuck/$(SRC)" -o "$(BIN)" --target "$(TARGET)"

run: build
	./$(BIN) hello.bf

clean:
	rm -f $(BIN)
