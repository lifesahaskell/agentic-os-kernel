#!/bin/sh
# Build every ring-3 test program into out/<name>.bin (flat image). Runs inside the build
# container: `cd kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test'`.
set -eu
cd "$(dirname "$0")"
cargo build --release -p aok-tests --bins
objcopy="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-objcopy"
mkdir -p out
for src in tests/src/bin/*.rs; do
  name=$(basename "$src" .rs)
  "$objcopy" -O binary "target/x86_64-aok-user/release/$name" "out/$name.bin"
  if [ "$(head -c 8 "out/$name.bin")" != "AOKIMG01" ]; then
    echo "build.sh: out/$name.bin is missing the AOKIMG01 header" >&2
    exit 1
  fi
done
