//! Process entry: `entry!(main)` or `entry!(main, handler)` defines `_start` and the redirect
//! handler for a ring-3 binary.

use core::sync::atomic::{AtomicU64, Ordering};

/// xmm0-xmm15 as the two-argument `_aok_handler` found them, captured before any Rust code ran.
pub static HANDLER_XMM: [AtomicU64; 32] = [const { AtomicU64::new(0) }; 32];

pub fn handler_xmm() -> [[u64; 2]; 16] {
    core::array::from_fn(|i| {
        [
            HANDLER_XMM[2 * i].load(Ordering::Relaxed),
            HANDLER_XMM[2 * i + 1].load(Ordering::Relaxed),
        ]
    })
}

/// `main: fn(u64) -> u64` receives the kernel-provided argument in `rdi`; its return value is
/// the exit code. The one-argument form's `_aok_handler` exits with `rdi` = the interrupted pc
/// (the B2 redirect semantics). The two-argument form stores xmm0-xmm15 into `HANDLER_XMM`,
/// calls `handler: fn(pc: u64) -> u64` and exits with its return value.
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        $crate::__aok_start!($main);
        // SAFETY: U-02 `_aok_handler` only issues exit with the registers the kernel set.
        #[allow(unsafe_code)]
        core::arch::global_asm!(
            ".pushsection .text._aok_handler,\"ax\"",
            ".globl _aok_handler",
            "_aok_handler:",
            "mov eax, 60",
            "syscall",
            "ud2",
            ".popsection",
        );
    };
    ($main:path, $handler:path) => {
        $crate::__aok_start!($main);
        extern "C" fn __aok_handler_shim(pc: u64) -> u64 {
            let f: fn(u64) -> u64 = $handler;
            f(pc)
        }

        // SAFETY: U-02 the stub stores xmm0-xmm15 before any Rust code can touch them, using rax
        // as the only scratch (the thread exits afterwards, so nothing needs rax); rdi, the
        // interrupted pc, is untouched until the shim call; rsp is aligned before `call`.
        #[allow(unsafe_code)]
        core::arch::global_asm!(
            ".pushsection .text._aok_handler,\"ax\"",
            ".globl _aok_handler",
            "_aok_handler:",
            "lea rax, [rip + {xmm}]",
            "movdqu [rax], xmm0",
            "movdqu [rax + 16], xmm1",
            "movdqu [rax + 32], xmm2",
            "movdqu [rax + 48], xmm3",
            "movdqu [rax + 64], xmm4",
            "movdqu [rax + 80], xmm5",
            "movdqu [rax + 96], xmm6",
            "movdqu [rax + 112], xmm7",
            "movdqu [rax + 128], xmm8",
            "movdqu [rax + 144], xmm9",
            "movdqu [rax + 160], xmm10",
            "movdqu [rax + 176], xmm11",
            "movdqu [rax + 192], xmm12",
            "movdqu [rax + 208], xmm13",
            "movdqu [rax + 224], xmm14",
            "movdqu [rax + 240], xmm15",
            "and rsp, -16",
            "call {shim}",
            "mov rdi, rax",
            "mov eax, 60",
            "syscall",
            "ud2",
            ".popsection",
            xmm = sym $crate::rt::HANDLER_XMM,
            shim = sym __aok_handler_shim,
        );
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __aok_start {
    ($main:path) => {
        extern "C" fn __aok_main(arg: u64) -> u64 {
            let f: fn(u64) -> u64 = $main;
            f(arg)
        }

        // SAFETY: U-02 `_start` runs once with rsp at the kernel-mapped stack top and rdi = arg;
        // it aligns rsp and never returns (exit, then ud2).
        #[allow(unsafe_code)]
        core::arch::global_asm!(
            ".pushsection .text._start,\"ax\"",
            ".globl _start",
            "_start:",
            "xor ebp, ebp",
            "and rsp, -16",
            "call {main}",
            "mov rdi, rax",
            "mov eax, 60",
            "syscall",
            "ud2",
            ".popsection",
            main = sym __aok_main,
        );
    };
}
