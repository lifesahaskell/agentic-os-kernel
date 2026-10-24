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
    pub const ENOMEM: isize = 12;
    pub const EFAULT: isize = 14;
    pub const EINVAL: isize = 22;
    pub const ENOSYS: isize = 38;
    pub const EMSGSIZE: isize = 90;
    pub const ETIMEDOUT: isize = 110;
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

impl IpcArgs {
    /// The wire form: 21 little-endian u64 in field order, which is the `repr(C)` layout.
    pub fn to_bytes(&self) -> [u8; 168] {
        let q = [
            self.label,
            self.words[0],
            self.words[1],
            self.words[2],
            self.words[3],
            self.send_buf,
            self.send_len,
            self.recv_buf,
            self.recv_len,
            self.caps[0],
            self.caps[1],
            self.caps[2],
            self.caps[3],
            self.cap_rights[0],
            self.cap_rights[1],
            self.cap_rights[2],
            self.cap_rights[3],
            self.ncaps,
            self.tid,
            self.tag,
            self.reply,
        ];
        let mut out = [0u8; 168];
        for (chunk, v) in out.as_chunks_mut::<8>().0.iter_mut().zip(q) {
            *chunk = v.to_le_bytes();
        }
        out
    }

    pub fn from_bytes(b: &[u8; 168]) -> Self {
        let mut q = b.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c));
        let mut next = || q.next().unwrap();
        Self {
            label: next(),
            words: [next(), next(), next(), next()],
            send_buf: next(),
            send_len: next(),
            recv_buf: next(),
            recv_len: next(),
            caps: [next(), next(), next(), next()],
            cap_rights: [next(), next(), next(), next()],
            ncaps: next(),
            tid: next(),
            tag: next(),
            reply: next(),
        }
    }
}

/// First address above user space on x86-64 (47-bit canonical lower half).
pub const USER_TOP: usize = 0x0000_8000_0000_0000;
/// Largest byte count the kernel copies for one syscall buffer.
pub const MAX_IPC_BYTES: usize = 64 * 1024;

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
