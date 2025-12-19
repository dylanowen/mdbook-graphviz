default: build

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
    mdbook serve book

build:
    cargo build

build-book:
    mdbook build book

release:
    cargo build --release

test:
    cargo test

pre-commit: fix fmt lint test release

install:
    cargo install --force --path .

clean:
    cargo clean
