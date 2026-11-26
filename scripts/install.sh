#!/bin/sh
set -eu
# Run from an unpacked release: sh install.sh --source ./devmesh-... [--offline]
source_dir=""
for arg in "$@"; do
  if [ "$source_dir" = pending ]; then source_dir="$arg"; break; fi
  if [ "$arg" = --source ]; then source_dir=pending; fi
done
if [ -z "$source_dir" ] || [ "$source_dir" = pending ]; then
  echo 'Usage: sh scripts/install.sh --source UNPACKED_BUNDLE [--prefix DIR] [--runtime all|codex|pi|pi-durable] [--offline]' >&2
  exit 2
fi
exec python3 "$source_dir/install-local.py" "$@"
