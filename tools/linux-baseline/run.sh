#!/usr/bin/env bash
# Suite driver for the Linux baseline harness. Builds nothing; run `make -C tools/linux-baseline` first.
#
#   tools/linux-baseline/run.sh --run-id ID [--suites micro,broker,deadline,cgroup]
#       [--configs LIST] [--rounds A-B] [--cpu N] [--smoke] [--tag T]
#
# Every benchmark process is pinned to one CPU with taskset. Configurations are interleaved within
# each round (C0 C1a C1b ... in order) and each runs in a fresh process.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
TOOLS=tools/linux-baseline
BIN="$TOOLS/bin"

RUN_ID=""
SUITES="micro,broker,deadline,cgroup"
CONFIGS=""
ROUNDS="1-5"
CPU=2
SMOKE=0
TAG=""

usage() {
	sed -n '2,8p' "$0" >&2
	exit 64
}

while [ $# -gt 0 ]; do
	case "$1" in
	--run-id) RUN_ID="$2"; shift 2 ;;
	--suites) SUITES="$2"; shift 2 ;;
	--configs) CONFIGS="$2"; shift 2 ;;
	--rounds) ROUNDS="$2"; shift 2 ;;
	--cpu) CPU="$2"; shift 2 ;;
	--smoke) SMOKE=1; shift ;;
	--tag) TAG="$2"; shift 2 ;;
	*) usage ;;
	esac
done

[ -n "$RUN_ID" ] || usage
case "$ROUNDS" in
[0-9]*-[0-9]*) ;;
*) echo "run.sh: --rounds must look like 1-5" >&2; exit 64 ;;
esac
R_FROM=${ROUNDS%-*}
R_TO=${ROUNDS#*-}
if [ "$SMOKE" = 1 ]; then
	case "$RUN_ID" in
	smoke-*) ;;
	*) echo "run.sh: --smoke needs a run id starting with smoke-" >&2; exit 64 ;;
	esac
	R_FROM=1
	R_TO=2
	ROUNDS="1-2"
fi
case "$RUN_ID" in
*[!A-Za-z0-9._-]*) echo "run.sh: bad run id" >&2; exit 64 ;;
esac
if [ -n "$TAG" ]; then
	case ",$SUITES," in
	,micro,) ;;
	*) echo "run.sh: --tag is valid with --suites micro only" >&2; exit 64 ;;
	esac
fi

for b in micro broker deadline cgbench floor; do
	[ -x "$BIN/$b" ] || { echo "run.sh: $BIN/$b missing; run make -C $TOOLS" >&2; exit 1; }
done

RUNDIR="$TOOLS/results/$RUN_ID"
case "$RUN_ID" in
smoke-*)
	# A smoke directory is recreated from scratch, once per invocation.
	rm -rf "$RUNDIR"
	;;
esac
mkdir -p "$RUNDIR"
LOG="$RUNDIR/run.log"
touch "$LOG"

log() { printf '%s\n' "$*" | tee -a "$LOG" >&2; }

kv() { printf '%s=%s\n' "$1" "$2" >>"$RUNDIR/env.txt"; }

# ---- work directories -------------------------------------------------------------------------
mkdir -p "$TOOLS/.work"
WORK_TMPFS=$(mktemp -d /dev/shm/aok-linux-baseline.XXXXXX)
WORK_ROOTFS=$(mktemp -d "$PWD/$TOOLS/.work/rootfs.XXXXXX")
cleanup() {
	rm -rf "$WORK_TMPFS" "$WORK_ROOTFS"
}
trap cleanup EXIT
: >"$WORK_TMPFS/f"
: >"$WORK_ROOTFS/f"

# ---- env.txt ----------------------------------------------------------------------------------
write_env() {
	: >"$RUNDIR/env.txt"
	kv date_utc "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
	kv harness_rev "$(git rev-parse HEAD)"
	if git status --porcelain "$TOOLS" | grep -v "$TOOLS/results/" | grep -q .; then
		kv harness_dirty yes
	else
		kv harness_dirty no
	fi
	kv uname_r "$(uname -r)"
	kv uname_a "$(uname -a)"
	kv cpu_model "$(lscpu | awk -F: '/^Model name/{sub(/^[ \t]+/,"",$2); print $2; exit}')"
	kv nproc "$(nproc --all)"
	kv cpu_pinned "$CPU"
	core=$(lscpu -e=CPU,CORE | awk -v c="$CPU" '$1==c{print $2}')
	sib=$(lscpu -e=CPU,CORE | awk -v c="$CPU" -v k="$core" '$2==k && $1!=c{printf "%s%s", s, $1; s=","}')
	kv cpu_pinned_core_siblings "${sib:-none}"
	if grep -qi 'WSL2' /proc/sys/kernel/osrelease || grep -qi microsoft /proc/version; then
		kv wsl2 yes
	else
		kv wsl2 no
	fi
	if lscpu | awk -F: '/^Hypervisor vendor/{print $2}' | grep -qi microsoft; then
		kv hyperv yes
	else
		kv hyperv no
	fi
	kv virt_systemd_detect "$(systemd-detect-virt 2>/dev/null || true)"
	if [ -e /dev/kvm ]; then kv kvm_dev present; else kv kvm_dev absent; fi
	kv clocksource "$(cat /sys/devices/system/clocksource/clocksource0/current_clocksource 2>/dev/null || echo unreadable)"
	kv cmdline "$(cat /proc/cmdline)"
	if [ -r /proc/config.gz ]; then
		kv config_lsm "$(zcat /proc/config.gz | grep '^CONFIG_LSM=' || echo absent)"
		kv config_bpf_lsm "$(zcat /proc/config.gz | grep '^CONFIG_BPF_LSM=' || echo absent)"
	else
		kv config_lsm unreadable
		kv config_bpf_lsm unreadable
	fi
	kv unprivileged_bpf_disabled "$(cat /proc/sys/kernel/unprivileged_bpf_disabled 2>/dev/null || echo unreadable)"
	if [ -e /proc/pressure/memory ]; then kv psi present; else kv psi absent; fi
	kv gcc "$(gcc --version | head -n 1)"
	kv glibc "$(ldd --version 2>&1 | head -n 1 || true)"
	kv shell_seccomp "$(awk '/^Seccomp:/{print $2}' /proc/$$/status)"
	kv shell_nonewprivs "$(awk '/^NoNewPrivs:/{print $2}' /proc/$$/status)"
	# comm names only, never argv or environment.
	top=$(ps -eo comm= --sort=-pcpu 2>/dev/null | head -n 5 | paste -sd';' || true)
	kv top_cpu_before "$top"
	"$BIN/micro" --env >>"$RUNDIR/env.txt"
	# ENV-02: 12 floor_ rows, each with a floormethod_ key, plus the unprivileged host keys.
	"$BIN/floor" >>"$RUNDIR/env.txt" || kv floor_probe unreadable
}

if [ ! -s "$RUNDIR/env.txt" ]; then
	write_env
fi

# ---- helpers ----------------------------------------------------------------------------------
have_rows() { # csv round config
	[ -s "$1" ] && awk -F, -v r="$2" -v c="$3" 'NR>1 && $2==r && $3==c {f=1} END{exit !f}' "$1"
}

list_configs() { # default list, override from --configs
	if [ -n "$CONFIGS" ]; then
		printf '%s\n' "${CONFIGS//,/ }"
	else
		printf '%s\n' "$1"
	fi
}

suite_begin() { # suite configs
	local s=$1 cfgs=$2
	kv "loadavg_before_${s}_${ROUNDS}" "$(cat /proc/loadavg)"
	kv "procs_running_before_${s}_${ROUNDS}" "$(awk '/^procs_running/{print $2}' /proc/stat)"
	if [ -n "$TAG" ]; then
		local l=""
		for c in $cfgs; do l="$l $c-$TAG"; done
		cfgs=${l# }
	fi
	kv "requested_${s}_configs" "${cfgs// /,}"
	kv "requested_${s}_rounds" "$ROUNDS"
	SUITE_T0=$(date +%s)
}

suite_end() {
	local s=$1
	kv "loadavg_after_${s}_${ROUNDS}" "$(cat /proc/loadavg)"
	kv "elapsed_s_${s}_${ROUNDS}" "$(($(date +%s) - SUITE_T0))"
	python3 -I "$TOOLS/summarize.py" "$RUNDIR"
}

SMOKE_ARG=()
[ "$SMOKE" = 1 ] && SMOKE_ARG=(--smoke)

# ---- suites -----------------------------------------------------------------------------------
run_micro() {
	local cfgs
	cfgs=$(list_configs "C0 C1a C1b C1c C2a C2b C3")
	suite_begin micro "$cfgs"
	local r c label extra
	for ((r = R_FROM; r <= R_TO; r++)); do
		for c in $cfgs; do
			label=$c
			extra=()
			if [ -n "$TAG" ]; then
				label="$c-$TAG"
				extra=(--label "$label")
			fi
			if [ "$SMOKE" = 0 ] && have_rows "$RUNDIR/micro.csv" "$r" "$label"; then
				log "run.sh: micro round $r config $label already has rows"
				exit 3
			fi
			log "micro round $r config $label"
			taskset -c "$CPU" "$BIN/micro" --config "$c" --round "$r" \
				--tmpfs-dir "$WORK_TMPFS" --root-dir "$WORK_ROOTFS" \
				--out "$RUNDIR/micro.csv" --selfcheck "$RUNDIR/selfcheck.txt" \
				"${SMOKE_ARG[@]}" "${extra[@]}" 2>>"$LOG" || {
				log "run.sh: micro round $r config $label failed"
				exit 1
			}
		done
	done
	suite_end micro
}

# ---- cgroup probe (L10, A7) -------------------------------------------------------------------
# Runs once per run directory. Records what this session may do with cgroup v2 and sets DELEG.
pk() { printf '%s=%s\n' "$1" "$2" >>"$RUNDIR/cgroup-probe.txt"; }

probe_cgroup() {
	local P="$RUNDIR/cgroup-probe.txt"
	if [ ! -s "$P" ]; then
		local uid own d first out
		uid=$(id -u)
		: >"$P"
		own=$(sed -n 's/^0:://p' /proc/self/cgroup)
		pk own_cgroup "$own"
		if [ -w "/sys/fs/cgroup${own}/cgroup.procs" ]; then pk own_cgroup_writable yes; else pk own_cgroup_writable no; fi
		d=$(find /sys/fs/cgroup/user.slice -maxdepth 4 -type d -user "$uid" 2>/dev/null | head -n 20 | paste -sd';' || true)
		pk user_owned_dirs "${d:-none}"
		first=${d%%;*}
		if [ -n "$first" ]; then
			mkdir "$first/aok-probe.$$" 2>/dev/null || true
			out=$(bash -c 'echo $$ >"$1"' _ "$first/aok-probe.$$/cgroup.procs" 2>&1 || true)
			pk direct_move "${out:-ok}"
			sleep 0.2
			rmdir "$first/aok-probe.$$" 2>/dev/null || true
		else
			pk direct_move "no user-owned directory"
		fi
		if out=$(timeout 20 systemd-run --user --scope --quiet -p Delegate=yes -- cat /proc/self/cgroup 2>&1); then
			pk systemd_run_scope "ok:$(printf '%s' "$out" | tr '\n' ' ')"
		else
			pk systemd_run_scope "error:$(printf '%s' "$out" | tr '\n' ' ')"
		fi
		local svc_ok=no svc_cg="" svc_owner="" svc_ctl="" svc_w=""
		if out=$(timeout 20 systemd-run --user --wait --pipe --collect --quiet -p Delegate=yes -- sh -c \
			'p=$(sed -n "s/^0:://p" /proc/self/cgroup); echo "cg=$p"; echo "owner=$(stat -c %u /sys/fs/cgroup$p)"; echo "ctl=$(cat /sys/fs/cgroup$p/cgroup.controllers)"; if [ -w /sys/fs/cgroup$p/cgroup.procs ]; then echo w=yes; else echo w=no; fi' 2>&1); then
			svc_ok=yes
			svc_cg=$(printf '%s\n' "$out" | sed -n 's/^cg=//p')
			svc_owner=$(printf '%s\n' "$out" | sed -n 's/^owner=//p')
			svc_ctl=$(printf '%s\n' "$out" | sed -n 's/^ctl=//p')
			svc_w=$(printf '%s\n' "$out" | sed -n 's/^w=//p')
			pk systemd_run_service ok
		else
			pk systemd_run_service "error:$(printf '%s' "$out" | tr '\n' ' ')"
		fi
		pk service_cgroup "$svc_cg"
		pk service_owner "$svc_owner"
		pk service_controllers "$svc_ctl"
		pk service_writable "$svc_w"
		out=$(timeout 20 unshare -Ucr --cgroup sh -c 'sed -n "s/^0:://p" /proc/self/cgroup | tr "\n" " "; if [ -w /sys/fs/cgroup/cgroup.procs ]; then echo writable; else echo not-writable; fi' 2>&1 || true)
		pk unshare_cgroupns "$(printf '%s' "$out" | tr '\n' ' ')"
		local deleg=no
		if [ "$svc_ok" = yes ] && [ "$svc_owner" = "$uid" ] && [ "$svc_w" = yes ]; then
			case " $svc_ctl " in
			*" memory "*) case " $svc_ctl " in *" pids "*) deleg=yes ;; esac ;;
			esac
		fi
		pk delegation "$deleg"
		if [ "$deleg" = no ]; then
			local why="no delegated writable cgroup with memory and pids: service_owner=${svc_owner:-none} writable=${svc_w:-none} controllers=${svc_ctl:-none}"
			for leg in cgkill spawn oom pids psi; do
				printf 'NOT MEASURED leg=%s reason=%s\n' "$leg" "$why" >>"$P"
			done
		fi
	fi
	DELEG=$(sed -n 's/^delegation=//p' "$P" | head -n 1)
}

# Runs a benchmark binary pinned to the CPU. With delegation, inside a transient service that
# owns a delegated cgroup; absolute paths only, because the service does not inherit the shell.
run_pinned() {
	if [ "$DELEG" = yes ]; then
		systemd-run --user --wait --pipe --collect --quiet --same-dir -p Delegate=yes -- \
			taskset -c "$CPU" "$@"
	else
		taskset -c "$CPU" "$@"
	fi
}

ABS="$PWD"
ABS_RUN="$ABS/$RUNDIR"
DELEG=no

run_broker() {
	local cfgs
	cfgs=$(list_configs "direct broker_noid broker_id egress_direct egress_broker")
	suite_begin broker "$cfgs"
	local r c
	for ((r = R_FROM; r <= R_TO; r++)); do
		for c in $cfgs; do
			if [ "$SMOKE" = 0 ] && have_rows "$RUNDIR/broker.csv" "$r" "$c"; then
				log "run.sh: broker round $r variant $c already has rows"
				exit 3
			fi
			log "broker round $r variant $c"
			taskset -c "$CPU" "$BIN/broker" --variant "$c" --round "$r" \
				--tmpfs-dir "$WORK_TMPFS" --out "$RUNDIR/broker.csv" \
				--selfcheck "$RUNDIR/selfcheck.txt" "${SMOKE_ARG[@]}" 2>>"$LOG" || {
				log "run.sh: broker round $r variant $c failed"
				exit 1
			}
		done
	done
	suite_end broker
}

run_deadline() {
	probe_cgroup
	local all="pidfd_L0 pidfd_L1 pidfd_L4"
	[ "$DELEG" = yes ] && all="$all cgkill_L0 cgkill_L1 cgkill_L4"
	local cfgs c keep=""
	cfgs=$(list_configs "$all")
	for c in $cfgs; do
		case "$c" in
		cgkill_*) [ "$DELEG" = yes ] || { log "deadline $c NOT MEASURED: no delegated cgroup"; continue; } ;;
		esac
		keep="$keep $c"
	done
	cfgs=${keep# }
	suite_begin deadline "$cfgs"
	local r
	for ((r = R_FROM; r <= R_TO; r++)); do
		for c in $cfgs; do
			if [ "$SMOKE" = 0 ] && have_rows "$RUNDIR/deadline.csv" "$r" "$c"; then
				log "run.sh: deadline round $r config $c already has rows"
				exit 3
			fi
			log "deadline round $r config $c"
			run_pinned "$ABS/$BIN/deadline" --config "$c" --round "$r" \
				--out "$ABS_RUN/deadline.csv" --selfcheck "$ABS_RUN/selfcheck.txt" \
				"${SMOKE_ARG[@]}" 2>>"$LOG" || {
				log "run.sh: deadline round $r config $c failed"
				exit 1
			}
		done
	done
	suite_end deadline
}

run_cgroup() {
	probe_cgroup
	if [ "$DELEG" != yes ]; then
		log "cgroup suite NOT MEASURED: no delegated cgroup (see cgroup-probe.txt)"
		return 0
	fi
	local cfgs
	cfgs=$(list_configs "spawn_fork spawn_clone3_into spawn_fork_move")
	suite_begin cgroup "$cfgs"
	local r c
	for ((r = R_FROM; r <= R_TO; r++)); do
		for c in $cfgs; do
			if [ "$SMOKE" = 0 ] && have_rows "$RUNDIR/cgroup.csv" "$r" "$c"; then
				log "run.sh: cgroup round $r config $c already has rows"
				exit 3
			fi
			log "cgroup round $r config $c"
			run_pinned "$ABS/$BIN/cgbench" --config "$c" --round "$r" \
				--out "$ABS_RUN/cgroup.csv" --selfcheck "$ABS_RUN/selfcheck.txt" \
				"${SMOKE_ARG[@]}" 2>>"$LOG" || {
				log "run.sh: cgroup round $r config $c failed"
				exit 1
			}
		done
		if [ "$r" = 1 ]; then
			if [ "$SMOKE" = 0 ] && [ -s "$RUNDIR/cgroup-behaviour.txt" ]; then
				log "run.sh: cgroup-behaviour.txt already exists"
				exit 3
			fi
			log "cgroup round 1 behaviour legs"
			run_pinned "$ABS/$BIN/cgbench" --behaviour --round 1 \
				--out "$ABS_RUN/cgroup-behaviour.txt" --selfcheck "$ABS_RUN/selfcheck.txt" \
				"${SMOKE_ARG[@]}" 2>>"$LOG" || {
				log "run.sh: cgroup behaviour legs failed"
				exit 1
			}
		fi
	done
	suite_end cgroup
}

IFS=',' read -r -a SUITE_LIST <<<"$SUITES"
for s in "${SUITE_LIST[@]}"; do
	case "$s" in
	micro) run_micro ;;
	broker) run_broker ;;
	deadline) run_deadline ;;
	cgroup) run_cgroup ;;
	*) log "run.sh: suite $s is not implemented yet"; exit 1 ;;
	esac
done
log "done: $RUNDIR"
