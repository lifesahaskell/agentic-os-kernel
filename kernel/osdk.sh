#!/bin/sh
# Run a command inside the spike's build container with this directory mounted at /work.
# Usage: ./osdk.sh cargo osdk run
set -eu
here=$(cd "$(dirname "$0")" && pwd)
exec docker run --rm --device /dev/kvm -v "$here":/work -w /work \
  -v agentic-os-cargo-registry:/root/.cargo/registry \
  agentic-os-kernel-dev:0.19.0 \
  sh -c '"$@"; s=$?; chown -R '"$(id -u):$(id -g)"' /work; exit $s' sh "$@"
