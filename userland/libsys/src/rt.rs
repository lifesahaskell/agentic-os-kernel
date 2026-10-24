//! Process entry: `entry!(main)` defines `_start` and the redirect handler for a ring-3 binary.

/// `main: fn(u64) -> u64` receives the kernel-provided argument in `rdi`; its return value is
/// the exit code. `_aok_handler` exits with `rdi` = the interrupted pc (the B2 redirect
/// semantics).
#[macro_export]
macro_rules! entry {
    ($main:path) => {
        extern "C" fn __aok_main(arg: u64) -> u64 {
            let f: fn(u64) -> u64 = $main;
            f(arg)
        }

        // SAFETY: U-02 `_start` runs once with rsp at the kernel-mapped stack top and rdi = arg;
        // it aligns rsp, never returns (exit, then ud2), and `_aok_handler` only issues exit
        // with the registers the kernel set.
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
            ".globl _aok_handler",
            "_aok_handler:",
            "mov eax, 60",
            "syscall",
            "ud2",
            ".popsection",
            main = sym __aok_main,
        );
    };
}
