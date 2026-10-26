//! The syscall boundary: raw registers are decoded once into a typed `Syscall`, and `dispatch`
//! runs it. Every ring-3 value (pointer, length, capability slot) is validated at decode time or
//! by the user-copy primitives in `vm`; nothing below this layer sees a raw register.
//!
//! ABI: `rax` is the number, arguments are `rdi, rsi, rdx, r10, r8, r9`, and a failure returns
//! `-errno` in `rax`. Numbers and errno values live in the `abi` crate.

use core::{marker::PhantomData, mem::size_of};

use abi::{
    IpcArgs, MAX_IPC_BYTES, SYS_CALL, SYS_EXIT, SYS_FUTEX, SYS_RECV, SYS_REPLY, SYS_REPLY_RECV,
    SYS_SCHED_YIELD, Timespec, USER_TOP, errno,
};
use ostd::{mm::VmSpace, task::Task, timer::Jiffies};

use crate::{
    agent::AgentError,
    cap::CapError,
    futex::{self, FutexError},
    ipc::{self, IpcError},
    thread::Thread,
    vm::copy_from_user,
};

/// A positive errno value; `dispatch` returns it negated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Errno(pub isize);

impl Errno {
    pub const EPERM: Errno = Errno(errno::EPERM);
    pub const EINTR: Errno = Errno(errno::EINTR);
    pub const EBADF: Errno = Errno(errno::EBADF);
    pub const EAGAIN: Errno = Errno(errno::EAGAIN);
    pub const ENOMEM: Errno = Errno(errno::ENOMEM);
    pub const EFAULT: Errno = Errno(errno::EFAULT);
    pub const EINVAL: Errno = Errno(errno::EINVAL);
    pub const EMFILE: Errno = Errno(errno::EMFILE);
    pub const ENOSYS: Errno = Errno(errno::ENOSYS);
    pub const EMSGSIZE: Errno = Errno(errno::EMSGSIZE);
    pub const ETIMEDOUT: Errno = Errno(errno::ETIMEDOUT);
}

impl From<CapError> for Errno {
    fn from(e: CapError) -> Self {
        match e {
            CapError::NoSlot | CapError::Revoked => Errno::EBADF,
            CapError::Rights => Errno::EPERM,
            CapError::Full => Errno::EINVAL,
        }
    }
}

impl From<IpcError> for Errno {
    fn from(e: IpcError) -> Self {
        match e {
            IpcError::Cap(c) => c.into(),
            IpcError::WrongType | IpcError::Invalid => Errno::EINVAL,
            IpcError::Full => Errno::EAGAIN,
            IpcError::NoRoom => Errno::EMFILE,
            IpcError::Interrupted => Errno::EINTR,
            IpcError::TimedOut => Errno::ETIMEDOUT,
        }
    }
}

impl From<FutexError> for Errno {
    fn from(e: FutexError) -> Self {
        Errno(e.errno())
    }
}

impl From<AgentError> for Errno {
    fn from(e: AgentError) -> Self {
        match e {
            AgentError::Budget => Errno::ENOMEM,
            AgentError::NoMemory => Errno::ENOMEM,
            AgentError::Cap(c) => c.into(),
        }
    }
}

fn in_user_space(addr: usize, len: usize) -> Result<(), Errno> {
    match addr.checked_add(len) {
        Some(end) if end <= USER_TOP => Ok(()),
        _ => Err(Errno::EFAULT),
    }
}

/// An address in user space where a `T` fits. Only the range is checked; mapping, alignment and
/// permissions are the copy's business.
pub struct UserPtr<T> {
    addr: usize,
    _t: PhantomData<fn() -> T>,
}

impl<T> UserPtr<T> {
    pub fn new(addr: usize) -> Result<Self, Errno> {
        in_user_space(addr, size_of::<T>())?;
        Ok(Self {
            addr,
            _t: PhantomData,
        })
    }

    pub fn addr(&self) -> usize {
        self.addr
    }
}

impl<T> Clone for UserPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for UserPtr<T> {}

impl<T> PartialEq for UserPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.addr == other.addr
    }
}

impl<T> Eq for UserPtr<T> {}

impl<T> core::fmt::Debug for UserPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "UserPtr({:#x})", self.addr)
    }
}

/// A byte range in user space of at most `MAX_IPC_BYTES`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserSlice {
    addr: usize,
    len: usize,
}

impl UserSlice {
    /// An empty slice is valid at any address: nothing is touched.
    pub fn new(addr: usize, len: usize) -> Result<Self, Errno> {
        if len > MAX_IPC_BYTES {
            return Err(Errno::EINVAL);
        }
        if len != 0 {
            in_user_space(addr, len)?;
        }
        Ok(Self { addr, len })
    }

    pub fn addr(&self) -> usize {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// A capability slot index, bounded at decode time. The bound is the largest cspace the kernel
/// will address; whether the slot holds anything is checked at use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle(usize);

impl Handle {
    pub const LIMIT: usize = crate::cap::CSpace::MAX_SLOTS;

    pub fn new(raw: usize) -> Result<Self, Errno> {
        if raw < Self::LIMIT {
            Ok(Self(raw))
        } else {
            Err(Errno::EBADF)
        }
    }

    pub fn index(&self) -> usize {
        self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Syscall {
    Exit(u64),
    SchedYield,
    FutexWait {
        addr: UserPtr<u32>,
        val: u32,
        /// A relative timeout; `None` waits until woken or interrupted.
        timeout: Option<UserPtr<Timespec>>,
    },
    FutexWake {
        addr: UserPtr<u32>,
        n: usize,
    },
    FutexRequeue {
        addr: UserPtr<u32>,
        n: usize,
        addr2: UserPtr<u32>,
        n2: usize,
        /// `Some` for CMP_REQUEUE: the word must still hold this value.
        cmp: Option<u32>,
    },
    Call {
        ep: Handle,
        args: UserPtr<IpcArgs>,
    },
    Recv {
        ep: Handle,
        args: UserPtr<IpcArgs>,
        /// Milliseconds to wait for a message; 0 waits until one arrives or the thread is
        /// interrupted.
        timeout_ms: u64,
    },
    Reply {
        args: UserPtr<IpcArgs>,
    },
    ReplyRecv {
        ep: Handle,
        args: UserPtr<IpcArgs>,
        /// As for `Recv`; the reply half is never timed.
        timeout_ms: u64,
    },
}

const FUTEX_WAIT: usize = 0;
const FUTEX_WAKE: usize = 1;
const FUTEX_REQUEUE: usize = 3;
const FUTEX_CMP_REQUEUE: usize = 4;
/// Ignored: futexes are keyed by physical address, so private and shared behave the same.
const FUTEX_PRIVATE_FLAG: usize = 128;

/// Decodes `rax` and the six argument registers. An unknown number or futex op is `ENOSYS`; a
/// pointer, length or slot outside its bound is `EFAULT`, `EINVAL` or `EBADF`.
pub fn decode(nr: usize, a: [usize; 6]) -> Result<Syscall, Errno> {
    Ok(match nr {
        SYS_EXIT => Syscall::Exit(a[0] as u64),
        SYS_SCHED_YIELD => Syscall::SchedYield,
        SYS_FUTEX => {
            let (val, val2, val3) = (a[2], a[3], a[5]);
            let n = val as u32 as usize;
            match a[1] & !FUTEX_PRIVATE_FLAG {
                // `a[3]` is a relative timeout as in futex(2); null waits forever. The pointer is
                // read in dispatch, so an unmapped one is EFAULT from the copy.
                FUTEX_WAIT => Syscall::FutexWait {
                    addr: UserPtr::new(a[0])?,
                    val: val as u32,
                    timeout: if val2 == 0 {
                        None
                    } else {
                        Some(UserPtr::new(val2)?)
                    },
                },
                FUTEX_WAKE => Syscall::FutexWake {
                    addr: UserPtr::new(a[0])?,
                    n,
                },
                op @ (FUTEX_REQUEUE | FUTEX_CMP_REQUEUE) => Syscall::FutexRequeue {
                    addr: UserPtr::new(a[0])?,
                    n,
                    addr2: UserPtr::new(a[4])?,
                    n2: val2,
                    cmp: (op == FUTEX_CMP_REQUEUE).then_some(val3 as u32),
                },
                _ => return Err(Errno::ENOSYS),
            }
        }
        SYS_CALL => Syscall::Call {
            ep: Handle::new(a[0])?,
            args: UserPtr::new(a[1])?,
        },
        SYS_RECV => Syscall::Recv {
            ep: Handle::new(a[0])?,
            args: UserPtr::new(a[1])?,
            timeout_ms: a[2] as u64,
        },
        SYS_REPLY => Syscall::Reply {
            args: UserPtr::new(a[0])?,
        },
        SYS_REPLY_RECV => Syscall::ReplyRecv {
            ep: Handle::new(a[0])?,
            args: UserPtr::new(a[1])?,
            timeout_ms: a[2] as u64,
        },
        _ => return Err(Errno::ENOSYS),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The value for `rax`: non-negative on success, `-errno` on failure.
    Return(isize),
    Exit(u64),
}

fn ret(r: Result<usize, Errno>) -> Outcome {
    Outcome::Return(match r {
        Ok(n) => n as isize,
        Err(e) => -e.0,
    })
}

/// Whole milliseconds in `ts`, rounded up, saturating. `EINVAL` for a negative `sec` or an `nsec`
/// outside `[0, 1e9)`.
fn timespec_ms(ts: Timespec) -> Result<u64, Errno> {
    if ts.sec < 0 || !(0..1_000_000_000).contains(&ts.nsec) {
        return Err(Errno::EINVAL);
    }
    Ok((ts.sec as u64)
        .saturating_mul(1000)
        .saturating_add((ts.nsec as u64).div_ceil(1_000_000)))
}

/// `FUTEX_WAIT`. With a timeout of `ms` the deadline is `now + ms + 1` ticks: the extra tick covers
/// the part of the current tick already elapsed, so the wait is never early. A zero timeout reads
/// the word and never blocks.
fn futex_wait(
    t: &Thread,
    space: &VmSpace,
    addr: UserPtr<u32>,
    val: u32,
    timeout: Option<UserPtr<Timespec>>,
) -> Result<(), Errno> {
    let Some(p) = timeout else {
        return Ok(futex::wait(t, space, addr.addr(), val)?);
    };
    let mut raw = [0u8; size_of::<Timespec>()];
    let src = UserSlice::new(p.addr(), size_of::<Timespec>())?;
    copy_from_user(space, &mut raw, src)?;
    let ms = timespec_ms(Timespec::from_bytes(&raw))?;
    if ms == 0 {
        return Ok(futex::probe(t, space, addr.addr(), val)?);
    }
    let deadline = Jiffies::elapsed()
        .as_u64()
        .saturating_add(ms)
        .saturating_add(1);
    Ok(futex::wait_until(
        t,
        space,
        addr.addr(),
        val,
        Some(deadline),
    )?)
}

/// Runs `call` for thread `t`, whose address space `space` is active.
pub fn dispatch(t: &Thread, space: &VmSpace, call: Syscall) -> Outcome {
    match call {
        Syscall::Exit(code) => Outcome::Exit(code),
        Syscall::SchedYield => {
            Task::yield_now();
            Outcome::Return(0)
        }
        Syscall::FutexWait { addr, val, timeout } => {
            ret(futex_wait(t, space, addr, val, timeout).map(|()| 0))
        }
        Syscall::FutexWake { addr, n } => {
            ret(futex::wake(t, space, addr.addr(), n).map_err(Errno::from))
        }
        // Return values follow futex(2): REQUEUE reports woken, CMP_REQUEUE woken plus moved.
        Syscall::FutexRequeue {
            addr,
            n,
            addr2,
            n2,
            cmp,
        } => ret(
            futex::requeue(t, space, addr.addr(), n, addr2.addr(), n2, cmp)
                .map(|(woken, moved)| if cmp.is_some() { woken + moved } else { woken })
                .map_err(Errno::from),
        ),
        Syscall::Call { ep, args } => ret(ipc::sys_call(t, space, ep, args).map(|()| 0)),
        Syscall::Recv { ep, args, .. } => ret(ipc::sys_recv_args(t, space, ep, args).map(|()| 0)),
        Syscall::Reply { args } => ret(ipc::sys_reply(t, space, args).map(|()| 0)),
        Syscall::ReplyRecv { ep, args, .. } => {
            ret(ipc::sys_reply_recv(t, space, ep, args).map(|()| 0))
        }
    }
}

#[cfg(ktest)]
mod test {
    use abi::USER_TOP;
    use ostd::prelude::*;

    use super::*;

    const KERNEL_HALF: usize = 0xffff_8000_0000_0000;

    #[ktest]
    fn decode_rejects_bad_numbers_pointers_and_lengths() {
        let z = [0usize; 6];
        let err = |nr, a| decode(nr, a).err();

        assert_eq!(err(0xFFFF, z), Some(Errno::ENOSYS));
        assert_eq!(
            err(SYS_FUTEX, [0x1000, 99, 0, 0, 0, 0]),
            Some(Errno::ENOSYS)
        );
        // A futex wait takes its timeout from a[3]: null is none, a pointer is read in dispatch.
        assert_eq!(
            decode(SYS_FUTEX, [0x1000, 0, 7, 0, 0, 0]),
            Ok(Syscall::FutexWait {
                addr: UserPtr::new(0x1000).unwrap(),
                val: 7,
                timeout: None,
            })
        );
        assert_eq!(
            decode(SYS_FUTEX, [0x1000, 0, 7, 0x2000, 0, 0]),
            Ok(Syscall::FutexWait {
                addr: UserPtr::new(0x1000).unwrap(),
                val: 7,
                timeout: Some(UserPtr::new(0x2000).unwrap()),
            })
        );
        assert_eq!(
            err(SYS_FUTEX, [0x1000, 0, 0, KERNEL_HALF, 0, 0]),
            Some(Errno::EFAULT)
        );
        assert_eq!(
            err(SYS_FUTEX, [0x1000, 0, 0, USER_TOP - 8, 0, 0]),
            Some(Errno::EFAULT)
        );
        assert_eq!(timespec_ms(Timespec { sec: 0, nsec: 0 }), Ok(0));
        assert_eq!(timespec_ms(Timespec { sec: 0, nsec: 1 }), Ok(1));
        assert_eq!(
            timespec_ms(Timespec {
                sec: 1,
                nsec: 1_000_001
            }),
            Ok(1002)
        );
        assert_eq!(
            timespec_ms(Timespec {
                sec: i64::MAX,
                nsec: 999_999_999
            }),
            Ok(u64::MAX)
        );
        assert_eq!(
            timespec_ms(Timespec { sec: -1, nsec: 0 }),
            Err(Errno::EINVAL)
        );
        assert_eq!(
            timespec_ms(Timespec { sec: 0, nsec: -1 }),
            Err(Errno::EINVAL)
        );
        assert_eq!(
            timespec_ms(Timespec {
                sec: 0,
                nsec: 1_000_000_000
            }),
            Err(Errno::EINVAL)
        );
        assert_eq!(decode(SYS_EXIT, [7, 0, 0, 0, 0, 0]), Ok(Syscall::Exit(7)));
        assert_eq!(decode(SYS_SCHED_YIELD, z), Ok(Syscall::SchedYield));

        // Pointers: kernel half, crossing the top, overflowing.
        assert_eq!(
            err(SYS_CALL, [0, KERNEL_HALF, 0, 0, 0, 0]),
            Some(Errno::EFAULT)
        );
        assert_eq!(
            err(SYS_CALL, [0, USER_TOP - 100, 0, 0, 0, 0]),
            Some(Errno::EFAULT)
        );
        assert_eq!(
            err(SYS_CALL, [0, usize::MAX, 0, 0, 0, 0]),
            Some(Errno::EFAULT)
        );
        assert_eq!(
            err(SYS_FUTEX, [KERNEL_HALF, 1, 1, 0, 0, 0]),
            Some(Errno::EFAULT)
        );
        assert_eq!(
            err(SYS_FUTEX, [USER_TOP - 2, 1, 1, 0, 0, 0]),
            Some(Errno::EFAULT)
        );
        assert_eq!(
            err(SYS_FUTEX, [0x1000, 3, 1, 1, KERNEL_HALF, 0]),
            Some(Errno::EFAULT)
        );

        // Slots.
        assert_eq!(
            err(SYS_CALL, [1 << 40, 0x1000, 0, 0, 0, 0]),
            Some(Errno::EBADF)
        );
        assert_eq!(
            err(SYS_CALL, [Handle::LIMIT, 0x1000, 0, 0, 0, 0]),
            Some(Errno::EBADF)
        );

        // Slices.
        assert_eq!(
            UserSlice::new(0x1000, MAX_IPC_BYTES + 1),
            Err(Errno::EINVAL)
        );
        assert_eq!(UserSlice::new(USER_TOP - 8, 16), Err(Errno::EFAULT));
        assert_eq!(UserSlice::new(usize::MAX - 4, 16), Err(Errno::EFAULT));
        assert!(UserSlice::new(0x1000, MAX_IPC_BYTES).is_ok());
        assert!(UserSlice::new(USER_TOP - 16, 16).is_ok());
        assert!(UserSlice::new(KERNEL_HALF, 0).is_ok());

        // Well-formed calls decode.
        assert_eq!(
            decode(SYS_CALL, [3, 0x1000, 0, 0, 0, 0]),
            Ok(Syscall::Call {
                ep: Handle::new(3).unwrap(),
                args: UserPtr::new(0x1000).unwrap(),
            })
        );
        assert_eq!(
            decode(SYS_RECV, [3, 0x1000, 0, 0, 0, 0]),
            Ok(Syscall::Recv {
                ep: Handle::new(3).unwrap(),
                args: UserPtr::new(0x1000).unwrap(),
                timeout_ms: 0,
            })
        );
        assert_eq!(
            decode(SYS_REPLY, [0x1000, 0, 0, 0, 0, 0]),
            Ok(Syscall::Reply {
                args: UserPtr::new(0x1000).unwrap(),
            })
        );
        assert_eq!(
            decode(SYS_REPLY_RECV, [3, 0x1000, 0, 0, 0, 0]),
            Ok(Syscall::ReplyRecv {
                ep: Handle::new(3).unwrap(),
                args: UserPtr::new(0x1000).unwrap(),
                timeout_ms: 0,
            })
        );
        assert_eq!(
            decode(SYS_FUTEX, [0x1000, 4 | 128, 2, 5, 0x2000, 9]),
            Ok(Syscall::FutexRequeue {
                addr: UserPtr::new(0x1000).unwrap(),
                n: 2,
                addr2: UserPtr::new(0x2000).unwrap(),
                n2: 5,
                cmp: Some(9),
            })
        );
    }

    #[ktest]
    fn decode_recv_timeout_is_bounded() {
        let at = |nr, ms| decode(nr, [3, 0x1000, ms, 0, 0, 0]);
        for nr in [SYS_RECV, SYS_REPLY_RECV] {
            let want = |timeout_ms| match nr {
                SYS_RECV => Syscall::Recv {
                    ep: Handle::new(3).unwrap(),
                    args: UserPtr::new(0x1000).unwrap(),
                    timeout_ms,
                },
                _ => Syscall::ReplyRecv {
                    ep: Handle::new(3).unwrap(),
                    args: UserPtr::new(0x1000).unwrap(),
                    timeout_ms,
                },
            };
            assert_eq!(at(nr, 0), Ok(want(0)), "nr {nr:#x}: 0 is no timeout");
            assert_eq!(at(nr, 20), Ok(want(20)));
            let max = abi::MAX_TIMEOUT_MS as usize;
            assert_eq!(at(nr, max), Ok(want(max as u64)));
            assert_eq!(at(nr, max + 1), Err(Errno::EINVAL), "nr {nr:#x}");
            assert_eq!(at(nr, usize::MAX), Err(Errno::EINVAL));
        }
    }

    /// xorshift64: deterministic, so a failing case index reproduces.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// One syscall as `run_ctx` makes it: the value `rax` would hold, or 0 for an exit.
    fn step(t: &Thread, space: &VmSpace, nr: usize, a: [usize; 6]) -> isize {
        match decode(nr, a) {
            Err(e) => -e.0,
            Ok(call) => match dispatch(t, space, call) {
                Outcome::Return(v) => v,
                Outcome::Exit(_) => 0,
            },
        }
    }

    const KNOWN: [isize; 11] = [
        errno::EPERM,
        errno::EINTR,
        errno::EBADF,
        errno::EAGAIN,
        errno::ENOMEM,
        errno::EFAULT,
        errno::EINVAL,
        errno::EMFILE,
        errno::ENOSYS,
        errno::EMSGSIZE,
        errno::ETIMEDOUT,
    ];

    #[ktest]
    fn fuzz_syscalls_never_panic() {
        use abi::USER_STACK_PAGES;
        use ostd::mm::PAGE_SIZE;

        use crate::{
            agent::Agent,
            cap::{Cap, Object, Rights},
            ipc::Endpoint,
            user::{load_image, user_image},
        };

        const ITERATIONS: usize = 100_000;
        let stack_base = abi::USER_STACK_TOP - USER_STACK_PAGES * PAGE_SIZE;
        let mapped = stack_base + 0x100;

        let img = load_image(user_image!("tracer")).unwrap();
        let agent = Agent::new_root(7, 64);
        let slot = agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::SEND))
            .unwrap();
        assert_eq!(slot, 0);
        // Slot 1 can receive, on an endpoint nothing sends to: the receive paths block, so the
        // pending interrupt below turns them into EINTR.
        let recv_slot = agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::RECV))
            .unwrap();
        assert_eq!(recv_slot, 1);
        let t = Thread::new(agent);
        img.space.activate();
        t.set_space(img.space.clone());
        // Pending, so every blocking path returns EINTR instead of hanging the fuzzer.
        t.interrupt(0x1);
        let space = &*img.space;

        // Each decode and copy error path once, with its exact errno.
        let z = [0usize; 6];
        let targeted: [(usize, [usize; 6], isize); 14] = [
            (0xFFFF, z, errno::ENOSYS),
            (SYS_FUTEX, [mapped, 99, 0, 0, 0, 0], errno::ENOSYS),
            // The timeout pointer is address 1, which is unmapped.
            (SYS_FUTEX, [mapped, 0, 0, 1, 0, 0], errno::EFAULT),
            // A zero Timespec (the stack page is zeroed) against a word holding 0: never blocks.
            (
                SYS_FUTEX,
                [mapped, 0, 0, mapped + 64, 0, 0],
                errno::ETIMEDOUT,
            ),
            // Same word, a different expected value: EAGAIN, still without blocking.
            (SYS_FUTEX, [mapped, 0, 1, mapped + 64, 0, 0], errno::EAGAIN),
            (SYS_CALL, [0, KERNEL_HALF, 0, 0, 0, 0], errno::EFAULT),
            (SYS_CALL, [0, USER_TOP - 100, 0, 0, 0, 0], errno::EFAULT),
            (SYS_CALL, [0, 0x1000, 0, 0, 0, 0], errno::EFAULT),
            (SYS_FUTEX, [KERNEL_HALF, 1, 1, 0, 0, 0], errno::EFAULT),
            (SYS_FUTEX, [mapped + 2, 1, 1, 0, 0, 0], errno::EINVAL),
            (SYS_CALL, [1 << 40, mapped, 0, 0, 0, 0], errno::EBADF),
            (SYS_CALL, [5, mapped, 0, 0, 0, 0], errno::EBADF),
            (SYS_CALL, [0, mapped, 0, 0, 0, 0], errno::EINTR),
            // A receive timeout above the cap is refused at decode, before any slot is looked up.
            (SYS_RECV, [1, mapped, 600_001, 0, 0, 0], errno::EINVAL),
        ];
        for (i, (nr, a, want)) in targeted.into_iter().enumerate() {
            assert_eq!(
                step(&t, space, nr, a),
                -want,
                "targeted case {} nr={:#x} a={:x?}",
                i,
                nr,
                a
            );
        }

        // An out-of-range nsec is EINVAL. The bytes are written first and zeroed again after, so
        // the zero-Timespec cases above and the random cases below see the page they expect.
        let ts_at = UserSlice::new(mapped + 64, size_of::<Timespec>()).unwrap();
        let bad = Timespec {
            sec: 0,
            nsec: 1_000_000_000,
        };
        crate::vm::copy_to_user(space, t.agent(), ts_at, &bad.to_bytes()).unwrap();
        assert_eq!(
            step(&t, space, SYS_FUTEX, [mapped, 0, 0, mapped + 64, 0, 0]),
            -errno::EINVAL,
            "targeted nsec = 1e9"
        );
        crate::vm::copy_to_user(space, t.agent(), ts_at, &[0u8; 16]).unwrap();

        let mut rng = Rng(0x9E3779B97F4A7C15);
        let numbers = [
            SYS_EXIT,
            SYS_SCHED_YIELD,
            SYS_FUTEX,
            SYS_CALL,
            SYS_RECV,
            SYS_REPLY,
            SYS_REPLY_RECV,
        ];
        let (mut enosys, mut efault, mut einval, mut ebadf, mut eintr) = (0, 0, 0, 0, 0);
        for i in 0..ITERATIONS {
            let nr = match rng.below(6) {
                0..=3 => numbers[rng.below(numbers.len())],
                4 => rng.below(512),
                _ => rng.next() as usize,
            };
            let mut a = z;
            for x in &mut a {
                *x = match rng.below(8) {
                    0 => 0,
                    1 => rng.below(4),
                    2 => rng.next() as usize,
                    3 | 4 => (mapped + rng.below(0x8000 - 0x200)) & !3,
                    5 => USER_TOP - rng.below(256),
                    6 => KERNEL_HALF + rng.below(4096),
                    _ => 1 << rng.below(64),
                };
            }
            if nr == SYS_FUTEX && rng.below(4) != 0 {
                a[1] = [0, 1, 3, 4, 99, 128, 129, 131, 132][rng.below(9)];
            }
            let v = step(&t, space, nr, a);
            assert!(
                v >= 0 || KNOWN.contains(&-v),
                "case {}: nr={:#x} a={:x?} returned {}",
                i,
                nr,
                a,
                v
            );
            match -v {
                errno::ENOSYS => enosys += 1,
                errno::EFAULT => efault += 1,
                errno::EINVAL => einval += 1,
                errno::EBADF => ebadf += 1,
                errno::EINTR => eintr += 1,
                _ => {}
            }
        }
        t.take_redirect();
        println!(
            "aok: fuzz n={} enosys={} efault={} einval={} ebadf={} eintr={}",
            ITERATIONS, enosys, efault, einval, ebadf, eintr
        );
    }
}
