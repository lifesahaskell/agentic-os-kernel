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
| U-01 | `userland/libsys/src/raw.rs` `syscall6` | The `syscall` instruction has no safe wrapper; this is the only ring-3 trap into the kernel. | `syscall` clobbers only rax, rcx and r11, all declared. Memory the kernel writes is reachable through pointer arguments, so the default memory clobber stays. | Never: it is the ABI boundary. Keep it one block. |
| U-02 | `userland/libsys/src/rt.rs` `entry!` (`_start`, `_aok_handler`) | Process entry and the redirect handler run before any Rust runtime exists, so they must be `global_asm!`. | `_start` runs once with rsp at the kernel-mapped stack top and rdi = arg, aligns rsp, never returns. `_aok_handler` only issues exit with the registers the kernel set. The two-argument form stores xmm0-xmm15 into `HANDLER_XMM` before any Rust code runs, using rax as the only scratch, keeps rdi (the interrupted pc) until the handler shim call, and aligns rsp before `call`. | When the runtime gains a signal trampoline in a later phase, move the stubs into libaok. |
| U-03 | `userland/libsys/src/fpu.rs` `hold` | The FPU test needs xmm0-xmm15 loaded and compared with no compiler-generated code between, which only inline asm can promise. | Every xmm register, rax, rcx and r11 are declared clobbered (`syscall` takes rcx, r11 and rax); the pattern and result arrays outlive the block. | Test-only code; delete if the FPU gate moves to a native test harness. |

## FPU/SSE route (KERN-01)

Not yet decided (01-02 Task 2).
