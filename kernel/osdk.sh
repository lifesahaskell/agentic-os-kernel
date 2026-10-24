#!/bin/sh
# Run a command inside the spike's build container with the repo root mounted at /work and the
# working directory at /work/kernel (userland/ is a sibling the kernel build reads).
# Usage: ./osdk.sh cargo osdk run
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
exec docker run --rm --device /dev/kvm -v "$root":/work -w /work/kernel \
  -v agentic-os-cargo-registry:/root/.cargo/registry \
  agentic-os-kernel-dev:0.19.0 \
  sh -c '"$@"; s=$?; chown -R '"$(id -u):$(id -g)"' /work/kernel /work/userland; exit $s' sh "$@"
