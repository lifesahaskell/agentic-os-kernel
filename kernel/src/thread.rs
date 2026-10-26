//! Threads with interrupt and redirect: the kernel half of POSIX signal delivery.
//!
//! `interrupt` aborts a blocking IPC (the call returns `Interrupted`, i.e. EINTR) and leaves a
//! pending redirect. The thread applies the redirect itself between `UserMode::execute` calls, so
//! the register rewrite can never race syscall entry.
//!
//! Applying a redirect pushes a `RedirectFrame` (the full user context, the FPU state and the FS
//! base) onto a stack the kernel holds for the thread, and `SYS_SIGRETURN` pops it. Nothing is
//! written to user memory either way, and user code never supplies the context that comes back
//! (OD-01, OD-16).

use alloc::{
    boxed::Box,
    collections::VecDeque,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use ostd::{
    arch::cpu::context::{FpuContext, FsBase, UserContext},
    irq::DisabledLocalIrqGuard,
    mm::{Vaddr, VmSpace},
    sync::{LocalIrqDisabled, SpinLock, WaitQueue, Waiter, Waker},
    timer::{Jiffies, register_callback_on_cpu},
    user::UserModeHooks,
};

use crate::{
    agent::{Agent, Payer},
    ipc::{IpcError, ReplyTable},
    space::Space,
    user::Exit,
};

/// Why `Thread::block_until` gave up.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockError {
    /// A redirect is pending (`Thread::interrupt`), or the thread was killed.
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

/// The most frames a thread's signal stack holds (OD-01). One more `try_interrupt` is refused.
pub const MAX_FRAMES: usize = 8;

/// The bytes below `rsp` that a leaf function may use without moving `rsp` (System V red zone). A
/// handler's stack starts below them (OD-16).
pub const RED_ZONE: usize = 128;

/// `try_interrupt` found the frame stack at `MAX_FRAMES`.
#[derive(Debug, PartialEq, Eq)]
pub struct FramesFull;

/// What a redirect saved: everything `SYS_SIGRETURN` needs to put the thread back exactly as it
/// was. Lives in kernel memory only.
pub struct RedirectFrame {
    ctx: UserContext,
    fpu: FpuContext,
    fs_base: usize,
}

impl core::fmt::Debug for RedirectFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RedirectFrame")
            .field("rip", &self.ctx.rip())
            .field("rsp", &self.ctx.rsp())
            .field("fs_base", &self.fs_base)
            .finish_non_exhaustive()
    }
}

/// A frame equals only itself: the contexts it holds have no meaningful comparison, and this lets
/// `Outcome` keep its derives.
impl PartialEq for RedirectFrame {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for RedirectFrame {}

struct State {
    /// Where to resume user code, set by `interrupt`, consumed by `take_redirect`.
    redirect: Option<Vaddr>,
    /// The waker of the IPC this thread is blocked in, if any.
    blocked: Option<Arc<Waker>>,
    /// The address space the thread runs in, loaded on every switch to its task.
    space: Option<Arc<VmSpace>>,
    /// The process this thread runs in. `space` is its `vm`, kept separately so the scheduler hook
    /// and the futex code never see the process.
    process: Option<Arc<Space>>,
}

type SharedFpu = Arc<SpinLock<FpuContext, LocalIrqDisabled>>;

/// The thread whose xmm/x87 state is live in the CPU. Swapped lazily in `pre_user_run`.
// shortcut: one global owner because the kernel runs `-smp 1`; SMP needs a per-CPU owner.
static FPU_OWNER: SpinLock<Option<SharedFpu>, LocalIrqDisabled> = SpinLock::new(None);

/// Ktest-only switch that turns the swap off, so a test can prove it would see a leak.
#[cfg(ktest)]
pub(crate) static FPU_SWAP_DISABLED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Ktest-only fault injection for `restore_frame`, so a test can prove the register probe would
/// notice: 1 adds one to the saved `rbx`, 2 skips the FPU write, 0 is the real behaviour.
#[cfg(ktest)]
pub(crate) static CORRUPT_FRAME: core::sync::atomic::AtomicU8 =
    core::sync::atomic::AtomicU8::new(0);

/// The next thread id. Never reused, so a tid names one thread for the life of the kernel.
static NEXT_TID: AtomicU64 = AtomicU64::new(1);

/// Threads that exist: raised by `new_tagged`, lowered when one is dropped.
static LIVE_THREADS: AtomicUsize = AtomicUsize::new(0);

/// Threads that exist now, for the leak checks.
pub(crate) fn live_threads() -> usize {
    LIVE_THREADS.load(Ordering::Acquire)
}

/// Deadline waiters registered now. A wait that ended leaves none behind.
pub(crate) fn deadline_count() -> usize {
    DEADLINES.lock().len()
}

/// 1 when `FPU_OWNER` holds a context, 0 otherwise. The slot keeps the last user thread's context
/// until another thread runs, so it is bounded by 1, not zero.
pub(crate) fn fpu_owner_count() -> usize {
    usize::from(FPU_OWNER.lock().is_some())
}

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
    /// The user-mode FS base: saved after every return from ring 3, loaded before every entry.
    fs_base: AtomicUsize,
    /// The user address of the `u32` the kernel zeroes and futex-wakes when this thread exits, 0
    /// for none. Set at creation from `ThreadCtx.clear_tid`; taken (swapped to 0) at the exit.
    clear_tid: AtomicUsize,
    /// The signal stack: one frame per redirect applied and not yet returned from, at most
    /// `MAX_FRAMES`. IRQ-safe like `state`, because a timer callback may call `try_interrupt`.
    /// Lock order: `frames`, then `state`. Boxed so a frame is built (and `SYS_SIGRETURN` hands it
    /// over) without a copy or an allocation under this IRQ-disabled lock.
    #[allow(clippy::vec_box)]
    frames: SpinLock<Vec<Box<RedirectFrame>>, LocalIrqDisabled>,
    /// How the thread's run ended; `None` while it can still run.
    exited: SpinLock<Option<Exit>, LocalIrqDisabled>,
    /// Set by `kill`, never cleared: every blocking wait gives up and ring 3 is never entered
    /// again.
    killed: AtomicBool,
    // IRQ-safe: a timer callback may interrupt the thread.
    state: SpinLock<State, LocalIrqDisabled>,
}

impl Drop for Thread {
    fn drop(&mut self) {
        LIVE_THREADS.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Thread {
    pub fn new(agent: Arc<Agent>) -> Arc<Self> {
        Self::new_tagged(agent, 0)
    }

    /// A thread whose messages carry `tag`. This is the kernel half of `thread_create` until
    /// Phase 2 adds Thread capabilities.
    pub fn new_tagged(agent: Arc<Agent>, tag: u64) -> Arc<Self> {
        LIVE_THREADS.fetch_add(1, Ordering::AcqRel);
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            agent,
            tid: NEXT_TID.fetch_add(1, Ordering::Relaxed),
            tag,
            replies: SpinLock::new(ReplyTable::default()),
            cancels: SpinLock::new(VecDeque::new()),
            fpu: Arc::new(SpinLock::new(FpuContext::default())),
            fs_base: AtomicUsize::new(0),
            clear_tid: AtomicUsize::new(0),
            frames: SpinLock::new(Vec::with_capacity(MAX_FRAMES)),
            exited: SpinLock::new(None),
            killed: AtomicBool::new(false),
            state: SpinLock::new(State {
                redirect: None,
                blocked: None,
                space: None,
                process: None,
            }),
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

    /// Attaches the thread to `process` and its address space. For a thread that runs ring-3 code;
    /// `set_space` is for kernel-only threads that have no process.
    pub fn set_process(&self, process: Arc<Space>) {
        let vm = process.vm().clone();
        let _old = {
            let mut s = self.state.lock();
            (s.process.replace(process), s.space.replace(vm))
        };
    }

    pub fn process(&self) -> Option<Arc<Space>> {
        self.state.lock().process.clone()
    }

    /// Who pays for a copy-on-write break made on this thread's behalf: its process, whose ledger
    /// teardown refunds, or its agent when it runs no process.
    pub fn payer(&self) -> Arc<dyn Payer> {
        match self.process() {
            Some(p) => p,
            None => self.agent().clone(),
        }
    }

    pub fn set_fs_base(&self, base: usize) {
        self.fs_base.store(base, Ordering::Relaxed);
    }

    pub fn fs_base(&self) -> usize {
        self.fs_base.load(Ordering::Relaxed)
    }

    pub fn set_clear_tid(&self, va: usize) {
        self.clear_tid.store(va, Ordering::Relaxed);
    }

    pub fn clear_tid(&self) -> usize {
        self.clear_tid.load(Ordering::Relaxed)
    }

    /// Returns the `clear_tid` address and leaves 0 behind, so the exit-time clear runs at most
    /// once per thread.
    pub(crate) fn take_clear_tid(&self) -> usize {
        self.clear_tid.swap(0, Ordering::Relaxed)
    }

    /// Records how the thread's run ended.
    pub fn set_exit(&self, exit: Exit) {
        *self.exited.lock() = Some(exit);
    }

    /// Forgets a previous run's exit, for a thread that is started again.
    pub fn clear_exit(&self) {
        *self.exited.lock() = None;
    }

    pub fn exit_status(&self) -> Option<Exit> {
        *self.exited.lock()
    }

    pub fn space(&self) -> Option<Arc<VmSpace>> {
        self.state.lock().space.clone()
    }

    pub fn is_blocked(&self) -> bool {
        self.state.lock().blocked.is_some()
    }

    /// Aborts any blocking IPC and makes the thread resume at `entry` before it next runs user
    /// code, with its interrupted context pushed on its signal stack. `FramesFull` when the stack
    /// already holds `MAX_FRAMES`; nothing changes then. A redirect not yet applied is replaced, so
    /// frames plus the pending redirect never exceed `MAX_FRAMES`.
    pub fn try_interrupt(&self, entry: Vaddr) -> Result<(), FramesFull> {
        let waker = {
            let frames = self.frames.lock();
            if frames.len() >= MAX_FRAMES {
                return Err(FramesFull);
            }
            let mut s = self.state.lock();
            s.redirect = Some(entry);
            let waker = s.blocked.clone();
            drop(s);
            drop(frames);
            waker
        };
        if let Some(w) = waker {
            w.wake_up();
        }
        Ok(())
    }

    /// `try_interrupt` for kernel-side callers (ktests, boot checks) that have no errno to return:
    /// a full signal stack drops the redirect. Ring-3 paths use `try_interrupt`.
    pub fn interrupt(&self, entry: Vaddr) {
        let _ = self.try_interrupt(entry);
    }

    /// Kills the thread: every wait it is in, or enters, gives up (`Interrupted`, even an
    /// uninterruptible one), and `run_ctx` ends its run as `Exit::Killed` before it next enters
    /// ring 3. Nothing is redirected. The flag is set before the blocked waker is read, and a wait
    /// publishes its waker before it reads the flag, so a kill either finds the waker or is seen
    /// by the wait's cancel check.
    pub fn kill(&self) {
        self.killed.store(true, Ordering::SeqCst);
        let waker = self.state.lock().blocked.clone();
        if let Some(w) = waker {
            w.wake_up();
        }
    }

    pub fn is_killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }

    /// The thread's last act, run on its way out of the space: every call parked in its reply
    /// table is failed with `Dead` (the callers wake instead of waiting for an answer that cannot
    /// come), pending Cancels are dropped, and the FPU is released if the CPU still holds this
    /// thread's state.
    pub fn retire(&self) {
        // Taken out under the lock, failed after it: `fail` wakes the caller.
        let parked = self.replies.lock().drain();
        for w in parked {
            w.fail(IpcError::Dead);
        }
        self.cancels.lock().clear();
        // A dead thread holds no FPU snapshots: the frames are kernel heap, `MAX_FRAMES` at most.
        self.frames.lock().clear();
        let mut owner = FPU_OWNER.lock();
        if owner.as_ref().is_some_and(|o| Arc::ptr_eq(o, &self.fpu)) {
            *owner = None;
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

    /// Applies a pending redirect to `ctx`, the thread's registers between two `execute` calls:
    /// pushes a frame holding the interrupted context, the FPU state and the FS base, then points
    /// `ctx` at the handler (`rdi` = the interrupted `rip`) with `rsp` below the red zone. Only
    /// registers change; no user memory is written. Returns whether a redirect was applied.
    pub fn apply_redirect(&self, ctx: &mut UserContext) -> bool {
        // Only this thread takes its own redirect, so one seen pending stays pending.
        if self.state.lock().redirect.is_none() {
            return false;
        }
        // The kernel is soft-float, so while this thread owns the FPU the CPU still holds its
        // registers: save them before the copy.
        let fpu = {
            let owner = FPU_OWNER.lock();
            if owner.as_ref().is_some_and(|o| Arc::ptr_eq(o, &self.fpu)) {
                self.fpu.lock().save();
            }
            self.fpu.lock().clone()
        };
        let frame = Box::new(RedirectFrame {
            ctx: ctx.clone(),
            fpu,
            fs_base: self.fs_base(),
        });
        let mut frames = self.frames.lock();
        // `try_interrupt` refused at MAX_FRAMES under this lock, so there is room.
        let Some(handler) = self.state.lock().redirect.take() else {
            return false;
        };
        frames.push(frame);
        drop(frames);
        ctx.set_rdi(ctx.rip());
        ctx.set_rip(handler);
        // Wrapping: a result outside user space faults in the handler, which is the handler's
        // problem, not the kernel's.
        let sp = ctx.rsp().wrapping_sub(RED_ZONE) & !15;
        ctx.set_rsp(sp.wrapping_sub(8));
        true
    }

    /// Pops the newest frame, `None` when no redirect is outstanding (`SYS_SIGRETURN` is then
    /// `EINVAL`).
    pub fn pop_frame(&self) -> Option<Box<RedirectFrame>> {
        self.frames.lock().pop()
    }

    /// Puts the thread back as `f` saved it: registers (`rax` included), FS base and FPU/SSE
    /// state. The FPU owner slot is cleared when it points here, so `pre_user_run` loads the
    /// restored state instead of trusting the registers the handler left in the CPU.
    pub fn restore_frame(&self, ctx: &mut UserContext, f: Box<RedirectFrame>) {
        let RedirectFrame {
            ctx: saved,
            fpu,
            fs_base,
        } = *f;
        *ctx = saved;
        self.set_fs_base(fs_base);
        #[cfg(ktest)]
        let corrupt = CORRUPT_FRAME.load(Ordering::Relaxed);
        #[cfg(ktest)]
        if corrupt == 1 {
            let regs = ctx.general_regs_mut();
            regs.rbx = regs.rbx.wrapping_add(1);
        }
        let mut owner = FPU_OWNER.lock();
        #[cfg(ktest)]
        let skip_fpu = corrupt == 2;
        #[cfg(not(ktest))]
        let skip_fpu = false;
        // Skipping the restore also leaves the owner slot alone, so the handler's registers stay
        // live: that is the failure the control exists to show.
        if !skip_fpu {
            *self.fpu.lock() = fpu;
            if owner.as_ref().is_some_and(|o| Arc::ptr_eq(o, &self.fpu)) {
                *owner = None;
            }
        }
    }

    /// Redirects applied and not yet returned from.
    pub fn frame_depth(&self) -> usize {
        self.frames.lock().len()
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
    /// the server already holds, which must see the answer (Cancel-then-wait). Returns once `cond`
    /// yields, or `None` when the thread is killed: a kill is the one thing that ends it early.
    pub fn block_until_committed<R>(
        &self,
        wq: &WaitQueue,
        cond: impl FnMut() -> Option<R>,
    ) -> Option<R> {
        self.block(wq, cond, None, false).ok()
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
                if self.is_killed() {
                    // In every mode: a kill is not an interrupt a committed wait may ignore.
                    Err(BlockError::Interrupted)
                } else if !interruptible {
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
        self.is_killed() || self.state.lock().redirect.is_some()
    }

    /// Hands the CPU's FPU/SSE registers to this thread: if another thread owns them, save its
    /// state and load ours. Kernel code is soft-float, so nothing else touches them in between.
    fn pre_user_run(&self, _guard: &DisabledLocalIrqGuard) {
        // Before the FPU early returns: every entry to ring 3 carries this thread's own FS base.
        FsBase::new(self.fs_base()).load();
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

    /// `apply_redirect` on a hand-built context: `rsp` lands below the red zone, 16-aligned minus
    /// the return-address slot; `rdi` carries the old `rip`; the frame holds the old context, which
    /// `restore_frame` puts back. No user memory is involved: the stack address is unmapped.
    #[ktest]
    fn sigreturn_moves_rsp_below_the_red_zone() {
        const RSP: usize = 0x7000_0000 - 8;
        const RIP: usize = 0x40_1234;
        const HANDLER: usize = 0x40_2000;
        let t = Thread::new(Agent::new_root(1, 0));
        t.set_fs_base(0xAB_C000);
        let mut ctx = UserContext::default();
        ctx.set_rip(RIP);
        ctx.set_rsp(RSP);
        ctx.set_rbx(0x1111);
        ctx.set_rax(0x2222);

        assert!(!t.apply_redirect(&mut ctx), "nothing pending");
        assert_eq!(ctx.rip(), RIP);

        t.interrupt(HANDLER);
        assert!(t.apply_redirect(&mut ctx));
        assert_eq!(ctx.rsp(), ((RSP - RED_ZONE) & !15) - 8);
        assert_eq!(ctx.rdi(), RIP);
        assert_eq!(ctx.rip(), HANDLER);
        assert_eq!(t.frame_depth(), 1);
        assert!(!t.apply_redirect(&mut ctx), "the redirect was consumed");

        let frame = t.pop_frame().unwrap();
        assert_eq!(frame.ctx.rsp(), RSP);
        assert_eq!(frame.ctx.rip(), RIP);
        assert_eq!(frame.ctx.rbx(), 0x1111);
        assert_eq!(frame.fs_base, 0xAB_C000);
        assert_eq!(t.frame_depth(), 0);
        assert!(t.pop_frame().is_none());

        t.set_fs_base(0);
        t.restore_frame(&mut ctx, frame);
        assert_eq!(ctx.rsp(), RSP);
        assert_eq!(ctx.rip(), RIP);
        assert_eq!(ctx.rax(), 0x2222, "rax comes from the frame");
        assert_eq!(t.fs_base(), 0xAB_C000);
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
