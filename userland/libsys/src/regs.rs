//! All-register redirect probe (KERN-11, OD-01, OD-16): proves that a redirect followed by
//! `SYS_SIGRETURN` gives back every general register, xmm0-xmm15 and the 4 KiB of user stack below
//! the interrupted `rsp`, and that the handler ran at the red-zone-safe `rsp`.
//!
//! `regs_probe!()` defines `_start` and `_aok_handler` in the bin that invokes it. `_start` loads
//! known values into every register, then spins on `DONE` without touching any register. The
//! handler records `rsp` and the interrupted `pc`, scrambles every register, sets `DONE` and
//! returns through `SYS_SIGRETURN`. After the spin `_start` stores every register, moves `rsp`
//! out of the canary range and calls [`check`], whose result is the exit code.
#![allow(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

/// The red zone the kernel skips when it redirects (`kernel::thread::RED_ZONE`).
const RED_ZONE: u64 = 128;
/// Quads of canary below the interrupted `rsp`: one page.
const CANARY_QUADS: usize = 512;
/// The canary value.
pub const CANARY: u64 = 0xC0DE_CAFE_F00D_BEEF;

const fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) | 1
}

const fn table<const N: usize>(base: u64) -> [u64; N] {
    let mut a = [0u64; N];
    let mut i = 0;
    while i < N {
        a[i] = mix(base + i as u64);
        i += 1;
    }
    a
}

/// Values for rax, rbx, rcx, rdx, rsi, rdi, rbp, r8-r15, in that order. Distinct and non-zero.
pub static GPR_WANT: [u64; 15] = table(0x100);
/// Values for xmm0-xmm15, low quad first. Distinct and non-zero.
pub static XMM_WANT: [u64; 32] = table(0x200);

/// Set by the handler; the spin in `_start` waits for it.
pub static DONE: AtomicU64 = AtomicU64::new(0);
/// `rsp` when `_start` began the spin.
pub static RSP0: AtomicU64 = AtomicU64::new(0);
/// `rsp` on entry to the handler.
pub static HANDLER_RSP: AtomicU64 = AtomicU64::new(0);
/// `rdi` on entry to the handler: the interrupted `pc`.
pub static HANDLER_PC: AtomicU64 = AtomicU64::new(0);
/// The general registers as the spin found them after the handler returned.
pub static GPR_GOT: [AtomicU64; 15] = [const { AtomicU64::new(0) }; 15];
/// xmm0-xmm15 as the spin found them after the handler returned.
pub static XMM_GOT: [AtomicU64; 32] = [const { AtomicU64::new(0) }; 32];

/// The probe verdict, used as the exit code:
/// 0 everything matches; 1 + i the first general register i that differs; 20 + i the first xmm
/// register that differs; 40 a canary quad changed; 45 the handler's `rsp` is not
/// `((RSP0 - 128) & !15) - 8`; 46 the handler never saw a pc.
pub extern "C" fn check() -> u64 {
    for (i, want) in GPR_WANT.iter().enumerate() {
        if GPR_GOT[i].load(Ordering::Relaxed) != *want {
            return 1 + i as u64;
        }
    }
    for i in 0..16 {
        let lo = XMM_GOT[2 * i].load(Ordering::Relaxed) != XMM_WANT[2 * i];
        let hi = XMM_GOT[2 * i + 1].load(Ordering::Relaxed) != XMM_WANT[2 * i + 1];
        if lo || hi {
            return 20 + i as u64;
        }
    }
    let rsp0 = RSP0.load(Ordering::Relaxed);
    let base = (rsp0 - 8 * CANARY_QUADS as u64) as *const u64;
    // SAFETY: U-07 `_start` filled [RSP0 - 4096, RSP0) with the canary before the spin; that range
    // is inside the mapped user stack, 8-aligned (RSP0 is 16-aligned at entry), and the check runs
    // on a stack 16 KiB below it, so nothing else writes there.
    let canary = unsafe { core::slice::from_raw_parts(base, CANARY_QUADS) };
    if canary.iter().any(|q| *q != CANARY) {
        return 40;
    }
    if HANDLER_RSP.load(Ordering::Relaxed) != ((rsp0 - RED_ZONE) & !15) - 8 {
        return 45;
    }
    if HANDLER_PC.load(Ordering::Relaxed) == 0 {
        return 46;
    }
    0
}

/// Defines `_start` and `_aok_handler` for the probe. See the module docs for what they do.
#[macro_export]
macro_rules! regs_probe {
    () => {
        // SAFETY: U-07 `_start` and `_aok_handler` touch only the probe's own statics and the
        // 4 KiB canary below the entry `rsp`. The handler uses no stack, because the kernel put
        // `rsp` below the interrupted red zone and the probe must not write the canary range.
        // After the spin `_start` moves `rsp` 16 KiB below the canary before calling Rust.
        #[allow(unsafe_code)]
        core::arch::global_asm!(
            ".pushsection .text._start,\"ax\"",
            ".globl _start",
            "_start:",
            "mov [rip + {rsp0}], rsp",
            "cld",
            "lea rdi, [rsp - 4096]",
            "mov ecx, 512",
            "mov rax, {canary}",
            "rep stosq",
            "movdqu xmm0, [rip + {xw}]",
            "movdqu xmm1, [rip + {xw} + 16]",
            "movdqu xmm2, [rip + {xw} + 32]",
            "movdqu xmm3, [rip + {xw} + 48]",
            "movdqu xmm4, [rip + {xw} + 64]",
            "movdqu xmm5, [rip + {xw} + 80]",
            "movdqu xmm6, [rip + {xw} + 96]",
            "movdqu xmm7, [rip + {xw} + 112]",
            "movdqu xmm8, [rip + {xw} + 128]",
            "movdqu xmm9, [rip + {xw} + 144]",
            "movdqu xmm10, [rip + {xw} + 160]",
            "movdqu xmm11, [rip + {xw} + 176]",
            "movdqu xmm12, [rip + {xw} + 192]",
            "movdqu xmm13, [rip + {xw} + 208]",
            "movdqu xmm14, [rip + {xw} + 224]",
            "movdqu xmm15, [rip + {xw} + 240]",
            "mov rax, [rip + {gw}]",
            "mov rbx, [rip + {gw} + 8]",
            "mov rcx, [rip + {gw} + 16]",
            "mov rdx, [rip + {gw} + 24]",
            "mov rsi, [rip + {gw} + 32]",
            "mov rdi, [rip + {gw} + 40]",
            "mov rbp, [rip + {gw} + 48]",
            "mov r8, [rip + {gw} + 56]",
            "mov r9, [rip + {gw} + 64]",
            "mov r10, [rip + {gw} + 72]",
            "mov r11, [rip + {gw} + 80]",
            "mov r12, [rip + {gw} + 88]",
            "mov r13, [rip + {gw} + 96]",
            "mov r14, [rip + {gw} + 104]",
            "mov r15, [rip + {gw} + 112]",
            "2:",
            "cmp qword ptr [rip + {done}], 0",
            "je 2b",
            "mov [rip + {gg}], rax",
            "mov [rip + {gg} + 8], rbx",
            "mov [rip + {gg} + 16], rcx",
            "mov [rip + {gg} + 24], rdx",
            "mov [rip + {gg} + 32], rsi",
            "mov [rip + {gg} + 40], rdi",
            "mov [rip + {gg} + 48], rbp",
            "mov [rip + {gg} + 56], r8",
            "mov [rip + {gg} + 64], r9",
            "mov [rip + {gg} + 72], r10",
            "mov [rip + {gg} + 80], r11",
            "mov [rip + {gg} + 88], r12",
            "mov [rip + {gg} + 96], r13",
            "mov [rip + {gg} + 104], r14",
            "mov [rip + {gg} + 112], r15",
            "movdqu [rip + {xg}], xmm0",
            "movdqu [rip + {xg} + 16], xmm1",
            "movdqu [rip + {xg} + 32], xmm2",
            "movdqu [rip + {xg} + 48], xmm3",
            "movdqu [rip + {xg} + 64], xmm4",
            "movdqu [rip + {xg} + 80], xmm5",
            "movdqu [rip + {xg} + 96], xmm6",
            "movdqu [rip + {xg} + 112], xmm7",
            "movdqu [rip + {xg} + 128], xmm8",
            "movdqu [rip + {xg} + 144], xmm9",
            "movdqu [rip + {xg} + 160], xmm10",
            "movdqu [rip + {xg} + 176], xmm11",
            "movdqu [rip + {xg} + 192], xmm12",
            "movdqu [rip + {xg} + 208], xmm13",
            "movdqu [rip + {xg} + 224], xmm14",
            "movdqu [rip + {xg} + 240], xmm15",
            "mov rax, [rip + {rsp0}]",
            "sub rax, 16384",
            "and rax, -16",
            "mov rsp, rax",
            "call {check}",
            "mov rdi, rax",
            "mov eax, 60",
            "syscall",
            "ud2",
            ".popsection",
            ".pushsection .text._aok_handler,\"ax\"",
            ".globl _aok_handler",
            "_aok_handler:",
            "mov [rip + {hrsp}], rsp",
            "mov [rip + {hpc}], rdi",
            "xor eax, eax",
            "xor ebx, ebx",
            "xor ecx, ecx",
            "xor edx, edx",
            "xor esi, esi",
            "xor edi, edi",
            "xor ebp, ebp",
            "xor r8d, r8d",
            "xor r9d, r9d",
            "xor r10d, r10d",
            "xor r11d, r11d",
            "xor r12d, r12d",
            "xor r13d, r13d",
            "xor r14d, r14d",
            "xor r15d, r15d",
            "pxor xmm0, xmm0",
            "pxor xmm1, xmm1",
            "pxor xmm2, xmm2",
            "pxor xmm3, xmm3",
            "pxor xmm4, xmm4",
            "pxor xmm5, xmm5",
            "pxor xmm6, xmm6",
            "pxor xmm7, xmm7",
            "pxor xmm8, xmm8",
            "pxor xmm9, xmm9",
            "pxor xmm10, xmm10",
            "pxor xmm11, xmm11",
            "pxor xmm12, xmm12",
            "pxor xmm13, xmm13",
            "pxor xmm14, xmm14",
            "pxor xmm15, xmm15",
            "mov qword ptr [rip + {done}], 1",
            "mov eax, 0x108",
            "syscall",
            "mov edi, 41",
            "mov eax, 60",
            "syscall",
            "ud2",
            ".popsection",
            rsp0 = sym $crate::regs::RSP0,
            canary = const $crate::regs::CANARY,
            xw = sym $crate::regs::XMM_WANT,
            gw = sym $crate::regs::GPR_WANT,
            done = sym $crate::regs::DONE,
            gg = sym $crate::regs::GPR_GOT,
            xg = sym $crate::regs::XMM_GOT,
            hrsp = sym $crate::regs::HANDLER_RSP,
            hpc = sym $crate::regs::HANDLER_PC,
            check = sym $crate::regs::check,
        );
    };
}
