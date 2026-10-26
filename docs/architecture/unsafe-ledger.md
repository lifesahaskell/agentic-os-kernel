# Unsafe ledger (SAFE-01)

Rule: `#![deny(unsafe_code)]` stays on every crate root. A file may carry `unsafe` only if it is
listed below, every `unsafe` token has a `// SAFETY: <ID>` comment within the 3 lines before it,
and every row here has a matching `// SAFETY: <ID>` comment in its file. `tools/unsafe_check.py`
enforces this over `kernel/src` and `userland/*/src`; `scripts/check.sh` runs it first.

Recorded signal (ADR 0002): needing an OSTD patch, or an `unsafe` need that cannot be contained
in a small named module, is the trigger to re-score the Redox fork.

## Kernel

| ID | Location | Why no safe route | Invariant relied on | How to retire |
|----|----------|-------------------|---------------------|---------------|

## Userland

| ID | Location | Why no safe route | Invariant relied on | How to retire |
|----|----------|-------------------|---------------------|---------------|
| U-01 | `userland/libsys/src/raw.rs` `syscall6` | The `syscall` instruction has no safe wrapper; this is the only ring-3 trap into the kernel. | `syscall` clobbers only rax, rcx and r11, all declared. Memory the kernel writes is reachable through pointer arguments, so the default memory clobber stays. | Never: it is the ABI boundary. Keep it one block. Revisit in a hardening pass. |
| U-02 | `userland/libsys/src/rt.rs` `entry!` (`_start`, `_aok_handler`) | Process entry and the redirect handler run before any Rust runtime exists, so they must be `global_asm!`. | `_start` runs once with rsp at the kernel-mapped stack top and rdi = arg, aligns rsp, never returns. `_aok_handler` only issues exit with the registers the kernel set. The two-argument form stores xmm0-xmm15 into `HANDLER_XMM` before any Rust code runs, using rax as the only scratch, keeps rdi (the interrupted pc) until the handler shim call, and aligns rsp before `call`. | When the runtime gains a signal trampoline in a later phase, move the stubs into libaok. Revisit in a hardening pass. |
| U-03 | `userland/libsys/src/fpu.rs` `hold` | The FPU test needs xmm0-xmm15 loaded and compared with no compiler-generated code between, which only inline asm can promise. | Every xmm register, rax, rcx and r11 are declared clobbered (`syscall` takes rcx, r11 and rax); the pattern and result arrays outlive the block. | Test-only code; delete if the FPU gate moves to a native test harness. Revisit in a hardening pass. |
| U-04 | `userland/libsys/src/lib.rs` `testpage::test_page` | A fixed-address `&'static [AtomicU32]` cannot be built without `slice::from_raw_parts` on a raw address. | `abi::TEST_DATA` is page-aligned and one page long, so 1024 `AtomicU32` are aligned and in bounds; every access is an atomic operation. The slice is dereferenceable only once the harness has mapped the page; a program that touches it earlier takes a hardware page fault, which the pager tests rely on. | Test-only code; delete when the test harness passes the page as a bin argument or the programs get a real shared-memory capability. Revisit in a hardening pass. |
| U-05 | `userland/libsys/src/lib.rs` `clock::tsc` | `core::arch::x86_64::_rdtsc` is an `unsafe fn` on this nightly, and there is no safe clock syscall yet. | `rdtsc` reads a counter with no memory or register side effects beyond its output. CR4.TSD is clear in ring 3 on this kernel (OSTD never sets it). The read is not serialised, so a single sample carries a few tens of cycles of skew. | Replace with a clock syscall or a vDSO-style read when userland gets a time service. Revisit in a hardening pass. |
| U-06 | `userland/libsys/src/lib.rs` `trap::ud2` | The invalid-opcode exception test needs a guaranteed `ud2`, which only inline asm can emit. | `ud2` raises #UD and never returns; it reads and writes no memory and touches no register the compiler relies on. | Test-only code; delete if the exception tests move to a native harness. Revisit in a hardening pass. |
| U-07 | `userland/libsys/src/regs.rs` `regs_probe!` (`_start`, `_aok_handler`) and `check` | The redirect probe must load, scramble and compare every general register and xmm0-xmm15 with no compiler-generated code in between, and read the stack canary below `rsp`: register-exact, so only `global_asm!` can promise it. | The handler touches no memory except the probe's own statics and uses no stack, because the kernel put `rsp` below the interrupted red zone; `_start` fills [rsp-4096, rsp) with a canary before the spin and moves `rsp` 16 KiB below it before calling Rust, so the `check` slice read is of mapped, 8-aligned memory that nothing else writes. | Test-only code; delete if the gate moves to a native harness. Revisit in a hardening pass. |

## FPU/SSE route (KERN-01)

Route: safe. No kernel `unsafe`; the Kernel table stays empty. Decided in 01-02 (D-01 spike
passed, so the D-02 module `kernel/src/fpu.rs` was not created and D-04 did not fire).

Mechanism: each `Thread` owns an `ostd::arch::cpu::context::FpuContext` (default MXCSR 0x1F80
and FCW 0x037F, no explicit initialisation needed). `FPU_OWNER` in `kernel/src/thread.rs` names
the thread whose state is live in the CPU. `UserModeHooks::pre_user_run` saves the previous owner
and loads the incoming thread when they differ. OSTD's `save`/`load` hold the `xsave`/`xrstor`
`unsafe`; the kernel is soft-float, so nothing else touches the registers between swaps.

Evidence (ktests in QEMU, plus the boot check; earlier notes said "under KVM", which was never
verified, and the 2026-10-26 re-run was under TCG, see `ipc-measurements.md`):

- `fpu_context_default_mxcsr_is_0x1f80`: the default context has the sane MXCSR and FCW.
- `xmm_survives_yield_between_two_threads`: two ring-3 threads hold different 16-register xmm
  patterns across 200 yields each and both exit 0.
- `xmm_swap_disabled_is_detected`: negative control. With the swap off by the ktest-only
  `FPU_SWAP_DISABLED` switch, the same test reports corruption, so the positive test can fail.
- `xmm_survives_redirect`: the signal-style redirect handler stub captures xmm0-xmm15 before any
  Rust code runs and they equal the interrupted pattern (KERN-01).
- Boot: `aok: fpu preemption check passed`. Two non-yielding spinners with different patterns,
  interleaved only by the timer (ticks [26, 26] in the recorded run), are redirected and both
  find their own pattern.

Limits: single CPU (`-smp 1`), so SMP needs a per-CPU owner. SSE only; AVX and x87 are not
stressed (D-03). The negative control covers the yield path; the timer-preemption check has no
swap-off counterpart because the BudgetScheduler cannot be injected in a ktest.
