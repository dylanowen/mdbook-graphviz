SHELL:=/bin/bash

.DEFAULT_GOAL := default

format:
	cargo fmt

build: format
	cargo build

install: format
	cargo install --force --path .

default: build

clean:
	cargo clean