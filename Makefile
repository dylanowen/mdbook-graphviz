SHELL:=/bin/bash

.DEFAULT_GOAL := default

fix:
	cargo fix --all-targets --all-features --allow-staged
	cargo clippy --fix --all-targets --all-features --allow-staged

fmt:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
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