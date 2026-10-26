//! Syscall numbers, errno values, the IPC argument block and the flat-image header constants,
//! shared by the kernel and ring-3 code. Numbers are never reused and carry no version field;
//! new calls start at `0x100`.

#![no_std]
#![deny(unsafe_code)]

pub const SYS_SCHED_YIELD: usize = 24;
pub const SYS_EXIT: usize = 60;
pub const SYS_FUTEX: usize = 202;
pub const SYS_CALL: usize = 0x100;
pub const SYS_RECV: usize = 0x101;
pub const SYS_REPLY: usize = 0x102;
pub const SYS_REPLY_RECV: usize = 0x103;
/// `timer_arm(ep, delay_ms, cookie)`: after `delay_ms` the kernel sends one `LABEL_TIMER` message
/// to the endpoint in slot `ep` (needs SEND) and the call returns the timer id.
pub const SYS_TIMER_ARM: usize = 0x106;
/// `thread_create(space, ctx, tag)`: starts a thread in the space named by `space`, a slot holding
/// a Space capability with CREATE, or `SPACE_SELF` for the caller's own space. `ctx` points at a
/// `ThreadCtx`; the kernel writes the new thread's id back into `ctx.tid` before the thread runs.
/// `tag` is stamped on the thread's messages; with `SPACE_SELF` it must be 0 and the caller's tag
/// is inherited. Returns the slot of a new Thread capability (SIGNAL).
pub const SYS_THREAD_CREATE: usize = 0x104;
/// `thread_interrupt(thread, entry)`: redirects the thread named by a Thread capability with
/// SIGNAL to `entry`, aborting a blocking IPC with `EINTR`. `ESRCH` once the thread has exited.
pub const SYS_THREAD_INTERRUPT: usize = 0x105;
/// `space_notify(space, ep)`: makes the Endpoint in slot `ep` (needs SEND) the one the kernel sends
/// exit and fault notes to for the threads of `space`, a slot holding a Space capability with
/// SIGNAL, or `SPACE_SELF` for the caller's own. A holder may replace the endpoint at any time;
/// a thread naming its own space may set it only once (`EPERM` when one is already set), so a
/// thread cannot redirect the notes its parent arranged (OD-18).
pub const SYS_SPACE_NOTIFY: usize = 0x107;

/// `thread_create`'s `space` argument for "the caller's own space". No slot can hold this value:
/// slots are below `CSpace::MAX_SLOTS`.
pub const SPACE_SELF: u64 = u64::MAX;

/// Linux errno values. A syscall returns `-errno` in `rax` on failure.
pub mod errno {
    pub const EPERM: isize = 1;
    pub const ESRCH: isize = 3;
    pub const EINTR: isize = 4;
    pub const EBADF: isize = 9;
    pub const EAGAIN: isize = 11;
    pub const ENOMEM: isize = 12;
    pub const EFAULT: isize = 14;
    pub const EINVAL: isize = 22;
    pub const EMFILE: isize = 24;
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

    /// Points `send_buf` and `send_len` at `buf`. The kernel copies the bytes at call time.
    pub fn with_send(mut self, buf: &[u8]) -> Self {
        self.send_buf = buf.as_ptr() as u64;
        self.send_len = buf.len() as u64;
        self
    }

    /// Points `recv_buf` and `recv_len` at `buf`. The kernel writes at most `buf.len()` bytes.
    pub fn with_recv(mut self, buf: &mut [u8]) -> Self {
        self.recv_buf = buf.as_mut_ptr() as u64;
        self.recv_len = buf.len() as u64;
        self
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

/// The block `thread_create` reads from the pointer in `rsi`: where the new thread starts. The
/// kernel writes `tid` back, so the creator can name the thread before it runs. `clear_tid` must
/// be 0 for now.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct ThreadCtx {
    pub rip: u64,
    pub rsp: u64,
    /// Arrives in `rdi`.
    pub arg: u64,
    pub fs_base: u64,
    pub clear_tid: u64,
    /// Output only: the new thread's id. Input is ignored.
    pub tid: u64,
}

const _: () = assert!(core::mem::size_of::<ThreadCtx>() == 48);

/// Byte offset of `ThreadCtx::tid`, where the kernel writes the new thread's id.
pub const THREAD_CTX_TID_OFFSET: usize = 40;

impl ThreadCtx {
    /// The wire form: six little-endian u64 in field order, which is the `repr(C)` layout.
    pub fn to_bytes(&self) -> [u8; 48] {
        let q = [
            self.rip,
            self.rsp,
            self.arg,
            self.fs_base,
            self.clear_tid,
            self.tid,
        ];
        let mut out = [0u8; 48];
        for (chunk, v) in out.as_chunks_mut::<8>().0.iter_mut().zip(q) {
            *chunk = v.to_le_bytes();
        }
        out
    }

    pub fn from_bytes(b: &[u8; 48]) -> Self {
        let mut q = b.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c));
        let mut next = || q.next().unwrap();
        Self {
            rip: next(),
            rsp: next(),
            arg: next(),
            fs_base: next(),
            clear_tid: next(),
            tid: next(),
        }
    }
}

/// A relative timeout, as `FUTEX_WAIT` reads it from `a[3]`: `sec >= 0` and `0 <= nsec < 1e9`,
/// otherwise the call is `EINVAL`. The kernel rounds up to whole 1 ms ticks.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Timespec {
    pub sec: i64,
    pub nsec: i64,
}

const _: () = assert!(core::mem::size_of::<Timespec>() == 16);

impl Timespec {
    /// The wire form: two little-endian i64, which is the `repr(C)` layout.
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&self.sec.to_le_bytes());
        out[8..].copy_from_slice(&self.nsec.to_le_bytes());
        out
    }

    pub fn from_bytes(b: &[u8; 16]) -> Self {
        let (s, n) = b.split_at(8);
        Self {
            sec: i64::from_le_bytes(s.try_into().unwrap()),
            nsec: i64::from_le_bytes(n.try_into().unwrap()),
        }
    }
}

/// Longest timeout, in milliseconds, for a timed `recv`, a timed `reply_recv` and `SYS_TIMER_ARM`
/// (ten minutes). A larger value is `EINVAL`. A futex timespec is not capped: its tick count
/// saturates.
pub const MAX_TIMEOUT_MS: u64 = 600_000;

/// First address above user space on x86-64 (47-bit canonical lower half).
pub const USER_TOP: usize = 0x0000_8000_0000_0000;
/// Largest byte count the kernel copies for one syscall buffer.
pub const MAX_IPC_BYTES: usize = 64 * 1024;
/// Most capabilities one message carries (`IpcArgs.caps`).
pub const MAX_IPC_CAPS: usize = 4;
/// The `label` of the kernel's Cancel message: a caller was interrupted after its call was
/// received, `reply` is the handle of that call. Only the kernel sends it; a ring-3 `call` with
/// this label is EINVAL.
pub const LABEL_CANCEL: u64 = u64::MAX;

/// Labels at or above this are the kernel's: `send` and `call` refuse them with EINVAL, so a
/// receiver can trust one. `LABEL_CANCEL` is `u64::MAX`; the timer takes `u64::MAX - 1`, the two
/// notes below take `u64::MAX - 2` and `u64::MAX - 3`, and SIG-03 takes one (OD-12). The rest of
/// the range stays free.
pub const LABEL_KERNEL_BASE: u64 = u64::MAX - 15;

/// The label of a timer message. `words` are `[cookie, due, now, id]`: the cookie given to
/// `timer_arm`, the tick it was due, the tick it was sent, and the timer id. The sender credential
/// is the arming agent's, `tid` is 0 and `tag` is the arming thread's.
pub const LABEL_TIMER: u64 = u64::MAX - 1;

/// The label of a fault note, inside the kernel range (`LABEL_KERNEL_BASE`), so ring 3 cannot
/// send it. The kernel makes it as a `call` from the faulting thread to the space's notify
/// endpoint: `words` are `[vector, address, rip, error code]`, and the sender credential, `tid`
/// and `tag` are the faulting thread's. The reply decides: `words[0] == 0` resumes the thread,
/// anything else kills it. A note carries no capability and no data.
pub const NOTE_FAULT: u64 = u64::MAX - 2;

/// The label of an exit note: one message when a thread of the space exits, stamped like
/// `NOTE_FAULT`. `words` are `[kind, value, 0, 0]`, with the `NOTE_KIND_*` values below.
pub const NOTE_EXIT: u64 = u64::MAX - 3;

/// `NOTE_EXIT` kind: the thread called `exit`; `value` is the code.
pub const NOTE_KIND_CODE: u64 = 0;
/// `NOTE_EXIT` kind: a fault that was not copy-on-write ended the thread; `value` is the address.
pub const NOTE_KIND_FAULT: u64 = 1;
/// `NOTE_EXIT` kind: a copy-on-write fault the agent's memory budget could not pay for.
pub const NOTE_KIND_BUDGET: u64 = 2;
/// `NOTE_EXIT` kind: a non-page-fault exception ended the thread; `value` is the x86 vector.
pub const NOTE_KIND_EXCEPTION: u64 = 3;
/// `NOTE_EXIT` kind: the thread was killed from outside (plan 02-06 sends it).
pub const NOTE_KIND_KILLED: u64 = 4;

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
/// Address of the one test data page. The kernel's ktests and boot checks map a RW page here when
/// a test program needs shared or inspectable memory; `libsys::test_page` reads it. It sits above
/// any image (`MAX_IMAGE_SIZE` is not reached by the test programs) and below the stack.
pub const TEST_DATA: usize = 0x50_0000;
