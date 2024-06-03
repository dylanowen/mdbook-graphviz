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

dev:
    MDBOOK_preprocessor__graphviz__command="cargo run -p mdbook-graphviz" \
    MDBOOK_preprocessor__d2_interactive__command="cargo run -p mdbook-d2-interactive" \
      mdbook serve book

build: build-book
	cargo build

build-book:
    MDBOOK_preprocessor__graphviz__command="cargo run -p mdbook-graphviz" \
    MDBOOK_preprocessor__d2_interactive__command="cargo run -p mdbook-d2-interactive" \
      mdbook build book

release: release-js release-rust release-book

release-rust:
    cargo build --release

release-js:
    just --justfile crates/mdbook-svg-inline-preprocessor/justfile release-js

release-book:
    MDBOOK_preprocessor__graphviz__command="cargo run -p mdbook-graphviz --release" \
    MDBOOK_preprocessor__d2_interactive__command="cargo run -p mdbook-d2-interactive --release" \
      mdbook build book

test:
	cargo test

pre-commit-rust: fix fmt lint test release-rust release-book

pre-commit: fix fmt lint test release

install:
    # don't enforce the js runtime to be available.
    -just --justfile crates/mdbook-svg-inline-preprocessor/justfile release-js
    just --justfile crates/mdbook-d2/justfile install
    just --justfile crates/mdbook-graphviz/justfile install

clean:
	cargo clean
