//! SSE state probes for the FPU/SSE switch tests (KERN-01, D-03): SSE only, no AVX or x87.
#![allow(unsafe_code)]

use core::arch::asm;

/// A distinct, non-zero 128-bit value per register, from a splitmix64 stream of `seed`.
pub fn pattern(seed: u64) -> [[u64; 2]; 16] {
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) | 1
    };
    core::array::from_fn(|_| [next(), next()])
}

/// Loads `pattern(seed)` into xmm0-xmm15, runs `iters` iterations (each a `sched_yield` when
/// `yield_each`, else a bare spin), stores xmm0-xmm15 and returns a mask of registers that differ.
/// No Rust code runs between the load and the store, so only the kernel can change the registers.
/// `iters == u64::MAX` runs until the thread is redirected, which never returns here.
pub fn hold(seed: u64, iters: u64, yield_each: bool) -> u16 {
    let want = pattern(seed);
    let mut got = [[0u64; 2]; 16];
    // SAFETY: U-03 every xmm register, rax, rcx and r11 are declared clobbered (`syscall` takes
    // rcx and r11 and returns in rax); `want` is read and `got` is written within their 256 bytes.
    unsafe {
        asm!(
            "movdqu xmm0, [{w}]",
            "movdqu xmm1, [{w} + 16]",
            "movdqu xmm2, [{w} + 32]",
            "movdqu xmm3, [{w} + 48]",
            "movdqu xmm4, [{w} + 64]",
            "movdqu xmm5, [{w} + 80]",
            "movdqu xmm6, [{w} + 96]",
            "movdqu xmm7, [{w} + 112]",
            "movdqu xmm8, [{w} + 128]",
            "movdqu xmm9, [{w} + 144]",
            "movdqu xmm10, [{w} + 160]",
            "movdqu xmm11, [{w} + 176]",
            "movdqu xmm12, [{w} + 192]",
            "movdqu xmm13, [{w} + 208]",
            "movdqu xmm14, [{w} + 224]",
            "movdqu xmm15, [{w} + 240]",
            "2:",
            "test {n}, {n}",
            "jz 3f",
            "test {y}, {y}",
            "jz 4f",
            "mov eax, 24",
            "syscall",
            "4:",
            "dec {n}",
            "jmp 2b",
            "3:",
            "movdqu [{g}], xmm0",
            "movdqu [{g} + 16], xmm1",
            "movdqu [{g} + 32], xmm2",
            "movdqu [{g} + 48], xmm3",
            "movdqu [{g} + 64], xmm4",
            "movdqu [{g} + 80], xmm5",
            "movdqu [{g} + 96], xmm6",
            "movdqu [{g} + 112], xmm7",
            "movdqu [{g} + 128], xmm8",
            "movdqu [{g} + 144], xmm9",
            "movdqu [{g} + 160], xmm10",
            "movdqu [{g} + 176], xmm11",
            "movdqu [{g} + 192], xmm12",
            "movdqu [{g} + 208], xmm13",
            "movdqu [{g} + 224], xmm14",
            "movdqu [{g} + 240], xmm15",
            w = in(reg) want.as_ptr(),
            g = in(reg) got.as_mut_ptr(),
            n = inout(reg) iters => _,
            y = in(reg) yield_each as u64,
            out("rax") _,
            out("rcx") _,
            out("r11") _,
            out("xmm0") _, out("xmm1") _, out("xmm2") _, out("xmm3") _,
            out("xmm4") _, out("xmm5") _, out("xmm6") _, out("xmm7") _,
            out("xmm8") _, out("xmm9") _, out("xmm10") _, out("xmm11") _,
            out("xmm12") _, out("xmm13") _, out("xmm14") _, out("xmm15") _,
            options(nostack),
        );
    }
    (0..16).fold(0, |mask, i| mask | u16::from(got[i] != want[i]) << i)
}
