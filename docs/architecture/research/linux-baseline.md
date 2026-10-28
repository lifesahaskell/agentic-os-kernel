# Linux baseline: what the kernel mechanisms cost on this host

Status: draft 1, 2026-10-28 (run date, UTC).

This report grounds three rows of [scoping-linux.md](../scoping-linux.md) section 4 that were
proposed without a measurement: S3 (per-call overhead on kernel-enforced paths), S4 (broker round
trip) and the wall-time leg of S6 (budget enforcement). It follows
[ADR 0004](../adr/0004-linux-prototype-first.md), which keeps those rows open until a baseline
exists. Every number below was printed by the harness in `tools/linux-baseline/` during one run
on one host. The run directory is committed:
[20261028T034700Z](../../../tools/linux-baseline/results/20261028T034700Z/summary.md). Nothing here
accepts or changes a threshold; section 11 proposes revisions and the user decides.

## 1. Verdict

Partially stable. The ten C0 microbenchmark rows are stable on the L12 test (spread of round p50
under 10 percent, stable 10, unstable 0, noise-dominated 0), and C0 `getppid` and `openat_tmpfs`
medians sit inside the ASSUMED plausibility windows. The p99 columns are not stable on four C0
rows (`getppid` 22.14, `getppid_x1000` 15.48, `openat_tmpfs` 12.11, `openat_rootfs` 14.37 percent
spread). The deadline suite is stable on 7 of its 21 rows, unstable on 3 and noise-dominated on 11;
both L4 `deadline_to_reaped` rows are noise-dominated (summary.md, section "Deadline enforcement").
The summary.md verdict block, which tests C0 p50 spread only, agrees with the micro half of this
paragraph.

Plausibility windows (ASSUMED): C0 `getppid` med_p50 is 130.000 ns against a window of 60 to 400 ns;
C0 `openat_tmpfs` med_p50 is 1763.000 ns against a window of 800 to 5000 ns. Both are inside.

One-line answers:

- **S3.** With seccomp (64 instructions) and Landlock (fs, TCP connect, scopes) active and no BPF-LSM,
  `openat` fails the proposed bound: `openat_tmpfs` adds 25.01 percent at p50 and 30.09 percent at
  p99, `openat_rootfs` adds 17.48 percent at p50 (19.29 percent at p99 is under 25). The other
  five rows are inside 10 percent at p50 and 25 percent at p99 by point estimate.
- **S4.** A `SOCK_SEQPACKET` broker with caller identity adds 19116.000 ns at p99 over an in-process
  lookup; a broker-performed egress connect adds 20118.000 ns at p99 over a direct connect. The
  proposed 1 ms bound (1000000 ns) is not exceeded. The p99 spreads of both rows are above 10
  percent, so no revision is derived.
- **S6.** The wall-time leg is dominated by scheduler delay under same-CPU load. With four busy
  competitors, p99 `deadline_to_reaped` is 17948443 ns with `pidfd_send_signal` and 3870886 ns with
  `cgroup.kill`; the proposed 1 s bound is not exceeded in either, but both rows are
  noise-dominated, so no revision is derived.

## 2. Host

Source: [env.txt](../../../tools/linux-baseline/results/20261028T034700Z/env.txt).

| Item | Value |
|---|---|
| Kernel | 6.18.33.2-microsoft-standard-WSL2 |
| CPU model | AMD Ryzen 9 7900X3D 12-Core Processor |
| nproc | 12 |
| Pinned CPU, core sibling | 2, 3 |
| WSL2, Hyper-V, systemd-detect-virt | yes, yes, wsl |
| /dev/kvm | absent |
| Clocksource | tsc |
| Timer resolution, timer quantum | 1 ns, 10 ns |
| Active LSMs (lsm_list_modules) | capability, landlock, yama, safesetid, selinux, ima |
| Landlock ABI | 7 |
| Toolchain | gcc 13.3.0, glibc 2.39 |
| Harness revision, dirty | 90b8c332d6f7c1a3c735568e0a0a31e7de5e48a9, no |
| Top CPU users before the run (comm names) | MainThread, MainThread, MainThread, claude, cpptools |

Load average (1, 5, 15 minutes) before and after each suite. The "after" figures include the
suite's own load.

| Suite | Before | After | Elapsed (s) |
|---|---|---|---|
| micro | 0.09 0.23 0.28 | 1.49 0.94 0.57 | 200 |
| broker | 1.49 0.94 0.57 | 1.56 1.00 0.59 | 18 |
| deadline | 1.56 1.00 0.59 | 2.64 1.49 0.81 | 85 |
| cgroup | 2.64 1.49 0.81 | 2.46 1.49 0.81 | 8 |

Before the run, `ps` showed no process above 5.5 percent of a CPU (observed by the executor, not a
harness file). This host is a WSL2 guest on Hyper-V. It is not one of the five target distros in
[scoping-linux.md](../scoping-linux.md) section 3.

## 3. Method

**Clock.** Micro, broker and cgroup suites time each call with `clock_gettime(CLOCK_MONOTONIC_RAW)`
(vDSO, tsc). The harness reports timer resolution 1 ns and timer quantum 10 ns: consecutive reads
differ in multiples of 10 ns. `timer_pair` (two back-to-back reads) therefore reads 20.000 ns at
p50 and is the floor under every other row. The deadline suite uses `CLOCK_MONOTONIC`, because
`timerfd` cannot be armed on the raw clock.

**Warm-up and N per round.** Micro, broker and cgroup benches warm up on 10 percent of their sample
count, discarded; the deadline suite discards 10 warm-up trials. Samples per round: `timer_pair` and `getppid` 1000000; `getppid_x1000` 2000 batches; `openat_*` 200000;
`unix_connect` 50000; `tcp_connect` 20000; `fork_exec_wait` and `posix_spawn` 1000;
`unix_dgram_pingpong` 200000; broker `direct` 200000, `broker_noid` and `broker_id` 100000, egress
variants 20000; deadline 200 trials per round after 10 warm-up trials; cgroup spawn 1000. Five
rounds each; the n_total column is the sum.

**Statistics.** Quantiles are nearest-rank over a round's sorted samples. Per config the table shows
the median of the five round values. Spread is 100 x (max - min) / median over the five round
values. Stability classes: stable under 10 percent, unstable 10 to 25, noise-dominated over 25,
tested on the p50 spread. `within_noise` is yes when the round-p50 ranges of the config and its
baseline overlap. Added overhead is config median minus baseline median, in ns and as a percent of
the baseline.

**Interleaving and isolation.** Rounds are interleaved (C0 C1a C1b C1c C2a C2b C3, repeated). Every
(round, config) pair runs in a fresh process, pinned with `taskset` to CPU 2. Landlock is applied
before seccomp.

**What each config applies.**

| Config | Restriction |
|---|---|
| C0 | none |
| C1a | seccomp allow-all, 1 instruction |
| C1b | seccomp allowlist of 58 syscalls, 64 instructions |
| C1c | C1b plus one load of `args[0]`, 65 instructions (A1) |
| C2a | Landlock filesystem ruleset, all ABI 7 fs rights |
| C2b | Landlock TCP connect rules plus abstract-unix-socket and signal scopes |
| C3 | C2a plus C2b plus the C1b filter |

**Self-checks (A4).** Before measuring, each process reads `/proc/self/status` (Seccomp,
Seccomp_filters, NoNewPrivs), counts filter instructions, and runs negative tests that must be
denied: a syscall outside the allowlist, a file outside the ruleset, a TCP connect to a denied port,
an abstract unix connect, a signal to a process outside the scope. A failed check aborts the run.
[selfcheck.txt](../../../tools/linux-baseline/results/20261028T034700Z/selfcheck.txt) holds 106 lines
and every one reads `result=ok`; C3 passes all five negatives and C1c has one instruction more than
C1b.

**Planner additions.** Each was proposed by the planner and can be dropped.

- A1, config C1c: since Linux 5.11 the seccomp action cache precomputes an allow bitmap for filters
  whose verdict depends only on arch and syscall number, so C1a and C1b can skip BPF execution for
  allowed calls. C1c loads `args[0]` first, which defeats the cache and measures filter execution.
- A2, `getppid_x1000`: 1000 consecutive `getppid` calls timed together and divided, because the
  10 ns timer quantum against a call of about 130 ns resolves single-call medians in coarse steps.
- A3, TCP clients set `SO_LINGER {1,0}` so `close` sends RST; the listener is in the measuring
  process and is drained with non-blocking `accept4` outside the timed region. This avoids TIME_WAIT
  port exhaustion and SYN drops from a full accept queue.
- A4, self-checks, above.
- A5, `run.sh --suites` and `--rounds`, so each foreground command stays under the 10 minute limit.
- A6, the S6 leg also runs with `cgroup.kill` as the kill mechanism (`cgkill_L0`, `cgkill_L1`,
  `cgkill_L4`), when the probe finds a delegated cgroup.
- A7, a PSI trigger registration probe on the delegated leaf, success or errno only.
- A8, the active LSM list through the `lsm_list_modules` syscall (461), because securityfs is not
  mounted.

**Run attempts.** One measured run, id `20261028T034700Z`, run once with no rerun. Suites ran in
order micro, broker, deadline, cgroup as separate foreground commands with nothing else running.
Smoke runs during development used ids starting with `smoke-`, are gitignored, and feed no number
here.

## 4. Microbenchmarks (S3)

Source: [summary.md](../../../tools/linux-baseline/results/20261028T034700Z/summary.md) and
[summary.csv](../../../tools/linux-baseline/results/20261028T034700Z/summary.csv), micro rows,
copied unchanged. All times are ns; `_pct` columns are percent; `baseline` is C0 for every added
column. The "mediated by" column lists which mechanism sits on that call path. Landlock does not
mediate `getppid`. Pathname unix connect is not covered by Landlock fs or scope rules on ABI 7, and
the `unix_dgram_pingpong` peer is unrestricted. Those cells are noise-floor controls: a Landlock-only
config shows `within_noise` yes there.

| config | bench | mediated by | med_p50_ns | spread_p50_pct | med_p99_ns | spread_p99_pct | med_p999_ns | added_p50_ns | added_p50_pct | added_p99_ns | added_p99_pct | baseline | within_noise | stability |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C0 | timer_pair | none (no syscall) | 20.000 | 0.00 | 21.000 | 0.00 | 30.000 | - | - | - | - | - | - | stable |
| C0 | getppid | none | 130.000 | 0.00 | 131.000 | 22.14 | 260.000 | - | - | - | - | - | - | stable |
| C0 | getppid_x1000 | none | 120.628 | 0.96 | 141.076 | 15.48 | 156.225 | - | - | - | - | - | - | stable |
| C0 | openat_tmpfs | none | 1763.000 | 9.64 | 2064.000 | 12.11 | 10710.000 | - | - | - | - | - | - | stable |
| C0 | openat_rootfs | none | 2414.000 | 0.46 | 3006.000 | 14.37 | 12413.000 | - | - | - | - | - | - | stable |
| C0 | unix_connect | none | 3417.000 | 0.91 | 5590.000 | 4.49 | 23073.000 | - | - | - | - | - | - | stable |
| C0 | tcp_connect | none | 18235.000 | 1.27 | 38443.000 | 2.55 | 278617.000 | - | - | - | - | - | - | stable |
| C0 | fork_exec_wait | none | 736572.000 | 1.30 | 975074.000 | 3.60 | 1069943.000 | - | - | - | - | - | - | stable |
| C0 | posix_spawn | none | 343830.000 | 1.02 | 618981.000 | 7.35 | 716174.000 | - | - | - | - | - | - | stable |
| C0 | unix_dgram_pingpong | none | 6372.000 | 6.61 | 10450.000 | 5.27 | 27451.000 | - | - | - | - | - | - | stable |
| C1a | timer_pair | none (no syscall) | 20.000 | 0.00 | 21.000 | 0.00 | 30.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C1a | getppid | seccomp | 170.000 | 0.00 | 171.000 | 5.26 | 340.000 | 40.000 | 30.77 | 40.000 | 30.53 | C0 | no | stable |
| C1a | getppid_x1000 | seccomp | 161.094 | 0.47 | 182.144 | 10.72 | 213.834 | 40.466 | 33.55 | 41.068 | 29.11 | C0 | no | stable |
| C1a | openat_tmpfs | seccomp | 1833.000 | 0.55 | 2094.000 | 9.07 | 10730.000 | 70.000 | 3.97 | 30.000 | 1.45 | C0 | yes | stable |
| C1a | openat_rootfs | seccomp | 2494.000 | 0.40 | 3106.000 | 19.35 | 12644.000 | 80.000 | 3.31 | 100.000 | 3.33 | C0 | no | stable |
| C1a | unix_connect | seccomp | 3496.000 | 0.54 | 5661.000 | 3.16 | 20299.000 | 79.000 | 2.31 | 71.000 | 1.27 | C0 | no | stable |
| C1a | tcp_connect | seccomp | 18385.000 | 1.96 | 37771.000 | 9.89 | 266645.000 | 150.000 | 0.82 | -672.000 | -1.75 | C0 | yes | stable |
| C1a | fork_exec_wait | seccomp | 734829.000 | 1.35 | 987087.000 | 6.73 | 1054945.000 | -1743.000 | -0.24 | 12013.000 | 1.23 | C0 | yes | stable |
| C1a | posix_spawn | seccomp | 345363.000 | 3.04 | 627316.000 | 11.30 | 709481.000 | 1533.000 | 0.45 | 8335.000 | 1.35 | C0 | yes | stable |
| C1a | unix_dgram_pingpong | seccomp | 6422.000 | 2.97 | 10519.000 | 5.32 | 27703.000 | 50.000 | 0.78 | 69.000 | 0.66 | C0 | yes | stable |
| C1b | timer_pair | none (no syscall) | 20.000 | 0.00 | 21.000 | 0.00 | 30.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C1b | getppid | seccomp | 170.000 | 0.00 | 171.000 | 28.65 | 340.000 | 40.000 | 30.77 | 40.000 | 30.53 | C0 | no | stable |
| C1b | getppid_x1000 | seccomp | 161.075 | 0.48 | 183.647 | 3.99 | 197.984 | 40.447 | 33.53 | 42.571 | 30.18 | C0 | no | stable |
| C1b | openat_tmpfs | seccomp | 1834.000 | 0.60 | 2204.000 | 16.38 | 10911.000 | 71.000 | 4.03 | 140.000 | 6.78 | C0 | yes | stable |
| C1b | openat_rootfs | seccomp | 2494.000 | 1.20 | 3045.000 | 9.23 | 12343.000 | 80.000 | 3.31 | 39.000 | 1.30 | C0 | no | stable |
| C1b | unix_connect | seccomp | 3506.000 | 0.54 | 5791.000 | 7.25 | 23584.000 | 89.000 | 2.60 | 201.000 | 3.60 | C0 | no | stable |
| C1b | tcp_connect | seccomp | 18335.000 | 1.42 | 38192.000 | 11.59 | 277825.000 | 100.000 | 0.55 | -251.000 | -0.65 | C0 | yes | stable |
| C1b | fork_exec_wait | seccomp | 737674.000 | 1.16 | 1009198.000 | 17.08 | 1097375.000 | 1102.000 | 0.15 | 34124.000 | 3.50 | C0 | yes | stable |
| C1b | posix_spawn | seccomp | 345103.000 | 0.77 | 636083.000 | 7.52 | 747293.000 | 1273.000 | 0.37 | 17102.000 | 2.76 | C0 | yes | stable |
| C1b | unix_dgram_pingpong | seccomp | 6412.000 | 0.62 | 10490.000 | 0.76 | 27622.000 | 40.000 | 0.63 | 40.000 | 0.38 | C0 | yes | stable |
| C1c | timer_pair | none (no syscall) | 20.000 | 0.00 | 21.000 | 0.00 | 30.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C1c | getppid | seccomp | 180.000 | 0.00 | 191.000 | 26.18 | 370.000 | 50.000 | 38.46 | 60.000 | 45.80 | C0 | no | stable |
| C1c | getppid_x1000 | seccomp | 175.372 | 0.49 | 197.634 | 12.61 | 217.898 | 54.744 | 45.38 | 56.558 | 40.09 | C0 | no | stable |
| C1c | openat_tmpfs | seccomp | 1893.000 | 0.58 | 2274.000 | 14.56 | 11392.000 | 130.000 | 7.37 | 210.000 | 10.17 | C0 | yes | stable |
| C1c | openat_rootfs | seccomp | 2545.000 | 0.43 | 3136.000 | 5.77 | 12544.000 | 131.000 | 5.43 | 130.000 | 4.32 | C0 | no | stable |
| C1c | unix_connect | seccomp | 3557.000 | 1.12 | 5861.000 | 2.56 | 21480.000 | 140.000 | 4.10 | 271.000 | 4.85 | C0 | no | stable |
| C1c | tcp_connect | seccomp | 18434.000 | 1.19 | 38052.000 | 5.79 | 243089.000 | 199.000 | 1.09 | -391.000 | -1.02 | C0 | yes | stable |
| C1c | fork_exec_wait | seccomp | 740340.000 | 0.79 | 993147.000 | 5.75 | 1127823.000 | 3768.000 | 0.51 | 18073.000 | 1.85 | C0 | no | stable |
| C1c | posix_spawn | seccomp | 348419.000 | 1.92 | 689363.000 | 14.24 | 757022.000 | 4589.000 | 1.33 | 70382.000 | 11.37 | C0 | yes | stable |
| C1c | unix_dgram_pingpong | seccomp | 6512.000 | 7.39 | 10610.000 | 4.82 | 27852.000 | 140.000 | 2.20 | 160.000 | 1.53 | C0 | yes | stable |
| C2a | timer_pair | none (no syscall) | 20.000 | 0.00 | 21.000 | 0.00 | 30.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C2a | getppid | none | 130.000 | 0.00 | 131.000 | 30.53 | 260.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C2a | getppid_x1000 | none | 120.618 | 0.96 | 142.449 | 3.71 | 179.209 | -0.010 | -0.01 | 1.373 | 0.97 | C0 | yes | stable |
| C2a | openat_tmpfs | Landlock fs | 2134.000 | 0.94 | 2595.000 | 26.67 | 11872.000 | 371.000 | 21.04 | 531.000 | 25.73 | C0 | no | stable |
| C2a | openat_rootfs | Landlock fs | 2775.000 | 1.12 | 3566.000 | 18.00 | 12975.000 | 361.000 | 14.95 | 560.000 | 18.63 | C0 | no | stable |
| C2a | unix_connect | none | 3417.000 | 0.29 | 5600.000 | 3.21 | 20720.000 | 0.000 | 0.00 | 10.000 | 0.18 | C0 | yes | stable |
| C2a | tcp_connect | none | 18285.000 | 0.55 | 37331.000 | 6.17 | 284418.000 | 50.000 | 0.27 | -1112.000 | -2.89 | C0 | yes | stable |
| C2a | fork_exec_wait | Landlock fs | 740461.000 | 1.73 | 1017554.000 | 12.37 | 1074922.000 | 3889.000 | 0.53 | 42480.000 | 4.36 | C0 | no | stable |
| C2a | posix_spawn | Landlock fs | 347047.000 | 0.61 | 641132.000 | 17.52 | 702779.000 | 3217.000 | 0.94 | 22151.000 | 3.58 | C0 | no | stable |
| C2a | unix_dgram_pingpong | none | 6311.000 | 6.66 | 10400.000 | 4.53 | 27152.000 | -61.000 | -0.96 | -50.000 | -0.48 | C0 | yes | stable |
| C2b | timer_pair | none (no syscall) | 20.000 | 0.00 | 21.000 | 0.00 | 30.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C2b | getppid | none | 130.000 | 0.00 | 131.000 | 22.14 | 260.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C2b | getppid_x1000 | none | 120.618 | 0.92 | 149.242 | 15.27 | 175.181 | -0.010 | -0.01 | 8.166 | 5.79 | C0 | yes | stable |
| C2b | openat_tmpfs | none | 1763.000 | 1.19 | 2034.000 | 49.71 | 10670.000 | 0.000 | 0.00 | -30.000 | -1.45 | C0 | yes | stable |
| C2b | openat_rootfs | none | 2415.000 | 1.24 | 3025.000 | 37.45 | 12243.000 | 1.000 | 0.04 | 19.000 | 0.63 | C0 | yes | stable |
| C2b | unix_connect | none | 3426.000 | 0.61 | 5681.000 | 4.42 | 23054.000 | 9.000 | 0.26 | 91.000 | 1.63 | C0 | yes | stable |
| C2b | tcp_connect | Landlock net | 18445.000 | 1.79 | 38453.000 | 19.93 | 260433.000 | 210.000 | 1.15 | 10.000 | 0.03 | C0 | yes | stable |
| C2b | fork_exec_wait | none | 736242.000 | 1.16 | 992577.000 | 9.30 | 1111061.000 | -330.000 | -0.04 | 17503.000 | 1.80 | C0 | yes | stable |
| C2b | posix_spawn | none | 343830.000 | 0.68 | 651562.000 | 13.06 | 741932.000 | 0.000 | 0.00 | 32581.000 | 5.26 | C0 | yes | stable |
| C2b | unix_dgram_pingpong | none | 6322.000 | 6.52 | 10379.000 | 5.11 | 26881.000 | -50.000 | -0.78 | -71.000 | -0.68 | C0 | yes | stable |
| C3 | timer_pair | none (no syscall) | 20.000 | 0.00 | 21.000 | 0.00 | 30.000 | 0.000 | 0.00 | 0.000 | 0.00 | C0 | yes | stable |
| C3 | getppid | seccomp | 170.000 | 0.00 | 171.000 | 22.81 | 340.000 | 40.000 | 30.77 | 40.000 | 30.53 | C0 | no | stable |
| C3 | getppid_x1000 | seccomp | 160.323 | 0.49 | 182.435 | 8.25 | 238.161 | 39.695 | 32.91 | 41.359 | 29.32 | C0 | no | stable |
| C3 | openat_tmpfs | seccomp, Landlock fs | 2204.000 | 0.45 | 2685.000 | 7.45 | 11943.000 | 441.000 | 25.01 | 621.000 | 30.09 | C0 | no | stable |
| C3 | openat_rootfs | seccomp, Landlock fs | 2836.000 | 0.71 | 3586.000 | 4.46 | 13125.000 | 422.000 | 17.48 | 580.000 | 19.29 | C0 | no | stable |
| C3 | unix_connect | seccomp | 3497.000 | 0.60 | 5640.000 | 3.56 | 22212.000 | 80.000 | 2.34 | 50.000 | 0.89 | C0 | no | stable |
| C3 | tcp_connect | seccomp, Landlock net | 18495.000 | 0.87 | 38423.000 | 4.43 | 288706.000 | 260.000 | 1.43 | -20.000 | -0.05 | C0 | no | stable |
| C3 | fork_exec_wait | seccomp, Landlock fs | 742654.000 | 0.84 | 999500.000 | 4.40 | 1101874.000 | 6082.000 | 0.83 | 24426.000 | 2.51 | C0 | no | stable |
| C3 | posix_spawn | seccomp, Landlock fs | 348169.000 | 0.32 | 624751.000 | 15.33 | 715753.000 | 4339.000 | 1.26 | 5770.000 | 0.93 | C0 | no | stable |
| C3 | unix_dgram_pingpong | seccomp | 6392.000 | 5.33 | 10490.000 | 8.03 | 27011.000 | 20.000 | 0.31 | 40.000 | 0.38 | C0 | yes | stable |

Findings (VERIFIED, N and spread in the table):

- Timer floor: `timer_pair` is 20.000 ns at p50 with 0.00 percent spread in every config.
- Controls hold. C2a (Landlock fs) shows `within_noise` yes on `getppid`, `getppid_x1000`,
  `unix_connect`, `tcp_connect` and `unix_dgram_pingpong`. C2b shows `within_noise` yes on all ten.
- seccomp costs about the same for C1a (1 instruction) and C1b (64 instructions) on the null syscall:
  `getppid_x1000` adds 40.466 ns and 40.447 ns. C1c adds 54.744 ns. *(interpretation)* The
  match between C1a and C1b is what the action cache predicts: both skip BPF execution and pay only
  the filter entry path, while C1c runs the program. The run does not read the kernel's seccomp
  cache state, so this explanation is not verified here.
- Landlock fs adds a fixed cost per path lookup: C2a `openat_tmpfs` adds 371.000 ns at p50 and
  `openat_rootfs` 361.000 ns. C2a also adds 3889.000 ns at p50 to `fork_exec_wait` and 3217.000 ns to
  `posix_spawn`, both not within noise.
- Landlock TCP rules on `tcp_connect` (C2b) add 210.000 ns at p50 and are `within_noise`.
- `unix_dgram_pingpong` C0 spread_p50 is 6.61 percent, the loosest stable p50 row; the added
  columns for it are `within_noise` yes in every config.

## 5. Broker round trip (S4)

Source: summary.md, broker rows, copied unchanged. `direct` is an in-process table lookup, so the
added columns of the broker variants are the full cost of the socket hop. `broker_noid` is a
`SOCK_SEQPACKET` request and reply to a forked broker child on the same CPU with a lookup in a
2048-entry table. `broker_id` first fetches the caller identity with `SO_PEERCRED`, `SO_PEERPIDFD`
and `PIDFD_GET_INFO`, and keys the lookup on uid and cgroup id. `egress_direct` is socket, connect
and close to a local TCP listener; `egress_broker` has the broker perform the connect and pass the
fd back with `SCM_RIGHTS`. The row `broker_id_vs_noid` is `broker_id` against `broker_noid`.

| config | bench | med_p50_ns | spread_p50_pct | med_p99_ns | spread_p99_pct | med_p999_ns | added_p50_ns | added_p50_pct | added_p99_ns | added_p99_pct | baseline | within_noise | stability |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| direct | roundtrip | 20.000 | 0.00 | 30.000 | 30.00 | 40.000 | - | - | - | - | - | - | stable |
| broker_noid | roundtrip | 5801.000 | 6.40 | 9689.000 | 9.20 | 26560.000 | 5781.000 | 28905.00 | 9659.000 | 32196.67 | direct | no | stable |
| broker_id | roundtrip | 10009.000 | 0.50 | 19146.000 | 23.35 | 43583.000 | 9989.000 | 49945.00 | 19116.000 | 63720.00 | direct | no | stable |
| broker_id_vs_noid | roundtrip | 10009.000 | 0.50 | 19146.000 | 23.35 | 43583.000 | 4208.000 | 72.54 | 9457.000 | 97.61 | broker_noid | no | stable |
| egress_direct | roundtrip | 20740.000 | 0.97 | 42220.000 | 13.41 | 422178.000 | - | - | - | - | - | - | stable |
| egress_broker | roundtrip | 30759.000 | 1.50 | 62338.000 | 12.63 | 287604.000 | 10019.000 | 48.31 | 20118.000 | 47.65 | egress_direct | no | stable |

Identity support on this host (VERIFIED, selfcheck.txt and env.txt): `SO_PEERCRED` ok,
`SO_PEERPIDFD` ok, `PIDFD_GET_INFO` ok, and the pid and cgroup id returned by `PIDFD_GET_INFO` match
the caller in every round (`pid_match=yes`, `cgroupid_match=yes`).

Findings: `broker_noid` adds 5781.000 ns at p50 over `direct`. Identity adds 4208.000 ns at p50
(72.54 percent) on top of `broker_noid`. The p99 spread is 30.00 percent for `direct` (a 30 ns
quantile on a 10 ns quantum), 9.20 for `broker_noid`, 23.35 for `broker_id`, 13.41 for
`egress_direct` and 12.63 for `egress_broker`.

## 6. Deadline enforcement (S6 wall-time leg)

Source: summary.md, deadline rows, copied unchanged. A spinner child is spawned and a `timerfd` is
armed 10 ms after the spawn. At expiry the supervisor kills the child with `pidfd_send_signal`
(`pidfd_*`) or by writing 1 to the leaf's `cgroup.kill` (`cgkill_*`). `L0`, `L1`, `L4` are 0, 1
and 4 busy-loop competitors on the same CPU. `deadline_to_wakeup` is timer expiry to the supervisor
running, `wakeup_to_reaped` is the kill to `waitpid` return, `deadline_to_reaped` is the sum, and
`deadline_to_empty` (cgkill only) is expiry to `cgroup.events` reporting `populated 0`. This suite
runs on `CLOCK_MONOTONIC`.

| config | bench | med_p50_ns | spread_p50_pct | med_p99_ns | spread_p99_pct | med_p999_ns | added_p50_ns | added_p50_pct | added_p99_ns | added_p99_pct | baseline | within_noise | stability |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| pidfd_L0 | deadline_to_wakeup | 1880643.000 | 1.22 | 1902989.000 | 0.17 | 1922752.000 | - | - | - | - | - | - | stable |
| pidfd_L0 | wakeup_to_reaped | 54122.000 | 22.33 | 101573.000 | 69.62 | 127332.000 | - | - | - | - | - | - | unstable |
| pidfd_L0 | deadline_to_reaped | 1935621.000 | 0.53 | 1968708.000 | 0.97 | 1992093.000 | - | - | - | - | - | - | stable |
| pidfd_L1 | deadline_to_wakeup | 9517.000 | 8.88 | 16312.000 | 989.42 | 25447.000 | - | - | - | - | - | - | stable |
| pidfd_L1 | wakeup_to_reaped | 59363.000 | 2501.08 | 5933542.000 | 0.41 | 5942628.000 | - | - | - | - | - | - | noise-dominated |
| pidfd_L1 | deadline_to_reaped | 70663.000 | 2102.53 | 5943192.000 | 0.53 | 5952396.000 | - | - | - | - | - | - | noise-dominated |
| pidfd_L4 | deadline_to_wakeup | 9733.000 | 12.98 | 28132.000 | 6619.94 | 1891531.000 | - | - | - | - | - | - | unstable |
| pidfd_L4 | wakeup_to_reaped | 9928335.000 | 40.24 | 17938385.000 | 0.05 | 17950848.000 | - | - | - | - | - | - | noise-dominated |
| pidfd_L4 | deadline_to_reaped | 9937684.000 | 40.20 | 17948443.000 | 0.07 | 17960495.000 | - | - | - | - | - | - | noise-dominated |
| cgkill_L0 | deadline_to_wakeup | 31694.000 | 17.65 | 89178.000 | 54.55 | 179874.000 | - | - | - | - | - | - | unstable |
| cgkill_L0 | wakeup_to_reaped | 6453.000 | 80.12 | 15780.000 | 32.88 | 21851.000 | - | - | - | - | - | - | noise-dominated |
| cgkill_L0 | deadline_to_reaped | 38211.000 | 28.57 | 97895.000 | 52.43 | 191937.000 | - | - | - | - | - | - | noise-dominated |
| cgkill_L0 | deadline_to_empty | 40542.000 | 29.56 | 101592.000 | 52.73 | 195574.000 | - | - | - | - | - | - | noise-dominated |
| cgkill_L1 | deadline_to_wakeup | 9503.000 | 2.98 | 2531366.000 | 35.87 | 3525553.000 | - | - | - | - | - | - | stable |
| cgkill_L1 | wakeup_to_reaped | 6161.000 | 4.07 | 13456.000 | 50.33 | 16792.000 | - | - | - | - | - | - | stable |
| cgkill_L1 | deadline_to_reaped | 15788.000 | 2.84 | 2539662.000 | 35.76 | 3532035.000 | - | - | - | - | - | - | stable |
| cgkill_L1 | deadline_to_empty | 17331.000 | 7.72 | 2541205.000 | 35.78 | 3533628.000 | - | - | - | - | - | - | stable |
| cgkill_L4 | deadline_to_wakeup | 17741.000 | 37.37 | 3863592.000 | 0.21 | 3872379.000 | - | - | - | - | - | - | noise-dominated |
| cgkill_L4 | wakeup_to_reaped | 6142.000 | 34.26 | 12844.000 | 51.48 | 22343.000 | - | - | - | - | - | - | noise-dominated |
| cgkill_L4 | deadline_to_reaped | 32612.000 | 49.54 | 3870886.000 | 0.18 | 3882257.000 | - | - | - | - | - | - | noise-dominated |
| cgkill_L4 | deadline_to_empty | 36890.000 | 51.08 | 3873481.000 | 0.17 | 3883840.000 | - | - | - | - | - | - | noise-dominated |

Findings (VERIFIED):

- At L0 with `pidfd`, the supervisor's wake-up takes 1880643.000 ns at p50 (spread 1.22 percent)
  and dominates `deadline_to_reaped` (1935621.000 ns). With one competitor, `deadline_to_wakeup`
  falls to 9517.000 ns. The cause of that inversion is not established by this run.
- At L4 the wait moves. With `pidfd`, `wakeup_to_reaped` is 9928335.000 ns at p50; with `cgroup.kill`,
  `wakeup_to_reaped` is 6142.000 ns and `deadline_to_wakeup` carries the delay at p90
  (3851008.000 ns).
- Of the 21 rows, 7 are stable, 3 are unstable (`pidfd_L0 wakeup_to_reaped`, `pidfd_L4
  deadline_to_wakeup`, `cgkill_L0 deadline_to_wakeup`) and 11 are noise-dominated, including all
  four `cgkill_L4` rows and both `pidfd_L4` rows that end in `reaped`. Read the p50 and p99 columns
  as point values from a noisy host, not as thresholds.

## 7. cgroup probe and legs

Sources: [cgroup-probe.txt](../../../tools/linux-baseline/results/20261028T034700Z/cgroup-probe.txt),
[cgroup-behaviour.txt](../../../tools/linux-baseline/results/20261028T034700Z/cgroup-behaviour.txt)
and summary.md.

Probe results (VERIFIED):

- The shell's own cgroup is `/init.scope` and is not writable (`own_cgroup_writable=no`).
- Moving a process directly into a uid-owned directory under `user.slice` fails with Permission
  denied (`direct_move`), as the cgroup v2 common-ancestor rule predicts.
- `systemd-run --user --scope` and `systemd-run --user` as a service both succeed. The service
  cgroup is owned by uid 1000, exposes controllers `cpu memory pids`, and is writable
  (`service_writable=yes`). Verdict `delegation=yes`.
- `unshare -Ucr --cgroup` yields a cgroup root that is not writable (`unshare_cgroupns`).
- No leg is NOT MEASURED: the probe reached a delegated subtree, so all four legs below ran inside
  a `systemd-run --user` service.

Spawn comparison (benchmark of fork, exec of `/bin/true`, wait; `spawn_fork` is the baseline):

| config | bench | med_p50_ns | spread_p50_pct | med_p99_ns | spread_p99_pct | med_p999_ns | added_p50_ns | added_p50_pct | added_p99_ns | added_p99_pct | baseline | within_noise | stability |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| spawn_fork | spawn_exec_wait | 392152.000 | 1.28 | 771238.000 | 3.82 | 928486.000 | - | - | - | - | - | - | stable |
| spawn_clone3_into | spawn_exec_wait | 391470.000 | 1.81 | 784955.000 | 3.51 | 866689.000 | -682.000 | -0.17 | 13717.000 | 1.78 | spawn_fork | yes | stable |
| spawn_fork_move | spawn_exec_wait | 402261.000 | 0.60 | 782379.000 | 3.56 | 853865.000 | 10109.000 | 2.58 | 11141.000 | 1.44 | spawn_fork | no | stable |

`clone3` with `CLONE_INTO_CGROUP` adds -682.000 ns at p50 over a plain fork and is within noise.
Forking and then moving the child by writing `cgroup.procs` adds 10109.000 ns at p50 (2.58 percent)
and is not within noise.

Behaviour legs (VERIFIED, 3 trials each for OOM and pids, 1 for the functional check):

| Leg | Result |
|---|---|
| clone3_into_cgroup_functional | yes |
| oom_trial 1, 2, 3 (memory.max 67108864, swap 0) | signal=9, oom=1, oom_kill=1, peak_bytes=67108864, chunks_mib=63 in each |
| pids_trial 1, 2, 3 (pids.max 8) | forks_ok=7, errno=EAGAIN, pids_events_max=1 in each |
| psi_trigger_register | ok |

Each OOM and pids trial ran only after `memory.max`, `memory.swap.max` or `pids.max` read back the
requested value.

## 8. Not measured

Four items are NOT MEASURED. Each needs a resource this run did not have. No number for any of them
appears anywhere in this report.

### BPF-LSM per-hook overhead

NOT MEASURED. Needs: root on a test host, a kernel with `bpf` in the active LSM list, `bpftool`,
clang with the BPF target, and `libbpf` headers.

VERIFIED host facts: `active_lsms=capability,landlock,yama,safesetid,selinux,ima` (no `bpf`);
`config_lsm=CONFIG_LSM="landlock,lockdown,yama,loadpin,safesetid,integrity,selinux,apparmor,tomoyo"`;
`CONFIG_BPF_LSM=y`; `unprivileged_bpf_disabled=2`; the kernel command line carries no `lsm=`.
The kernel supports BPF-LSM, the boot list does not enable it, and an unprivileged user cannot load
programs.

### BPF_LSM_CGROUP

NOT MEASURED. Needs the same as BPF-LSM plus a delegated cgroup to attach to; the probe found one.
The `lsm_cgroup` attach type through `bpftool` is ASSUMED, not checked on this host: `bpftool` is
absent here. The convention that an `lsm_cgroup` program returns 1 to allow is also ASSUMED.

### Commands for both BPF items

Not executed on this host. Run as root on a test host.

```sh
# 1. Is bpf in the active LSM list? It must be.
cat /sys/kernel/security/lsm
# If absent, append ,bpf to the CONFIG_LSM list as an lsm= argument on the kernel command line and
# reboot. On WSL2 the route is kernelCommandLine in .wslconfig, then wsl --shutdown from Windows.
# That route is ASSUMED.

# 2. Kernel type header.
bpftool btf dump file /sys/kernel/btf/vmlinux format c > vmlinux.h

# 3. Build the two objects (sources below).
clang -O2 -g -target bpf -c bpflsm.bpf.c -o bpflsm.bpf.o
clang -O2 -g -target bpf -c bpflsm_cg.bpf.c -o bpflsm_cg.bpf.o

# 4. Load and attach the lsm/ programs. Links are pinned under the directory.
bpftool prog loadall bpflsm.bpf.o /sys/fs/bpf/aok-bpflsm autoattach

# 5. BPF_LSM_CGROUP: load, then attach to a cgroup. ASSUMED, not checked here.
bpftool prog load bpflsm_cg.bpf.o /sys/fs/bpf/aok-bpflsm-cg
bpftool cgroup attach /sys/fs/cgroup/<leaf> lsm_cgroup pinned /sys/fs/bpf/aok-bpflsm-cg

# 6. Measure. The harness itself runs unprivileged; the hooks apply to every task.
tools/linux-baseline/run.sh --run-id bpflsm-<id> --suites micro --configs C0 --tag bpflsm
tools/linux-baseline/run.sh --run-id bpflsm-<id> --suites micro --configs C3 --tag bpflsm

# 7. Unload.
bpftool cgroup detach /sys/fs/cgroup/<leaf> lsm_cgroup pinned /sys/fs/bpf/aok-bpflsm-cg
rm /sys/fs/bpf/aok-bpflsm-cg
rm /sys/fs/bpf/aok-bpflsm/*
rmdir /sys/fs/bpf/aok-bpflsm
```

The program, `bpflsm.bpf.c`. Each hook does one hash-map lookup keyed on the current cgroup id and
returns 0 (allow). The map stays empty, so the lookups measure the miss path; a hit variant needs
the map populated through `bpftool map update`.

```c
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

char LICENSE[] SEC("license") = "GPL";

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 1024);
	__type(key, __u64);
	__type(value, __u32);
} cg_policy SEC(".maps");

static __always_inline int check(void)
{
	__u64 id = bpf_get_current_cgroup_id();
	__u32 *v = bpf_map_lookup_elem(&cg_policy, &id);

	(void)v;
	return 0;
}

SEC("lsm/file_open")
int BPF_PROG(h_file_open, struct file *file)
{
	return check();
}

SEC("lsm/socket_connect")
int BPF_PROG(h_socket_connect, struct socket *sock, struct sockaddr *address, int addrlen)
{
	return check();
}

SEC("lsm/bprm_check_security")
int BPF_PROG(h_bprm_check, struct linux_binprm *bprm)
{
	return check();
}
```

The cgroup variant, `bpflsm_cg.bpf.c`, repeats the map and `check()` with the section names
`lsm_cgroup/file_open`, `lsm_cgroup/socket_connect` and `lsm_cgroup/bprm_check_security`, and each
program returns 1 to allow (ASSUMED convention).

### S7 inference queue

NOT MEASURED. No inference backend and no broker queue exist, so no command can be given yet. It
needs no privilege. S7 compares a queue the project has not built against a backend it has not
chosen.

### S8 PSI trigger to freeze

NOT MEASURED. Needs a delegated cgroup with the memory controller and a working PSI trigger
registration. The probe found both (`service_controllers=cpu memory pids`,
`psi_trigger_register=ok`). The freeze step needs a write to `cgroup.freeze` on the leaf, which the
probe did not exercise (ASSUMED to work). Measuring trigger-to-freeze latency is a separate task
*(interpretation)*, because it needs a memory-pressure generator and an observer, neither of which
this harness has.

No cgroup leg was NOT MEASURED: the probe reached a delegated subtree and all legs ran (section 7).

## 9. Verified and assumed

**VERIFIED** (measured in run 20261028T034700Z, five rounds, one host):

- C0 micro p50 spread is under 10 percent on all ten benches (largest: `openat_tmpfs` 9.64 percent,
  n_total 1000000).
- C0 `getppid` med_p50 130.000 ns (n_total 5000000, spread 0.00 percent); C0 `getppid_x1000`
  120.628 ns (n_total 10000, spread 0.96 percent); C0 `openat_tmpfs` 1763.000 ns (spread 9.64
  percent).
- C3 adds 25.01 percent at p50 on `openat_tmpfs` (spread 0.45 percent) and 17.48 percent on
  `openat_rootfs` (spread 0.71 percent); the C0 p50 spreads of those rows are 9.64 and 0.46 percent.
- C3 adds under 2.5 percent at p50 on `unix_connect`, `tcp_connect`, `fork_exec_wait`, `posix_spawn`
  and `unix_dgram_pingpong`.
- `broker_id` adds 19116.000 ns at p99 over `direct` (n_total 500000, p99 spread 23.35 percent);
  `egress_broker` adds 20118.000 ns at p99 over `egress_direct` (n_total 100000, p99 spread 12.63
  percent).
- `SO_PEERPIDFD`, `PIDFD_GET_INFO` and `SO_PEERCRED` work, and the identity they return matches the
  caller.
- `pidfd_L4` and `cgkill_L4` `deadline_to_reaped` p99 are 17948443.000 ns and 3870886.000 ns
  (n_total 1000, p50 spreads 40.20 and 49.54 percent, noise-dominated).
- `CLONE_INTO_CGROUP` works; a 64 MiB `memory.max` produced an OOM kill in 3 of 3 trials;
  `pids.max=8` refused the eighth fork with EAGAIN in 3 of 3 trials; PSI trigger registration
  succeeds.
- The active LSM list excludes `bpf`.

**ASSUMED:**

- The plausibility windows: C0 `getppid` p50 between 60 and 400 ns, C0 `openat_tmpfs` p50 between
  800 and 5000 ns. They were set from a planner probe and are not published figures.
- The WSL2 `kernelCommandLine` route to enable `bpf` in the LSM list.
- `bpftool` support for the `lsm_cgroup` attach type, and that an `lsm_cgroup` program returns 1 to
  allow.
- The seccomp action cache explanation for C1a against C1c (section 4); the cache state was not read.
- That these numbers transfer to bare-metal kernels on the five target distros. A WSL2 guest adds
  hypervisor scheduling; no number here is a prediction for bare metal.
- That `cgroup.freeze` works on the delegated leaf.
- That the single run is representative of other times on this host. Five rounds measure
  round-to-round spread inside one run, not day-to-day drift.

## 10. Caveats

- Hyper-V schedules guest vCPUs freely, so `taskset` pins a vCPU, not a physical core. Pinned CPU 2
  has sibling 3 in the guest topology; the host places vCPUs freely.
- SMT sibling activity on the host is outside the guest's view and not controlled.
- The 10 ns timer quantum bounds single-call resolution. `getppid` reads 130.000 ns in steps of 10;
  `getppid_x1000` exists to resolve below that.
- The seccomp action cache (A1) means C1a and C1b can understate the cost of a filter that depends
  on arguments or a longer path. C1c is the measure of filter execution.
- Same-CPU IPC: the ping-pong and the broker round trip on one CPU cost two context switches and no
  cross-CPU wake-up. A broker on another CPU is not measured and would show a different profile.
- Closing TCP clients with RST (A3) skips TIME_WAIT. `tcp_connect` therefore measures connect and
  the RST close, not the connection lifecycle a real client has.
- The ping-pong peer in `unix_dgram_pingpong` is unrestricted; only the measured side carries the
  config. The peer's own cost is the same in every config.
- The installed uapi headers are 6.8 and the kernel is 6.18. The harness takes the Landlock, pidfd
  and clone constants from upstream v6.18 under `AOK_` names, and confirms the layouts at runtime
  through the self-checks and the `PIDFD_GET_INFO` probe.
- The guest has no cpufreq control. Frequency scaling happens on the host and is not held fixed.

## 11. Proposed revisions to S3, S4 and S6

Rules *(interpretation)*; the user may keep the current values instead.

- **Eligibility.** A revised value derives only from a row that summary.md marks stable and whose
  baseline and config spreads are both under 10 percent, tested on the spread of the quantile the
  bound constrains (spread_p50 for a p50 bound, spread_p99 for a p99 bound). Every other row reads
  "none: unstable on this host".
- **R1 (S3, relative).** Revised bound = max(current bound, C3 added percent + 2 x the larger of the
  two spreads), rounded up to a whole percent, for p50 and p99 separately. The current bound is kept
  when the measurement leaves headroom.
- **R2 (S4 and S6, absolute).** Revised bound = the smallest value in the 1-2-5 series (10 us, 20 us,
  50 us, 100 us, ... 1 ms, 2 ms, ...) at or above 4 x the measured p99 (added, for S4). The bound is
  never set above the current proposal unless the measured p99 already exceeds it; then the current
  proposal fails on this host.

C3 here is seccomp (C1b), Landlock fs, TCP connect and scopes, without BPF-LSM. BPF-LSM is not
measured, so the BPF-LSM share of every S3 bound stays ungrounded.

Arithmetic for R1 (inputs copied from summary.md; sum then rounded up). Example, `openat_tmpfs` p50:
25.01 + 2 x 9.64 = 44.29, rounded up to 45.

| ID | Leg | Current proposed threshold | Measured here | Rule | Revised proposal | Status |
|---|---|---|---|---|---|---|
| S3 | `openat_tmpfs`, C3 vs C0 | p50 at most 10%, p99 at most 25% | p50 added 25.01%, p99 added 30.09%; N 1000000; spread_p50 9.64 / 0.45, spread_p99 12.11 / 7.45. The current proposal fails at both quantiles on this host. | R1 | p50: 45% (25.01 + 2 x 9.64 = 44.29). p99: none: unstable on this host | proposed, awaiting user acceptance |
| S3 | `openat_rootfs`, C3 vs C0 | p50 at most 10%, p99 at most 25% | p50 added 17.48%, p99 added 19.29%; N 1000000; spread_p50 0.46 / 0.71, spread_p99 14.37 / 4.46. The current p50 bound fails. | R1 | p50: 19% (17.48 + 2 x 0.71 = 18.90). p99: none: unstable on this host | proposed, awaiting user acceptance |
| S3 | `unix_connect`, C3 vs C0 | p50 at most 10%, p99 at most 25% | p50 added 2.34%, p99 added 0.89%; N 250000; spread_p50 0.91 / 0.60, spread_p99 4.49 / 3.56 | R1 | p50: keep 10% (2.34 + 2 x 0.91 = 4.16). p99: keep 25% (0.89 + 2 x 4.49 = 9.87) | proposed, awaiting user acceptance |
| S3 | `tcp_connect`, C3 vs C0 | p50 at most 10%, p99 at most 25% | p50 added 1.43%, p99 added -0.05%; N 100000; spread_p50 1.27 / 0.87, spread_p99 2.55 / 4.43 | R1 | p50: keep 10% (1.43 + 2 x 1.27 = 3.97). p99: keep 25% (-0.05 + 2 x 4.43 = 8.81) | proposed, awaiting user acceptance |
| S3 | `fork_exec_wait` (execve path), C3 vs C0 | p50 at most 10%, p99 at most 25% | p50 added 0.83%, p99 added 2.51%; N 5000; spread_p50 1.30 / 0.84, spread_p99 3.60 / 4.40 | R1 | p50: keep 10% (0.83 + 2 x 1.30 = 3.43). p99: keep 25% (2.51 + 2 x 4.40 = 11.31) | proposed, awaiting user acceptance |
| S3 | `posix_spawn`, C3 vs C0 | p50 at most 10%, p99 at most 25% | p50 added 1.26%, p99 added 0.93%; N 5000; spread_p50 1.02 / 0.32, spread_p99 7.35 / 15.33 | R1 | p50: keep 10% (1.26 + 2 x 1.02 = 3.30). p99: none: unstable on this host | proposed, awaiting user acceptance |
| S3 | `unix_dgram_pingpong` (sendmsg), C3 vs C0 | p50 at most 10%, p99 at most 25% | p50 added 0.31%, p99 added 0.38%; N 1000000; spread_p50 6.61 / 5.33, spread_p99 5.27 / 8.03 | R1 | p50: 14% (0.31 + 2 x 6.61 = 13.53). p99: keep 25% (0.38 + 2 x 8.03 = 16.44) | proposed, awaiting user acceptance |
| S4 | Tool invocation, `broker_id` vs `direct` | p99 at most 1 ms added | p99 added 19116.000 ns; N 500000; spread_p99 30.00 / 23.35 | R2 | none: unstable on this host | proposed, awaiting user acceptance |
| S4 | Egress, `egress_broker` vs `egress_direct` | p99 at most 1 ms added | p99 added 20118.000 ns; N 100000; spread_p99 13.41 / 12.63 | R2 | none: unstable on this host | proposed, awaiting user acceptance |
| S6 | Wall time via `pidfd_send_signal`, L4 | Agent cgroup killed within 1 s of its deadline | `deadline_to_reaped` p99 17948443.000 ns; N 1000; spread_p50 40.20; noise-dominated. The current proposal is not exceeded. | R2 | none: unstable on this host | proposed, awaiting user acceptance |
| S6 | Wall time via `cgroup.kill`, L4 | Agent cgroup killed within 1 s of its deadline | `deadline_to_reaped` p99 3870886.000 ns; N 1000; spread_p50 49.54; noise-dominated. The current proposal is not exceeded. | R2 | none: unstable on this host | proposed, awaiting user acceptance |
| S6 | Tokens and money | At most one in-flight request per agent | NOT MEASURED | none | none: threshold unchanged | proposed, awaiting user acceptance |

Reading the table *(interpretation)*: the one S3 finding that matters is `openat`. A relative
bound on a call that costs about 1.7 us at baseline is dominated by a fixed per-call cost of a few
hundred ns, so 10 percent cannot hold with Landlock fs and seccomp alone, before BPF-LSM adds its
share. An absolute added-ns bound for `openat` would not move with the baseline call cost; that is
an alternative the user can choose and this report does not propose it. No S4 or S6 revision is
derived. Both proposed bounds (1 ms, 1 s) are far above every measured p99, so the proposals are
not contradicted, but the host is too noisy to ground a tighter value.

## Sources

- Run directory `20261028T034700Z`:
  [summary.md](../../../tools/linux-baseline/results/20261028T034700Z/summary.md),
  [summary.csv](../../../tools/linux-baseline/results/20261028T034700Z/summary.csv),
  [env.txt](../../../tools/linux-baseline/results/20261028T034700Z/env.txt),
  [selfcheck.txt](../../../tools/linux-baseline/results/20261028T034700Z/selfcheck.txt),
  [cgroup-probe.txt](../../../tools/linux-baseline/results/20261028T034700Z/cgroup-probe.txt),
  [cgroup-behaviour.txt](../../../tools/linux-baseline/results/20261028T034700Z/cgroup-behaviour.txt),
  [run.log](../../../tools/linux-baseline/results/20261028T034700Z/run.log).
- [scoping-linux.md](../scoping-linux.md), sections 3 and 4.
- [ADR 0004](../adr/0004-linux-prototype-first.md).
- Upstream Linux v6.18 `include/uapi/linux/landlock.h` and `include/uapi/linux/pidfd.h`, the source of
  the `AOK_` constants in `tools/linux-baseline/uapi_compat.h`.
