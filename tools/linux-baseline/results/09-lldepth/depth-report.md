# Landlock depth reads (09-05, D-16)

indicative, WSL2. This phase gives no S3 verdict (D-16/D-18). The numbers below are what
`summarize.py` and `r1.py` print for this run directory; none of them is compared with the
proposed S3 bound here.

Host: WSL2, kernel 6.18, 12 CPUs, micro pinned to CPU 2 with taskset, harness_dirty=no
(`env.txt`, harness_rev 744d3f6). Five rounds per label.

## Wrap

Each wrapped label ran `run.sh --run-id 09-lldepth --suites micro --configs C0 --rounds R-R --tag
lldepth-N --wrap W`, one invocation per round, each under its own `systemd-run --user --scope -q -p
Delegate=yes`. W for depth N, with REPO = `/home/rakdos/repos/agentic-os-kernel`:

```
REPO/tools/aok-launch/target/release/aok-launch nest --name lldepth-N --depth N --grant rx:REPO/tools/linux-baseline/bin --grant rw:/dev/shm --grant rw:REPO/tools/linux-baseline/.work --grant rw:REPO/tools/linux-baseline/results/09-lldepth --
```

`run.log` carries 15 `aok-launch: role=root` lines (one per wrapped invocation) and 20
`aok-launch: role=sub` lines (5 at depth 2, 15 at depth 4, none at depth 1), so the layers
stacked. `selfcheck.txt` reports `Seccomp_filters` 1, 2 and 4 for lldepth-1, -2 and -4. The launcher
installs one seccomp filter per layer.

Each layer in this wrap is the launcher's whole containment (Landlock ruleset plus the seccomp
allow-list), not Landlock alone. The wrapped-minus-C0 difference therefore includes the seccomp
filter cost. `getppid` shows +40 ns at every depth, which is seccomp and not the fs ruleset.

## added_p50 against C0

Source: `summary.csv` (`added_p50_ns`, `added_p50_pct`), baseline C0 unwrapped in the same run dir.

| depth | bench | C0 med_p50 ns | wrapped med_p50 ns | added_p50 ns | added_p50 % | r1.py eligibility (p50) |
|---|---|---|---|---|---|---|
| lldepth-1 | openat_rootfs | 2245 | 2725 | 480 | 21.38 | eligible (R1 39, sum 38.30) |
| lldepth-1 | openat_tmpfs | 1754 | 2245 | 491 | 27.99 | ineligible: none: unstable on this host |
| lldepth-2 | openat_rootfs | 2245 | 2766 | 521 | 23.21 | eligible (R1 41, sum 40.13) |
| lldepth-2 | openat_tmpfs | 1754 | 2285 | 531 | 30.27 | ineligible: none: unstable on this host |
| lldepth-4 | openat_rootfs | 2245 | 2845 | 600 | 26.73 | eligible (R1 44, sum 43.65) |
| lldepth-4 | openat_tmpfs | 1754 | 2365 | 611 | 34.83 | ineligible: none: unstable on this host |

Why openat_tmpfs is ineligible at every depth: the unwrapped C0 openat_tmpfs spread_p50 is 10.21
percent in this run, and `r1.py` requires both spreads strictly below 10 percent. Nothing was tuned
to change that. The openat_tmpfs added_p50 values are printed for completeness only. The p99
openat legs: openat_tmpfs p99 and openat_rootfs p99 read "none: unstable on this host" at all three
depths.

`r1.py` output for the three configs is the `--config C0-lldepth-N --against C0` invocation named
in the plan; it is informational under D-18.

## Wrapped-C0 check

The sandbox.c C0 self-check itself passed at every depth: `micro` got past `sb_check` and ran its
benches (it returns 2 before any bench when `sb_check` fails). `selfcheck.txt` nevertheless records
`result=FAIL errors=22000` for all 15 wrapped rounds and `result=ok errors=0` for the 5 unwrapped
C0 rounds. The 22000 errors are all the `tcp_connect` bench: every connect is refused.

## Finding: tcp_connect cannot run under the launcher (flagged)

The launcher's Landlock ruleset handles `ConnectTcp` (09-03). `micro` binds its loopback TCP
listener to an ephemeral port chosen at run time, and the grant syntax is `tcp:PORT`, so no static
grant covers it. A probe under `aok-launch run` (python, bind to port 0, connect to it) returns
EACCES; the same probe outside the launcher connects. Consequences:

- `micro` exits 2 on every wrapped round, so `run.sh` exits 1 after the round's rows are written
  and skips its `suite_end`. The per-round invocations are the workaround (rows land in
  `micro.csv` before `run.sh` aborts); `summarize.py` was then run once by hand on the run dir,
  which is what `suite_end` would have run.
- Every `tcp_connect` row for lldepth-1/2/4 measures a refused connect, not a connect
  (med_p50 about 2400 ns against 18235 ns unwrapped, added_p50 about -86 percent). Those rows are
  not a measurement of Landlock cost and must not be read as one.
- The openat, getppid, unix_connect, fork_exec_wait, posix_spawn and unix_dgram_pingpong rows have
  errors=0 in every wrapped round.

No grant was widened beyond the plan's list, `micro.c` and `sandbox.c` were not edited, and the
default grant in `src/landlock.rs` was not touched.

This phase gives no S3 verdict (D-16/D-18).
