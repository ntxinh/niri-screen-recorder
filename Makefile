.PHONY: check fmt lint test build run clean install uninstall
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

PREFIX ?= $(HOME)/.local

install: build
	cargo install --path . --locked
	install -Dm644 data/io.github.niri_recorder.desktop $(PREFIX)/share/applications/io.github.niri_recorder.desktop
	install -Dm644 data/icons/hicolor/scalable/apps/niri-recorder.svg $(PREFIX)/share/icons/hicolor/scalable/apps/niri-recorder.svg
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null || true
	-gtk-update-icon-cache $(PREFIX)/share/icons/hicolor 2>/dev/null || true

uninstall:
	rm -f $(PREFIX)/share/applications/io.github.niri_recorder.desktop
	rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/niri-recorder.svg
	cargo uninstall niri-recorder || true
