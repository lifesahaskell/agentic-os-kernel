//! IPC endpoints. The kernel stamps the sender's credential, tid and tag on every message, and a
//! blocked receive can be cancelled by `Thread::interrupt`. A ring-3 server answers a call
//! through a reply handle the kernel parks in its own thread's `ReplyTable`.

use alloc::{
    collections::VecDeque,
    sync::{Arc, Weak},
    vec,
    vec::Vec,
};
use core::{
    fmt,
    mem::offset_of,
    sync::atomic::{AtomicU32, Ordering},
};

use abi::{IpcArgs, LABEL_CANCEL};
use ostd::{
    mm::VmSpace,
    sync::{SpinLock, WaitQueue},
};

use crate::{
    agent::Credential,
    cap::{CapError, Object, Rights},
    syscall::{Errno, Handle, UserPtr, UserSlice},
    thread::{BlockError, Thread},
    vm::{copy_from_user, copy_to_user},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub sender: Credential,
    /// The sending thread's id and creation-time tag, stamped by the kernel like `sender`.
    pub tid: u64,
    pub tag: u64,
    pub words: [u64; 4],
    /// The bytes of the sender's `send_buf`, copied at send time.
    pub data: Vec<u8>,
    /// Chosen by the sender and opaque to the kernel, unlike `sender`.
    pub label: u64,
    /// Present on a `call`: the server answers through it.
    pub reply: Option<Arc<ReplyWait>>,
}

/// Where a call is. Changes under the endpoint's queue lock (Queued to Received) or the state
/// lock alone (the rest), so the caller's interrupt sees exactly one of them.
enum ReplyState {
    /// In the endpoint queue; no server has it.
    Queued,
    /// A server holds it. `server` and its reply handle are known when it was received into a
    /// reply table (`recv_parked`), `None` when a kernel task took the message directly.
    Received(Option<(Weak<Thread>, u64)>),
    Replied(([u64; 4], Vec<u8>)),
    /// The caller has the answer.
    Done,
}

/// The rendezvous between a `call`er blocked for its answer and the server that replies.
pub struct ReplyWait {
    state: SpinLock<ReplyState>,
    caller: Weak<Thread>,
    wq: WaitQueue,
}

impl ReplyWait {
    fn new(caller: Weak<Thread>) -> Arc<Self> {
        Arc::new(Self {
            state: SpinLock::new(ReplyState::Queued),
            caller,
            wq: WaitQueue::new(),
        })
    }

    /// Hands the answer to the caller and wakes it. An answer for a caller that no longer exists
    /// is dropped.
    pub fn reply(&self, words: [u64; 4]) {
        self.reply_with(words, Vec::new());
    }

    pub fn reply_with(&self, words: [u64; 4], data: Vec<u8>) {
        if self.caller.strong_count() == 0 {
            return;
        }
        *self.state.lock() = ReplyState::Replied((words, data));
        self.wq.wake_all();
    }

    fn take_answer(&self) -> Option<([u64; 4], Vec<u8>)> {
        let mut s = self.state.lock();
        match core::mem::replace(&mut *s, ReplyState::Done) {
            ReplyState::Replied(a) => Some(a),
            other => {
                *s = other;
                None
            }
        }
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

/// Most calls a server thread may hold received-but-unanswered.
pub const MAX_REPLIES: usize = 256;

/// Source of reply-handle generations. Global, so a handle is valid in exactly one table, the
/// one that issued it: another thread's table never holds the same generation.
// shortcut: 32 bits wrap after 4e9 parked calls, and a handle that old could then alias a live
// one; widen the handle before a server can run that long.
static NEXT_REPLY_GEN: AtomicU32 = AtomicU32::new(1);

/// The calls a thread has received and not yet answered. A handle is `generation << 32 | index`.
#[derive(Default)]
pub struct ReplyTable {
    slots: Vec<Option<(u32, Arc<ReplyWait>)>>,
}

impl ReplyTable {
    /// Parks `w` and returns its handle, or `None` when `MAX_REPLIES` are already parked.
    fn insert(&mut self, w: Arc<ReplyWait>) -> Option<u64> {
        let index = match self.slots.iter().position(Option::is_none) {
            Some(i) => i,
            None if self.slots.len() < MAX_REPLIES => {
                self.slots.push(None);
                self.slots.len() - 1
            }
            None => return None,
        };
        let generation = NEXT_REPLY_GEN.fetch_add(1, Ordering::Relaxed);
        self.slots[index] = Some((generation, w));
        Some((generation as u64) << 32 | index as u64)
    }

    fn has(&self, handle: u64) -> bool {
        let slot = self.slots.get((handle & 0xFFFF_FFFF) as usize);
        slot.and_then(Option::as_ref)
            .is_some_and(|(g, _)| *g as u64 == handle >> 32)
    }

    /// Takes the call out: a handle answers once. Unknown and stale handles are `None`.
    fn take(&mut self, handle: u64) -> Option<Arc<ReplyWait>> {
        let slot = self.slots.get_mut((handle & 0xFFFF_FFFF) as usize)?;
        if slot.as_ref()?.0 as u64 != handle >> 32 {
            return None;
        }
        let (_, w) = slot.take()?;
        Some(w)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum IpcError {
    Cap(CapError),
    WrongType,
    Full,
    Interrupted,
    TimedOut,
    /// A label only the kernel may send.
    Invalid,
}

impl From<CapError> for IpcError {
    fn from(e: CapError) -> Self {
        IpcError::Cap(e)
    }
}

impl From<BlockError> for IpcError {
    fn from(e: BlockError) -> Self {
        match e {
            BlockError::Interrupted => IpcError::Interrupted,
            BlockError::TimedOut => IpcError::TimedOut,
        }
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
        self.push(from, 0, words, Vec::new(), None)
    }

    /// Queues a message and blocks until the server replies. The sender credential is taken
    /// from `from`, never from the caller's arguments. An interrupt (Cancel-then-wait) takes a
    /// call no server has received back out of the queue (`Interrupted`); once a server holds it,
    /// the server is sent a Cancel and the caller waits, uninterruptibly, for its reply.
    pub fn call(&self, from: &Thread, label: u64, words: [u64; 4]) -> Result<[u64; 4], IpcError> {
        Ok(self.call_bulk(from, label, words, Vec::new())?.0)
    }

    /// `call` with request bytes, returning the reply's words and bytes.
    pub fn call_bulk(
        &self,
        from: &Thread,
        label: u64,
        words: [u64; 4],
        data: Vec<u8>,
    ) -> Result<([u64; 4], Vec<u8>), IpcError> {
        let reply = ReplyWait::new(from.downgrade());
        self.push(from, label, words, data, Some(reply.clone()))?;
        match from.block_until(&reply.wq, || reply.take_answer(), None) {
            Ok(answer) => Ok(answer),
            Err(e) => self.abandon(from, &reply, e),
        }
    }

    /// The caller of `reply` was interrupted (or timed out) while waiting.
    fn abandon(
        &self,
        from: &Thread,
        reply: &Arc<ReplyWait>,
        why: BlockError,
    ) -> Result<([u64; 4], Vec<u8>), IpcError> {
        // The queue lock orders this against a receive: `Queued` flips to `Received` only while
        // a receiver holds it, so under it the message is either still queued or already taken.
        let mut q = self.queue.lock();
        let server = match &*reply.state.lock() {
            ReplyState::Queued => {
                q.retain(|m| !m.reply.as_ref().is_some_and(|r| Arc::ptr_eq(r, reply)));
                return Err(why.into());
            }
            ReplyState::Received(server) => server.clone(),
            ReplyState::Replied(_) | ReplyState::Done => None,
        };
        drop(q);
        if let Some((server, handle)) = server.and_then(|(t, h)| Some((t.upgrade()?, h))) {
            server.push_cancel(handle);
        }
        // shortcut: a server that never answers, or whose thread is gone, strands this caller
        // (accepted, T-01-26); only the Phase 2 kill path frees it.
        Ok(from.block_until_committed(&reply.wq, || reply.take_answer()))
    }

    fn push(
        &self,
        from: &Thread,
        label: u64,
        words: [u64; 4],
        data: Vec<u8>,
        reply: Option<Arc<ReplyWait>>,
    ) -> Result<(), IpcError> {
        if label == LABEL_CANCEL {
            return Err(IpcError::Invalid);
        }
        let mut q = self.queue.lock();
        if q.len() >= Self::QUEUE_LEN {
            return Err(IpcError::Full);
        }
        q.push_back(Message {
            sender: from.agent().credential(),
            tid: from.tid(),
            tag: from.tag(),
            words,
            data,
            label,
            reply,
        });
        drop(q);
        self.wq.wake_one();
        Ok(())
    }

    /// For a kernel task as the server: the call's `ReplyWait` stays in the message and no Cancel
    /// can reach the task.
    pub fn recv(&self, me: &Thread) -> Result<Message, IpcError> {
        Ok(me.block_until(
            &self.wq,
            || {
                let mut q = self.queue.lock();
                let m = q.pop_front()?;
                if let Some(w) = &m.reply {
                    *w.state.lock() = ReplyState::Received(None);
                }
                Some(m)
            },
            None,
        )?)
    }

    /// Like `recv`, for a server that answers later: a call's `ReplyWait` moves into `me`'s
    /// reply table and the returned handle (0 for a plain `send`) answers it. `Full` when the
    /// table has no room; the message then stays queued. A pending Cancel comes first, as a
    /// message labelled `LABEL_CANCEL` whose handle is that of the cancelled call, which stays
    /// in the table until the server answers it.
    fn recv_parked(&self, me: &Thread) -> Result<(Message, u64), IpcError> {
        me.block_until(
            &self.wq,
            || {
                if let Some(handle) = next_cancel(me) {
                    return Some(Ok((cancel_message(me), handle)));
                }
                let mut q = self.queue.lock();
                let handle = match &q.front()?.reply {
                    Some(w) => match me.replies.lock().insert(w.clone()) {
                        Some(h) => {
                            *w.state.lock() = ReplyState::Received(Some((me.downgrade(), h)));
                            h
                        }
                        None => return Some(Err(IpcError::Full)),
                    },
                    None => 0,
                };
                let mut m = q.pop_front()?;
                m.reply = None;
                Some(Ok((m, handle)))
            },
            None,
        )?
    }

    /// Undoes a `recv_parked` whose delivery to ring 3 failed, so the call is not lost.
    fn unreceive(&self, me: &Thread, mut m: Message, handle: u64) {
        if m.label == LABEL_CANCEL {
            me.cancels.lock().push_front(handle);
            return;
        }
        let mut q = self.queue.lock();
        m.reply = me.replies.lock().take(handle);
        if let Some(w) = &m.reply {
            *w.state.lock() = ReplyState::Queued;
        }
        q.push_front(m);
        drop(q);
        self.wq.wake_one();
    }
}

/// The first queued Cancel whose call is still parked in `me`'s table; the others (the server
/// answered first) are dropped.
fn next_cancel(me: &Thread) -> Option<u64> {
    loop {
        let handle = me.cancels.lock().pop_front()?;
        if me.replies.lock().has(handle) {
            return Some(handle);
        }
    }
}

fn cancel_message(me: &Thread) -> Message {
    Message {
        sender: me.agent().credential(),
        tid: 0,
        tag: 0,
        words: [0; 4],
        data: Vec::new(),
        label: LABEL_CANCEL,
        reply: None,
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

fn read_args(space: &VmSpace, args: UserPtr<IpcArgs>) -> Result<IpcArgs, Errno> {
    let mut raw = [0u8; size_of::<IpcArgs>()];
    let slice = UserSlice::new(args.addr(), raw.len())?;
    copy_from_user(space, &mut raw, slice)?;
    let a = IpcArgs::from_bytes(&raw);
    // Capability transfer is not implemented yet (01-06), so `ncaps` must be zero.
    if a.ncaps != 0 {
        return Err(Errno::EINVAL);
    }
    Ok(a)
}

/// A bulk buffer named by the caller: EINVAL above `MAX_IPC_BYTES`, EFAULT outside user space.
fn user_slice(addr: u64, len: u64) -> Result<UserSlice, Errno> {
    UserSlice::new(addr as usize, len as usize)
}

/// Copies the caller's bytes into kernel memory, in the caller's own context.
// shortcut: staging memory is not charged to the sender's agent, so a client can pin
// MAX_IPC_BYTES * QUEUE_LEN of kernel heap per endpoint; Phase 2 resource accounting charges it.
fn read_bytes(space: &VmSpace, src: UserSlice) -> Result<Vec<u8>, Errno> {
    let mut buf = vec![0u8; src.len()];
    if !buf.is_empty() {
        copy_from_user(space, &mut buf, src)?;
    }
    Ok(buf)
}

/// Writes the first `min(data.len(), dst.len())` bytes of `data` to `dst`, breaking
/// copy-on-write there, charged to `t`'s agent.
fn write_bytes(t: &Thread, space: &VmSpace, dst: UserSlice, data: &[u8]) -> Result<(), Errno> {
    let n = data.len().min(dst.len());
    if n == 0 {
        return Ok(());
    }
    copy_to_user(space, t.agent(), UserSlice::new(dst.addr(), n)?, &data[..n])
}

/// Writes `bytes` at `offset` in the `IpcArgs` at `args`. Only the fields the call produces are
/// written, so a concurrent change to the others by the caller's own threads is not undone.
fn write_args(
    t: &Thread,
    space: &VmSpace,
    args: UserPtr<IpcArgs>,
    offset: usize,
    bytes: &[u8],
) -> Result<(), Errno> {
    let dst = UserSlice::new(args.addr() + offset, bytes.len())?;
    copy_to_user(space, t.agent(), dst, bytes)
}

fn quads<const N: usize, const B: usize>(q: [u64; N]) -> [u8; B] {
    core::array::from_fn(|i| q[i / 8].to_le_bytes()[i % 8])
}

/// Reports how many bytes arrived (possibly more than fit in `recv_buf`) in `IpcArgs.send_len`.
fn write_len(t: &Thread, space: &VmSpace, args: UserPtr<IpcArgs>, n: usize) -> Result<(), Errno> {
    let at = offset_of!(IpcArgs, send_len);
    write_args(t, space, args, at, &(n as u64).to_le_bytes())
}

/// `SYS_CALL`: sends `IpcArgs` (read from `args` in the caller's active `space`) to the endpoint
/// in `ep`, writes the reply words back to `IpcArgs.words`, the first `recv_len` reply bytes to
/// `recv_buf`, and the full reply length to `send_len`.
pub fn sys_call(
    t: &Thread,
    space: &VmSpace,
    ep: Handle,
    args: UserPtr<IpcArgs>,
) -> Result<(), Errno> {
    let ep = endpoint(t, ep.index(), Rights::SEND)?;
    let a = read_args(space, args)?;
    let (send, recv) = (
        user_slice(a.send_buf, a.send_len)?,
        user_slice(a.recv_buf, a.recv_len)?,
    );
    let data = read_bytes(space, send)?;
    let (words, reply) = ep.call_bulk(t, a.label, a.words, data)?;
    write_bytes(t, space, recv, &reply)?;
    write_args(
        t,
        space,
        args,
        offset_of!(IpcArgs, words),
        &quads::<4, 32>(words),
    )?;
    write_len(t, space, args, reply.len())
}

/// Receives into the `IpcArgs` at `args`: `label`, `words`, `tid`, `tag` and `reply` are written,
/// the first `recv_len` message bytes go to `recv_buf` and the full length to `send_len`;
/// everything the sender could influence besides `label`, `words` and the bytes is
/// kernel-stamped. A failed delivery puts the message back.
fn recv_into(
    t: &Thread,
    space: &VmSpace,
    ep: &Endpoint,
    args: UserPtr<IpcArgs>,
    recv: UserSlice,
) -> Result<(), Errno> {
    let (m, handle) = ep.recv_parked(t)?;
    let head = quads::<5, 40>([m.label, m.words[0], m.words[1], m.words[2], m.words[3]]);
    let tail = quads::<3, 24>([m.tid, m.tag, handle]);
    let delivered = write_bytes(t, space, recv, &m.data)
        .and_then(|()| write_args(t, space, args, offset_of!(IpcArgs, label), &head))
        .and_then(|()| write_args(t, space, args, offset_of!(IpcArgs, tid), &tail))
        .and_then(|()| write_len(t, space, args, m.data.len()));
    if delivered.is_err() {
        ep.unreceive(t, m, handle);
    }
    delivered
}

/// `SYS_RECV`: waits for a message on `ep` (needs RECV) and writes it into `args`.
pub fn sys_recv_args(
    t: &Thread,
    space: &VmSpace,
    ep: Handle,
    args: UserPtr<IpcArgs>,
) -> Result<(), Errno> {
    let ep = endpoint(t, ep.index(), Rights::RECV)?;
    let a = read_args(space, args)?;
    recv_into(t, space, &ep, args, user_slice(a.recv_buf, a.recv_len)?)
}

/// Answers the call parked under `a.reply` in `t`'s own table with `a.words` and the bytes of
/// `send_buf`, copied in the server's context. A bad buffer leaves the handle usable.
fn answer(t: &Thread, space: &VmSpace, a: &IpcArgs) -> Result<(), Errno> {
    let data = read_bytes(space, user_slice(a.send_buf, a.send_len)?)?;
    let w = t.replies.lock().take(a.reply).ok_or(Errno::EBADF)?;
    w.reply_with(a.words, data);
    Ok(())
}

/// `SYS_REPLY`: answers the call named by `args.reply`.
pub fn sys_reply(t: &Thread, space: &VmSpace, args: UserPtr<IpcArgs>) -> Result<(), Errno> {
    answer(t, space, &read_args(space, args)?)
}

/// `SYS_REPLY_RECV`: `sys_reply`, then `sys_recv_args` into the same block. The endpoint is
/// resolved first and a failed reply returns before receiving.
pub fn sys_reply_recv(
    t: &Thread,
    space: &VmSpace,
    ep: Handle,
    args: UserPtr<IpcArgs>,
) -> Result<(), Errno> {
    let ep = endpoint(t, ep.index(), Rights::RECV)?;
    let a = read_args(space, args)?;
    let recv = user_slice(a.recv_buf, a.recv_len)?;
    answer(t, space, &a)?;
    recv_into(t, space, &ep, args, recv)
}

#[cfg(ktest)]
mod test {
    use ostd::{
        mm::{FallibleVmRead, PAGE_SIZE, VmWriter},
        prelude::*,
        task::{Task, TaskOptions},
        timer::Jiffies,
    };

    use super::*;
    use crate::{
        agent::Agent,
        cap::Cap,
        user::{
            Exit, Image, load_image,
            testkit::{join, spawn_image},
            user_image,
        },
        vm::cow_clone,
    };

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

    type Log = Arc<SpinLock<Vec<(u64, [u64; 4])>>>;

    /// A kernel task that calls `ep` as `t` and logs (label, reply words) when it returns.
    fn spawn_call(ep: &Arc<Endpoint>, t: &Arc<Thread>, label: u64, log: &Log) {
        let (ep, t, log) = (ep.clone(), t.clone(), log.clone());
        TaskOptions::new(move || {
            let words = ep.call(&t, label, [label, 0, 0, 0]).unwrap();
            log.lock().push((label, words));
        })
        .spawn()
        .unwrap();
    }

    fn wait_blocked(t: &Thread) {
        while !t.is_blocked() {
            Task::yield_now();
        }
    }

    fn wait_len(log: &Log, n: usize) {
        while log.lock().len() < n {
            Task::yield_now();
        }
    }

    /// An agent holding `ep` in slot 0 with all rights.
    fn agent_with(ep: &Arc<Endpoint>) -> Arc<Agent> {
        let agent = Agent::new_root(42, 64);
        let slot = agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(ep.clone()), Rights::ALL))
            .unwrap();
        assert_eq!(slot, 0);
        agent
    }

    #[ktest]
    fn server_parks_a_request_and_replies_later() {
        let ep = Endpoint::new();
        let agent = agent_with(&ep);
        let log: Log = Arc::new(SpinLock::new(Vec::new()));
        let (c1, c2, c3) = (
            Thread::new(agent.clone()),
            Thread::new(agent.clone()),
            Thread::new(agent.clone()),
        );
        // Queue order is 1 then 2; the server answers 2 first.
        spawn_call(&ep, &c1, 1, &log);
        wait_blocked(&c1);
        spawn_call(&ep, &c2, 2, &log);
        wait_blocked(&c2);

        let img = load_image(user_image!("park_srv")).unwrap();
        let srv = Thread::new(agent);
        let exit = spawn_image(&srv, &img, 0);
        wait_len(&log, 2);
        assert_eq!(log.lock()[0], (2, [200, 0, 0, 0]));
        assert_eq!(log.lock()[1], (1, [100, 0, 0, 0]));

        // The same reply_recv that answered 1 now receives 3.
        spawn_call(&ep, &c3, 3, &log);
        assert_eq!(join(&exit), Exit::Code(0));
        wait_len(&log, 3);
        assert_eq!(log.lock()[2], (3, [300, 0, 0, 0]));
    }

    #[ktest]
    fn siblings_are_distinguished_by_tid_and_tag() {
        let ep = Endpoint::new();
        let server_agent = agent_with(&ep);
        let client = server_agent
            .spawn_child(&[(0, Rights::SEND)], 16, 1)
            .unwrap();
        let (t1, t2) = (
            Thread::new_tagged(client.clone(), 1),
            Thread::new_tagged(client.clone(), 2),
        );
        let img = load_image(user_image!("sibling")).unwrap();
        let clone_of = |img: &Image| Image {
            space: Arc::new(cow_clone(&img.space, abi::IMAGE_BASE..abi::USER_STACK_TOP)),
            entry: img.entry,
            handler: img.handler,
            text: img.text.clone(),
        };
        let seen: Arc<SpinLock<Vec<(Credential, u64, u64)>>> = Arc::new(SpinLock::new(Vec::new()));
        let (server, seen2) = (Thread::new(server_agent), seen.clone());
        TaskOptions::new(move || {
            for _ in 0..2 {
                let m = sys_recv(&server, 0).unwrap();
                seen2.lock().push((m.sender.clone(), m.tid, m.tag));
                m.reply.unwrap().reply([0; 4]);
            }
        })
        .spawn()
        .unwrap();

        let (e1, e2) = (
            spawn_image(&t1, &clone_of(&img), 0),
            spawn_image(&t2, &clone_of(&img), 0),
        );
        assert_eq!(join(&e1), Exit::Code(0));
        assert_eq!(join(&e2), Exit::Code(0));

        let seen = seen.lock();
        assert_eq!(seen.len(), 2);
        assert_ne!(t1.tid(), t2.tid());
        for (sender, tid, tag) in seen.iter() {
            assert_eq!(*sender, client.credential());
            let (want_tid, want_tag) = if *tid == t1.tid() {
                (t1.tid(), 1)
            } else {
                (t2.tid(), 2)
            };
            assert_eq!((*tid, *tag), (want_tid, want_tag));
        }
        assert_ne!(seen[0].1, seen[1].1);
    }

    #[ktest]
    fn reply_handle_works_once_and_only_in_its_own_table() {
        let ep = Endpoint::new();
        let agent = agent_with(&ep);
        let (s1, s2, c) = (
            Thread::new(agent.clone()),
            Thread::new(agent.clone()),
            Thread::new(agent),
        );
        let log: Log = Arc::new(SpinLock::new(Vec::new()));
        spawn_call(&ep, &c, 5, &log);
        wait_blocked(&c);

        let (m, handle) = ep.recv_parked(&s1).unwrap();
        assert_eq!((m.label, m.reply.is_none()), (5, true));
        let a = IpcArgs {
            reply: handle,
            words: [9, 0, 0, 0],
            ..IpcArgs::default()
        };
        let nowhere = VmSpace::new();
        assert_eq!(answer(&s2, &nowhere, &a), Err(Errno::EBADF));
        assert_eq!(answer(&s1, &nowhere, &a), Ok(()));
        assert_eq!(answer(&s1, &nowhere, &a), Err(Errno::EBADF));
        wait_len(&log, 1);
        assert_eq!(log.lock()[0], (5, [9, 0, 0, 0]));
    }

    #[ktest]
    fn failed_delivery_requeues_the_message() {
        let ep = Endpoint::new();
        let agent = agent_with(&ep);
        let (s, c) = (Thread::new(agent.clone()), Thread::new(agent));
        let log: Log = Arc::new(SpinLock::new(Vec::new()));
        spawn_call(&ep, &c, 6, &log);
        wait_blocked(&c);

        // Not the active space, so the write-back fails after the message was taken.
        let elsewhere = VmSpace::new();
        let args = UserPtr::new(0x1000).unwrap();
        let none = user_slice(0, 0).unwrap();
        assert!(recv_into(&s, &elsewhere, &ep, args, none).is_err());
        let m = ep.recv(&s).unwrap();
        assert_eq!(m.label, 6);
        m.reply.unwrap().reply([1, 2, 3, 4]);
        wait_len(&log, 1);
        assert_eq!(log.lock()[0], (6, [1, 2, 3, 4]));
    }

    /// Runs `bulk` (in a copy-on-write clone of its loaded image) against `bulk_srv` and checks
    /// both exit 0 and that the parent's pages past the code, `.bss` included, are still zero.
    fn bulk_round_trip(client_arg: u64, server_arg: u64) {
        let ep = Endpoint::new();
        let server_agent = agent_with(&ep);
        let client_agent = server_agent
            .spawn_child(&[(0, Rights::SEND)], 32, 1)
            .unwrap();
        let parent = load_image(user_image!("bulk")).unwrap();
        let child = Image {
            space: Arc::new(cow_clone(
                &parent.space,
                abi::IMAGE_BASE..abi::USER_STACK_TOP,
            )),
            entry: parent.entry,
            handler: parent.handler,
            text: parent.text.clone(),
        };
        let server_img = load_image(user_image!("bulk_srv")).unwrap();
        let (client, server) = (Thread::new(client_agent), Thread::new(server_agent));
        let server_exit = spawn_image(&server, &server_img, server_arg);
        let client_exit = spawn_image(&client, &child, client_arg);
        assert_eq!(join(&client_exit), Exit::Code(0));
        assert_eq!(join(&server_exit), Exit::Code(0));

        // The image header's last quad is `mem_end`.
        let header = user_image!("bulk");
        let mem_end = u64::from_le_bytes(header[32..40].try_into().unwrap()) as usize;
        parent.space.activate();
        let mut page = vec![0u8; PAGE_SIZE];
        for va in (parent.text.end..mem_end.next_multiple_of(PAGE_SIZE)).step_by(PAGE_SIZE) {
            let mut r = parent.space.reader(va, PAGE_SIZE).unwrap();
            r.read_fallible(&mut VmWriter::from(&mut page[..])).unwrap();
            assert!(
                page.iter().all(|&b| b == 0),
                "parent page {va:#x} was written"
            );
        }
    }

    #[ktest]
    fn multi_page_bulk_round_trip_matches_after_clone() {
        bulk_round_trip(0, 0);
    }

    #[ktest]
    fn short_receive_buffers_truncate_and_report_the_full_length() {
        // The client's reply buffer holds 100 of the 12411 reply bytes.
        bulk_round_trip(100 << 8, 0);
        // The server's request buffer holds 100 of the 12411 request bytes; it replies with those
        // 100 and the client sees a reply of 100.
        bulk_round_trip(100 << 32, 100 << 8);
    }

    #[ktest]
    fn bulk_buffers_are_bounded_and_must_be_user_memory() {
        assert_eq!(
            user_slice(0x40_0000, abi::MAX_IPC_BYTES as u64 + 1),
            Err(Errno::EINVAL)
        );
        assert_eq!(user_slice(0xffff_8000_0000_0000, 8), Err(Errno::EFAULT));
        assert!(user_slice(0x40_0000, abi::MAX_IPC_BYTES as u64).is_ok());
    }

    type Outcome = Arc<SpinLock<Option<Result<([u64; 4], Vec<u8>), IpcError>>>>;

    /// A kernel task that calls `ep` as `t` with label 7; the result appears in the slot.
    fn spawn_call_outcome(ep: &Arc<Endpoint>, t: &Arc<Thread>) -> Outcome {
        let out: Outcome = Arc::new(SpinLock::new(None));
        let (ep, t, o) = (ep.clone(), t.clone(), out.clone());
        TaskOptions::new(move || {
            *o.lock() = Some(ep.call_bulk(&t, 7, [7, 0, 0, 0], Vec::new()));
        })
        .spawn()
        .unwrap();
        out
    }

    fn wait_outcome(out: &Outcome) -> Result<([u64; 4], Vec<u8>), IpcError> {
        loop {
            if let Some(r) = out.lock().take() {
                return r;
            }
            Task::yield_now();
        }
    }

    /// A call that `server` has received (reply handle returned) and whose caller was then
    /// interrupted at pc 0x1.
    fn interrupted_call() -> (Arc<Endpoint>, Arc<Thread>, Arc<Thread>, u64, Outcome) {
        let ep = Endpoint::new();
        let agent = agent_with(&ep);
        let (server, caller) = (Thread::new(agent.clone()), Thread::new(agent));
        let out = spawn_call_outcome(&ep, &caller);
        let (m, handle) = ep.recv_parked(&server).unwrap();
        assert_eq!(m.label, 7);
        wait_blocked(&caller);
        caller.interrupt(0x1);
        (ep, server, caller, handle, out)
    }

    #[ktest]
    fn queued_call_interrupt_is_not_started() {
        let ep = Endpoint::new();
        let agent = agent_with(&ep);
        let (server, caller) = (Thread::new(agent.clone()), Thread::new(agent));
        let out = spawn_call_outcome(&ep, &caller);
        while ep.queue.lock().is_empty() {
            Task::yield_now();
        }
        wait_blocked(&caller);
        caller.interrupt(0x1);
        assert_eq!(wait_outcome(&out).err(), Some(IpcError::Interrupted));
        // The call is gone from the queue, so no server can start it.
        assert!(ep.queue.lock().is_empty());
        caller.take_redirect();
        ep.send(&caller, [3, 0, 0, 0]).unwrap();
        assert_eq!(ep.recv(&server).unwrap().words[0], 3);
    }

    #[ktest]
    fn interrupted_call_waits_for_reply() {
        let (ep, server, caller, handle, out) = interrupted_call();
        // The server hears about it: a Cancel for the same handle, which stays answerable.
        let (c, h) = ep.recv_parked(&server).unwrap();
        assert_eq!((c.label, h, c.words), (LABEL_CANCEL, handle, [0; 4]));
        assert!(server.replies.lock().has(handle));
        for _ in 0..50 {
            Task::yield_now();
        }
        assert!(out.lock().is_none() && caller.is_blocked());

        let w = server.replies.lock().take(handle).unwrap();
        w.reply_with([5, 0, 0, 0], vec![1, 2]);
        assert_eq!(wait_outcome(&out), Ok(([5, 0, 0, 0], vec![1, 2])));
        // The interrupt is still pending for ring 3 and nothing else is queued.
        assert_eq!(caller.take_redirect(), Some(0x1));
        assert!(ep.queue.lock().is_empty());
    }

    #[ktest]
    fn never_answering_server_keeps_caller_blocked() {
        let (_ep, server, caller, handle, out) = interrupted_call();
        // Cancel is a request, not a kill: 30 timer ticks later the caller still waits.
        let until = Jiffies::elapsed().as_u64() + 30;
        while Jiffies::elapsed().as_u64() < until {
            Task::yield_now();
        }
        assert!(out.lock().is_none() && caller.is_blocked());
        server
            .replies
            .lock()
            .take(handle)
            .unwrap()
            .reply([1, 2, 3, 4]);
        assert_eq!(wait_outcome(&out), Ok(([1, 2, 3, 4], Vec::new())));
    }

    #[ktest]
    fn blocked_recv_still_aborts_at_once() {
        let ep = Endpoint::new();
        let agent = agent_with(&ep);
        let server = Thread::new(agent);
        let result: Arc<SpinLock<Option<Result<(Message, u64), IpcError>>>> =
            Arc::new(SpinLock::new(None));
        let (t, e, r) = (server.clone(), ep.clone(), result.clone());
        TaskOptions::new(move || *r.lock() = Some(e.recv_parked(&t)))
            .spawn()
            .unwrap();
        wait_blocked(&server);
        server.interrupt(0x2);
        while result.lock().is_none() {
            Task::yield_now();
        }
        assert_eq!(
            result.lock().take().unwrap().err(),
            Some(IpcError::Interrupted)
        );
    }

    #[ktest]
    fn reply_to_dead_waiter_is_dropped() {
        let ep = Endpoint::new();
        let agent = agent_with(&ep);
        let (server, caller) = (Thread::new(agent.clone()), Thread::new(agent));
        let w = ReplyWait::new(caller.downgrade());
        let handle = server.replies.lock().insert(w.clone()).unwrap();
        drop(caller);
        let a = IpcArgs {
            reply: handle,
            words: [1, 0, 0, 0],
            ..IpcArgs::default()
        };
        let nowhere = VmSpace::new();
        assert_eq!(answer(&server, &nowhere, &a), Ok(()));
        assert!(matches!(*w.state.lock(), ReplyState::Queued));
        // The slot is free again.
        assert_eq!(answer(&server, &nowhere, &a), Err(Errno::EBADF));
    }

    #[ktest]
    fn only_the_kernel_sends_cancel() {
        let (_server, client) = pair();
        let ep = endpoint(&client, 0, Rights::SEND).unwrap();
        assert_eq!(
            ep.call(&client, LABEL_CANCEL, [0; 4]).err(),
            Some(IpcError::Invalid)
        );
        assert!(ep.queue.lock().is_empty());
    }

    /// `reader` streams 64 bytes from a kernel-task server in 8-byte calls and is interrupted
    /// while it waits for the first answer. `commit`: the server has the call, gets a Cancel and
    /// answers it, so the chunk arrives despite the interrupt. Otherwise the server has not
    /// looked yet, the call is taken back and made again. Either way no byte is lost or doubled.
    fn reader_run(commit: bool) {
        let ep = Endpoint::new();
        let server_agent = agent_with(&ep);
        let client_agent = server_agent
            .spawn_child(&[(0, Rights::SEND)], 32, 1)
            .unwrap();
        let img = load_image(user_image!("reader")).unwrap();
        let (server, client) = (Thread::new(server_agent), Thread::new(client_agent));
        let exit = spawn_image(&client, &img, 0);
        let chunk =
            |k: usize| -> Vec<u8> { (8 * k..8 * k + 8).map(|i| (i * 7 % 251) as u8).collect() };

        let mut next = 0;
        if commit {
            let (_, handle) = ep.recv_parked(&server).unwrap();
            wait_blocked(&client);
            client.interrupt(img.handler);
            let (cancel, h) = ep.recv_parked(&server).unwrap();
            assert_eq!((cancel.label, h), (LABEL_CANCEL, handle));
            let w = server.replies.lock().take(handle).unwrap();
            w.reply_with([0; 4], chunk(0));
            next = 1;
        } else {
            while ep.queue.lock().is_empty() {
                Task::yield_now();
            }
            wait_blocked(&client);
            client.interrupt(img.handler);
        }
        while next < 8 {
            let (_, handle) = ep.recv_parked(&server).unwrap();
            let w = server.replies.lock().take(handle).unwrap();
            w.reply_with([0; 4], chunk(next));
            next += 1;
        }
        // End of stream.
        let (_, handle) = ep.recv_parked(&server).unwrap();
        server.replies.lock().take(handle).unwrap().reply([0; 4]);
        assert_eq!(join(&exit), Exit::Code(0));
        assert!(ep.queue.lock().is_empty());
    }

    #[ktest]
    fn interrupted_read_loses_no_data() {
        reader_run(true);
        reader_run(false);
    }
}
