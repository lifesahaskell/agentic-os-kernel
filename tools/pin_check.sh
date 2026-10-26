#!/bin/sh
# Static gate for the version pins that must move together: the OSTD requirement and lock entry,
# the two OSDK allocator crates the ktest probe reads, the build image tag, the cargo-osdk install,
# and both toolchain files.
# Usage: sh tools/pin_check.sh [ROOT]      check ROOT (default: the repo root)
#        sh tools/pin_check.sh --self-test break one pin at a time in a scratch copy
# Needs no Docker and no KVM.
set -eu

ostd_req='version = "=0.19.0"'
image_from='FROM asterinas/osdk-dev:0.19.0-'
osdk_install='cargo-osdk --version 0.19.0'
toolchain='channel = "nightly-2026-07-21"'
# The probe crates (kernel/src/probe.rs) ride the OSTD release: one exact pin for all three.
probe_crates='osdk-frame-allocator osdk-heap-allocator'
probe_req='"=0.19.0"'

check() {
  root=$1
  bad=0
  fail() {
    echo "pin_check: $1: expected $2" >&2
    bad=1
  }
  grep -Fq "$ostd_req" "$root/kernel/Cargo.toml" || fail kernel/Cargo.toml "$ostd_req"
  # The lock entry is the version line right after the exact package name.
  lock_ver=$(awk '/^name = "ostd"$/ { getline; print; exit }' "$root/kernel/Cargo.lock")
  [ "$lock_ver" = 'version = "0.19.0"' ] || fail kernel/Cargo.lock 'ostd version = "0.19.0"'
  for crate in $probe_crates; do
    grep -Fqx "$crate = $probe_req" "$root/kernel/Cargo.toml" || fail kernel/Cargo.toml "$crate = $probe_req"
    probe_ver=$(awk -v want="name = \"$crate\"" '$0 == want { getline; print; exit }' "$root/kernel/Cargo.lock")
    [ "$probe_ver" = 'version = "0.19.0"' ] || fail kernel/Cargo.lock "$crate version = \"0.19.0\""
  done
  grep -Fq "$image_from" "$root/kernel/Dockerfile" || fail kernel/Dockerfile "$image_from"
  grep -Fq "$osdk_install" "$root/kernel/Dockerfile" || fail kernel/Dockerfile "$osdk_install"
  grep -Fq "$toolchain" "$root/kernel/rust-toolchain.toml" || fail kernel/rust-toolchain.toml "$toolchain"
  grep -Fq "$toolchain" "$root/userland/rust-toolchain.toml" || fail userland/rust-toolchain.toml "$toolchain"
  return $bad
}

self_test() {
  src=$1
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  files="kernel/Cargo.toml kernel/Cargo.lock kernel/Dockerfile kernel/rust-toolchain.toml userland/rust-toolchain.toml"
  fresh() {
    rm -rf "$tmp/t"
    mkdir -p "$tmp/t/kernel" "$tmp/t/userland"
    for f in $files; do cp "$src/$f" "$tmp/t/$f"; done
  }
  fresh
  check "$tmp/t" 2>/dev/null || { echo "pin_check: self-test: clean copy fails" >&2; exit 1; }
  # One round per pin: file, then the sed script that breaks exactly that pin.
  round() {
    fresh
    sed "$2" "$tmp/t/$1" >"$tmp/t/$1.new"
    mv "$tmp/t/$1.new" "$tmp/t/$1"
    if check "$tmp/t" 2>/dev/null; then
      echo "pin_check: self-test: breaking $1 ($2) went unnoticed" >&2
      exit 1
    fi
  }
  round kernel/Cargo.toml 's/"=0\.19\.0"/"0.19.0"/'
  round kernel/Cargo.lock '/^name = "ostd"$/{n;s/0\.19\.0/0.19.1/;}'
  for crate in $probe_crates; do
    round kernel/Cargo.toml "s/^$crate = \"=0\.19\.0\"/$crate = \"0.19.0\"/"
    round kernel/Cargo.lock "/^name = \"$crate\"\$/{n;s/0\.19\.0/0.19.1/;}"
  done
  round kernel/Dockerfile 's#osdk-dev:0\.19\.0-#osdk-dev:0.19.1-#'
  round kernel/Dockerfile 's#--version 0\.19\.0#--version 0.19.1#'
  round kernel/rust-toolchain.toml 's/nightly-2026-07-21/nightly-2026-07-22/'
  round userland/rust-toolchain.toml 's/nightly-2026-07-21/nightly-2026-07-22/'
  echo "pin_check: self-test ok"
}

here=$(cd "$(dirname "$0")/.." && pwd)
case "${1:-}" in
  --self-test) self_test "$here" ;;
  *)
    check "${1:-$here}"
    echo "pin_check: ok"
    ;;
esac
