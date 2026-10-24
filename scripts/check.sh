#!/bin/sh
# Run every gate locally, in CI's order. Usage: sh scripts/check.sh [--no-kvm]
# --no-kvm skips the gates that boot QEMU under KVM and says so on the last line.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
kvm=1
[ "${1:-}" = "--no-kvm" ] && kvm=0

echo "== unsafe_check self-test"
python3 -I tools/unsafe_check.py --self-test
echo "== unsafe_check"
python3 -I tools/unsafe_check.py

cd kernel
echo "== userland build, fmt, clippy; kernel fmt, clippy"
./osdk.sh sh -c '../userland/build.sh &&
  (cd ../userland && cargo fmt --check && cargo clippy --release -- -D warnings) &&
  cargo fmt --check && cargo osdk clippy'

if [ "$kvm" = 1 ]; then
  echo "== cargo osdk test (ktests in QEMU)"
  ./osdk.sh sh -c '../userland/build.sh && cargo osdk test'
  echo "== cargo osdk run (boot checks)"
  mkdir -p target
  log=target/boot-check.log
  # OSDK reuses the ktest bundle for run unless target/osdk is cleared first.
  ./osdk.sh sh -c 'rm -rf target/osdk && ../userland/build.sh && cargo osdk run' >"$log" 2>&1 || true
  cat "$log"
  for want in "aok: cpu budget check passed"; do
    grep -q "$want" "$log" || { echo "check: boot output lacks '$want'" >&2; exit 1; }
  done
  if grep -q FAILED "$log"; then echo "check: boot output contains FAILED" >&2; exit 1; fi
  echo "check: all gates passed"
else
  echo "== cargo osdk build (no KVM)"
  ./osdk.sh sh -c '../userland/build.sh && cargo osdk build'
  echo "check: static gates passed; LOCAL-ONLY (needs /dev/kvm): cargo osdk test, cargo osdk run boot checks"
fi
