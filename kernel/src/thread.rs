//! Threads with interrupt and redirect: the kernel half of POSIX signal delivery.
//!
//! `interrupt` aborts a blocking IPC (the call returns `Interrupted`, i.e. EINTR) and leaves a
//! pending redirect. The thread applies the redirect itself between `UserMode::execute` calls, so
//! the register rewrite can never race syscall entry.

use alloc::{
    collections::VecDeque,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ostd::{
    arch::cpu::context::FpuContext,
    irq::DisabledLocalIrqGuard,
    mm::{Vaddr, VmSpace},
    sync::{LocalIrqDisabled, SpinLock, WaitQueue, Waiter, Waker},
    timer::{Jiffies, register_callback_on_cpu},
    user::UserModeHooks,
};

use crate::{agent::Agent, cap::Cap, ipc::ReplyTable};

/// Why `Thread::block_until` gave up.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockError {
    /// A redirect is pending (`Thread::interrupt`).
    Interrupted,
    /// The deadline tick was reached.
    TimedOut,
}

/// Waiters with a deadline: (absolute `Jiffies` tick, waker). The timer callback wakes and drops
/// every entry whose tick has passed; `block_until` drops its own entry on every exit path.
static DEADLINES: SpinLock<Vec<(u64, Arc<Waker>)>, LocalIrqDisabled> = SpinLock::new(Vec::new());
static DEADLINE_TIMER: AtomicBool = AtomicBool::new(false);

// shortcut: the callback is registered on the CPU of the first deadline wait only; `-smp 1` makes
// that the only CPU. SMP needs one registration per CPU.
fn arm_deadline_timer() {
    if DEADLINE_TIMER.swap(true, Ordering::AcqRel) {
        return;
    }
    register_callback_on_cpu(|| {
        let now = Jiffies::elapsed().as_u64();
        DEADLINES.lock().retain(|(due, w)| {
            let pending = *due > now;
            if !pending {
                w.wake_up();
            }
            pending
        });
    });
}

/// Ktest-only hook for forcing a wake-versus-interrupt interleaving inside `block_until`: the
/// closure runs on the blocking thread when execution reaches the numbered point (1: waker
/// published; 2: condition false, about to sleep; 3: wait over, waker not yet cleared).
#[cfg(ktest)]
pub(crate) type FailHook = Arc<dyn Fn(&Thread) + Send + Sync>;
#[cfg(ktest)]
pub(crate) static FAILPOINT: SpinLock<Option<(u8, FailHook)>, LocalIrqDisabled> =
    SpinLock::new(None);

/// A handler's verdict on a faulted thread. A redirect needs no variant: the handler calls
/// `Thread::interrupt`, which aborts the wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExcReply {
    /// Re-execute the faulting instruction.
    Resume,
    /// The fault is fatal to the thread.
    Kill,
}

struct State {
    /// Where to resume user code, set by `interrupt`, consumed by `take_redirect`.
    redirect: Option<Vaddr>,
    /// The waker of the IPC this thread is blocked in, if any.
    blocked: Option<Arc<Waker>>,
    /// The address space the thread runs in, loaded on every switch to its task.
    space: Option<Arc<VmSpace>>,
    /// The handler's answer to the exception message this thread is waiting on.
    exc_reply: Option<ExcReply>,
}

type SharedFpu = Arc<SpinLock<FpuContext, LocalIrqDisabled>>;

/// The thread whose xmm/x87 state is live in the CPU. Swapped lazily in `pre_user_run`.
// shortcut: one global owner because the kernel runs `-smp 1`; SMP needs a per-CPU owner.
static FPU_OWNER: SpinLock<Option<SharedFpu>, LocalIrqDisabled> = SpinLock::new(None);

/// Ktest-only switch that turns the swap off, so a test can prove it would see a leak.
#[cfg(ktest)]
pub(crate) static FPU_SWAP_DISABLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// The next thread id. Never reused, so a tid names one thread for the life of the kernel.
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

pub struct Thread {
    /// This thread's own `Arc`, so a wait that outlives a borrow can name it weakly.
    me: Weak<Thread>,
    agent: Arc<Agent>,
    /// Kernel-assigned and stamped on every message this thread sends (IPC-03).
    tid: u64,
    /// Chosen by whoever created the thread, fixed for its life; stamped like `tid`.
    tag: u64,
    /// Calls this thread has received and not yet answered, by reply handle.
    pub(crate) replies: SpinLock<ReplyTable>,
    /// Handles of calls this thread has received whose caller was interrupted; the next receive
    /// delivers each as a Cancel message.
    pub(crate) cancels: SpinLock<VecDeque<u64>>,
    /// This thread's saved FPU/SSE state; live in the CPU only while `FPU_OWNER` points here.
    fpu: SharedFpu,
    // IRQ-safe: a timer callback may interrupt the thread.
    state: SpinLock<State, LocalIrqDisabled>,
    /// Where this thread's user exceptions are sent. Kept as the capability, not the endpoint,
    /// so revoking the grant takes effect at the next fault.
    exc_handler: SpinLock<Option<Cap>>,
    exc_wq: WaitQueue,
}

impl Thread {
    pub fn new(agent: Arc<Agent>) -> Arc<Self> {
        Self::new_tagged(agent, 0)
    }

    /// A thread whose messages carry `tag`. This is the kernel half of `thread_create` until
    /// Phase 2 adds Thread capabilities.
    pub fn new_tagged(agent: Arc<Agent>, tag: u64) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            agent,
            tid: NEXT_TID.fetch_add(1, Ordering::Relaxed),
            tag,
            replies: SpinLock::new(ReplyTable::default()),
            cancels: SpinLock::new(VecDeque::new()),
            fpu: Arc::new(SpinLock::new(FpuContext::default())),
            state: SpinLock::new(State {
                redirect: None,
                blocked: None,
                space: None,
                exc_reply: None,
            }),
            exc_handler: SpinLock::new(None),
            exc_wq: WaitQueue::new(),
        })
    }

    pub fn downgrade(&self) -> Weak<Thread> {
        self.me.clone()
    }

    pub fn agent(&self) -> &Arc<Agent> {
        &self.agent
    }

    pub fn tid(&self) -> u64 {
        self.tid
    }

    pub fn tag(&self) -> u64 {
        self.tag
    }

    pub fn set_space(&self, space: Arc<VmSpace>) {
        // Bound so the old space, if any, drops after the IRQ-disabled lock is released.
        let _old = self.state.lock().space.replace(space);
    }

    pub fn set_exception_handler(&self, cap: Cap) {
        *self.exc_handler.lock() = Some(cap);
    }

    pub fn exception_handler(&self) -> Option<Cap> {
        self.exc_handler.lock().clone()
    }

    pub fn clear_exception_reply(&self) {
        self.state.lock().exc_reply = None;
    }

    /// Called by the exception handler to answer the thread's fault message.
    pub fn reply_exception(&self, r: ExcReply) {
        self.state.lock().exc_reply = Some(r);
        self.exc_wq.wake_all();
    }

    /// Blocks until the handler replies; `Interrupted` means the handler redirected the thread.
    pub fn wait_exception_reply(&self) -> Result<ExcReply, BlockError> {
        self.block_until(&self.exc_wq, || self.state.lock().exc_reply.take(), None)
    }

    pub fn space(&self) -> Option<Arc<VmSpace>> {
        self.state.lock().space.clone()
    }

    pub fn is_blocked(&self) -> bool {
        self.state.lock().blocked.is_some()
    }

    /// Aborts any blocking IPC and makes the thread resume at `entry` before it next runs
    /// user code.
    pub fn interrupt(&self, entry: Vaddr) {
        let waker = {
            let mut s = self.state.lock();
            s.redirect = Some(entry);
            s.blocked.clone()
        };
        if let Some(w) = waker {
            w.wake_up();
        }
    }

    /// Queues a Cancel for the call parked under `handle` and wakes this thread if it is blocked,
    /// so a receive in progress delivers it. Unlike `interrupt`, nothing is redirected.
    pub(crate) fn push_cancel(&self, handle: u64) {
        self.cancels.lock().push_back(handle);
        let waker = self.state.lock().blocked.clone();
        if let Some(w) = waker {
            w.wake_up();
        }
    }

    pub fn take_redirect(&self) -> Option<Vaddr> {
        self.state.lock().redirect.take()
    }

    #[cfg(ktest)]
    fn failpoint(&self, point: u8) {
        let hook = match &*FAILPOINT.lock() {
            Some((p, f)) if *p == point => Some(f.clone()),
            _ => None,
        };
        if let Some(f) = hook {
            f(self);
        }
    }

    #[cfg(not(ktest))]
    fn failpoint(&self, _: u8) {}

    /// The one blocking primitive. Blocks on `wq` until `cond` yields a value, an interrupt is
    /// pending or arrives (`Interrupted`), or the absolute `Jiffies` tick `deadline` is reached
    /// (`TimedOut`). A condition that holds wins over both, including at the moment of giving up:
    /// OSTD evaluates `cond` before the cancel check and again after it.
    pub fn block_until<R>(
        &self,
        wq: &WaitQueue,
        cond: impl FnMut() -> Option<R>,
        deadline: Option<u64>,
    ) -> Result<R, BlockError> {
        self.block(wq, cond, deadline, true)
    }

    /// `block_until` that no interrupt can abort and with no deadline: for a caller whose request
    /// the server already holds, which must see the answer (Cancel-then-wait). Only returns once
    /// `cond` yields.
    pub fn block_until_committed<R>(&self, wq: &WaitQueue, cond: impl FnMut() -> Option<R>) -> R {
        match self.block(wq, cond, None, false) {
            Ok(r) => r,
            Err(_) => unreachable!("an uninterruptible wait without a deadline cannot give up"),
        }
    }

    fn block<R>(
        &self,
        wq: &WaitQueue,
        mut cond: impl FnMut() -> Option<R>,
        deadline: Option<u64>,
        interruptible: bool,
    ) -> Result<R, BlockError> {
        let (waiter, waker) = Waiter::new_pair();
        // Published under the same lock `interrupt` takes, so an interrupt either sees this
        // waker or is seen by the cancel check below.
        self.state.lock().blocked = Some(waker.clone());
        if let Some(due) = deadline {
            arm_deadline_timer();
            DEADLINES.lock().push((due, waker.clone()));
        }
        self.failpoint(1);
        let res = waiter.wait_until_or_cancelled(
            || {
                wq.enqueue(waiter.waker());
                let r = cond();
                if r.is_none() {
                    self.failpoint(2);
                }
                r
            },
            || {
                if !interruptible {
                    Ok(())
                } else if self.state.lock().redirect.is_some() {
                    Err(BlockError::Interrupted)
                } else if deadline.is_some_and(|due| Jiffies::elapsed().as_u64() >= due) {
                    Err(BlockError::TimedOut)
                } else {
                    Ok(())
                }
            },
        );
        self.failpoint(3);
        self.state.lock().blocked = None;
        if deadline.is_some() {
            DEADLINES.lock().retain(|(_, w)| !Arc::ptr_eq(w, &waker));
        }
        res
    }
}

impl UserModeHooks for Thread {
    fn has_kernel_event(&self) -> bool {
        self.state.lock().redirect.is_some()
    }

    /// Hands the CPU's FPU/SSE registers to this thread: if another thread owns them, save its
    /// state and load ours. Kernel code is soft-float, so nothing else touches them in between.
    fn pre_user_run(&self, _guard: &DisabledLocalIrqGuard) {
        #[cfg(ktest)]
        if FPU_SWAP_DISABLED.load(core::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let mut owner = FPU_OWNER.lock();
        if owner.as_ref().is_some_and(|o| Arc::ptr_eq(o, &self.fpu)) {
            return;
        }
        if let Some(old) = owner.as_ref() {
            old.lock().save();
        }
        self.fpu.lock().load();
        *owner = Some(self.fpu.clone());
    }
}

#[cfg(ktest)]
mod test {
    use core::sync::atomic::{AtomicBool, Ordering};

    use ostd::{
        prelude::*,
        task::{Task, TaskOptions},
        timer::Jiffies,
    };

    use super::*;
    use crate::{
        agent::Agent,
        user::{
            Exit, load_image,
            testkit::{ExitSlot, join, spawn_image},
            user_image,
        },
    };

    /// `mode | seed << 8` for `userland/tests/src/bin/fpu.rs`.
    fn arg(mode: u64, seed: u64) -> u64 {
        mode | seed << 8
    }

    /// Each thread gets its own image: a shared one would share the user stack.
    fn spawn_fpu(mode: u64, seed: u64) -> (Arc<Thread>, ExitSlot, Vaddr) {
        let img = load_image(user_image!("fpu")).unwrap();
        let t = Thread::new(Agent::new_root(1, 0));
        let exit = spawn_image(&t, &img, arg(mode, seed));
        (t, exit, img.handler)
    }

    /// Exits of two yield-mode threads with different xmm patterns, run to completion.
    fn two_yielders() -> [Exit; 2] {
        let a = spawn_fpu(1, 1);
        let b = spawn_fpu(1, 2);
        [join(&a.1), join(&b.1)]
    }

    #[ktest]
    fn fpu_context_default_mxcsr_is_0x1f80() {
        let ctx = FpuContext::default();
        let b = ctx.as_bytes();
        assert_eq!(u16::from_le_bytes([b[0], b[1]]), 0x037F, "FCW");
        assert_eq!(
            u32::from_le_bytes(b[24..28].try_into().unwrap()),
            0x1F80,
            "MXCSR"
        );
    }

    #[ktest]
    fn xmm_survives_yield_between_two_threads() {
        assert_eq!(two_yielders(), [Exit::Code(0), Exit::Code(0)]);
    }

    #[ktest]
    fn xmm_swap_disabled_is_detected() {
        struct Reenable;
        impl Drop for Reenable {
            fn drop(&mut self) {
                FPU_SWAP_DISABLED.store(false, Ordering::Relaxed);
            }
        }
        FPU_SWAP_DISABLED.store(true, Ordering::Relaxed);
        let _guard = Reenable;
        let exits = two_yielders();
        assert!(
            exits.iter().any(|e| *e != Exit::Code(0)),
            "without the swap the two threads must corrupt each other: {exits:?}"
        );
    }

    /// A kernel task blocked in `block_until` on a flag, with the levers a test pulls.
    struct Rig {
        t: Arc<Thread>,
        wq: Arc<WaitQueue>,
        flag: Arc<AtomicBool>,
        out: Arc<SpinLock<Option<Result<(), BlockError>>>>,
    }

    impl Rig {
        fn spawn(deadline: Option<u64>) -> Self {
            let t = Thread::new(Agent::new_root(1, 0));
            let wq = Arc::new(WaitQueue::new());
            let flag = Arc::new(AtomicBool::new(false));
            let out = Arc::new(SpinLock::new(None));
            let (t2, wq2, f2, o2) = (t.clone(), wq.clone(), flag.clone(), out.clone());
            TaskOptions::new(move || {
                let r = t2.block_until(&wq2, || f2.load(Ordering::Acquire).then_some(()), deadline);
                *o2.lock() = Some(r);
            })
            .spawn()
            .unwrap();
            Self { t, wq, flag, out }
        }

        fn wake(&self) {
            self.flag.store(true, Ordering::Release);
            self.wq.wake_all();
        }

        fn interrupt(&self) {
            self.t.interrupt(0x1);
        }

        /// Applies action 0 (interrupt), 1 (wake) or 2 (both).
        fn act(&self, action: u8) {
            if action != 1 {
                self.interrupt();
            }
            if action != 0 {
                self.wake();
            }
        }

        fn result(&self) -> Result<(), BlockError> {
            loop {
                if let Some(r) = self.out.lock().take() {
                    return r;
                }
                Task::yield_now();
            }
        }
    }

    /// One forced interleaving. Points 1 and 2 inject `action` from inside `block_until`. At
    /// point 3 the wait is already over, so `action` is delivered from outside while the thread
    /// is blocked and the hook injects the opposite event late: a late interrupt must not undo
    /// a wake, and a late wake must not undo an interrupt.
    fn run_case(point: u8, action: u8, deadline: u64) -> Result<(), BlockError> {
        let rig = Rig::spawn(Some(deadline));
        let target = Arc::as_ptr(&rig.t) as usize;
        let (wq, flag, t) = (rig.wq.clone(), rig.flag.clone(), rig.t.clone());
        let late = match action {
            0 => 1,
            1 => 0,
            _ => 3, // nothing
        };
        let hook_action = if point == 3 { late } else { action };
        *FAILPOINT.lock() = Some((
            point,
            Arc::new(move |me: &Thread| {
                if me as *const Thread as usize != target {
                    return;
                }
                if hook_action == 0 || hook_action == 2 {
                    t.interrupt(0x1);
                }
                if hook_action == 1 || hook_action == 2 {
                    flag.store(true, Ordering::Release);
                    wq.wake_all();
                }
            }),
        ));
        if point == 3 {
            while !rig.t.is_blocked() {
                Task::yield_now();
            }
            rig.act(action);
        }
        let r = rig.result();
        *FAILPOINT.lock() = None;
        rig.t.take_redirect();
        r
    }

    fn expected(action: u8) -> Result<(), BlockError> {
        // A satisfied condition beats a pending interrupt, so only interrupt-alone is an error.
        if action == 0 {
            Err(BlockError::Interrupted)
        } else {
            Ok(())
        }
    }

    #[ktest]
    fn block_until_failpoints_resolve_wake_versus_interrupt() {
        for point in 1..=3 {
            for action in 0..3 {
                let deadline = Jiffies::elapsed().as_u64() + 50;
                assert_eq!(
                    run_case(point, action, deadline),
                    expected(action),
                    "point {point} action {action}"
                );
            }
        }
    }

    #[ktest]
    fn block_until_interleavings_1000x() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        // counts[point - 1][action] = (ok, interrupted, timed out)
        let mut counts = [[(0u32, 0u32, 0u32); 3]; 3];
        for i in 0..1000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let (point, action) = (1 + (x % 3) as u8, ((x >> 8) % 3) as u8);
            let deadline = Jiffies::elapsed().as_u64() + 50;
            let r = run_case(point, action, deadline);
            let c = &mut counts[point as usize - 1][action as usize];
            match r {
                Ok(()) => c.0 += 1,
                Err(BlockError::Interrupted) => c.1 += 1,
                Err(BlockError::TimedOut) => c.2 += 1,
            }
            assert_eq!(
                r,
                expected(action),
                "iteration {i}: point {point} action {action}"
            );
        }
        let total =
            |f: fn(&(u32, u32, u32)) -> u32| -> u32 { counts.iter().flatten().map(f).sum() };
        println!(
            "block_until 1000x: ok={} interrupted={} timed_out={} {:?}",
            total(|c| c.0),
            total(|c| c.1),
            total(|c| c.2),
            counts
        );
        assert_eq!(total(|c| c.2), 0, "a timeout here is a lost wake");
    }

    #[ktest]
    fn block_until_times_out() {
        let wq = WaitQueue::new();
        let t = Thread::new(Agent::new_root(1, 0));

        let due = Jiffies::elapsed().as_u64() + 3;
        assert_eq!(
            t.block_until(&wq, || None::<()>, Some(due)),
            Err(BlockError::TimedOut)
        );
        assert!(Jiffies::elapsed().as_u64() >= due);
        assert!(DEADLINES.lock().is_empty(), "the deadline entry is removed");

        // A wake before the deadline returns the value and also removes the entry.
        let rig = Rig::spawn(Some(Jiffies::elapsed().as_u64() + 50));
        while !rig.t.is_blocked() {
            Task::yield_now();
        }
        rig.wake();
        assert_eq!(rig.result(), Ok(()));
        assert!(DEADLINES.lock().is_empty());

        // Without sleeping: a satisfied condition wins over a pending interrupt; otherwise the
        // interrupt wins over an unreached deadline.
        t.interrupt(0x1);
        let far = Some(Jiffies::elapsed().as_u64() + 50);
        assert_eq!(t.block_until(&wq, || Some(7), far), Ok(7));
        assert_eq!(
            t.block_until(&wq, || None::<()>, far),
            Err(BlockError::Interrupted)
        );
        t.take_redirect();
    }

    #[ktest]
    fn xmm_survives_redirect() {
        let (t3, e3, handler) = spawn_fpu(3, 3);
        let (_t1, e1, _) = spawn_fpu(1, 4);
        for _ in 0..50 {
            Task::yield_now();
        }
        t3.interrupt(handler);
        assert_eq!(join(&e3), Exit::Code(0));
        assert_eq!(join(&e1), Exit::Code(0));
    }
}
