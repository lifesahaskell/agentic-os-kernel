//! Hostile-argument program: every syscall below is malformed and must come back as a negative
//! errno, never a kernel fault. `arg` is the SEND endpoint slot; the exit code is the number of
//! the first case that misbehaved, 0 if none did.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use abi::{IpcArgs, SYS_CALL, SYS_FUTEX, USER_TOP, errno};
use libsys::raw::syscall6;

const KERNEL_HALF: usize = 0xffff_8000_0000_0000;
const FUTEX_WAKE: usize = 1;

fn main(ep: u64) -> u64 {
    let ep = ep as usize;
    let mut args = IpcArgs::default();
    let args_ptr = &mut args as *mut IpcArgs as usize;

    let cases: [(isize, isize); 7] = [
        (
            syscall6(SYS_CALL, [ep, KERNEL_HALF, 0, 0, 0, 0]),
            -errno::EFAULT,
        ),
        (syscall6(SYS_CALL, [ep, 0x1000, 0, 0, 0, 0]), -errno::EFAULT),
        (
            syscall6(SYS_CALL, [ep, USER_TOP - 8, 0, 0, 0, 0]),
            -errno::EFAULT,
        ),
        (syscall6(0x1ff, [0; 6]), -errno::ENOSYS),
        (
            syscall6(SYS_FUTEX, [args_ptr, 99, 0, 0, 0, 0]),
            -errno::ENOSYS,
        ),
        (
            syscall6(SYS_FUTEX, [KERNEL_HALF, FUTEX_WAKE, 1, 0, 0, 0]),
            -errno::EFAULT,
        ),
        (
            syscall6(SYS_CALL, [1 << 40, args_ptr, 0, 0, 0, 0]),
            -errno::EBADF,
        ),
    ];
    for (i, (got, want)) in cases.into_iter().enumerate() {
        if got != want {
            return i as u64 + 1;
        }
    }
    0
}

libsys::entry!(main);
