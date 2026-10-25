//! The one ring-3 syscall instruction.
#![allow(unsafe_code)]

/// `rax` = number, `rdi, rsi, rdx, r10, r8, r9` = arguments; the result is `rax`, negative errno
/// on failure.
pub fn syscall6(nr: usize, a: [usize; 6]) -> isize {
    let ret: isize;
    // SAFETY: U-01 `syscall` clobbers only rcx, r11 and rax, all declared. Memory the kernel
    // writes is reachable through the pointer arguments, so the default memory clobber stays.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr => ret,
            in("rdi") a[0],
            in("rsi") a[1],
            in("rdx") a[2],
            in("r10") a[3],
            in("r8") a[4],
            in("r9") a[5],
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}
