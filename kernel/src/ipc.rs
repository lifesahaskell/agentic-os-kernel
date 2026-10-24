//! IPC endpoints. The kernel stamps the sender's credential on every message, and a blocked
//! receive can be cancelled by `Thread::interrupt`.

use alloc::{collections::VecDeque, sync::Arc};

use ostd::sync::{SpinLock, WaitQueue};

use crate::{
    agent::Credential,
    cap::{CapError, Object, Rights},
    thread::{Interrupted, Thread},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub sender: Credential,
    pub words: [u64; 4],
}

#[derive(Debug, PartialEq, Eq)]
pub enum IpcError {
    Cap(CapError),
    WrongType,
    Full,
    Interrupted,
}

impl From<CapError> for IpcError {
    fn from(e: CapError) -> Self {
        IpcError::Cap(e)
    }
}

impl From<Interrupted> for IpcError {
    fn from(_: Interrupted) -> Self {
        IpcError::Interrupted
    }
}

pub struct Endpoint {
    queue: SpinLock<VecDeque<Message>>,
    wq: WaitQueue,
}

impl Endpoint {
    // Bounded so a sender cannot exhaust kernel memory.
    pub const QUEUE_LEN: usize = 16;

    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            queue: SpinLock::new(VecDeque::new()),
            wq: WaitQueue::new(),
        })
    }

    pub fn send(&self, from: &Thread, words: [u64; 4]) -> Result<(), IpcError> {
        let mut q = self.queue.lock();
        if q.len() >= Self::QUEUE_LEN {
            return Err(IpcError::Full);
        }
        q.push_back(Message {
            sender: from.agent().credential(),
            words,
        });
        drop(q);
        self.wq.wake_one();
        Ok(())
    }

    pub fn recv(&self, me: &Thread) -> Result<Message, IpcError> {
        Ok(me.block_on(&self.wq, || self.queue.lock().pop_front())?)
    }
}

fn endpoint(t: &Thread, slot: usize, need: Rights) -> Result<Arc<Endpoint>, IpcError> {
    match t.agent().cspace.lock().get(slot)?.resolve(need)? {
        Object::Endpoint(ep) => Ok(ep),
        _ => Err(IpcError::WrongType),
    }
}

/// Routes this thread's user exceptions to the endpoint in `slot` (needs SEND).
pub fn sys_set_exception_handler(t: &Thread, slot: usize) -> Result<(), IpcError> {
    let cap = t.agent().cspace.lock().get(slot)?.clone();
    match cap.resolve(Rights::SEND)? {
        Object::Endpoint(_) => {
            t.set_exception_handler(cap);
            Ok(())
        }
        _ => Err(IpcError::WrongType),
    }
}

/// Send through a capability slot in the calling thread's agent.
pub fn sys_send(t: &Thread, slot: usize, words: [u64; 4]) -> Result<(), IpcError> {
    endpoint(t, slot, Rights::SEND)?.send(t, words)
}

pub fn sys_recv(t: &Thread, slot: usize) -> Result<Message, IpcError> {
    endpoint(t, slot, Rights::RECV)?.recv(t)
}

#[cfg(ktest)]
mod test {
    use ostd::{
        prelude::*,
        task::{Task, TaskOptions},
    };

    use super::*;
    use crate::{agent::Agent, cap::Cap};

    /// A server agent holding RECV and a client child agent holding SEND on one endpoint.
    fn pair() -> (Arc<Thread>, Arc<Thread>) {
        let server = Agent::new_root(42, 16);
        let slot = server
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))
            .unwrap();
        assert_eq!(slot, 0);
        let client = server.spawn_child(&[(slot, Rights::SEND)], 1, 1).unwrap();
        (Thread::new(server), Thread::new(client))
    }

    #[ktest]
    fn kernel_stamps_sender_credential() {
        let (server, client) = pair();
        sys_send(&client, 0, [1, 2, 3, 4]).unwrap();
        let m = sys_recv(&server, 0).unwrap();
        assert_eq!(m.sender, client.agent().credential());
        assert_eq!(m.sender.parent, Some(server.agent().credential().agent_id));
        assert_eq!(m.words, [1, 2, 3, 4]);
    }

    #[ktest]
    fn rights_are_enforced_and_queue_is_bounded() {
        let (_server, client) = pair();
        assert_eq!(
            sys_recv(&client, 0).err(),
            Some(IpcError::Cap(CapError::Rights))
        );
        assert_eq!(
            sys_send(&client, 5, [0; 4]).err(),
            Some(IpcError::Cap(CapError::NoSlot))
        );
        for _ in 0..Endpoint::QUEUE_LEN {
            sys_send(&client, 0, [0; 4]).unwrap();
        }
        assert_eq!(sys_send(&client, 0, [0; 4]).err(), Some(IpcError::Full));
    }

    #[ktest]
    fn interrupt_aborts_blocked_recv() {
        let (server, client) = pair();
        let result: Arc<SpinLock<Option<Result<Message, IpcError>>>> =
            Arc::new(SpinLock::new(None));

        let (t, r) = (server.clone(), result.clone());
        TaskOptions::new(move || *r.lock() = Some(sys_recv(&t, 0)))
            .spawn()
            .unwrap();
        while !server.is_blocked() {
            Task::yield_now();
        }

        server.interrupt(0x1234);
        while result.lock().is_none() {
            Task::yield_now();
        }

        assert_eq!(result.lock().take(), Some(Err(IpcError::Interrupted)));
        assert_eq!(server.take_redirect(), Some(0x1234));
        // With the interrupt consumed, the endpoint works again.
        sys_send(&client, 0, [9, 0, 0, 0]).unwrap();
        assert_eq!(sys_recv(&server, 0).unwrap().words[0], 9);
    }

    #[ktest]
    fn pending_interrupt_fails_recv_without_blocking() {
        let (server, _client) = pair();
        server.interrupt(0x1);
        assert_eq!(sys_recv(&server, 0).err(), Some(IpcError::Interrupted));
    }

    #[ktest]
    fn send_wakes_blocked_receiver() {
        let (server, client) = pair();
        let result: Arc<SpinLock<Option<Result<Message, IpcError>>>> =
            Arc::new(SpinLock::new(None));
        let (t, r) = (server.clone(), result.clone());
        TaskOptions::new(move || *r.lock() = Some(sys_recv(&t, 0)))
            .spawn()
            .unwrap();
        while !server.is_blocked() {
            Task::yield_now();
        }
        sys_send(&client, 0, [7, 0, 0, 0]).unwrap();
        while result.lock().is_none() {
            Task::yield_now();
        }
        assert_eq!(result.lock().take().unwrap().unwrap().words[0], 7);
    }
}
