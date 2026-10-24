//! B2 spike: capability-microkernel primitives on Asterinas OSTD, plus the B3 entry checks
//! (CPU-budget scheduler, per-task address-space switch).
//! See docs/architecture/b2-spike-report.md for scope and findings.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

pub mod agent;
pub mod cap;
pub mod futex;
pub mod ipc;
pub mod sched;
pub mod syscall;
pub mod thread;
pub mod user;
pub mod vm;

use alloc::sync::Arc;

use ostd::{
    power::{ExitCode, poweroff},
    prelude::*,
    sync::SpinLock,
    task::{Task, TaskOptions},
    timer::Jiffies,
};

use crate::{
    agent::{Agent, CPU_PERIOD_TICKS},
    thread::Thread,
    user::{Exit, prog, user_image},
};

#[ostd::main]
fn kernel_main() {
    sched::install();
    TaskOptions::new(cpu_budget_check).spawn().unwrap();
}

/// Two agents each run a ring-3 image that holds a different 16-register xmm pattern in a bare
/// spin, never yielding. Only the timer can interleave them, so both threads must be charged
/// CPU, and each must still find its own pattern when redirected (KERN-01, D-03).
fn fpu_preemption_check() -> bool {
    const RUN_TICKS: u64 = 50;
    let runs = [(Agent::new_root(1, 0), 1u64), (Agent::new_root(2, 0), 2)].map(|(agent, seed)| {
        let img = user::load_image(user_image!("fpu")).unwrap();
        let t = Thread::new(agent);
        let exit: Arc<SpinLock<Option<Exit>>> = Arc::new(SpinLock::new(None));
        let (t2, i2, e) = (t.clone(), img.clone(), exit.clone());
        TaskOptions::new(move || *e.lock() = Some(user::start(&t2, &i2, 2 | seed << 8)))
            .data(t.clone())
            .spawn()
            .unwrap();
        (t, img.handler, exit)
    });

    let start = Jiffies::elapsed().as_u64();
    while Jiffies::elapsed().as_u64() < start + RUN_TICKS {
        Task::yield_now();
    }
    let ticks = [0, 1].map(|i| runs[i].0.agent().cpu_used_ticks());
    for (t, handler, _) in &runs {
        t.interrupt(*handler);
    }
    while runs.iter().any(|(_, _, e)| e.lock().is_none()) {
        Task::yield_now();
    }
    let exits_ok = runs
        .iter()
        .all(|(_, _, e)| matches!(*e.lock(), Some(Exit::Code(0))));
    let ok = exits_ok && ticks.iter().all(|&n| n > 0);
    println!(
        "aok: fpu preemption: ticks {:?}, exits ok {}",
        ticks, exits_ok
    );
    println!(
        "aok: fpu preemption check {}",
        if ok { "passed" } else { "FAILED" }
    );
    ok
}

/// The CPU-budget check that ktests cannot run, since the scheduler must be injected before
/// the first task. A root agent and a child holding 2 of its 10 ticks per period each spin in
/// their own address space, until both are redirected to exit.
fn cpu_budget_check() {
    let fpu_ok = fpu_preemption_check();
    const RUN_TICKS: u64 = 300;
    const CHILD_QUOTA: u64 = 2;
    let root = Agent::new_root(1, 0);
    let child = root.spawn_child(&[], 0, CHILD_QUOTA).unwrap();

    let start = Jiffies::elapsed().as_u64();
    let spinners = [(root.clone(), 0x40_0000), (child.clone(), 0x60_0000)].map(|(agent, code)| {
        let t = Thread::new(agent);
        let exit: Arc<SpinLock<Option<Exit>>> = Arc::new(SpinLock::new(None));
        let (t2, e) = (t.clone(), exit.clone());
        TaskOptions::new(move || {
            let space = user::load(code);
            *e.lock() = Some(user::run(&t2, &space, code + prog::SPINNER));
        })
        .data(t.clone())
        .spawn()
        .unwrap();
        (t, code, exit)
    });

    // This task is never budgeted, so it is also the idle fallback when both agents are out.
    while Jiffies::elapsed().as_u64() < start + RUN_TICKS {
        Task::yield_now();
    }
    for (t, code, _) in &spinners {
        t.interrupt(code + prog::HANDLER);
    }
    while spinners.iter().any(|(_, _, e)| e.lock().is_none()) {
        Task::yield_now();
    }
    let periods = (Jiffies::elapsed().as_u64() - start).div_ceil(CPU_PERIOD_TICKS) + 1;

    let (root_ticks, child_ticks) = (root.cpu_used_ticks(), child.cpu_used_ticks());
    println!(
        "aok: {} periods: root agent {} ticks (quota 8), child {} (quota {})",
        periods, root_ticks, child_ticks, CHILD_QUOTA
    );
    // Exiting through HANDLER with the pc inside its own SPINNER proves each thread was
    // running in its own address space when redirected; the wrong space faults instead.
    let mut ok = spinners.iter().all(|(_, code, e)| {
        let spin = code + prog::SPINNER..code + prog::SPINNER + 10;
        matches!(*e.lock(), Some(Exit::Code(pc)) if spin.contains(&pc))
    });
    ok &= child_ticks <= CHILD_QUOTA * periods
        && root_ticks <= (CPU_PERIOD_TICKS - CHILD_QUOTA) * periods;
    ok &= child_ticks > 0 && root_ticks > child_ticks;
    println!(
        "aok: cpu budget check {}",
        if ok { "passed" } else { "FAILED" }
    );
    poweroff(if ok && fpu_ok {
        ExitCode::Success
    } else {
        ExitCode::Failure
    });
}
