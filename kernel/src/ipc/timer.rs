//! Timers that deliver a message: `timer_arm(ep, delay_ms, cookie)` makes the kernel send one
//! `LABEL_TIMER` message to `ep` after the delay.
//!
//! One kernel task owns the table. It sleeps in `Thread::block_until` with the earliest due tick as
//! its deadline, so the only code that runs in interrupt context is the existing `DEADLINES`
//! callback in `thread.rs`, which only wakes. The endpoint lock, the allocator and the table all
//! stay in task context.

use alloc::{
    collections::BTreeMap,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use abi::LABEL_TIMER;
use ostd::{
    mm::VmSpace,
    sync::{SpinLock, WaitQueue},
    task::TaskOptions,
    timer::Jiffies,
};

use super::{Endpoint, IpcError, endpoint};
use crate::{
    agent::{Agent, Credential},
    cap::Rights,
    thread::Thread,
};

/// Longest delay `arm` accepts, in milliseconds.
pub const MAX_DELAY_MS: u64 = abi::MAX_TIMEOUT_MS;

/// Timers one agent may have armed at once.
pub const MAX_ARMED_PER_AGENT: usize = 16;

struct Armed {
    ep: Arc<Endpoint>,
    cookie: u64,
    /// The tick the timer was due; a retry after `Full` moves the key but not this.
    due: u64,
    /// The arming agent's credential and the arming thread's tag, stamped on the message.
    sender: Credential,
    tag: u64,
    /// Counts against `MAX_ARMED_PER_AGENT`.
    agent: u64,
    /// The address space the arming thread ran in, so a dying space can drop its timers.
    #[allow(
        dead_code,
        reason = "plan 02-06 drops a dying space's timers by this field"
    )]
    owner: Option<Weak<VmSpace>>,
}

/// Armed timers by `(due tick, id)`: the first entry is the next to fire, and equal ticks fire in
/// arming order.
// shortcut: one global lock, and the task fires every due entry while holding it; fine for 16
// timers per agent on one CPU, SMP or many agents want a per-endpoint or sharded table.
static ARMED: SpinLock<BTreeMap<(u64, u64), Armed>> = SpinLock::new(BTreeMap::new());

/// Timer ids, from 1. Never reused.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The timer task sleeps here; `KICK` changing is its wake condition.
static TIMER_WQ: WaitQueue = WaitQueue::new();
static KICK: AtomicU64 = AtomicU64::new(0);
static TASK_STARTED: AtomicBool = AtomicBool::new(false);

/// Arms a timer that sends `LABEL_TIMER` with `[cookie, due, sent, id]` to `ep` once
/// `delay_ms` (1 to `MAX_DELAY_MS`) has passed, and returns its id. The message is stamped with
/// `t`'s agent credential and tag. `Invalid` for a delay out of range, `Full` when `t`'s agent
/// already has `MAX_ARMED_PER_AGENT` armed. It fires between `delay_ms + 1` and `delay_ms + 3`
/// ticks later: the extra tick covers the part of the current tick already elapsed.
pub fn arm(t: &Thread, ep: Arc<Endpoint>, delay_ms: u64, cookie: u64) -> Result<u64, IpcError> {
    if delay_ms == 0 || delay_ms > MAX_DELAY_MS {
        return Err(IpcError::Invalid);
    }
    let sender = t.agent().credential();
    let owner = t.space().map(|s| Arc::downgrade(&s));
    let due = Jiffies::elapsed()
        .as_u64()
        .saturating_add(delay_ms)
        .saturating_add(1);
    let id = {
        let mut armed = ARMED.lock();
        if armed
            .values()
            .filter(|a| a.agent == sender.agent_id)
            .count()
            >= MAX_ARMED_PER_AGENT
        {
            return Err(IpcError::Full);
        }
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        armed.insert(
            (due, id),
            Armed {
                ep,
                cookie,
                due,
                sender,
                tag: t.tag(),
                agent: sender.agent_id,
                owner,
            },
        );
        id
    };
    start_task();
    kick();
    Ok(id)
}

fn kick() {
    KICK.fetch_add(1, Ordering::Release);
    TIMER_WQ.wake_all();
}

fn start_task() {
    if TASK_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    // A thread of its own throwaway agent, used only to block: it is in no real agent's budget.
    let me = Thread::new(Agent::new_root(0, 0));
    TaskOptions::new(move || run(&me))
        .spawn()
        .expect("spawn the timer task");
}

/// Sends every entry due at `now`. An entry whose endpoint queue is full is retried at the next
/// tick, so a slow receiver delays a timer and never loses it. The whole pass holds `ARMED`
/// (preemption off), so no entry is ever out of the table while it is neither sent nor queued.
fn fire_due(now: u64) {
    let mut armed = ARMED.lock();
    let due: Vec<(u64, u64)> = armed.range(..=(now, u64::MAX)).map(|(k, _)| *k).collect();
    for key in due {
        let Some(a) = armed.remove(&key) else {
            continue;
        };
        let words = [a.cookie, a.due, now, key.1];
        if a.ep
            .push_kernel(LABEL_TIMER, a.sender, a.tag, words)
            .is_err()
        {
            armed.insert((now + 1, key.1), a);
        }
    }
}

fn run(me: &Thread) {
    loop {
        // Read before the table: an arm after this either shows in `next` below or changes `KICK`.
        let seen = KICK.load(Ordering::Acquire);
        fire_due(Jiffies::elapsed().as_u64());
        let next = ARMED.lock().keys().next().map(|k| k.0);
        // Woken (a sooner timer was armed) or timed out (one is due); both mean look again. The
        // thread is never interrupted, so there is no other outcome.
        let _ = me.block_until(
            &TIMER_WQ,
            || (KICK.load(Ordering::Acquire) != seen).then_some(()),
            next,
        );
    }
}

/// `SYS_TIMER_ARM`: arms a timer on the endpoint in `slot` (needs SEND); returns the timer id.
pub fn sys_timer_arm(t: &Thread, slot: usize, delay_ms: u64, cookie: u64) -> Result<u64, IpcError> {
    arm(t, endpoint(t, slot, Rights::SEND)?, delay_ms, cookie)
}

/// Ktest-only: timers armed and not yet sent.
#[cfg(ktest)]
pub(crate) fn armed_len() -> usize {
    ARMED.lock().len()
}

/// Ktest-only: forgets every armed timer, so none outlives its test and holds a deadline
/// registration that other tests assert is empty.
#[cfg(ktest)]
pub(crate) fn disarm_all() {
    ARMED.lock().clear();
    kick();
}

#[cfg(ktest)]
mod test {
    use abi::{LABEL_KERNEL_BASE, LABEL_TIMER};
    use alloc::vec::Vec;
    use ostd::{prelude::*, task::Task, timer::Jiffies};

    use super::*;
    use crate::{
        agent::Agent,
        cap::{Cap, Object},
        ipc::Message,
        syscall::Errno,
        user::{
            Exit, load_image,
            testkit::{join, spawn_image},
            user_image,
        },
    };

    fn now() -> u64 {
        Jiffies::elapsed().as_u64()
    }

    /// A thread tagged `tag` whose agent holds the endpoint in slot 0 with every right.
    fn rig(tag: u64) -> (Arc<Thread>, Arc<Endpoint>) {
        let agent = Agent::new_root(7, 16);
        let ep = Endpoint::new();
        let slot = agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(ep.clone()), Rights::ALL))
            .unwrap();
        assert_eq!(slot, 0);
        (Thread::new_tagged(agent, tag), ep)
    }

    /// The next message on `ep`, or a panic if none arrives within `ticks`.
    fn recv_within(ep: &Endpoint, t: &Thread, ticks: u64) -> Message {
        ep.recv_until(t, Some(now() + ticks)).expect("a message")
    }

    /// Not measured: the first timer spawns the timer task and runs every path cold, which under
    /// QEMU TCG costs extra ticks.
    fn warm_up(t: &Thread, ep: &Arc<Endpoint>) {
        arm(t, ep.clone(), 3, 0).unwrap();
        assert_eq!(recv_within(ep, t, 100).label, LABEL_TIMER);
    }

    #[ktest]
    fn timer_msg_delivers_to_endpoint() {
        let (t, ep) = rig(0x55);
        warm_up(&t, &ep);

        let id = arm(&t, ep.clone(), 10, 0xC0FFEE).unwrap();
        let m = recv_within(&ep, &t, 100);
        let (due, sent) = (m.words[1], m.words[2]);
        println!(
            "aok: timer_msg due={} sent={} late={}",
            due,
            sent,
            sent - due
        );

        assert_eq!(m.label, LABEL_TIMER);
        assert_eq!(m.words[0], 0xC0FFEE, "cookie");
        assert_eq!(m.words[3], id, "timer id");
        assert_eq!(m.sender, t.agent().credential());
        assert_eq!(m.tid, 0, "the kernel sent it, no thread did");
        assert_eq!(m.tag, 0x55, "the arming thread's tag");
        assert!(m.reply.is_none() && m.data.is_empty() && m.caps.is_empty());
        assert!(sent >= due && sent - due <= 2, "due {due} sent {sent}");
        assert_eq!(armed_len(), 0);
    }

    #[ktest]
    fn timer_msg_ring3_arm_and_recv() {
        let agent = Agent::new_root(1, 64);
        let ep = Endpoint::new();
        let send = agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(ep.clone()), Rights::SEND))
            .unwrap();
        let recv = agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(ep), Rights::RECV))
            .unwrap();
        assert_eq!((send, recv), (0, 1));
        let img = load_image(user_image!("timeout")).unwrap();
        let t = Thread::new_tagged(agent, 9);

        let start = now();
        let exit = spawn_image(&t, &img, 2);
        // A program still waiting after 200 ticks has no working timer: free it so the test fails
        // instead of hanging.
        while exit.lock().is_none() && now() - start < 200 {
            Task::yield_now();
        }
        let elapsed = now() - start;
        if exit.lock().is_none() {
            t.interrupt(img.handler);
        }
        let code = join(&exit);
        println!("aok: timer_msg ring3 20 ms elapsed={} ticks", elapsed);

        assert_eq!(code, Exit::Code(0));
        assert!((20..=40).contains(&elapsed), "elapsed {elapsed}");
        disarm_all();
    }

    #[ktest]
    fn timer_msg_limits() {
        let (t, ep) = rig(0);
        assert_eq!(arm(&t, ep.clone(), 0, 1), Err(IpcError::Invalid));
        assert_eq!(
            arm(&t, ep.clone(), MAX_DELAY_MS + 1, 1),
            Err(IpcError::Invalid)
        );
        assert_eq!(armed_len(), 0, "a refused arm leaves nothing behind");

        // A slot without SEND is EPERM; an empty slot is EBADF.
        let recv_only = t
            .agent()
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(ep.clone()), Rights::RECV))
            .unwrap();
        assert_eq!(
            sys_timer_arm(&t, recv_only, 10, 1).map_err(Errno::from),
            Err(Errno::EPERM)
        );
        assert_eq!(
            sys_timer_arm(&t, 9, 10, 1).map_err(Errno::from),
            Err(Errno::EBADF)
        );

        let mut ids = Vec::new();
        for cookie in 0..MAX_ARMED_PER_AGENT as u64 {
            ids.push(arm(&t, ep.clone(), 20, cookie).unwrap());
        }
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "ids increase: {ids:?}");
        assert_eq!(arm(&t, ep.clone(), 20, 99), Err(IpcError::Full));
        assert_eq!(
            sys_timer_arm(&t, 0, 20, 99).map_err(Errno::from),
            Err(Errno::EAGAIN)
        );

        // Another agent has its own allowance.
        let (other, other_ep) = rig(0);
        let extra = arm(&other, other_ep.clone(), 20, 7).unwrap();
        assert_eq!(recv_within(&other_ep, &other, 200).words[3], extra);

        for (cookie, id) in ids.into_iter().enumerate() {
            let m = recv_within(&ep, &t, 200);
            assert_eq!((m.words[0], m.words[3]), (cookie as u64, id));
        }
        assert_eq!(armed_len(), 0);
        // The allowance is back once they have fired.
        arm(&t, ep.clone(), 1, 5).unwrap();
        assert_eq!(recv_within(&ep, &t, 100).words[0], 5);
    }

    #[ktest]
    fn timer_msg_full_queue_retries() {
        let (t, ep) = rig(0);
        warm_up(&t, &ep);
        for i in 0..Endpoint::QUEUE_LEN as u64 {
            ep.send(&t, [i, 0, 0, 0]).unwrap();
        }
        arm(&t, ep.clone(), 1, 0xBEEF).unwrap();

        // The queue is full for 10 ticks: the timer waits, it is not dropped.
        let start = now();
        while now() - start < 10 {
            Task::yield_now();
        }
        assert_eq!(armed_len(), 1, "still pending");
        assert_eq!(ep.queue.lock().len(), Endpoint::QUEUE_LEN);

        // One receive frees a slot; the timer lands within 3 ticks.
        assert_eq!(recv_within(&ep, &t, 10).label, 0);
        let freed = now();
        while ep.queue.lock().len() < Endpoint::QUEUE_LEN && now() - freed <= 20 {
            Task::yield_now();
        }
        let waited = now() - freed;
        println!(
            "aok: timer_msg full queue delivered {} ticks after a slot freed",
            waited
        );
        assert!(waited <= 3, "waited {waited}");
        assert_eq!(armed_len(), 0);

        let mut last = None;
        for _ in 0..Endpoint::QUEUE_LEN {
            last = Some(recv_within(&ep, &t, 10));
        }
        let last = last.unwrap();
        assert_eq!((last.label, last.words[0]), (LABEL_TIMER, 0xBEEF));
    }

    #[ktest]
    fn timer_msg_label_is_reserved() {
        let (t, ep) = rig(0);
        // `send`, `call` and the ring-3 syscalls all go through `push`. It is called directly
        // because a `call` that was wrongly accepted would block this test forever.
        for label in [
            LABEL_KERNEL_BASE,
            LABEL_KERNEL_BASE + 7,
            LABEL_TIMER,
            abi::LABEL_CANCEL,
        ] {
            assert_eq!(
                ep.push(&t, label, [0; 4], Vec::new(), Vec::new(), None),
                Err(IpcError::Invalid),
                "label {label:#x}"
            );
        }
        assert_eq!(ep.queue.lock().len(), 0, "nothing was queued");

        // The label just below the range is an ordinary one.
        ep.push(
            &t,
            LABEL_KERNEL_BASE - 1,
            [0; 4],
            Vec::new(),
            Vec::new(),
            None,
        )
        .unwrap();
        assert_eq!(recv_within(&ep, &t, 10).label, LABEL_KERNEL_BASE - 1);
    }
}
