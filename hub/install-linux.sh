#!/bin/sh
# Installs RustDo Hub as a service on Linux (systemd), without Docker.
#
#   Download RustDo-Hub-<version>-linux-x86_64 (PCs) or -linux-arm64 (Raspberry Pi),
#   rustdo-hub.service and this script from the Releases page into one folder, then:
#     sudo sh install-linux.sh
#
# Run it again with a newer RustDo-Hub-… file to update; your lists are kept in
# /var/lib/rustdo-hub.
set -eu
cd "$(dirname "$0")"

[ "$(id -u)" -eq 0 ] || { echo "Run it with sudo: sudo sh $0"; exit 1; }
command -v systemctl >/dev/null || { echo "This needs systemd. Start the hub by hand instead: ./RustDo-Hub-…-linux-… --data <folder>"; exit 1; }

case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=arm64 ;;
  *) echo "No RustDo Hub build for $(uname -m); use Docker or build it from source."; exit 1 ;;
esac
bin=$(ls RustDo-Hub-*-linux-"$arch" 2>/dev/null | sort -V | tail -n 1)
[ -n "$bin" ] || { echo "Put RustDo-Hub-<version>-linux-$arch next to this script first."; exit 1; }
[ -f rustdo-hub.service ] || { echo "Put rustdo-hub.service next to this script first."; exit 1; }

install -m 755 "$bin" /usr/local/bin/rustdo-hub
install -m 644 rustdo-hub.service /etc/systemd/system/rustdo-hub.service
systemctl daemon-reload
systemctl enable rustdo-hub >/dev/null
systemctl restart rustdo-hub

ip=$(hostname -I 2>/dev/null | awk '{print $1}')
echo "RustDo Hub ($bin) is running and starts with the computer."
echo "In RustDo, use the hub address: http://${ip:-<this computer's IP>}:7979"
echo "Status: systemctl status rustdo-hub    Logs: journalctl -u rustdo-hub"
