//! Syscall numbers, errno values, the IPC argument block and the flat-image header constants,
//! shared by the kernel and ring-3 code. Numbers are never reused and carry no version field;
//! new calls start at `0x100`.

#![no_std]
#![deny(unsafe_code)]

pub const SYS_SCHED_YIELD: usize = 24;
pub const SYS_EXIT: usize = 60;
pub const SYS_FUTEX: usize = 202;
pub const SYS_CALL: usize = 0x100;

/// Linux errno values. A syscall returns `-errno` in `rax` on failure.
pub mod errno {
    pub const EPERM: isize = 1;
    pub const EINTR: isize = 4;
    pub const EBADF: isize = 9;
    pub const EAGAIN: isize = 11;
    pub const EFAULT: isize = 14;
    pub const EINVAL: isize = 22;
    pub const ENOSYS: isize = 38;
}

/// The one argument block for multi-value IPC calls. The kernel reads it from the pointer in
/// `rsi` with a single fixed-size copy. Input in `tid`, `tag` and `reply` is ignored.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct IpcArgs {
    pub label: u64,
    pub words: [u64; 4],
    pub send_buf: u64,
    pub send_len: u64,
    pub recv_buf: u64,
    pub recv_len: u64,
    pub caps: [u64; 4],
    pub cap_rights: [u64; 4],
    pub ncaps: u64,
    pub tid: u64,
    pub tag: u64,
    pub reply: u64,
}

const _: () = assert!(core::mem::size_of::<IpcArgs>() == 168);

/// Flat image: `userland/link.ld` puts five little-endian u64 at `IMAGE_BASE`: magic, entry,
/// handler, `rx_end`, `mem_end`. Pages below `rx_end` map RX, pages up to `mem_end` map RW and
/// zero-filled beyond the file.
pub const IMAGE_MAGIC: u64 = u64::from_le_bytes(*b"AOKIMG01");
pub const IMAGE_BASE: usize = 0x40_0000;
pub const IMAGE_HEADER_LEN: usize = 40;
pub const MAX_IMAGE_SIZE: usize = 16 * 1024 * 1024;
pub const USER_STACK_TOP: usize = 0x7000_0000;
/// 128 KiB: the null-IPC sample array lives on the stack.
pub const USER_STACK_PAGES: usize = 32;
