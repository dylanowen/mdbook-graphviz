SHELL:=/bin/bash

.DEFAULT_GOAL := default

format:
	cargo fmt

build: format
	cargo build

default: build

clean:
	cargo clean