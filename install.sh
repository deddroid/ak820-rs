#!/bin/sh
# Build and install ak820 for the current user (no root needed).
# Run from the source tree it builds with cargo; run from an unpacked release archive
# (ak820 and ak820-gui next to this script) it installs the prebuilt binaries.
#   binaries  -> ~/.local/bin
#   launcher  -> ~/.local/share/applications/ak820.desktop
#   icons     -> ~/.local/share/icons/hicolor/*/apps/ak820.png
# Options:
#   --udev       also install the udev rule (asks for the sudo password)
#   --uninstall  remove everything installed by this script
set -eu
cd "$(dirname "$0")"

BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
ICONS="$DATA/icons/hicolor"
SIZES="32 48 64 128 256"

refresh() {
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" || true
    command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN/ak820" "$BIN/ak820-gui" "$APPS/ak820.desktop"
    for s in $SIZES; do rm -f "$ICONS/${s}x${s}/apps/ak820.png"; done
    rm -f "$ICONS/512x512/apps/ak820.png"
    refresh
    echo "ak820 removed."
    exit 0
fi

if [ -f Cargo.toml ]; then
    cargo build --release
    SRC=target/release
else
    SRC=.
fi

install -Dm755 "$SRC/ak820" "$BIN/ak820"
install -Dm755 "$SRC/ak820-gui" "$BIN/ak820-gui"
for s in $SIZES; do
    install -Dm644 "assets/icon-$s.png" "$ICONS/${s}x${s}/apps/ak820.png"
done
install -Dm644 assets/icon.png "$ICONS/512x512/apps/ak820.png"
mkdir -p "$APPS"
sed "s|^Exec=.*|Exec=$BIN/ak820-gui|" packaging/linux/ak820.desktop > "$APPS/ak820.desktop.tmp"
install -Dm644 "$APPS/ak820.desktop.tmp" "$APPS/ak820.desktop"
rm -f "$APPS/ak820.desktop.tmp"
refresh

if [ "${1:-}" = "--udev" ]; then
    sudo install -Dm644 udev/70-ajazz-ak820.rules /etc/udev/rules.d/70-ajazz-ak820.rules
    sudo udevadm control --reload
    sudo udevadm trigger --subsystem-match=hidraw
fi

echo "Installed: $BIN/ak820, $BIN/ak820-gui and the \"AK820 Pro\" launcher."
case ":$PATH:" in *":$BIN:"*) ;; *) echo "Note: $BIN is not in your PATH." ;; esac
