# IPC measurements (IPC-06)

Report only. Phase 1 sets no ceiling on the null-IPC round trip; this file records what the
first run measured so later phases have a number to move against.

## Provisional status

The figures below were taken under **QEMU TCG (no KVM; software emulation), provisional:
re-measure on a KVM-enabled host.** The cycle counts come from a TSC read inside an emulated
guest. They are NOT a hardware baseline and must not be compared against bare-metal or KVM
numbers for other kernels.

Earlier docs say the tests run in "QEMU/KVM" (for example `docs/architecture/b2-spike-report.md`
line 14, and the FPU evidence in `unsafe-ledger.md`). That claim is unverified: this host has no
`/dev/kvm` (WSL2), `kernel/OSDK.toml` passes no `-accel` or `-enable-kvm` flag, and OSDK never
enabled KVM on its own. Those documents are not edited here.

## Setup

| Field | Value |
|-------|-------|
| Date | 2026-10-26 |
| Base commit | `df379e2` (Task 1 of plan 01-07); the `null_ipc_round_trip_is_recorded` ktest and the `nullsrv` and `nullipc` bins are added by the Task 2 commit on top |
| Host CPU | AMD Ryzen 9 7900X3D 12-Core Processor (`/proc/cpuinfo` `model name`), WSL2 guest |
| Accelerator | QEMU TCG (no KVM; software emulation), provisional: re-measure on a KVM-enabled host |
| QEMU flags | `-machine q35,kernel-irqchip=split`, `-cpu Icelake-Server,+x2apic`, `-smp 1`, `-m 2G`, no accelerator flag (from `kernel/OSDK.toml`) |
| Runner | `cd kernel && ./osdk.sh sh -c '../userland/build.sh && cargo osdk test'` |
| Guest TSC | 4,426,760,800 Hz, as reported by `ostd::arch::tsc_freq()` (printed by the ktest) |

## Method

- Two ring-3 programs on two threads of separate root agents: `nullsrv` (`userland/tests/src/bin/nullsrv.rs`)
  and `nullipc` (`userland/tests/src/bin/nullipc.rs`). A kernel thread receives the one report.
- `nullsrv` does one `recv`, then `reply_recv` in a loop with zero words, no data and no
  capabilities.
- `nullipc` makes 1,000 warm-up `call`s, then 10,000 timed `call`s. Each is bracketed by
  `libsys::tsc()` (`rdtsc`, ledger row U-05) and the delta is stored. The deltas are sorted with
  `sort_unstable`.
- Statistics: N = 10,000; median is sorted element 5,000; p99 is sorted element 9,900; max is
  element 9,999.
- `rdtsc` is not serialised against neighbouring instructions, so each sample carries a small
  skew. At the magnitudes below it is negligible next to the measurement noise.
- The ktest asserts only `n == 10_000` and `median > 0`. It prints the numbers; nothing in the
  suite fails because the round trip is slow.

## Result

Copied from the saved ktest log of this run:

```
aok: null-ipc n=10000 median=5429008 p99=7537376 max=19566536 cycles
aok: null-ipc tsc_freq=4426760800 Hz
```

| Statistic | Cycles | Time at the reported TSC frequency |
|-----------|--------|------------------------------------|
| N | 10,000 | |
| median (element 5,000) | 5,429,008 | about 1.23 ms |
| p99 (element 9,900) | 7,537,376 | about 1.70 ms |
| max | 19,566,536 | about 4.42 ms |

The time column is cycles divided by the reported frequency. Under TCG the TSC does not
correspond to executed guest work in any fixed way, so treat it as a label for the unit, not a
latency a KVM or bare-metal run would reproduce.

## B2 comparison

None. `b2-spike-report.md` records no IPC timing figure, so there is nothing to compare against.

## Reading the number

*(interpretation)* A median of 5.4 million cycles per null round trip is far above what a
capability microkernel should cost on hardware, and the p99 and median are within about 40
percent of each other, which suggests a roughly constant cost per call rather than rare stalls.
Two candidates, neither tested: software emulation of the ring transitions and context switches
(TCG), and the blocked client waiting for a scheduler tick before the server runs instead of a
direct handoff. The second would show up on KVM too. Re-measuring on a KVM host separates them.
