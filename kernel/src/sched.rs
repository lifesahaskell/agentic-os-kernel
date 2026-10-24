//! The CPU-budget scheduler and the per-task address-space switch, both built on OSTD's public
//! injection points (`inject_scheduler`, `inject_post_schedule_handler`).
//!
//! A task whose data is an `Arc<Thread>` runs in that thread's agent and address space. Other
//! tasks are kernel tasks: never budgeted, and they keep whichever space is active.

use alloc::{boxed::Box, collections::VecDeque, sync::Arc};

use ostd::{
    cpu::CpuId,
    sync::{LocalIrqDisabled, SpinLock},
    task::{
        Task, inject_post_schedule_handler,
        scheduler::{
            EnqueueFlags, LocalRunQueue, Scheduler, UpdateFlags, enable_preemption_on_cpu,
            inject_scheduler,
        },
    },
    timer::Jiffies,
};

use crate::thread::Thread;

/// Installs both hooks and turns on tick preemption. Must run before the first task is
/// spawned, so only the boot path can call it (not a ktest: the test runner is a task).
pub fn install() {
    inject_scheduler(Box::leak(Box::new(BudgetScheduler::default())));
    install_space_switch();
    enable_preemption_on_cpu();
}

/// Idempotent, so ktests can call it.
pub fn install_space_switch() {
    inject_post_schedule_handler(activate_current_space);
}

fn thread_of(task: &Task) -> Option<&Arc<Thread>> {
    task.data().downcast_ref()
}

/// Loads the incoming task's address space before it runs again, whether it resumes in user
/// mode or in a syscall that touches user memory.
fn activate_current_space() {
    let Some(task) = Task::current() else { return };
    if let Some(space) = thread_of(&task).and_then(|t| t.space()) {
        space.activate();
    }
}

fn now() -> u64 {
    Jiffies::elapsed().as_u64()
}

fn eligible(task: &Task, now: u64) -> bool {
    thread_of(task).is_none_or(|t| t.agent().has_cpu(now))
}

/// Round-robin with a one-tick slice over tasks whose agent still has CPU budget this period.
///
/// An out-of-budget thread is switched away from only if another task is eligible, so the boot
/// path keeps a kernel task (never budgeted) runnable as the idle fallback.
// shortcut: one run queue with every task on the boot CPU; per-CPU queues for SMP.
pub struct BudgetScheduler {
    rq: SpinLock<RunQueue, LocalIrqDisabled>,
}

impl Default for BudgetScheduler {
    fn default() -> Self {
        Self {
            rq: SpinLock::new(RunQueue::default()),
        }
    }
}

#[derive(Default)]
struct RunQueue {
    current: Option<Arc<Task>>,
    queue: VecDeque<Arc<Task>>,
}

impl RunQueue {
    fn update(&mut self, flags: UpdateFlags, now: u64) -> bool {
        let others = self.queue.iter().any(|t| eligible(t, now));
        match (flags, &self.current) {
            (UpdateFlags::Tick, Some(cur)) => {
                if let Some(t) = thread_of(cur) {
                    t.agent().charge_tick(now);
                }
                others || !eligible(cur, now)
            }
            (UpdateFlags::Tick, None) => false,
            _ => others,
        }
    }

    fn pick(&mut self, now: u64) -> Option<&Arc<Task>> {
        let i = self.queue.iter().position(|t| eligible(t, now))?;
        let next = self.queue.remove(i).unwrap();
        if let Some(prev) = self.current.replace(next) {
            self.queue.push_back(prev);
        }
        self.current.as_ref()
    }
}

impl LocalRunQueue for RunQueue {
    fn current(&self) -> Option<&Arc<Task>> {
        self.current.as_ref()
    }

    fn update_current(&mut self, flags: UpdateFlags) -> bool {
        self.update(flags, now())
    }

    fn try_pick_next(&mut self) -> Option<&Arc<Task>> {
        self.pick(now())
    }

    fn dequeue_current(&mut self) -> Option<Arc<Task>> {
        self.current
            .take()
            .inspect(|t| t.schedule_info().cpu.set_to_none())
    }
}

impl Scheduler for BudgetScheduler {
    fn enqueue(&self, task: Arc<Task>, _flags: EnqueueFlags) -> Option<CpuId> {
        let cpu = CpuId::bsp();
        // Same handshake as OSTD's FIFO scheduler: a task woken before it finished switching
        // out is still `current`, and must not be queued twice.
        let still_current = task.schedule_info().cpu.set_if_is_none(cpu).is_err();
        let mut rq = self.rq.lock();
        if still_current && task.schedule_info().cpu.set_if_is_none(cpu).is_err() {
            return None;
        }
        rq.queue.push_back(task);
        None
    }

    fn local_rq_with(&self, f: &mut dyn FnMut(&dyn LocalRunQueue)) {
        f(&*self.rq.lock());
    }

    fn mut_local_rq_with(&self, f: &mut dyn FnMut(&mut dyn LocalRunQueue)) {
        f(&mut *self.rq.lock());
    }
}

#[cfg(ktest)]
mod test {
    use alloc::vec::Vec;

    use ostd::{
        prelude::*,
        task::{Task, TaskOptions},
    };

    use super::*;
    use crate::{
        agent::{Agent, CPU_PERIOD_TICKS},
        user::{self, Exit, prog},
    };

    fn task(thread: Option<Arc<Thread>>) -> Arc<Task> {
        let opts = TaskOptions::new(|| {});
        let opts = match thread {
            Some(t) => opts.data(t),
            None => opts,
        };
        Arc::new(opts.build().unwrap())
    }

    fn is(picked: Option<&Arc<Task>>, want: &Arc<Task>) -> bool {
        picked.is_some_and(|p| Arc::ptr_eq(p, want))
    }

    #[ktest]
    fn out_of_budget_agent_is_skipped_until_the_next_period() {
        let root = Agent::new_root(1, 0);
        let child = root.spawn_child(&[], 0, 2).unwrap();
        let budgeted = task(Some(Thread::new(child.clone())));
        let kernel = task(None);
        let mut rq = RunQueue::default();
        rq.queue.extend([budgeted.clone(), kernel.clone()]);

        let now = 5 * CPU_PERIOD_TICKS;
        assert!(is(rq.pick(now), &budgeted));
        assert!(
            rq.update(UpdateFlags::Tick, now),
            "round-robin: kernel task is waiting"
        );
        assert!(rq.update(UpdateFlags::Tick, now + 1));
        assert_eq!(child.cpu_used_ticks(), 2);

        assert!(is(rq.pick(now + 2), &kernel));
        assert!(
            !rq.update(UpdateFlags::Tick, now + 2),
            "the only waiting task is out of budget"
        );
        assert!(rq.pick(now + 3).is_none());
        assert!(!rq.update(UpdateFlags::Yield, now + 3));

        let next_period = now + CPU_PERIOD_TICKS;
        assert!(rq.update(UpdateFlags::Yield, next_period));
        assert!(is(rq.pick(next_period), &budgeted));
        assert_eq!(child.cpu_used_ticks(), 2, "kernel tasks are never charged");
    }

    #[ktest]
    fn each_task_resumes_in_its_own_address_space() {
        install_space_switch();
        // The code sits at a different address in each space, so a thread resumed under the
        // other space's page table faults on its next instruction fetch.
        let codes = [0x40_0000, 0x60_0000];
        let results: Vec<Arc<SpinLock<Option<Exit>>>> = codes
            .iter()
            .map(|_| Arc::new(SpinLock::new(None)))
            .collect();
        for (&code, r) in codes.iter().zip(&results) {
            let t = Thread::new(Agent::new_root(1, 0));
            let (t2, r) = (t.clone(), r.clone());
            TaskOptions::new(move || {
                let space = user::load(code);
                *r.lock() = Some(user::run(&t2, &space, code + prog::YIELDER));
            })
            .data(t)
            .spawn()
            .unwrap();
        }
        while results.iter().any(|r| r.lock().is_none()) {
            Task::yield_now();
        }
        for r in &results {
            assert_eq!(r.lock().take(), Some(Exit::Code(0)));
        }
    }
}
