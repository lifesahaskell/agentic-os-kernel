#!/usr/bin/env bash
# Root wrapper for the BPF-LSM measurements (Phase 8). Root loads and attaches the programs, builds the
# cgroups, drops to an unprivileged uid with setpriv and tags a run.sh micro run. BPF state lives here,
# outside the harness (D-07); the harness only sees the --tag label.
#
#   tools/bpflsm/bpflsm.sh --probe --run-id ID
#   tools/bpflsm/bpflsm.sh --variant global-miss|global-hit|global-miss-nocount|anc --run-id ID --uid UID
#   tools/bpflsm/bpflsm.sh --harn03 --run-id ID --uid UID
#   tools/bpflsm/bpflsm.sh --adv baseline|agent --run-id ID --uid UID
#   tools/bpflsm/bpflsm.sh --dry-run [MODE ...]
#
# Every real mode needs uid 0 and a host with bpf in /sys/kernel/security/lsm. --dry-run needs neither
# and runs nothing privileged. Kernel behaviour named in the comments below is ASSUMED, not measured,
# until the reference host has run the mode (08-04 probe, 08-05 measurement).
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
TOOLS=tools/linux-baseline
BPF=tools/bpflsm
ADV=tools/aok-adv
PIN=/sys/fs/bpf/aok
CGROOT=/sys/fs/cgroup/aok-phase8
CPU=2
GOV_FILE="/sys/devices/system/cpu/cpu$CPU/cpufreq/scaling_governor"
STATS_FILE=/proc/sys/kernel/bpf_stats_enabled

MODE=""
VARIANT=""
ADV_MODE=""
RUN_ID=""
UID_ARG=""
DRY=0

# One entry per hook, in the slot order bpflsm.bpf.c uses for hook_count and hit_count.
HOOKS=(file_open socket_connect bprm_check_security socket_sendmsg)

usage() {
	sed -n '2,14p' "$0" >&2
	exit 64
}

while [ $# -gt 0 ]; do
	case "$1" in
	--probe) MODE=probe; shift ;;
	--variant)
		[ $# -ge 2 ] || usage
		MODE=variant; VARIANT="$2"; shift 2 ;;
	--harn03) MODE=harn03; shift ;;
	--adv)
		[ $# -ge 2 ] || usage
		MODE=adv; ADV_MODE="$2"; shift 2 ;;
	--run-id)
		[ $# -ge 2 ] || usage
		RUN_ID="$2"; shift 2 ;;
	--uid)
		[ $# -ge 2 ] || usage
		UID_ARG="$2"; shift 2 ;;
	--dry-run) DRY=1; shift ;;
	*) usage ;;
	esac
done

die() {
	echo "bpflsm.sh: $*" >&2
	exit 1
}

# ---- argument checks --------------------------------------------------------------------------
case "$MODE" in
variant)
	case "$VARIANT" in
	global-miss | global-hit | global-miss-nocount | anc) ;;
	*) usage ;;
	esac
	;;
adv)
	case "$ADV_MODE" in
	baseline | agent) ;;
	*) usage ;;
	esac
	;;
esac

if [ "$DRY" = 0 ]; then
	[ -n "$MODE" ] || usage
	[ -n "$RUN_ID" ] || usage
	case "$RUN_ID" in
	*[!A-Za-z0-9._-]*) echo "bpflsm.sh: bad run id" >&2; exit 64 ;;
	esac
	case "$MODE" in
	probe) ;;
	*)
		case "$UID_ARG" in
		'' | *[!0-9]*) echo "bpflsm.sh: --uid needs a numeric uid" >&2; exit 64 ;;
		esac
		if [ "$UID_ARG" -eq 0 ]; then
			echo "bpflsm.sh: --uid must be the unprivileged measuring user, not 0" >&2
			exit 64
		fi
		;;
	esac
fi

# ---- --dry-run --------------------------------------------------------------------------------
# Prints the command sequence each mode would run as root. Nothing here touches the host. The text is
# a hand-kept summary of the mode functions below, with placeholders for ids and paths.
dry_probe() {
	cat <<'EOF'
## --probe --run-id ID   (root; needs tools/linux-baseline/results/ID/env.txt, else exit 66)
bpftool version | sed -n 1p                              -> env.txt bpftool_version
grep -qw bpf /sys/kernel/security/lsm                    -> env.txt bpf_in_lsm
mount -t bpf bpf /sys/fs/bpf                             (only if not mounted)
bpftool prog loadall tools/bpflsm/probe.bpf.o /sys/fs/bpf/aok/probe pinmaps /sys/fs/bpf/aok/probe-maps autoattach
                                                         -> env.txt attach_global_file_open
bpftool -j map dump pinned /sys/fs/bpf/aok/probe-maps/fire_count   (sum the per-CPU slot 0, before)
exec 3<REGULAR_FILE; exec 3<&-                           (one open made by the wrapper)
bpftool -j map dump pinned /sys/fs/bpf/aok/probe-maps/fire_count   (sum again, after)
                                                         -> env.txt lsm_hook_fires = after - before
bpftool prog loadall tools/bpflsm/bpflsm_cg.bpf.o /sys/fs/bpf/aok/cg
bpftool cgroup attach /sys/fs/cgroup/aok-phase8/probe lsm_cgroup pinned /sys/fs/bpf/aok/cg/cg_allow_file_open
bpftool cgroup detach /sys/fs/cgroup/aok-phase8/probe lsm_cgroup pinned /sys/fs/bpf/aok/cg/cg_allow_file_open
                                                         -> env.txt attach_lsm_cgroup (yes, or no:ERRNO TEXT; recorded only)
dmesg | grep -qi 'LSM support for eBPF active'           -> env.txt dmesg_ebpf_lsm
sed -i floor_bpf_lsm_cgroup=ATTACH_RESULT, floormethod_bpf_lsm_cgroup=probe
exit 1 when lsm_hook_fires is 0 or attach_global_file_open is no; attach_lsm_cgroup never sets the status
EOF
}

dry_variant() {
	cat <<'EOF'
## --variant V --run-id ID --uid UID   (root; V = global-miss | global-hit | global-miss-nocount | anc)
mkdir /sys/fs/cgroup/aok-phase8/{timed,sibling,probe}; chown UID:GID each (controllers are never enabled)
bpftool prog loadall tools/bpflsm/OBJ /sys/fs/bpf/aok/V pinmaps /sys/fs/bpf/aok/V-maps autoattach
    OBJ = bpflsm.bpf.o (global-miss, global-hit), bpflsm_nocount.bpf.o (global-miss-nocount), bpflsm_anc.bpf.o (anc)
stat -c %i /sys/fs/cgroup/aok-phase8/probe   (and timed; A10: the inode number is the cgroup id)
untimed denial proof, every variant:
  bpftool map update pinned /sys/fs/bpf/aok/V-maps/cg_policy key hex PROBE_ID_LE64 value hex 01 00 00 00
  open SENTINEL from a shell moved into the probe cgroup          -> require EACCES
  bpftool map delete pinned /sys/fs/bpf/aok/V-maps/cg_policy key hex PROBE_ID_LE64
  open SENTINEL again from the probe cgroup                       -> require success
keying for the timed legs:
  global-hit, anc:                 bpftool map update ... key hex TIMED_ID_LE64 value hex 00 00 00 00   (allow value; every lookup hits)
  global-miss, global-miss-nocount: cg_policy stays empty (every lookup misses)
bpftool -j map dump pinned /sys/fs/bpf/aok/V-maps/hook_count   (before; sum per CPU for slots 0..3)
bpftool -j map dump pinned /sys/fs/bpf/aok/V-maps/hit_count    (before; no counters for global-miss-nocount)
echo 0 > /proc/sys/kernel/bpf_stats_enabled; echo performance > .../cpu2/cpufreq/scaling_governor
( echo BASHPID > /sys/fs/cgroup/aok-phase8/timed/cgroup.procs && exec \
  setpriv --reuid UID --regid GID --init-groups bash tools/linux-baseline/run.sh --run-id ID --suites micro --configs C3 --tag bpflsm-V )
bpftool -j map dump pinned ... hook_count / hit_count          (after)
tools/linux-baseline/results/ID/bpflsm-validity.txt, one line per hook:
  hook=H calls=DELTA hits=DELTA n=SUMMED_MICRO_N valid=yes|no
  valid=yes needs calls >= n; hits > 0 for global-hit and anc; hits == 0 for global-miss
  global-miss-nocount: hook=H calls=na hits=na valid=denial-proof
EXIT trap: detach, restore bpf_stats_enabled and the governor, rm -rf /sys/fs/bpf/aok, rmdir the aok-phase8 cgroups
EOF
}

dry_harn03() {
	cat <<'EOF'
## --harn03 --run-id ID --uid UID   (root)
bpftool prog loadall tools/bpflsm/bpflsm_cg.bpf.o /sys/fs/bpf/aok/cg      (lsm_cgroup programs cannot autoattach)
1 bpftool cgroup attach .../aok-phase8/probe lsm_cgroup pinned .../cg/cg_deny_file_open
  open SENTINEL in probe -> require EPERM; open SENTINEL in sibling -> require success; detach
2 bpftool cgroup attach .../aok-phase8/probe lsm_cgroup pinned .../cg/cg_allow_file_open
  open SENTINEL in probe -> require success; detach
3 bpftool cgroup attach /sys/fs/cgroup/aok-phase8 lsm_cgroup pinned .../cg/cg_deny_file_open
  open SENTINEL from the timed leaf -> EPERM means lsm_cgroup_inherits=yes (A4, assumed), else no; detach
4 attach every cg_allow_* program to .../aok-phase8/timed, then three timed runs, each
  ( echo BASHPID > CGROUP/cgroup.procs && exec setpriv --reuid UID --regid GID --init-groups \
    bash tools/linux-baseline/run.sh --run-id ID --suites micro --configs C3 --tag TAG )
    TAG bpflsm-cg from timed, bpflsm-cgsib from sibling, bpflsm-cgroot from /sys/fs/cgroup (root cgroup)
env.txt: lsm_cgroup_inherits, harn03_deny_probe, harn03_deny_sibling, harn03_allow_probe
On an attach failure: record the error, exit 2; measure --variant anc instead (D-11), not a bad host.
EOF
}

dry_adv() {
	cat <<'EOF'
## --adv baseline|agent --run-id ID --uid UID   (root)
mkdir tools/aok-adv/results/ID
mkdir /sys/fs/cgroup/aok-phase8/adv/{broker,agent-a,agent-b,nonagent}; chown UID:GID each
env.txt in tools/aok-adv/results/ID: yama_ptrace_scope read from /proc/sys/kernel/yama/ptrace_scope (never written)
agent only: bpftool prog loadall tools/bpflsm/bpfadv.bpf.o /sys/fs/bpf/aok/adv pinmaps /sys/fs/bpf/aok/adv-maps autoattach
            agent_cg: agent-a -> 1, agent-b -> 2; subtree: agent-a -> 1, agent-b -> 2
for each row of: tools/aok-adv/target/release/aok-adv list-routes   (family route topology; 23 rows)
  agent: sum every per-hook counter (before)
  ( echo BASHPID > /sys/fs/cgroup/aok-phase8/adv/cgroup.procs && exec setpriv --reuid UID --regid GID \
    --init-groups tools/aok-adv/target/release/aok-adv run --socket TMP/adv/stub.sock \
    --cgroup-root /sys/fs/cgroup/aok-phase8/adv --routes ROUTE --topology TOPOLOGY --mode MODE \
    --out TMP/adv/row.jsonl )   then the row is appended to tools/aok-adv/results/ID/MODE.jsonl
  agent: sum every per-hook counter (after); append to tools/aok-adv/results/ID/deny-counters.txt:
    route=R topology=T hook=H before=N after=N   (hooks: task_kill ptrace_access_check
    socket_sendmsg socket_bind file_open)
exit 1 if no rows ran or any row failed; the governor and the stats sysctl are not touched
Note A11 (assumed): the cgroup.kill deny counter is a file_open counter, not a task_kill counter.
EOF
}

if [ "$DRY" = 1 ]; then
	if [ -z "$MODE" ]; then
		dry_probe
		dry_variant
		dry_harn03
		dry_adv
	else
		"dry_${MODE}"
	fi
	exit 0
fi

# ---- helpers (real modes) ---------------------------------------------------------------------
ATTACHED=()
ATTACH_ERR=""
PIN_MADE=0
CG_MADE=0
SAVED_STATS=""
SAVED_GOV=""
TMPD=""
RUNDIR=""
UID_GID=""

kv() { printf '%s=%s\n' "$1" "$2" >>"$RUNDIR/env.txt"; }

need_root() {
	if [ "$(id -u)" -ne 0 ]; then
		echo "bpflsm.sh: needs uid 0 (run it with sudo); --dry-run needs none" >&2
		exit 77
	fi
	command -v bpftool >/dev/null 2>&1 || die "bpftool not found"
	command -v setpriv >/dev/null 2>&1 || die "setpriv not found"
}

# Detaches every cgroup attachment this run made, restores the sysctl and the governor, removes the
# pins and the cgroups it created. Runs on every exit path; keeps the original exit status.
cleanup() {
	local rc=$? e
	trap - EXIT
	set +e
	for e in ${ATTACHED[@]+"${ATTACHED[@]}"}; do
		if [ -n "$e" ]; then
			bpftool cgroup detach "${e%%|*}" lsm_cgroup pinned "${e#*|}" 2>/dev/null
		fi
	done
	if [ -n "$SAVED_STATS" ] && [ -w "$STATS_FILE" ]; then
		echo "$SAVED_STATS" >"$STATS_FILE"
	fi
	if [ -n "$SAVED_GOV" ] && [ -w "$GOV_FILE" ]; then
		echo "$SAVED_GOV" >"$GOV_FILE"
	fi
	if [ "$PIN_MADE" = 1 ]; then
		rm -rf -- "$PIN"
	fi
	if [ "$CG_MADE" = 1 ]; then
		find "$CGROOT" -depth -type d -exec rmdir {} \; 2>/dev/null
	fi
	if [ -n "$TMPD" ]; then
		rm -rf -- "$TMPD"
	fi
	exit "$rc"
}

prepare() {
	need_root
	if mountpoint -q /sys/fs/bpf; then
		:
	else
		mount -t bpf bpf /sys/fs/bpf
	fi
	if [ -e "$PIN" ]; then
		die "$PIN exists (a crashed run left pins): remove it by hand, then retry"
	fi
	if [ -e "$CGROOT" ]; then
		die "$CGROOT exists (a crashed run left cgroups): rmdir its leaves by hand, then retry"
	fi
	trap cleanup EXIT
	trap 'exit 130' INT TERM HUP
	TMPD=$(mktemp -d /dev/shm/aok-bpflsm.XXXXXX)
	mkdir "$PIN"
	PIN_MADE=1
	if [ -r "$STATS_FILE" ]; then SAVED_STATS=$(cat "$STATS_FILE"); fi
	if [ -r "$GOV_FILE" ]; then SAVED_GOV=$(cat "$GOV_FILE"); fi
}

need_bpf_lsm() {
	grep -qw bpf /sys/kernel/security/lsm 2>/dev/null || die "bpf is not in /sys/kernel/security/lsm: this host fails the D-04 gate"
}

user_ids() {
	local g
	g=$(id -g "$UID_ARG") || die "no such uid: $UID_ARG"
	UID_GID="$UID_ARG:$g"
}

# mkcg DIR...: create cgroups and hand them to the measuring user. Controllers are never enabled
# in cgroup.subtree_control, so no controller state changes on the host.
mkcg() {
	local d
	for d in "$@"; do
		mkdir "$d"
		CG_MADE=1
		chown "$UID_GID" "$d" "$d/cgroup.procs" "$d/cgroup.threads" "$d/cgroup.subtree_control"
	done
}

# timed_on / timed_off: the sysctl and the governor change only around the real timed legs.
timed_on() {
	if [ -w "$STATS_FILE" ]; then echo 0 >"$STATS_FILE"; fi
	if [ -w "$GOV_FILE" ]; then echo performance >"$GOV_FILE"; fi
}

timed_off() {
	if [ -n "$SAVED_STATS" ] && [ -w "$STATS_FILE" ]; then echo "$SAVED_STATS" >"$STATS_FILE"; fi
	if [ -n "$SAVED_GOV" ] && [ -w "$GOV_FILE" ]; then echo "$SAVED_GOV" >"$GOV_FILE"; fi
}

# Sum one slot of a pinned PERCPU_ARRAY across CPUs. The JSON shape of `bpftool -j map dump` is ASSUMED:
# formatted.key and formatted.values[].value as integers when BTF is present, byte arrays (integers or
# hex strings, little endian) otherwise. 08-04 is the first run that checks it.
PYSUM='
import json, sys
slot = int(sys.argv[1])
def tobytes(l):
	return bytes(int(b, 16) if isinstance(b, str) else b for b in l)
def num(v):
	if isinstance(v, list):
		return int.from_bytes(tobytes(v), "little")
	return int(v)
total = 0
for e in json.load(sys.stdin):
	f = e.get("formatted", e)
	if num(f["key"]) == slot:
		for v in f["values"]:
			total += num(v["value"])
print(total)
'

map_sum() {
	[ -e "$1" ] || die "map pin $1 is missing"
	bpftool -j map dump pinned "$1" | python3 -I -c "$PYSUM" "$2"
}

# san TEXT: one line, without the characters that would break a key=value line or the sed rewrite.
# shellcheck disable=SC1003 # the \\ inside the single quotes is a literal backslash for tr
san() { printf '%s' "$1" | tr '\n|&\\' '    '; }

# Little-endian 8 byte hex string for a cgroup id, for `key hex ...`.
le64() {
	local h
	h=$(printf '%016x' "$1")
	printf '%s %s %s %s %s %s %s %s' "${h:14:2}" "${h:12:2}" "${h:10:2}" "${h:8:2}" "${h:6:2}" "${h:4:2}" "${h:2:2}" "${h:0:2}"
}

# pol_set MAPS CGID VALUE(01|00) and pol_del MAPS CGID: the cg_policy map. Value 01 denies (-EACCES),
# 00 allows but still counts as a hit.
pol_set() {
	# shellcheck disable=SC2046 # le64 prints eight separate byte words on purpose
	bpftool map update pinned "$1/cg_policy" key hex $(le64 "$2") value hex "$3" 00 00 00
}

pol_del() {
	# shellcheck disable=SC2046 # see pol_set
	bpftool map delete pinned "$1/cg_policy" key hex $(le64 "$2")
}

# open_in_cg CGROUP FILE: a subshell moves itself into CGROUP and opens FILE with a bash builtin.
# Builtins only: under a file_open deny every exec and every library open in the cgroup would be denied
# as well, which would make any denial look like a pass. The subshell id comes from BASHPID because
# $$ is the parent in a subshell. Prints ok, EACCES, EPERM or other:TEXT.
open_in_cg() {
	local msg
	if msg=$( (echo "$BASHPID" >"$1/cgroup.procs" && exec 3<"$2") 2>&1); then
		echo ok
	elif printf '%s' "$msg" | grep -q 'Permission denied'; then
		echo EACCES
	elif printf '%s' "$msg" | grep -q 'Operation not permitted'; then
		echo EPERM
	else
		echo "other:$(san "$msg")"
	fi
}

cg_attach() {
	if ATTACH_ERR=$(bpftool cgroup attach "$1" lsm_cgroup pinned "$2" 2>&1); then
		ATTACHED+=("$1|$2")
		return 0
	fi
	return 1
}

cg_detach() {
	local i
	bpftool cgroup detach "$1" lsm_cgroup pinned "$2"
	for ((i = 0; i < ${#ATTACHED[@]}; i++)); do
		if [ "${ATTACHED[$i]}" = "$1|$2" ]; then
			ATTACHED[i]=""
		fi
	done
}

# run_as_uid CGROUP ARGS...: root moves a subshell into CGROUP, then drops to the measuring user and
# execs ARGS. The move has to be root's: a cgroup.procs write by an unprivileged user needs write access
# to the common ancestor as well. No --no-new-privs, so the C3 leg can still set its own.
# micro runs under plain taskset inside run.sh and inherits the cgroup; the harness's transient-unit
# helper would put it somewhere else (RESEARCH Pitfall 3), so nothing here uses one.
run_as_uid() {
	local cg=$1
	shift
	(
		echo "$BASHPID" >"$cg/cgroup.procs" &&
			exec setpriv --reuid "${UID_GID%%:*}" --regid "${UID_GID#*:}" --init-groups "$@"
	)
}

sentinel() {
	: >"$TMPD/sentinel"
	chmod 644 "$TMPD/sentinel"
	echo "$TMPD/sentinel"
}

# micro_n RUNDIR LABEL BENCH|BENCH...: sum of n over the micro.csv rows of one label and bench set.
micro_n() {
	awk -F, -v c="$2" -v b="$3" '$3 == c && $4 ~ "^(" b ")$" { s += $5 } END { print s + 0 }' "$1/micro.csv"
}

hook_benches() {
	case "$1" in
	file_open) echo 'openat_tmpfs|openat_rootfs' ;;
	socket_connect) echo 'unix_connect|tcp_connect' ;;
	bprm_check_security) echo 'fork_exec_wait|posix_spawn' ;;
	socket_sendmsg) echo 'unix_dgram_pingpong' ;;
	esac
}

# ---- --probe ----------------------------------------------------------------------------------
mode_probe() {
	RUNDIR="$TOOLS/results/$RUN_ID"
	if [ -f "$RUNDIR/env.txt" ]; then
		:
	else
		echo "bpflsm.sh: $RUNDIR/env.txt is missing; run an unprivileged run.sh pass for $RUN_ID first" >&2
		exit 66
	fi
	prepare
	local in_lsm=no attach=no fires=0 cgres="no:not attempted" dm=no f before after err
	if grep -qw bpf /sys/kernel/security/lsm 2>/dev/null; then in_lsm=yes; fi
	kv bpftool_version "$(bpftool version 2>&1 | sed -n 1p)"
	kv bpf_in_lsm "$in_lsm"

	# The D-04 fire test. Attaching proves nothing on a kernel whose active LSM list lacks bpf, so the
	# program counts its own calls and the wrapper reads the counter around one open. Any open on the
	# host moves the counter, so the delta is a "fires at all" signal, not an exact count.
	if err=$(bpftool prog loadall "$BPF/probe.bpf.o" "$PIN/probe" pinmaps "$PIN/probe-maps" autoattach 2>&1); then
		attach=yes
		f="$TMPD/fire"
		: >"$f"
		before=$(map_sum "$PIN/probe-maps/fire_count" 0)
		exec 3<"$f"
		exec 3<&-
		after=$(map_sum "$PIN/probe-maps/fire_count" 0)
		fires=$((after - before))
	else
		echo "bpflsm.sh: probe load failed: $err" >&2
	fi
	kv attach_global_file_open "$attach"
	kv lsm_hook_fires "$fires"

	# BPF_LSM_CGROUP attach, recorded only: it never sets the exit status (discussion Q6).
	mkdir "$CGROOT"
	CG_MADE=1
	mkdir "$CGROOT/probe"
	if err=$(bpftool prog loadall "$BPF/bpflsm_cg.bpf.o" "$PIN/cg" 2>&1); then
		if cg_attach "$CGROOT/probe" "$PIN/cg/cg_allow_file_open"; then
			cgres=yes
			cg_detach "$CGROOT/probe" "$PIN/cg/cg_allow_file_open"
		else
			cgres="no:$(san "$ATTACH_ERR")"
		fi
	else
		cgres="no:load failed: $(san "$err")"
	fi
	kv attach_lsm_cgroup "$cgres"
	if dmesg 2>/dev/null | grep -qi 'LSM support for eBPF active'; then dm=yes; fi
	kv dmesg_ebpf_lsm "$dm"

	# 08-01 writes both floor lines as derived. Rewrite them in place so each key stays unique.
	if grep -q '^floor_bpf_lsm_cgroup=' "$RUNDIR/env.txt"; then
		sed -i "s|^floor_bpf_lsm_cgroup=.*|floor_bpf_lsm_cgroup=$cgres|" "$RUNDIR/env.txt"
	else
		kv floor_bpf_lsm_cgroup "$cgres"
	fi
	if grep -q '^floormethod_bpf_lsm_cgroup=' "$RUNDIR/env.txt"; then
		sed -i 's|^floormethod_bpf_lsm_cgroup=.*|floormethod_bpf_lsm_cgroup=probe|' "$RUNDIR/env.txt"
	else
		kv floormethod_bpf_lsm_cgroup probe
	fi

	if [ "$fires" -eq 0 ] || [ "$attach" = no ]; then
		echo "bpflsm.sh: D-04 gate FAILED: attach_global_file_open=$attach lsm_hook_fires=$fires" >&2
		exit 1
	fi
	echo "bpflsm.sh: probe ok: lsm_hook_fires=$fires attach_lsm_cgroup=$cgres" >&2
}

# ---- --variant --------------------------------------------------------------------------------
mode_variant() {
	local v=$VARIANT obj maps probe_id timed_id key_probe key_timed proof i h calls hits n ok lbl
	local -a cb=() ca=() hb=() ha=()
	RUNDIR="$TOOLS/results/$RUN_ID"
	case "$v" in
	global-miss | global-hit) obj=bpflsm.bpf.o ;;
	global-miss-nocount) obj=bpflsm_nocount.bpf.o ;;
	anc) obj=bpflsm_anc.bpf.o ;;
	esac
	prepare
	need_bpf_lsm
	user_ids
	[ -f "$BPF/$obj" ] || die "$BPF/$obj is missing; run make -C $BPF on the host first"
	maps="$PIN/$v-maps"
	lbl="C3-bpflsm-$v"

	mkdir "$CGROOT"
	CG_MADE=1
	chown "$UID_GID" "$CGROOT"
	mkcg "$CGROOT/timed" "$CGROOT/sibling" "$CGROOT/probe"
	bpftool prog loadall "$BPF/$obj" "$PIN/$v" pinmaps "$maps" autoattach

	# A10 (assumed): on a 64-bit kernel the cgroup directory inode number is the cgroup id.
	probe_id=$(stat -c %i "$CGROOT/probe")
	timed_id=$(stat -c %i "$CGROOT/timed")
	# bpflsm_anc.bpf.o keys on the ancestor at the depth of the aok-phase8 cgroup (level 1 under the
	# cgroup2 root), so for anc the key is the aok-phase8 id and the proof denies the whole subtree.
	key_probe=$probe_id
	key_timed=$timed_id
	if [ "$v" = anc ]; then
		key_probe=$(stat -c %i "$CGROOT")
		key_timed=$key_probe
	fi

	# Untimed denial proof (D-09), every variant: the hook has to be able to deny, and the probe
	# cgroup is the only one keyed to 1. The sentinel is opened by a builtin, see open_in_cg.
	local sent
	sent=$(sentinel)
	pol_set "$maps" "$key_probe" 01
	proof=$(open_in_cg "$CGROOT/probe" "$sent")
	pol_del "$maps" "$key_probe"
	if [ "$proof" = EACCES ]; then
		proof=$(open_in_cg "$CGROOT/probe" "$sent")
		if [ "$proof" = ok ]; then
			proof=pass
		else
			proof="fail: open still denied after the entry was deleted ($proof)"
		fi
	else
		proof="fail: expected EACCES with the probe cgroup keyed to 1, got $proof"
	fi
	if [ "$proof" = pass ]; then
		echo "bpflsm.sh: denial proof pass ($v)" >&2
	else
		die "denial proof $proof ($v): no timed rows are produced for this leg"
	fi

	case "$v" in
	global-hit | anc) pol_set "$maps" "$key_timed" 00 ;;
	esac

	if [ "$v" = global-miss-nocount ]; then
		:
	else
		for i in 0 1 2 3; do
			cb[i]=$(map_sum "$maps/hook_count" "$i")
			hb[i]=$(map_sum "$maps/hit_count" "$i")
		done
	fi

	timed_on
	if run_as_uid "$CGROOT/timed" bash "$TOOLS/run.sh" --run-id "$RUN_ID" --suites micro --configs C3 --tag "bpflsm-$v"; then
		timed_off
	else
		timed_off
		die "run.sh failed for $v"
	fi

	if [ "$v" = global-miss-nocount ]; then
		:
	else
		for i in 0 1 2 3; do
			ca[i]=$(map_sum "$maps/hook_count" "$i")
			ha[i]=$(map_sum "$maps/hit_count" "$i")
		done
	fi

	# bpflsm-validity.txt: a leg is valid only when the hook demonstrably ran as many times as the
	# timed loops needed and took the path the variant claims. Anything else is INVALID, not a cost.
	: >"$RUNDIR/bpflsm-validity.txt"
	for i in 0 1 2 3; do
		h=${HOOKS[$i]}
		if [ "$v" = global-miss-nocount ]; then
			printf 'hook=%s calls=na hits=na valid=denial-proof\n' "$h" >>"$RUNDIR/bpflsm-validity.txt"
			continue
		fi
		calls=$((ca[i] - cb[i]))
		hits=$((ha[i] - hb[i]))
		n=$(micro_n "$RUNDIR" "$lbl" "$(hook_benches "$h")")
		ok=no
		if [ "$n" -gt 0 ] && [ "$calls" -ge "$n" ]; then
			case "$v" in
			global-hit | anc) if [ "$hits" -gt 0 ]; then ok=yes; fi ;;
			global-miss) if [ "$hits" -eq 0 ]; then ok=yes; fi ;;
			esac
		fi
		printf 'hook=%s calls=%s hits=%s n=%s valid=%s\n' "$h" "$calls" "$hits" "$n" "$ok" >>"$RUNDIR/bpflsm-validity.txt"
	done
	chown "$UID_GID" "$RUNDIR/bpflsm-validity.txt"
	echo "bpflsm.sh: wrote $RUNDIR/bpflsm-validity.txt" >&2
}

# ---- --harn03 ---------------------------------------------------------------------------------
mode_harn03() {
	local sent r_deny_probe r_deny_sib r_allow_probe inherits p
	local -a allow=()
	RUNDIR="$TOOLS/results/$RUN_ID"
	prepare
	need_bpf_lsm
	user_ids
	[ -f "$BPF/bpflsm_cg.bpf.o" ] || die "$BPF/bpflsm_cg.bpf.o is missing; run make -C $BPF on the host first"

	mkdir "$CGROOT"
	CG_MADE=1
	chown "$UID_GID" "$CGROOT"
	mkcg "$CGROOT/timed" "$CGROOT/sibling" "$CGROOT/probe"
	sent=$(sentinel)

	# lsm_cgroup programs cannot autoattach: load them, then attach each to a cgroup by hand.
	bpftool prog loadall "$BPF/bpflsm_cg.bpf.o" "$PIN/cg"
	for p in "$PIN"/cg/cg_allow_*; do
		allow+=("$p")
	done

	record_fail() {
		mkdir -p "$RUNDIR"
		kv lsm_cgroup_attach_error "$(san "$ATTACH_ERR")"
		echo "bpflsm.sh: lsm_cgroup attach failed: $ATTACH_ERR" >&2
		echo "bpflsm.sh: recorded in $RUNDIR/env.txt; measure --variant anc instead (D-11). Not a bad host." >&2
		exit 2
	}

	# Step 1: the deny program, attached to the probe cgroup only. Return 0 is expected to give EPERM.
	if cg_attach "$CGROOT/probe" "$PIN/cg/cg_deny_file_open"; then :; else record_fail; fi
	r_deny_probe=$(open_in_cg "$CGROOT/probe" "$sent")
	r_deny_sib=$(open_in_cg "$CGROOT/sibling" "$sent")
	cg_detach "$CGROOT/probe" "$PIN/cg/cg_deny_file_open"

	# Step 2: the allow programs on the probe cgroup. Return 1 is expected to allow.
	for p in "${allow[@]}"; do
		if cg_attach "$CGROOT/probe" "$p"; then :; else record_fail; fi
	done
	r_allow_probe=$(open_in_cg "$CGROOT/probe" "$sent")
	for p in "${allow[@]}"; do
		cg_detach "$CGROOT/probe" "$p"
	done

	# Step 3: A4, does an attachment on an ancestor reach a descendant leaf? Allow programs cannot show
	# it, so the deny program goes on aok-phase8 (never the root or the session cgroup) and the open is
	# made from the timed leaf. EPERM means the attachment was inherited.
	if cg_attach "$CGROOT" "$PIN/cg/cg_deny_file_open"; then :; else record_fail; fi
	case "$(open_in_cg "$CGROOT/timed" "$sent")" in
	EPERM) inherits=yes ;;
	*) inherits=no ;;
	esac
	cg_detach "$CGROOT" "$PIN/cg/cg_deny_file_open"

	if [ "$r_deny_probe" = EPERM ] && [ "$r_deny_sib" = ok ] && [ "$r_allow_probe" = ok ]; then
		:
	else
		mkdir -p "$RUNDIR"
		kv harn03_deny_probe "$r_deny_probe"
		kv harn03_deny_sibling "$r_deny_sib"
		kv harn03_allow_probe "$r_allow_probe"
		kv lsm_cgroup_inherits "$inherits"
		die "HARN-03 steps 1-2 did not give EPERM, ok, ok (got $r_deny_probe, $r_deny_sib, $r_allow_probe): no timed rows"
	fi

	# Step 4: the allow programs on the timed leaf only, then three timed runs. The sibling and the root
	# cgroup have no program attached, which makes them the membership controls.
	for p in "${allow[@]}"; do
		if cg_attach "$CGROOT/timed" "$p"; then :; else record_fail; fi
	done
	timed_on
	if run_as_uid "$CGROOT/timed" bash "$TOOLS/run.sh" --run-id "$RUN_ID" --suites micro --configs C3 --tag bpflsm-cg &&
		run_as_uid "$CGROOT/sibling" bash "$TOOLS/run.sh" --run-id "$RUN_ID" --suites micro --configs C3 --tag bpflsm-cgsib &&
		run_as_uid /sys/fs/cgroup bash "$TOOLS/run.sh" --run-id "$RUN_ID" --suites micro --configs C3 --tag bpflsm-cgroot; then
		timed_off
	else
		timed_off
		die "run.sh failed during the HARN-03 timed legs"
	fi

	# run.sh writes env.txt only while it is empty, so these keys go in after its first call.
	kv harn03_deny_probe "$r_deny_probe"
	kv harn03_deny_sibling "$r_deny_sib"
	kv harn03_allow_probe "$r_allow_probe"
	kv lsm_cgroup_inherits "$inherits"
	echo "bpflsm.sh: HARN-03 done: lsm_cgroup_inherits=$inherits" >&2
}

# ---- --adv ------------------------------------------------------------------------------------
# u64_put MAP_PIN CGID VALUE: write a u64 -> u32 entry (VALUE below 256) into a pinned HASH map.
u64_put() {
	# shellcheck disable=SC2046 # see pol_set
	bpftool map update pinned "$1" key hex $(le64 "$2") value hex "$(printf '%02x' "$3")" 00 00 00
}

# adv_read: read the five adv_count slots (summed over CPUs) into ADV_NOW[0..4].
adv_read() {
	local i
	for i in 0 1 2 3 4; do
		ADV_NOW[i]=$(map_sum "$PIN/adv-maps/adv_count" "$i")
	done
}

# One route row at a time: baseline runs every row with the non-agent cgroup and no BPF program, agent
# runs every row from the agent cgroups with bpfadv loaded. Both read the route list from the same
# aok-adv binary, which must already be built by the unprivileged user (cargo is never run as root).
#
# A11 (assumed): the cgroup.kill deny counter is a file_open counter, not a task_kill counter, because
# __cgroup_kill goes through send_sig and likely skips security_task_kill. The report carries that note.
#
# ptrace_scope is read and recorded as yama_ptrace_scope and never written: the user sets it to the OQ1
# Answer before the run (08-05 Task 3) and restores it afterwards.
mode_adv() {
	local m=$ADV_MODE bin="$ADV/target/release/aok-adv" outdir="$ADV/results/$RUN_ID" scope prev
	local family rest route topo i h failed=0 rows=0 maps="$PIN/adv-maps" a_id b_id
	local -a before=() ADV_NOW=()
	local -a ADV_HOOKS=(task_kill ptrace_access_check socket_sendmsg socket_bind file_open)
	RUNDIR=$outdir
	prepare
	user_ids
	[ -x "$bin" ] || die "$bin is missing; as the unprivileged user run: cargo build --release --manifest-path $ADV/Cargo.toml"
	if [ -e "$outdir/$m.jsonl" ]; then
		die "$outdir/$m.jsonl exists: use a fresh run id"
	fi
	if [ "$m" = agent ]; then
		need_bpf_lsm
		[ -f "$BPF/bpfadv.bpf.o" ] || die "$BPF/bpfadv.bpf.o is missing; run make -C $BPF on the host first"
	fi

	mkdir -p "$outdir"
	if [ -r /proc/sys/kernel/yama/ptrace_scope ]; then
		scope=$(cat /proc/sys/kernel/yama/ptrace_scope)
	else
		scope=absent
	fi
	if [ -f "$outdir/env.txt" ]; then
		prev=$(sed -n 's/^yama_ptrace_scope=//p' "$outdir/env.txt" | sed -n 1p)
		if [ "$prev" = "$scope" ]; then
			:
		else
			die "yama_ptrace_scope was $prev in the first run of $RUN_ID and is $scope now: baseline and agent must run under the same value"
		fi
	else
		kv yama_ptrace_scope "$scope"
		kv adv_cgroup_root "$CGROOT/adv"
	fi

	mkdir "$CGROOT"
	CG_MADE=1
	chown "$UID_GID" "$CGROOT"
	mkcg "$CGROOT/adv" "$CGROOT/adv/broker" "$CGROOT/adv/agent-a" "$CGROOT/adv/agent-b" "$CGROOT/adv/nonagent"
	# The runner is started inside adv, which the user owns, so aok-adv may move its own children into
	# the leaves below it (a cgroup.procs move needs write access to the common ancestor). The stub
	# socket and the row file live in a user-owned directory under the root-made temp dir.
	chmod 755 "$TMPD"
	mkdir "$TMPD/adv"
	chown "$UID_GID" "$TMPD/adv"

	if [ "$m" = agent ]; then
		bpftool prog loadall "$BPF/bpfadv.bpf.o" "$PIN/adv" pinmaps "$maps" autoattach
		a_id=$(stat -c %i "$CGROOT/adv/agent-a")
		b_id=$(stat -c %i "$CGROOT/adv/agent-b")
		u64_put "$maps/agent_cg" "$a_id" 1
		u64_put "$maps/agent_cg" "$b_id" 2
		u64_put "$maps/subtree" "$a_id" 1
		u64_put "$maps/subtree" "$b_id" 2
		: >"$outdir/deny-counters.txt"
	fi

	run_as_uid "$CGROOT/adv" "$bin" list-routes >"$TMPD/routes"
	: >"$outdir/$m.jsonl"
	# Not a timing run: the governor and the stats sysctl are left alone.
	while read -r family rest; do
		if [ -z "$family" ]; then
			continue
		fi
		# A row is "family route topology"; a route may be more than one word, the topology is last.
		topo=${rest##* }
		route=${rest% *}
		rows=$((rows + 1))
		if [ "$m" = agent ]; then
			adv_read
			before=("${ADV_NOW[@]}")
		fi
		rm -f "$TMPD/adv/row.jsonl"
		if run_as_uid "$CGROOT/adv" "$bin" run --socket "$TMPD/adv/stub.sock" \
			--cgroup-root "$CGROOT/adv" --routes "$route" --topology "$topo" --mode "$m" \
			--out "$TMPD/adv/row.jsonl"; then
			:
		else
			failed=$((failed + 1))
			echo "bpflsm.sh: aok-adv run failed for $family $route $topo" >&2
		fi
		if [ -f "$TMPD/adv/row.jsonl" ]; then
			cat "$TMPD/adv/row.jsonl" >>"$outdir/$m.jsonl"
		fi
		if [ "$m" = agent ]; then
			adv_read
			for i in 0 1 2 3 4; do
				h=${ADV_HOOKS[$i]}
				printf 'route=%s topology=%s hook=%s before=%s after=%s\n' \
					"$route" "$topo" "$h" "${before[$i]}" "${ADV_NOW[$i]}" >>"$outdir/deny-counters.txt"
			done
		fi
	done <"$TMPD/routes"

	chown -R "$UID_GID" "$outdir"
	echo "bpflsm.sh: --adv $m: $rows rows, $failed failed; wrote $outdir/$m.jsonl" >&2
	if [ "$rows" -eq 0 ] || [ "$failed" -gt 0 ]; then
		exit 1
	fi
}

case "$MODE" in
probe) mode_probe ;;
variant) mode_variant ;;
harn03) mode_harn03 ;;
adv) mode_adv ;;
esac
