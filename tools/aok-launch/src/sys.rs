//! The single raw-syscall module in this crate (D-11). Every block below
//! that opts out of the crate-wide deny carries a `// SAFETY:` comment.
//! `lib.rs` declares this module `#[allow(unsafe_code)] pub mod sys;`; every
//! other module in the crate is
//! safe Rust and calls only the wrappers this module exposes. Plans 09-02
//! (identity) and 09-03 (landlock, seccomp) take their raw syscalls from
//! here and add no raw-syscall module of their own.
//!
//! Copied and trimmed from `tools/aok-adv/src/sys.rs`. `libc` 0.2.191 supplies
//! `pidfd_info`, `PIDFD_GET_INFO`, `SO_PEERPIDFD` and `clone_args`; this module
//! hand-defines only what that libc lacks or mistypes: `CLONE_INTO_CGROUP` at
//! the correct width, and the Landlock ABI-version probe flag.
//!
//! The launcher is single-threaded. `clone3` and `fork` are called without
//! the libc atfork machinery (clone3 bypasses it outright), so the child
//! branches in `lib.rs` may allocate only because no other thread can hold
//! the allocator lock at the moment of the call. Do not call into this module
//! from a multi-threaded process and then allocate in the child.

use std::ffi::{CStr, CString};
use std::io;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::ptr;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Compile-time probes: a libc bump that renames, removes or resizes any of
// these breaks the build here instead of silently breaking the ioctl at
// runtime (aok-adv RESEARCH A6).
// ---------------------------------------------------------------------------

/// The `PIDFD_GET_INFO` ioctl request number as a plain integer. Exported so
/// the test-only shim (`src/bin/aok-shim.rs`) can match on it; the launcher
/// itself never inspects a syscall argument.
pub const PIDFD_GET_INFO_REQUEST: u64 = libc::PIDFD_GET_INFO;

const _: () = assert!(PIDFD_GET_INFO_REQUEST == 0xc040_ff0b);
const _: () = assert!(core::mem::size_of::<libc::pidfd_info>() == 64);
const _: () = assert!(core::mem::offset_of!(libc::pidfd_info, cgroupid) == 8);

#[allow(dead_code)] // referenced only to pin the symbol at compile time
const _PROBE_SO_PEERPIDFD: libc::c_int = libc::SO_PEERPIDFD;

/// libc 0.2.191 types this constant `c_int`, which cannot hold a 33-bit
/// value; redefined here at the correct width (D-11).
pub const CLONE_INTO_CGROUP: u64 = 0x2_0000_0000;

/// `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` flag.
pub const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;

// ---------------------------------------------------------------------------
// Kernel-stamped identity: PIDFD_GET_INFO on a pidfd, SO_PEERPIDFD on a
// socket, SO_PEERCRED as the older-kernel fallback.
// ---------------------------------------------------------------------------

/// The peer's kernel-stamped cgroup id and pid, read at the moment of the
/// call. Never cached.
pub struct PeerIdentity {
    pub cgroupid: u64,
    pub pid: i32,
}

/// `PIDFD_GET_INFO(cgroupid|pid)` on an already-open pidfd the caller owns.
/// Does not close `pidfd`.
pub fn pidfd_identity(pidfd: RawFd) -> io::Result<PeerIdentity> {
    // SAFETY: `info` is zero-initialized, valid for libc's `pidfd_info`
    // because every field is an integer (the all-zero bit pattern is a valid
    // value); `mask` is then set to request exactly the fields read below
    // before the ioctl call. `pidfd` is a caller-owned fd, valid for the
    // duration of this call; this function never closes it.
    unsafe {
        let mut info: libc::pidfd_info = core::mem::zeroed();
        info.mask = (libc::PIDFD_INFO_CGROUPID | libc::PIDFD_INFO_PID) as u64;
        let rc = libc::ioctl(
            pidfd,
            libc::PIDFD_GET_INFO,
            &mut info as *mut libc::pidfd_info,
        );
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(PeerIdentity {
            cgroupid: info.cgroupid,
            pid: info.pid as i32,
        })
    }
}

/// Read the identity of whoever is connected to `sock_fd` right now, via
/// `SO_PEERPIDFD` then `pidfd_identity`. The pidfd this opens is private to
/// this call and always closed before returning.
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

    let result = pidfd_identity(pidfd);
    // SAFETY: `pidfd` was opened by this function alone, just above; nothing
    // else holds or uses it, so closing it here is sound on every path.
    unsafe {
        libc::close(pidfd);
    }
    result
}

/// The peer pid from `SO_PEERCRED` on a connected Unix socket. This is the
/// older-kernel fallback for 09-02: the pid is the one the peer had at
/// connect time and can be recycled, so the caller must treat it as weaker
/// than a pidfd.
pub fn peer_cred_pid(sock_fd: RawFd) -> io::Result<i32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = core::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `sock_fd` is a caller-owned socket fd valid for this call;
    // `cred` and `len` are live locals, and getsockopt writes at most `len`
    // bytes, the exact size of `ucred`.
    let rc = unsafe {
        libc::getsockopt(
            sock_fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(cred.pid)
}

// ---------------------------------------------------------------------------
// pidfd and fd primitives.
// ---------------------------------------------------------------------------

/// `pidfd_open(pid, flags)`. libc 0.2.191 has the syscall number but no
/// function wrapper.
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

/// Wrap a raw fd this process owns into an `OwnedFd`, so callers outside
/// this module can use std's safe fd types without their own raw block.
pub fn owned_fd(fd: RawFd) -> OwnedFd {
    // SAFETY: the caller asserts ownership of `fd` and will only use it
    // again through the returned `OwnedFd`.
    unsafe { OwnedFd::from_raw_fd(fd) }
}

/// Close a raw fd this process owns.
pub fn close_fd(fd: RawFd) {
    // SAFETY: the caller asserts ownership of `fd` and does not use it
    // again after this call.
    unsafe {
        libc::close(fd);
    }
}

/// True if `fd` becomes readable (POLLIN) within `timeout`. A pidfd becomes
/// readable when the process it names exits.
pub fn poll_readable(fd: RawFd, timeout: Duration) -> io::Result<bool> {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
    loop {
        // SAFETY: `pfd` is a single live `pollfd` on this stack frame naming
        // an fd the caller owns; `poll` reads and writes only through this
        // pointer and for exactly the one element the length argument names.
        let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if rc < 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(e);
        }
        return Ok(rc > 0 && (pfd.revents & libc::POLLIN) != 0);
    }
}

/// Open a directory `O_PATH | O_DIRECTORY | O_CLOEXEC`, the handle clone3
/// takes as `cl_args.cgroup`.
pub fn open_cgroup_dir(path: &Path) -> io::Result<OwnedFd> {
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
    // SAFETY: `c` is a live NUL-terminated string for the duration of the
    // call; the flags name no creation, so no mode argument is needed.
    let fd = unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_PATH | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(owned_fd(fd))
}

// ---------------------------------------------------------------------------
// Process attributes the narrowing layers need.
// ---------------------------------------------------------------------------

/// `prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)`.
pub fn set_no_new_privs() -> io::Result<()> {
    // SAFETY: PR_SET_NO_NEW_PRIVS takes five plain integer arguments and
    // touches no memory of ours.
    let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The Landlock ABI version the running kernel supports, from
/// `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)`. This
/// is the raw probe behind the D-14 floor; it returns `Err(ENOSYS)` or
/// `Err(EOPNOTSUPP)` where Landlock is absent or disabled.
pub fn landlock_create_ruleset_version() -> io::Result<u32> {
    // SAFETY: with a NULL attr and size 0, landlock_create_ruleset reads no
    // user memory; the flags word is a plain integer.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            ptr::null::<libc::c_void>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if rc >= 0 {
        Ok(rc as u32)
    } else {
        Err(io::Error::last_os_error())
    }
}

// ---------------------------------------------------------------------------
// Process creation: clone3 into a cgroup, and the fork-then-move fallback.
// ---------------------------------------------------------------------------

/// Outcome of `clone3_fork_into_cgroup`, mirroring `fork()`'s two-sided
/// return without a raw `0`/`pid` integer.
pub enum ForkResult {
    Parent {
        child_pid: libc::pid_t,
        child_pidfd: RawFd,
    },
    Child,
}

/// `clone3` with `CLONE_INTO_CGROUP | CLONE_PIDFD`, placing the new child
/// directly into the cgroup opened at `cgroup_fd`. The child is in that
/// cgroup from its first instruction, with no window in which it lives in the
/// launcher's cgroup (T-09-01). Reimplements `tools/linux-baseline/cg.c`'s
/// `cg_clone3_into`.
pub fn clone3_fork_into_cgroup(cgroup_fd: RawFd) -> io::Result<ForkResult> {
    let mut child_pidfd: libc::c_int = -1;
    // SAFETY: `args` is zero-initialized (every field is an integer, so
    // all-zero is a valid clone_args).
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

/// Outcome of `fork_move`.
pub enum ForkMove {
    Parent {
        child_pid: libc::pid_t,
        child_pidfd: RawFd,
    },
    /// `move_errno` is 0 when the child wrote itself into the target cgroup,
    /// else the errno of the failed move. The caller reports it and exits.
    Child { move_errno: i32 },
}

/// The ENOSYS fallback for `clone3_fork_into_cgroup`: `fork`, then the child
/// writes `0` (itself) to `procs_path`, a cgroup's `cgroup.procs`. Unlike
/// clone3 this leaves a window between the fork and the move in which the
/// child lives in the launcher's cgroup. The child does only
/// async-signal-safe work here: open, write, close, no allocation.
pub fn fork_move(procs_path: &CStr) -> io::Result<ForkMove> {
    // SAFETY: fork() takes no arguments. The launcher is single-threaded, so
    // the child inherits no lock held by another thread; the child branch
    // below calls only open/write/close before returning to the caller.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(io::Error::last_os_error());
    }
    if pid == 0 {
        // SAFETY: `procs_path` is a live NUL-terminated string (the child
        // has a copy-on-write copy of the parent's memory); the buffer is a
        // static byte string of the length passed; `fd` is closed only if
        // the open succeeded.
        let move_errno = unsafe {
            let fd = libc::open(procs_path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if fd < 0 {
                *libc::__errno_location()
            } else {
                let buf = b"0";
                let n = libc::write(fd, buf.as_ptr() as *const libc::c_void, buf.len());
                let e = if n == buf.len() as isize {
                    0
                } else {
                    *libc::__errno_location()
                };
                libc::close(fd);
                e
            }
        };
        return Ok(ForkMove::Child { move_errno });
    }
    match pidfd_open(pid, 0) {
        Ok(child_pidfd) => Ok(ForkMove::Parent {
            child_pid: pid,
            child_pidfd,
        }),
        Err(e) => {
            // The child exists but cannot be tracked by pidfd; reap it so
            // nothing is left behind. It is blocked on nothing, so it runs
            // on to exec; kill it first.
            // SAFETY: `pid` is the child this call just forked.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
            let _ = wait_status(pid);
            Err(e)
        }
    }
}

/// A prepared `execvp` call. Built before `clone3`/`fork` so the child
/// branch performs no allocation to reach `execvp`.
pub struct ExecPlan {
    _args: Vec<CString>,
    ptrs: Vec<*const libc::c_char>,
}

impl ExecPlan {
    /// `EINVAL` if any argument contains a NUL byte or `args` is empty.
    pub fn new(args: &[String]) -> io::Result<ExecPlan> {
        if args.is_empty() {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        let owned: Vec<CString> = args
            .iter()
            .map(|a| CString::new(a.as_bytes()))
            .collect::<Result<_, _>>()
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
        let mut ptrs: Vec<*const libc::c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());
        Ok(ExecPlan { _args: owned, ptrs })
    }

    /// `execvp(argv[0], argv)`. Returns only on failure, with the errno.
    pub fn exec(&self) -> io::Error {
        // SAFETY: `ptrs[0]` points at a live NUL-terminated string owned by
        // `self._args`, and `ptrs` is a NULL-terminated array of pointers to
        // such strings, all alive for this call.
        unsafe {
            libc::execvp(self.ptrs[0], self.ptrs.as_ptr());
        }
        io::Error::last_os_error()
    }
}

/// `_exit(code)`: leave a child that must not run atexit handlers or flush
/// the parent's buffered stdio.
pub fn exit_now(code: i32) -> ! {
    // SAFETY: _exit takes a plain integer and never returns.
    unsafe { libc::_exit(code) }
}

/// Reap `pid` and return the exit code the launcher reports for it: the
/// exit status if it exited, else `128 + signal` if it was killed.
pub fn wait_status(pid: libc::pid_t) -> io::Result<i32> {
    let mut status: libc::c_int = 0;
    loop {
        // SAFETY: `status` is a live local written only within its own size;
        // `pid` is a child of this process; flags 0 requests a blocking wait.
        let rc = unsafe { libc::waitpid(pid, &mut status, 0) };
        if rc >= 0 {
            break;
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(libc::EINTR) {
            return Err(e);
        }
    }
    if libc::WIFEXITED(status) {
        Ok(libc::WEXITSTATUS(status))
    } else if libc::WIFSIGNALED(status) {
        Ok(128 + libc::WTERMSIG(status))
    } else {
        Ok(status)
    }
}
