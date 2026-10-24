//! Threads with interrupt and redirect: the kernel half of POSIX signal delivery.
//!
//! `interrupt` aborts a blocking IPC (the call returns `Interrupted`, i.e. EINTR) and leaves a
//! pending redirect. The thread applies the redirect itself between `UserMode::execute` calls, so
//! the register rewrite can never race syscall entry.

use alloc::sync::Arc;

use ostd::{
    mm::{Vaddr, VmSpace},
    sync::{LocalIrqDisabled, SpinLock, WaitQueue, Waiter, Waker},
    user::UserModeHooks,
};

use crate::agent::Agent;

#[derive(Debug, PartialEq, Eq)]
pub struct Interrupted;

struct State {
    /// Where to resume user code, set by `interrupt`, consumed by `take_redirect`.
    redirect: Option<Vaddr>,
    /// The waker of the IPC this thread is blocked in, if any.
    blocked: Option<Arc<Waker>>,
    /// The address space the thread runs in, loaded on every switch to its task.
    space: Option<Arc<VmSpace>>,
}

pub struct Thread {
    agent: Arc<Agent>,
    // IRQ-safe: a timer callback may interrupt the thread.
    state: SpinLock<State, LocalIrqDisabled>,
}

impl Thread {
    pub fn new(agent: Arc<Agent>) -> Arc<Self> {
        Arc::new(Self {
            agent,
            state: SpinLock::new(State {
                redirect: None,
                blocked: None,
                space: None,
            }),
        })
    }

    pub fn agent(&self) -> &Arc<Agent> {
        &self.agent
    }

    pub fn set_space(&self, space: Arc<VmSpace>) {
        // Bound so the old space, if any, drops after the IRQ-disabled lock is released.
        let _old = self.state.lock().space.replace(space);
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
}
