//! The single `unsafe` module in this crate (D-13). Every unsafe block below
//! carries a `// SAFETY:` comment. `main.rs` declares this module
//! `#[allow(unsafe_code)] mod sys;`; every other module in the crate is safe
//! Rust and calls only the safe wrappers this module exposes.
//!
//! Identity and peer/cgroup/clone items come from `libc` 0.2.191 by path
//! (`libc::pidfd_info`, `libc::PIDFD_GET_INFO`, `libc::PIDFD_INFO_CGROUPID`,
//! `libc::SO_PEERPIDFD`, `libc::clone_args`); this module hand-defines only
//! what that libc version lacks: `CLONE_INTO_CGROUP` at the correct width,
//! the Landlock ABI structs/constants, `SYS_LSM_LIST_MODULES`, and the
//! io_uring SQE/CQE/params structs (RESEARCH lines 454, 482).

use std::ffi::CStr;
use std::io;
use std::os::fd::RawFd;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Compile-time probe (RESEARCH A6): libc 0.2.191's `pidfd_info` must match
// the first 64 bytes of `struct aok_pidfd_info` in
// tools/linux-baseline/uapi_compat.h:76-93 (mask through exit_code). A libc
// bump that renames, removes, or resizes any of these breaks the build here
// instead of silently breaking the ioctl at runtime.
// ---------------------------------------------------------------------------

#[allow(dead_code)] // referenced only to pin the symbol at compile time
const _PROBE_PIDFD_GET_INFO: libc::Ioctl = libc::PIDFD_GET_INFO;
#[allow(dead_code)]
const _PROBE_SO_PEERPIDFD: libc::c_int = libc::SO_PEERPIDFD;

const _: () = assert!(core::mem::size_of::<libc::pidfd_info>() == 64);
const _: () = assert!(core::mem::offset_of!(libc::pidfd_info, cgroupid) == 8);

// ---------------------------------------------------------------------------
// Hand-defined: CLONE_INTO_CGROUP (route iv, Task 2's routes.rs).
// libc 0.2.191 ships this constant typed `c_int`, which cannot hold a
// 33-bit value (0x2_0000_0000) and is marked deprecated-for-overflow
// upstream since 0.2.188. Redefined here at the correct width instead of
// using libc's broken copy.
// ---------------------------------------------------------------------------

pub const CLONE_INTO_CGROUP: u64 = 0x2_0000_0000;

// ---------------------------------------------------------------------------
// Hand-defined: Landlock ABI (uapi_compat.h:15-46). libc 0.2.191 ships the
// landlock_* syscall numbers but not these structs/constants. Not consumed
// by Tasks 1-3's route or case set; reserved for later LSM-mitigation work
// (ADV-04, closable_by).
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[repr(C)]
pub struct LandlockRulesetAttr {
    pub handled_access_fs: u64,
    pub handled_access_net: u64,
    pub scoped: u64,
}

#[allow(dead_code)]
#[repr(C, packed)]
pub struct LandlockPathBeneathAttr {
    pub allowed_access: u64,
    pub parent_fd: i32,
}

#[allow(dead_code)]
#[repr(C)]
pub struct LandlockNetPortAttr {
    pub allowed_access: u64,
    pub port: u64,
}

#[allow(dead_code)]
pub const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;
#[allow(dead_code)]
pub const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;
#[allow(dead_code)]
pub const LANDLOCK_RULE_NET_PORT: u32 = 2;
#[allow(dead_code)]
pub const LANDLOCK_ACCESS_FS_ALL: u64 = 0xFFFF;
#[allow(dead_code)]
pub const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
#[allow(dead_code)]
pub const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
#[allow(dead_code)]
pub const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
#[allow(dead_code)]
pub const LANDLOCK_ACCESS_NET_BIND_TCP: u64 = 1 << 0;
#[allow(dead_code)]
pub const LANDLOCK_ACCESS_NET_CONNECT_TCP: u64 = 1 << 1;
#[allow(dead_code)]
pub const LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
#[allow(dead_code)]
pub const LANDLOCK_SCOPE_SIGNAL: u64 = 1 << 1;

/// libc 0.2.191 has no lsm_list_modules syscall number or wrapper at all.
/// Named in libc's own mixed-case syscall-number style
/// (`SYS_pidfd_open`-style), not idiomatic Rust SCREAMING_SNAKE_CASE, hence
/// the explicit lint allow. Reserved for later LSM-introspection work
/// (ADV-04); unused by Tasks 1-3.
#[allow(dead_code)]
#[allow(non_upper_case_globals)]
pub const SYS_lsm_list_modules: libc::c_long = 461;

// ---------------------------------------------------------------------------
// Hand-defined: io_uring SQE/CQE/params (RESEARCH lines 454, 482). libc
// 0.2.191 does not ship these at all. Consumed by `io_uring_openat_once`
// below, which Task 3's cases.rs calls for the io_uring S1 case.
// ---------------------------------------------------------------------------

const IORING_OFF_SQ_RING: i64 = 0;
const IORING_OFF_CQ_RING: i64 = 0x8000000;
const IORING_OFF_SQES: i64 = 0x1000_0000;
const IORING_OP_OPENAT: u8 = 18;
const IORING_ENTER_GETEVENTS: u32 = 1 << 0;

#[allow(dead_code)]
#[repr(C)]
struct IoSqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv1: u32,
    user_addr: u64,
}

#[allow(dead_code)]
#[repr(C)]
struct IoCqringOffsets {
    head: u32,
    tail: u32,
    ring_mask: u32,
    ring_entries: u32,
    overflow: u32,
    cqes: u32,
    flags: u32,
    resv1: u32,
    user_addr: u64,
}

#[allow(dead_code)]
#[repr(C)]
struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: IoSqringOffsets,
    cq_off: IoCqringOffsets,
}

#[allow(dead_code)]
#[repr(C)]
struct IoUringSqe {
    opcode: u8,
    flags: u8,
    ioprio: u16,
    fd: i32,
    off: u64,
    addr: u64,
    len: u32,
    op_flags: u32,
    user_data: u64,
    buf_index: u16,
    personality: u16,
    splice_fd_in: i32,
    addr3: u64,
    pad2: u64,
}

#[allow(dead_code)]
#[repr(C)]
struct IoUringCqe {
    user_data: u64,
    res: i32,
    flags: u32,
}

/// Submit one `IORING_OP_OPENAT` and wait for its completion: the minimal
/// single-shot ring the io_uring S1 case (Task 3) needs, not a
/// general-purpose ring. Every mmap'd region and the ring fd are torn down
/// on every return path.
#[allow(dead_code)] // used by Task 3's cases.rs (io_uring S1 case)
pub fn io_uring_openat_once(
    dirfd: RawFd,
    path: &CStr,
    oflag: libc::c_int,
    mode: libc::mode_t,
) -> io::Result<RawFd> {
    // SAFETY: `params` is zero-initialized (every field is an integer, so
    // all-zero is a valid `io_uring_params` to pass in; the kernel only
    // reads `flags`/`sq_thread_cpu`/`sq_thread_idle`/`wq_fd` from it, all
    // zero here meaning "no special setup"); the kernel fills the rest in
    // on return.
    let mut params: IoUringParams = unsafe { core::mem::zeroed() };
    // SAFETY: SYS_io_uring_setup takes an entry count and a mutable params
    // pointer sized for one `IoUringParams`, a live local on this stack
    // frame for the duration of the call.
    let ring_fd = unsafe {
        libc::syscall(
            libc::SYS_io_uring_setup,
            1u32,
            &mut params as *mut IoUringParams,
        )
    };
    if ring_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let ring_fd = ring_fd as RawFd;

    let sq_ring_size =
        params.sq_off.array as usize + params.sq_entries as usize * core::mem::size_of::<u32>();
    let cq_ring_size = params.cq_off.cqes as usize
        + params.cq_entries as usize * core::mem::size_of::<IoUringCqe>();
    let sqes_size = params.sq_entries as usize * core::mem::size_of::<IoUringSqe>();

    // SAFETY: `ring_fd` is the io_uring fd returned above and still open;
    // the offsets (IORING_OFF_SQ_RING / IORING_OFF_SQES / IORING_OFF_CQ_RING)
    // and sizes come from the kernel-filled `params`, the documented mmap
    // contract for io_uring_setup. MAP_SHARED is required so writes here are
    // visible to the kernel. Every mapping made here is torn down with
    // munmap below on every exit path, and `ring_fd` is closed there too.
    let (sq_ptr, sqes_ptr, cq_ptr) = unsafe {
        let sq_ptr = libc::mmap(
            ptr::null_mut(),
            sq_ring_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            ring_fd,
            IORING_OFF_SQ_RING,
        );
        if sq_ptr == libc::MAP_FAILED {
            let e = io::Error::last_os_error();
            libc::close(ring_fd);
            return Err(e);
        }
        let sqes_ptr = libc::mmap(
            ptr::null_mut(),
            sqes_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            ring_fd,
            IORING_OFF_SQES,
        );
        if sqes_ptr == libc::MAP_FAILED {
            let e = io::Error::last_os_error();
            libc::munmap(sq_ptr, sq_ring_size);
            libc::close(ring_fd);
            return Err(e);
        }
        let cq_ptr = libc::mmap(
            ptr::null_mut(),
            cq_ring_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            ring_fd,
            IORING_OFF_CQ_RING,
        );
        if cq_ptr == libc::MAP_FAILED {
            let e = io::Error::last_os_error();
            libc::munmap(sqes_ptr, sqes_size);
            libc::munmap(sq_ptr, sq_ring_size);
            libc::close(ring_fd);
            return Err(e);
        }
        (sq_ptr, sqes_ptr, cq_ptr)
    };

    let result = (|| -> io::Result<RawFd> {
        // SAFETY: every pointer below stays within the mmap'd regions sized
        // above, at byte offsets the kernel supplied in `params.sq_off` /
        // `params.cq_off`; the ring head/tail words are shared with the
        // kernel, so they are read and written through `AtomicU32` with the
        // acquire/release pairing io_uring's memory model requires instead
        // of plain loads/stores.
        unsafe {
            let sq_array = (sq_ptr as *mut u8).add(params.sq_off.array as usize) as *mut u32;
            let sq_tail_ptr =
                (sq_ptr as *mut u8).add(params.sq_off.tail as usize) as *mut AtomicU32;
            let sq_mask =
                *((sq_ptr as *const u8).add(params.sq_off.ring_mask as usize) as *const u32);

            let tail = (*sq_tail_ptr).load(Ordering::Acquire);
            let index = tail & sq_mask;

            let sqe = (sqes_ptr as *mut IoUringSqe).add(index as usize);
            ptr::write_bytes(sqe, 0, 1);
            (*sqe).opcode = IORING_OP_OPENAT;
            (*sqe).fd = dirfd;
            (*sqe).addr = path.as_ptr() as u64;
            (*sqe).len = mode;
            (*sqe).op_flags = oflag as u32;
            (*sqe).user_data = 1;

            *sq_array.add(index as usize) = index;
            (*sq_tail_ptr).store(tail.wrapping_add(1), Ordering::Release);

            let submitted = libc::syscall(
                libc::SYS_io_uring_enter,
                ring_fd,
                1u32,
                1u32,
                IORING_ENTER_GETEVENTS,
                ptr::null_mut::<libc::c_void>(),
                0usize,
            );
            if submitted < 0 {
                return Err(io::Error::last_os_error());
            }

            let cq_head_ptr =
                (cq_ptr as *mut u8).add(params.cq_off.head as usize) as *mut AtomicU32;
            let cq_tail_ptr =
                (cq_ptr as *mut u8).add(params.cq_off.tail as usize) as *mut AtomicU32;
            let cq_mask =
                *((cq_ptr as *const u8).add(params.cq_off.ring_mask as usize) as *const u32);

            let head = (*cq_head_ptr).load(Ordering::Acquire);
            let cq_tail = (*cq_tail_ptr).load(Ordering::Acquire);
            if head == cq_tail {
                return Err(io::Error::other("io_uring: no completion after enter"));
            }
            let cq_index = head & cq_mask;
            let cqe_ptr = (cq_ptr as *const u8).add(
                params.cq_off.cqes as usize
                    + cq_index as usize * core::mem::size_of::<IoUringCqe>(),
            ) as *const IoUringCqe;
            let res = (*cqe_ptr).res;
            (*cq_head_ptr).store(head.wrapping_add(1), Ordering::Release);

            if res >= 0 {
                Ok(res)
            } else {
                Err(io::Error::from_raw_os_error(-res))
            }
        }
    })();

    // SAFETY: tearing down exactly the mappings and fd opened above, all
    // still valid here regardless of which branch of `result` ran.
    unsafe {
        libc::munmap(cq_ptr, cq_ring_size);
        libc::munmap(sqes_ptr, sqes_size);
        libc::munmap(sq_ptr, sq_ring_size);
        libc::close(ring_fd);
    }

    result
}

// ---------------------------------------------------------------------------
// Kernel-stamped peer identity (D-15): SO_PEERPIDFD then PIDFD_GET_INFO,
// read fresh on every call. Mirrors tools/linux-baseline/broker.c's
// broker_ident path.
// ---------------------------------------------------------------------------

/// The peer's kernel-stamped cgroup id and pid, read at the moment of the
/// call. Never cached: a peer that moves cgroups between requests is read
/// as whatever it holds at call time (the read-then-act race named in
/// report.rs).
pub struct PeerIdentity {
    pub cgroupid: u64,
    pub pid: i32,
}

/// Read the identity of whoever is connected to `sock_fd` right now.
pub fn peer_identity(sock_fd: RawFd) -> io::Result<PeerIdentity> {
    // SAFETY: `sock_fd` is a valid, open socket fd owned by the caller for
    // the duration of this call. `pidfd` and `len` are live locals on this
    // stack frame; getsockopt writes at most `size_of::<c_int>()` bytes into
    // `pidfd`, exactly the size `len` names.
    let pidfd: RawFd = unsafe {
        let mut pidfd: libc::c_int = -1;
        let mut len = core::mem::size_of::<libc::c_int>() as libc::socklen_t;
        let rc = libc::getsockopt(
            sock_fd,
            libc::SOL_SOCKET,
            libc::SO_PEERPIDFD,
            &mut pidfd as *mut libc::c_int as *mut libc::c_void,
            &mut len,
        );
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        pidfd
    };

    // SAFETY: `info` is zero-initialized, valid for libc's #[non_exhaustive]
    // `pidfd_info` because every field is an integer (the all-zero bit
    // pattern is a valid value); `mask` is then set to request exactly the
    // fields read below before the ioctl call. `pidfd` is the fd opened
    // above, valid for the duration of this call and closed before every
    // return path below.
    unsafe {
        let mut info: libc::pidfd_info = core::mem::zeroed();
        info.mask = (libc::PIDFD_INFO_CGROUPID | libc::PIDFD_INFO_PID) as u64;
        let rc = libc::ioctl(
            pidfd,
            libc::PIDFD_GET_INFO,
            &mut info as *mut libc::pidfd_info,
        );
        let saved_err = io::Error::last_os_error();
        libc::close(pidfd);
        if rc != 0 {
            return Err(saved_err);
        }
        Ok(PeerIdentity {
            cgroupid: info.cgroupid,
            pid: info.pid as i32,
        })
    }
}

// ---------------------------------------------------------------------------
// pidfd primitives (ADV-02 kill/ptrace families).
// ---------------------------------------------------------------------------

/// `pidfd_open(pid, flags)`. libc 0.2.191 has the syscall number but no
/// function wrapper.
#[allow(dead_code)] // used by the kill-route test and by Task 2's routes.rs
pub fn pidfd_open(pid: libc::pid_t, flags: libc::c_uint) -> io::Result<RawFd> {
    // SAFETY: SYS_pidfd_open takes a pid and a flags word, both plain
    // integers; no pointers are passed, so there is no aliasing or lifetime
    // concern.
    let rc = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, flags) };
    if rc >= 0 {
        Ok(rc as RawFd)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `pidfd_send_signal(pidfd, sig, NULL, 0)`. Operates on a pidfd, which
/// resolves against the kernel's stable identity for that fd rather than a
/// raw pid, so it cannot be confused by pid reuse (D-09 bias toward
/// undercounting, never overcounting).
#[allow(dead_code)] // used by the kill-route test and by Task 2's routes.rs
pub fn pidfd_send_signal(pidfd: RawFd, sig: libc::c_int) -> io::Result<()> {
    // SAFETY: `pidfd` is a caller-owned, open pidfd; `flags` is 0 (no flags
    // are defined for this syscall); the `info` siginfo_t pointer is null,
    // the kernel's documented optional case.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd,
            sig,
            ptr::null_mut::<libc::siginfo_t>(),
            0 as libc::c_uint,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `pidfd_getfd(pidfd, targetfd, 0)`.
#[allow(dead_code)] // used by Task 2's routes.rs (ptrace family, pidfd_getfd route)
pub fn pidfd_getfd(pidfd: RawFd, targetfd: RawFd) -> io::Result<RawFd> {
    // SAFETY: `pidfd` and `targetfd` are caller-owned fds valid for the
    // duration of this call; `flags` is 0 (no flags defined for this
    // syscall).
    let rc = unsafe { libc::syscall(libc::SYS_pidfd_getfd, pidfd, targetfd, 0 as libc::c_uint) };
    if rc >= 0 {
        Ok(rc as RawFd)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Close a raw fd this process owns. A thin wrapper so every raw syscall
/// in this crate, `close()` included, stays inside this module (D-13).
#[allow(dead_code)] // used by the kill-route test and by Task 2/3
pub fn close_fd(fd: RawFd) {
    // SAFETY: the caller asserts ownership of `fd` and does not use it
    // again after this call.
    unsafe {
        libc::close(fd);
    }
}

/// True if `fd` becomes readable (POLLIN) within `timeout`. Used to confirm
/// a pidfd reports process exit without a busy loop or an unbounded wait.
#[allow(dead_code)] // used by the kill-route test and by Task 2/3
pub fn poll_readable(fd: RawFd, timeout: Duration) -> io::Result<bool> {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
    // SAFETY: `pfd` is a single live `pollfd` on this stack frame naming an
    // fd the caller owns; `poll` reads/writes only through this pointer and
    // for exactly the one element the length argument names.
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(rc > 0 && (pfd.revents & libc::POLLIN) != 0)
}

// ---------------------------------------------------------------------------
// Signal primitives (ADV-02 kill family: kill, tgkill, rt_sigqueueinfo).
// Each rejects pid/tgid <= 0 before the syscall: the suite never signals
// pid 0 (process group) or pid -1 (every process) (T-08-04).
// ---------------------------------------------------------------------------

/// `kill(pid, sig)`, refusing `pid <= 0`.
#[allow(dead_code)] // used by Task 2's routes.rs (kill family, kill route)
pub fn kill(pid: libc::pid_t, sig: libc::c_int) -> io::Result<()> {
    if pid <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "kill: refusing pid <= 0 (never kill(0)/kill(-1))",
        ));
    }
    // SAFETY: `pid > 0` is enforced above, so this can only signal one
    // specific process, never the caller's process group (pid 0) or every
    // process the caller may signal (pid -1).
    let rc = unsafe { libc::kill(pid, sig) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `tgkill(tgid, tid, sig)`, refusing `tgid <= 0` or `tid <= 0`.
#[allow(dead_code)] // used by Task 2's routes.rs (kill family, tgkill route)
pub fn tgkill(tgid: libc::pid_t, tid: libc::pid_t, sig: libc::c_int) -> io::Result<()> {
    if tgid <= 0 || tid <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "tgkill: refusing tgid/tid <= 0",
        ));
    }
    // SAFETY: tgkill takes three plain integers, no pointers; tgid and tid
    // are both positive, enforced above.
    let rc = unsafe { libc::syscall(libc::SYS_tgkill, tgid, tid, sig) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `rt_sigqueueinfo(tgid, sig, &info)` with `si_code = SI_QUEUE`, the value
/// the kernel requires from an unprivileged sender.
#[allow(dead_code)] // used by Task 2's routes.rs (kill family, rt_sigqueueinfo route)
pub fn rt_sigqueueinfo(tgid: libc::pid_t, sig: libc::c_int) -> io::Result<()> {
    if tgid <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "rt_sigqueueinfo: refusing tgid <= 0",
        ));
    }
    // SAFETY: `info` is zero-initialized then given the two fields the
    // kernel validates for an unprivileged sender (si_signo, si_code); it is
    // a live local for the duration of this call, passed by pointer.
    let mut info: libc::siginfo_t = unsafe { core::mem::zeroed() };
    info.si_signo = sig;
    info.si_code = libc::SI_QUEUE;
    // SAFETY: `tgid` is positive (enforced above); `&mut info` points at the
    // live local just initialized.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_rt_sigqueueinfo,
            tgid,
            sig,
            &mut info as *mut libc::siginfo_t,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

// ---------------------------------------------------------------------------
// ptrace primitives (ADV-02 ptrace family).
// ---------------------------------------------------------------------------

/// `PTRACE_ATTACH`.
#[allow(dead_code)] // used by Task 2's routes.rs (ptrace family)
pub fn ptrace_attach(pid: libc::pid_t) -> io::Result<()> {
    // SAFETY: PTRACE_ATTACH ignores both the addr and data arguments; null
    // is the documented convention for an ignored ptrace argument.
    let rc = unsafe {
        libc::ptrace(
            libc::PTRACE_ATTACH,
            pid,
            ptr::null_mut::<libc::c_void>(),
            ptr::null_mut::<libc::c_void>(),
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `PTRACE_SEIZE` with no options.
#[allow(dead_code)] // used by Task 2's routes.rs (ptrace family)
pub fn ptrace_seize(pid: libc::pid_t) -> io::Result<()> {
    // SAFETY: PTRACE_SEIZE ignores addr; its data argument is an options
    // word, here 0 (no options requested).
    let rc = unsafe {
        libc::ptrace(
            libc::PTRACE_SEIZE,
            pid,
            ptr::null_mut::<libc::c_void>(),
            0usize,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `PTRACE_DETACH` with no pending signal. Not in libc 0.2.191 (PTRACE_ATTACH
/// and PTRACE_SEIZE are, PTRACE_DETACH is not); value 17 is from
/// `linux/ptrace.h`, stable since that header's introduction.
#[allow(dead_code)] // used by Task 2's routes.rs (ptrace family)
pub fn ptrace_detach(pid: libc::pid_t) -> io::Result<()> {
    const PTRACE_DETACH: libc::c_uint = 17;
    // SAFETY: PTRACE_DETACH's data argument is the signal to deliver on
    // detach (0 = none); addr is ignored for this request.
    let rc = unsafe { libc::ptrace(PTRACE_DETACH, pid, ptr::null_mut::<libc::c_void>(), 0usize) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `process_vm_readv`: read `local.len()` bytes from `remote_addr` in
/// `pid`'s address space into `local`. libc 0.2.191 has the syscall number
/// but no function wrapper.
#[allow(dead_code)] // used by Task 2's routes.rs (ptrace family)
pub fn process_vm_readv(pid: libc::pid_t, local: &mut [u8], remote_addr: u64) -> io::Result<usize> {
    let liov = libc::iovec {
        iov_base: local.as_mut_ptr() as *mut libc::c_void,
        iov_len: local.len(),
    };
    let riov = libc::iovec {
        iov_base: remote_addr as *mut libc::c_void,
        iov_len: local.len(),
    };
    // SAFETY: `liov` points at `local`, a live, correctly sized slice owned
    // by the caller for the duration of this call, and the kernel only
    // writes into it. `riov` names a remote address the caller asserts is
    // valid in `pid`'s address space; if it is not, the syscall fails with
    // EFAULT rather than faulting this process.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_process_vm_readv,
            pid,
            &liov as *const libc::iovec,
            1usize,
            &riov as *const libc::iovec,
            1usize,
            0usize,
        )
    };
    if rc >= 0 {
        Ok(rc as usize)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `process_vm_writev`: write `local` into `remote_addr` in `pid`'s address
/// space.
#[allow(dead_code)] // used by Task 2's routes.rs (ptrace family)
pub fn process_vm_writev(pid: libc::pid_t, local: &[u8], remote_addr: u64) -> io::Result<usize> {
    let liov = libc::iovec {
        iov_base: local.as_ptr() as *mut libc::c_void,
        iov_len: local.len(),
    };
    let riov = libc::iovec {
        iov_base: remote_addr as *mut libc::c_void,
        iov_len: local.len(),
    };
    // SAFETY: `liov` points at `local`, a live, correctly sized slice owned
    // by the caller; process_vm_writev only reads from it (local -> remote).
    // `riov` names the remote target the caller asserts is valid and
    // writable in `pid`'s address space.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_process_vm_writev,
            pid,
            &liov as *const libc::iovec,
            1usize,
            &riov as *const libc::iovec,
            1usize,
            0usize,
        )
    };
    if rc >= 0 {
        Ok(rc as usize)
    } else {
        Err(io::Error::last_os_error())
    }
}

// ---------------------------------------------------------------------------
// SCM_RIGHTS fd passing (ADV-02 impersonation route ii).
// ---------------------------------------------------------------------------

/// Send one fd to the peer on `sock_fd` via `SCM_RIGHTS`.
#[allow(dead_code)] // used by Task 2's routes.rs (impersonation route ii)
pub fn send_fd(sock_fd: RawFd, fd_to_send: RawFd) -> io::Result<()> {
    let iov_buf: [u8; 1] = [0];
    let mut iov = libc::iovec {
        iov_base: iov_buf.as_ptr() as *mut libc::c_void,
        iov_len: iov_buf.len(),
    };
    let mut cbuf = [0u8; 64];

    // SAFETY: `libc::msghdr` is a plain-old-data struct of integers and
    // pointers; all-zero is a valid bit pattern, and every field it needs is
    // set explicitly below before the struct is used.
    let mut msg: libc::msghdr = unsafe { core::mem::zeroed() };
    msg.msg_iov = &mut iov as *mut libc::iovec;
    msg.msg_iovlen = 1;
    msg.msg_control = cbuf.as_mut_ptr() as *mut libc::c_void;
    // SAFETY: CMSG_SPACE is a pure arithmetic helper; the size requested
    // (room for one c_int) fits well within `cbuf`'s 64 bytes.
    msg.msg_controllen =
        unsafe { libc::CMSG_SPACE(core::mem::size_of::<libc::c_int>() as libc::c_uint) } as usize;

    // SAFETY: `msg` points at `iov` and `cbuf`, both live locals sized for
    // exactly one control message carrying one fd. CMSG_FIRSTHDR/CMSG_DATA
    // walk within that same `cbuf` allocation, bounded by `msg_controllen`
    // set above. `sock_fd` is caller-owned; `fd_to_send` is read only as the
    // payload of the control message, never closed or otherwise taken by
    // this call.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len =
            libc::CMSG_LEN(core::mem::size_of::<libc::c_int>() as libc::c_uint) as usize;
        ptr::write(libc::CMSG_DATA(cmsg) as *mut libc::c_int, fd_to_send);

        let rc = libc::sendmsg(sock_fd, &msg, 0);
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Receive one fd from the peer on `sock_fd` via `SCM_RIGHTS`.
#[allow(dead_code)] // used by Task 2's routes.rs (impersonation route ii)
pub fn recv_fd(sock_fd: RawFd) -> io::Result<RawFd> {
    let mut iov_buf = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: iov_buf.as_mut_ptr() as *mut libc::c_void,
        iov_len: iov_buf.len(),
    };
    let mut cbuf = [0u8; 64];

    // SAFETY: `libc::msghdr` is a plain-old-data struct of integers and
    // pointers; all-zero is a valid bit pattern, and every field it needs is
    // set explicitly below before the struct is used.
    let mut msg: libc::msghdr = unsafe { core::mem::zeroed() };
    msg.msg_iov = &mut iov as *mut libc::iovec;
    msg.msg_iovlen = 1;
    msg.msg_control = cbuf.as_mut_ptr() as *mut libc::c_void;
    // SAFETY: same arithmetic helper as send_fd; the requested size fits
    // within `cbuf`'s 64 bytes.
    msg.msg_controllen =
        unsafe { libc::CMSG_SPACE(core::mem::size_of::<libc::c_int>() as libc::c_uint) } as usize;

    // SAFETY: `msg` points at live locals `iov` and `cbuf` sized to receive
    // at most one fd's worth of control data; recvmsg fills `msg_controllen`
    // in on return (never larger than the `cbuf` capacity given), so the
    // CMSG_FIRSTHDR walk below stays inside `cbuf`.
    unsafe {
        let rc = libc::recvmsg(sock_fd, &mut msg, 0);
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null() || (*cmsg).cmsg_type != libc::SCM_RIGHTS {
            return Err(io::Error::other("recv_fd: no SCM_RIGHTS control message"));
        }
        Ok(ptr::read(libc::CMSG_DATA(cmsg) as *const libc::c_int) as RawFd)
    }
}

// ---------------------------------------------------------------------------
// clone3 CLONE_INTO_CGROUP (ADV-02 impersonation route iv).
// ---------------------------------------------------------------------------

/// Outcome of `clone3_fork_into_cgroup`, mirroring `fork()`'s two-sided
/// return without a raw `0`/`pid` integer.
#[allow(dead_code)] // fields read by Task 2's routes.rs (impersonation route iv)
pub enum ForkResult {
    Parent {
        child_pid: libc::pid_t,
        child_pidfd: RawFd,
    },
    Child,
}

/// `clone3` with `CLONE_INTO_CGROUP | CLONE_PIDFD`, placing the new child
/// directly into the cgroup opened at `cgroup_fd` (route iv). Reimplements
/// `tools/linux-baseline/cg.c`'s `cg_clone3_into` in Rust.
#[allow(dead_code)] // used by Task 2's routes.rs (impersonation route iv)
pub fn clone3_fork_into_cgroup(cgroup_fd: RawFd) -> io::Result<ForkResult> {
    let mut child_pidfd: libc::c_int = -1;
    // SAFETY: `args` is zero-initialized (every field is an integer, so
    // all-zero is a valid clone_args) then given exactly the fields this
    // call needs: flags, exit_signal, the target cgroup fd, and a pointer to
    // `child_pidfd`, a live local on this stack frame for the duration of
    // the call.
    let mut args: libc::clone_args = unsafe { core::mem::zeroed() };
    args.flags = CLONE_INTO_CGROUP | (libc::CLONE_PIDFD as u64);
    args.exit_signal = libc::SIGCHLD as u64;
    args.cgroup = cgroup_fd as u64;
    args.pidfd = &mut child_pidfd as *mut libc::c_int as u64;

    // SAFETY: `args` is fully initialized above and `args.pidfd` points at
    // `child_pidfd`, live for the duration of this call. SYS_clone3 returns
    // 0 in the child and the child's pid in the parent, exactly like
    // fork(2); no CLONE_VM flag is set, so the child gets its own
    // copy-on-write address space and this function returns independently
    // in both processes.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_clone3,
            &mut args as *mut libc::clone_args,
            core::mem::size_of::<libc::clone_args>(),
        )
    };

    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    if rc == 0 {
        Ok(ForkResult::Child)
    } else {
        Ok(ForkResult::Parent {
            child_pid: rc as libc::pid_t,
            child_pidfd,
        })
    }
}
