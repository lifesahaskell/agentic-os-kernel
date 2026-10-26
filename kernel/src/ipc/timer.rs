//! Timers that deliver a message: `timer_arm(ep, delay_ms, cookie)` makes the kernel send one
//! `LABEL_TIMER` message to `ep` after the delay.

use alloc::sync::Arc;

use super::{Endpoint, IpcError, endpoint};
use crate::{cap::Rights, thread::Thread};

/// Longest delay `arm` accepts, in milliseconds.
pub const MAX_DELAY_MS: u64 = abi::MAX_TIMEOUT_MS;

/// Timers one agent may have armed at once.
pub const MAX_ARMED_PER_AGENT: usize = 16;

/// RED stub: nothing is armed yet.
pub fn arm(_t: &Thread, _ep: Arc<Endpoint>, _delay_ms: u64, _cookie: u64) -> Result<u64, IpcError> {
    Err(IpcError::Invalid)
}

/// `SYS_TIMER_ARM`: arms a timer on the endpoint in `slot` (needs SEND); returns the timer id.
pub fn sys_timer_arm(t: &Thread, slot: usize, delay_ms: u64, cookie: u64) -> Result<u64, IpcError> {
    arm(t, endpoint(t, slot, Rights::SEND)?, delay_ms, cookie)
}

/// Ktest-only: timers armed and not yet sent.
#[cfg(ktest)]
pub(crate) fn armed_len() -> usize {
    0
}

/// Ktest-only: forgets every armed timer.
#[cfg(ktest)]
pub(crate) fn disarm_all() {}

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
