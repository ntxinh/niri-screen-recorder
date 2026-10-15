.PHONY: check fmt lint test build run clean

check: fmt lint test build

fmt:
	cargo fmt --all -- --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-targets

build:
	cargo build

run:
	cargo run

clean:
	cargo clean
