#!/usr/bin/env bash
# Byte-identity gate for the run.sh --wrap hook (09-05, D-16).
#
#   tools/linux-baseline/check-wrap-identity.sh [BASE]     (BASE defaults to df2d6b2)
#
# Runs run.sh as committed at BASE (the pre-wrap script) and the working-tree run.sh, each with
# --run-id smoke-wrapid --suites micro --smoke and no --wrap, with a fake taskset first on PATH
# that records its argv (one line per call, every argument in angle brackets so boundaries and
# empty arguments show) and never runs micro. Exits 0 only if both exit statuses are equal, both
# argv logs are non-empty, and the two logs are identical after the random work-dir paths are
# normalized. Exit 1: the logs differ. Exit 2: precondition not met (would pass vacuously).
# Needs the harness binaries: run `make -C tools/linux-baseline` first.
set -euo pipefail

BASE=${1:-df2d6b2}

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
TOOLS=tools/linux-baseline
PRE="$TOOLS/.wrapid-pre.sh"
SMOKE_DIR="$TOOLS/results/smoke-wrapid"

if ! git show "$BASE:$TOOLS/run.sh" >/dev/null 2>&1; then
	echo "check-wrap-identity: $TOOLS/run.sh does not exist at $BASE" >&2
	exit 2
fi
if git show "$BASE:$TOOLS/run.sh" | grep -q 'WRAP_ARGV'; then
	echo "check-wrap-identity: run.sh at $BASE already has WRAP_ARGV; pick a pre-edit baseline" >&2
	exit 2
fi
if ! grep -q 'WRAP_ARGV' "$TOOLS/run.sh"; then
	echo "check-wrap-identity: working-tree run.sh has no WRAP_ARGV; nothing to compare" >&2
	exit 2
fi

SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/wrapid.XXXXXX")
cleanup() {
	rm -rf "$SCRATCH" "$PRE" "$SMOKE_DIR"
}
trap cleanup EXIT

mkdir "$SCRATCH/fakebin"
cat >"$SCRATCH/fakebin/taskset" <<'EOF'
#!/bin/sh
for a in "$@"; do
	printf '<%s>' "$a" >>"$WRAPID_LOG"
done
printf '\n' >>"$WRAPID_LOG"
exit 0
EOF
chmod +x "$SCRATCH/fakebin/taskset"

git show "$BASE:$TOOLS/run.sh" >"$PRE"

run_one() { # script log -> prints exit status
	local script=$1 log=$2 rc=0
	: >"$log"
	PATH="$SCRATCH/fakebin:$PATH" WRAPID_LOG="$log" \
		bash "$script" --run-id smoke-wrapid --suites micro --smoke >"$log.out" 2>&1 || rc=$?
	echo "$rc"
}

RC_PRE=$(run_one "$PRE" "$SCRATCH/pre.log")
RC_NEW=$(run_one "$TOOLS/run.sh" "$SCRATCH/new.log")

norm() {
	sed -e 's#/dev/shm/aok-linux-baseline\.[A-Za-z0-9]*#WORK_TMPFS#g' \
		-e 's#rootfs\.[A-Za-z0-9]*#WORK_ROOTFS#g' "$1" >"$2"
}
norm "$SCRATCH/pre.log" "$SCRATCH/pre.norm"
norm "$SCRATCH/new.log" "$SCRATCH/new.norm"

if [ "$RC_PRE" != "$RC_NEW" ]; then
	echo "check-wrap-identity: exit status differs: base=$RC_PRE edited=$RC_NEW" >&2
	tail -n 5 "$SCRATCH/pre.log.out" "$SCRATCH/new.log.out" >&2 || true
	exit 1
fi
if [ ! -s "$SCRATCH/pre.norm" ] || [ ! -s "$SCRATCH/new.norm" ]; then
	echo "check-wrap-identity: an argv log is empty (exit status base=$RC_PRE edited=$RC_NEW); the gate would be vacuous" >&2
	tail -n 5 "$SCRATCH/pre.log.out" "$SCRATCH/new.log.out" >&2 || true
	exit 1
fi
if ! cmp -s "$SCRATCH/pre.norm" "$SCRATCH/new.norm"; then
	echo "check-wrap-identity: taskset argv differs between $BASE and the working tree:" >&2
	diff "$SCRATCH/pre.norm" "$SCRATCH/new.norm" >&2 || true
	exit 1
fi
echo "check-wrap-identity: OK, $(wc -l <"$SCRATCH/new.norm") identical taskset invocations, exit status $RC_NEW both, base $BASE"
