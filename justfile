gradefault: build

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

release: release-js release-rust

release-rust:
    cargo build --release

release-js:
    just --justfile crates/mdbook-svg-inline-preprocessor/justfile release-js

test:
	cargo test

pre-commit-rust: fix fmt lint test release-rust

pre-commit: fix fmt lint test release

install:
    # don't enforce the js runtime to be available.
    -just --justfile crates/mdbook-svg-inline-preprocessor/justfile release-js
    just --justfile crates/mdbook-d2/justfile install
    just --justfile crates/mdbook-graphviz/justfile install

clean:
	cargo clean

_graphviz command:
    just --justfile crates/mdbook-graphviz {{command}}
