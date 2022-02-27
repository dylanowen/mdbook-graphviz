SHELL:=/bin/bash

.DEFAULT_GOAL := default

fix:
	cargo fix --allow-staged

fmt:
	cargo fmt --all -- --check

lint:
	cargo clippy -- -D warnings
	-cargo audit

check:
	cargo check

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

pre-commit: fix fmt lint test release

install:
	cargo install --force --path .

default: build

clean:
	cargo clean