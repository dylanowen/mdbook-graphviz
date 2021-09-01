SHELL:=/bin/bash

.DEFAULT_GOAL := default

fix:
	cargo fix --allow-staged

format:
	cargo fmt

lint:
	cargo clippy
	-cargo audit

build: format lint
	cargo build

test: format
	cargo test

install: format lint
	cargo install --force --path .

default: build

clean:
	cargo clean