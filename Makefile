.PHONY: check fmt lint test build run clean install uninstall install-dms-plugin uninstall-dms-plugin install-daemon uninstall-daemon
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
XDG_CONFIG_HOME ?= $(HOME)/.config
SYSTEMD_USER_DIR ?= $(XDG_CONFIG_HOME)/systemd/user

install: build
	cargo install --path . --locked
	install -Dm644 data/io.github.niri_recorder.desktop $(PREFIX)/share/applications/io.github.niri_recorder.desktop
	install -Dm644 data/icons/hicolor/scalable/apps/niri-recorder.svg $(PREFIX)/share/icons/hicolor/scalable/apps/niri-recorder.svg
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null || true
	-gtk-update-icon-cache $(PREFIX)/share/icons/hicolor 2>/dev/null || true

uninstall:
	rm -f $(PREFIX)/share/applications/io.github.niri_recorder.desktop
	rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/niri-recorder.svg
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null || true
	-gtk-update-icon-cache $(PREFIX)/share/icons/hicolor 2>/dev/null || true
	cargo uninstall niri-recorder || true

# Optional Phase 6 integrations (not part of `make install`).
install-dms-plugin:
	install -d $(XDG_CONFIG_HOME)/DankMaterialShell/plugins/niriRecorder
	install -m644 data/dms-plugin/niriRecorder/plugin.json $(XDG_CONFIG_HOME)/DankMaterialShell/plugins/niriRecorder/plugin.json
	install -m644 data/dms-plugin/niriRecorder/NiriRecorder.qml $(XDG_CONFIG_HOME)/DankMaterialShell/plugins/niriRecorder/NiriRecorder.qml
	@echo "Enable Niri Recorder in DMS Settings → Plugins, or run: dms ipc call plugins reload niriRecorder"

uninstall-dms-plugin:
	rm -rf $(XDG_CONFIG_HOME)/DankMaterialShell/plugins/niriRecorder

install-daemon:
	install -d $(SYSTEMD_USER_DIR)
	install -m644 data/niri-recorder-daemon.service $(SYSTEMD_USER_DIR)/niri-recorder-daemon.service
	@echo "Then: systemctl --user import-environment WAYLAND_DISPLAY && systemctl --user enable --now niri-recorder-daemon"

uninstall-daemon:
	rm -f $(SYSTEMD_USER_DIR)/niri-recorder-daemon.service
