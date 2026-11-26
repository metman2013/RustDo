#!/bin/sh
# Starts the hub as its own unprivileged user (rustdo). When the container starts as
# root (the default), it first makes the data folder the hub's own: Home Assistant and
# NAS bind mounts often hand it over owned by root, and the hub couldn't save there.
set -e
DATA="${RUSTDO_HUB_DATA:-/data}"
if [ "$(id -u)" = "0" ]; then
  mkdir -p "$DATA"
  if ! su-exec rustdo test -w "$DATA"; then
    chown -R rustdo:rustdo "$DATA"
  fi
  exec su-exec rustdo rustdo-hub "$@"
fi
exec rustdo-hub "$@"
