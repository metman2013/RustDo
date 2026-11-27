#!/bin/sh
# Installs RustDo from the .tar.gz download (any Linux distribution).
#   sh install.sh            just for you  (~/.local: no password needed)
#   sudo sh install.sh       for everyone  (/usr/local)
#   sh install.sh --remove   uninstall (sudo for a system-wide install)
# Your tasks are never touched (they stay in your Documents/RustDo folder).
set -eu
cd "$(dirname "$0")"

if [ "$(id -u)" -eq 0 ]; then prefix=/usr/local; else prefix="$HOME/.local"; fi
bin="$prefix/bin/rustdo"
desktop="$prefix/share/applications/rustdo.desktop"
icon="$prefix/share/icons/hicolor/512x512/apps/rustdo.png"

if [ "${1:-}" = "--remove" ]; then
  rm -f "$bin" "$desktop" "$icon"
  echo "RustDo removed from $prefix."
  exit 0
fi

install -Dm755 rustdo "$bin"
install -Dm644 rustdo.png "$icon"
# The menu entry starts RustDo by its full path, so it works even if $prefix/bin isn't
# on the PATH.
mkdir -p "$(dirname "$desktop")"
sed "s|^Exec=rustdo|Exec=$bin|" rustdo.desktop > "$desktop"
chmod 644 "$desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$desktop")" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" 2>/dev/null || true

echo "RustDo is installed in $prefix. Find it in your app menu, or run: $bin"
