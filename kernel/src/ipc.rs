//! IPC endpoints. The kernel stamps the sender's credential on every message, and a blocked
//! receive can be cancelled by `Thread::interrupt`.

use alloc::{collections::VecDeque, sync::Arc};
use core::{fmt, mem::offset_of};

use abi::{IpcArgs, errno};
use ostd::{
    mm::{FallibleVmRead, FallibleVmWrite, Vaddr, VmReader, VmSpace, VmWriter},
    sync::{SpinLock, WaitQueue},
};

use crate::{
    agent::Credential,
    cap::{CapError, Object, Rights},
    thread::{Interrupted, Thread},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub sender: Credential,
    pub words: [u64; 4],
    /// Chosen by the sender and opaque to the kernel, unlike `sender`.
    pub label: u64,
    /// Present on a `call`: the server answers through it.
    pub reply: Option<Arc<ReplyWait>>,
}

/// The rendezvous between a `call`er blocked for its answer and the server that replies.
pub struct ReplyWait {
    words: SpinLock<Option<[u64; 4]>>,
    wq: WaitQueue,
}

impl ReplyWait {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            words: SpinLock::new(None),
            wq: WaitQueue::new(),
        })
    }

    /// Hands the answer to the caller and wakes it. A caller that gave up never sees it.
    pub fn reply(&self, words: [u64; 4]) {
        *self.words.lock() = Some(words);
        self.wq.wake_all();
    }
}

// Identity, not contents: two messages are equal only if they answer through the same wait.
impl PartialEq for ReplyWait {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl Eq for ReplyWait {}

impl fmt::Debug for ReplyWait {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReplyWait")
    }
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
        self.push(from, 0, words, None)
    }

    /// Queues a message and blocks until the server replies. The sender credential is taken
    /// from `from`, never from the caller's arguments.
    // shortcut: an interrupt abandons the message in the queue and the server's late reply goes
    // nowhere; 01-06 replaces this with Cancel-then-wait.
    pub fn call(&self, from: &Thread, label: u64, words: [u64; 4]) -> Result<[u64; 4], IpcError> {
        let reply = ReplyWait::new();
        self.push(from, label, words, Some(reply.clone()))?;
        Ok(from.block_on(&reply.wq, || reply.words.lock().take())?)
    }

    fn push(
        &self,
        from: &Thread,
        label: u64,
        words: [u64; 4],
        reply: Option<Arc<ReplyWait>>,
    ) -> Result<(), IpcError> {
        let mut q = self.queue.lock();
        if q.len() >= Self::QUEUE_LEN {
            return Err(IpcError::Full);
        }
        q.push_back(Message {
            sender: from.agent().credential(),
            words,
            label,
            reply,
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

fn errno_of(e: &IpcError) -> isize {
    match e {
        IpcError::Cap(CapError::NoSlot | CapError::Revoked) => errno::EBADF,
        IpcError::Cap(CapError::Rights) => errno::EPERM,
        IpcError::Cap(CapError::Full) | IpcError::WrongType => errno::EINVAL,
        IpcError::Full => errno::EAGAIN,
        IpcError::Interrupted => errno::EINTR,
    }
}

fn read_args(space: &VmSpace, at: Vaddr) -> Option<IpcArgs> {
    let mut raw = [0u8; size_of::<IpcArgs>()];
    let mut r = space.reader(at, raw.len()).ok()?;
    if r.read_fallible(&mut VmWriter::from(&mut raw[..])).ok()? != raw.len() {
        return None;
    }
    let mut q = raw
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| u64::from_le_bytes(*c));
    let mut next = || q.next().unwrap();
    Some(IpcArgs {
        label: next(),
        words: [next(), next(), next(), next()],
        send_buf: next(),
        send_len: next(),
        recv_buf: next(),
        recv_len: next(),
        caps: [next(), next(), next(), next()],
        cap_rights: [next(), next(), next(), next()],
        ncaps: next(),
        tid: next(),
        tag: next(),
        reply: next(),
    })
}

fn write_words(space: &VmSpace, args: Vaddr, words: [u64; 4]) -> Option<()> {
    let raw: [u8; 32] = core::array::from_fn(|i| words[i / 8].to_le_bytes()[i % 8]);
    let mut w = space
        .writer(args.checked_add(offset_of!(IpcArgs, words))?, raw.len())
        .ok()?;
    (w.write_fallible(&mut VmReader::from(&raw[..])).ok()? == raw.len()).then_some(())
}

/// `SYS_CALL`: sends `IpcArgs` (read from `args` in the caller's active `space`) to the endpoint
/// in `slot` and writes the reply words back to `IpcArgs.words`. Buffers and capability transfer
/// are not implemented yet, so those fields must be zero.
pub fn sys_call(t: &Thread, space: &VmSpace, slot: usize, args: Vaddr) -> isize {
    let ep = match endpoint(t, slot, Rights::SEND) {
        Ok(ep) => ep,
        Err(e) => return -errno_of(&e),
    };
    let Some(a) = read_args(space, args) else {
        return -errno::EFAULT;
    };
    if [a.send_buf, a.send_len, a.recv_buf, a.recv_len, a.ncaps]
        .iter()
        .any(|&x| x != 0)
    {
        return -errno::EINVAL;
    }
    match ep.call(t, a.label, a.words) {
        Ok(words) => match write_words(space, args, words) {
            Some(()) => 0,
            None => -errno::EFAULT,
        },
        Err(e) => -errno_of(&e),
    }
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
