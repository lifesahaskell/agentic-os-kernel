//! Threads with interrupt and redirect: the kernel half of POSIX signal delivery.
//!
//! `interrupt` aborts a blocking IPC (the call returns `Interrupted`, i.e. EINTR) and leaves a
//! pending redirect. The thread applies the redirect itself between `UserMode::execute` calls, so
//! the register rewrite can never race syscall entry.

use alloc::sync::Arc;

use ostd::{
    arch::cpu::context::FpuContext,
    irq::DisabledLocalIrqGuard,
    mm::{Vaddr, VmSpace},
    sync::{LocalIrqDisabled, SpinLock, WaitQueue, Waiter, Waker},
    user::UserModeHooks,
};

use crate::{agent::Agent, cap::Cap};

#[derive(Debug, PartialEq, Eq)]
pub struct Interrupted;

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

pub struct Thread {
    agent: Arc<Agent>,
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
        Arc::new(Self {
            agent,
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

    pub fn agent(&self) -> &Arc<Agent> {
        &self.agent
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
    pub fn wait_exception_reply(&self) -> Result<ExcReply, Interrupted> {
        self.block_on(&self.exc_wq, || self.state.lock().exc_reply.take())
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

    pub fn take_redirect(&self) -> Option<Vaddr> {
        self.state.lock().redirect.take()
    }

    /// Blocks on `wq` until `cond` yields a value, or fails with `Interrupted` if an interrupt is
    /// pending or arrives while blocked.
    pub fn block_on<R>(
        &self,
        wq: &WaitQueue,
        mut cond: impl FnMut() -> Option<R>,
    ) -> Result<R, Interrupted> {
        let (waiter, waker) = Waiter::new_pair();
        // Published under the same lock `interrupt` takes, so an interrupt either sees this
        // waker or is seen by the cancel check below.
        self.state.lock().blocked = Some(waker);
        let res = waiter.wait_until_or_cancelled(
            || {
                wq.enqueue(waiter.waker());
                cond()
            },
            || match self.state.lock().redirect {
                Some(_) => Err(Interrupted),
                None => Ok(()),
            },
        );
        self.state.lock().blocked = None;
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
    use core::sync::atomic::Ordering;

    use ostd::{prelude::*, task::Task};

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
