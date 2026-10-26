//! The process as a kernel object: a `Space` is an address space plus the agent that owns it, and
//! the threads running in it. A Space capability names it; `thread_create` starts a thread in it
//! and `thread_interrupt` redirects one through a Thread capability.
//!
//! Every thread's credential is its space owner's, never its creator's: a thread started in
//! another space does not run with the creator's authority (OD-11).

use abi::{
    NOTE_EXIT, NOTE_KIND_BUDGET, NOTE_KIND_CODE, NOTE_KIND_EXCEPTION, NOTE_KIND_FAULT,
    NOTE_KIND_KILLED, USER_TOP,
};
use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};

use core::{
    ops::Range,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use ostd::{
    mm::{PAGE_SIZE, Vaddr, VmSpace},
    sync::SpinLock,
    task::disable_preempt,
};

use crate::{
    agent::{Agent, AgentError, Payer},
    cap::{Cap, CapError, Object, Rights},
    ipc::Endpoint,
    thread::Thread,
    user::{self, Exit, Image},
    vm::{CloneError, cow_clone},
};

/// Most live threads one space holds. The 65th `thread_create` is refused (`Limit`).
pub const MAX_THREADS_PER_SPACE: usize = 64;

/// A process: an address space, the agent that owns it, and its threads.
///
/// Teardown (D-01, process level): when the last thread is reaped (or at `kill`, when none is
/// live), the space unmaps the memory it owns, drops the notify capability, and uncharges every
/// page it charged to its owner. The
/// `Space` itself stays as a tombstone until its last capability drops (OD-14): `dying` is set and
/// `register` answers `Dead`.
///
/// Known gap (D-07): data frames a cow-cloned child still shares stay charged to the parent, not
/// to the child. They are uncharged at the parent's teardown even while the child maps them, so
/// the error is an under-charge, never a leak. The child pays for its page-table node frames at
/// clone time and for every copy it makes on a write fault; nothing charges a shared data frame
/// twice.
pub struct Space {
    vm: Arc<VmSpace>,
    owner: Arc<Agent>,
    /// `(tid, thread)` of every thread started in this space and not yet reaped. Weak, so a
    /// registered thread never outlives its own last reference because of this list.
    threads: SpinLock<Vec<(u64, Weak<Thread>)>>,
    /// Where exit and fault notes for this space's threads go: an Endpoint capability, kept as the
    /// capability and not the endpoint it names, so revoking it stops delivery (OD-06, OD-17).
    notify: SpinLock<Option<Cap>>,
    /// Pages charged to `owner` through this space (see `impl Payer`), refunded at teardown.
    charged_pages: AtomicUsize,
    /// Set once the space is going away, by a kill or by the last thread leaving; `register`
    /// refuses from then on. Never cleared. Set under the `threads` lock.
    dying: AtomicBool,
    /// Set by whoever wins the swap and runs `teardown`, so it runs once.
    torn: AtomicBool,
    /// Whether teardown unmaps `vm`. False when the caller keeps the memory (`adopt`).
    owns_vm: bool,
}

impl Space {
    fn build(vm: Arc<VmSpace>, owner: Arc<Agent>, charged: usize, owns_vm: bool) -> Arc<Space> {
        Arc::new(Space {
            vm,
            owner,
            threads: SpinLock::new(Vec::new()),
            notify: SpinLock::new(None),
            charged_pages: AtomicUsize::new(charged),
            dying: AtomicBool::new(false),
            torn: AtomicBool::new(false),
            owns_vm,
        })
    }

    /// A space that owns `vm`: teardown unmaps it. Charges nothing, so the ledger starts empty.
    pub fn new(vm: Arc<VmSpace>, owner: Arc<Agent>) -> Arc<Space> {
        Space::build(vm, owner, 0, true)
    }

    /// A space over memory the caller keeps: teardown unmaps nothing and refunds nothing. A
    /// copy-on-write copy made while it runs is charged through the ledger and stays charged,
    /// because the frame stays mapped in the caller's `vm` after the process is gone.
    pub fn adopt(vm: Arc<VmSpace>, owner: Arc<Agent>) -> Arc<Space> {
        Space::build(vm, owner, 0, false)
    }

    /// A space over a loaded image: charges the image's frames (`img.pages`: segments and stack)
    /// to `owner`, records them in the ledger, and owns the vm. `Budget` charges nothing.
    pub fn from_image(img: &Image, owner: Arc<Agent>) -> Result<Arc<Space>, AgentError> {
        owner.charge_pages(img.pages)?;
        Ok(Space::build(img.space.clone(), owner, img.pages, true))
    }

    /// A space over a copy-on-write clone of `parent` in `range`, owned by `owner`. The clone's
    /// page-table node frames are charged to `owner` and recorded in the ledger, so teardown
    /// refunds them. `Err` leaves `owner` as it was (see `cow_clone`).
    pub fn from_clone(
        parent: &VmSpace,
        range: Range<Vaddr>,
        owner: Arc<Agent>,
    ) -> Result<Arc<Space>, CloneError> {
        let tally = Tally {
            payer: &*owner,
            pages: AtomicUsize::new(0),
        };
        let vm = cow_clone(parent, range, &tally)?;
        let charged = tally.pages.load(Ordering::Acquire);
        Ok(Space::build(Arc::new(vm), owner, charged, true))
    }

    pub fn vm(&self) -> &Arc<VmSpace> {
        &self.vm
    }

    /// The agent every thread in this space runs as.
    pub fn owner(&self) -> &Arc<Agent> {
        &self.owner
    }

    /// True once teardown has begun. Never goes back.
    pub fn is_dying(&self) -> bool {
        self.dying.load(Ordering::Acquire)
    }

    /// Pages currently recorded in the ledger.
    pub fn charged(&self) -> usize {
        self.charged_pages.load(Ordering::Acquire)
    }

    /// Adds `th` to the space. Entries of threads that no longer exist are dropped first;
    /// `Dead` once the space is dying, `Limit` when `MAX_THREADS_PER_SPACE` threads are still
    /// registered. The dying check and the insert share the lock `reap` takes to decide on
    /// teardown, so a thread cannot join a space that is being emptied.
    pub fn register(&self, th: &Thread) -> Result<(), SpaceError> {
        let mut threads = self.threads.lock();
        if self.is_dying() {
            return Err(SpaceError::Dead);
        }
        threads.retain(|(_, w)| w.strong_count() > 0);
        if threads.len() >= MAX_THREADS_PER_SPACE {
            return Err(SpaceError::Limit);
        }
        threads.push((th.tid(), th.downgrade()));
        Ok(())
    }

    /// Kills the process from outside (OD-15; a kernel API, ring 3 gets it with procmgr).
    ///
    /// `dying` is set under the `threads` lock, so `register` refuses from then on, and every live
    /// thread is killed: each leaves its waits, ends as `Exit::Killed` (a note goes out like any
    /// exit), retires and is reaped, and the last reap tears the space down. A space with no live
    /// thread is torn down here. Returns without waiting for any of it.
    pub fn kill(&self) {
        let (victims, no_thread_left) = {
            let threads = self.threads.lock();
            self.dying.store(true, Ordering::Release);
            let victims: Vec<Arc<Thread>> =
                threads.iter().filter_map(|(_, w)| w.upgrade()).collect();
            let none = victims.is_empty();
            (victims, none && !self.torn.swap(true, Ordering::AcqRel))
        };
        for t in &victims {
            t.kill();
        }
        if no_thread_left {
            self.teardown();
        }
    }

    /// Takes back a registration that never ran. Does not tear the space down.
    pub fn unregister(&self, tid: u64) {
        self.threads.lock().retain(|(t, _)| *t != tid);
    }

    /// Removes thread `tid` from the space; when that leaves no live thread, tears the space down.
    pub fn reap(&self, tid: u64) {
        let last = {
            let mut threads = self.threads.lock();
            threads.retain(|(t, _)| *t != tid);
            let empty = !threads.iter().any(|(_, w)| w.strong_count() > 0);
            // Set under the lock `register` takes, so no registration slips in after the decision.
            if empty {
                self.dying.store(true, Ordering::Release);
            }
            empty && !self.torn.swap(true, Ordering::AcqRel)
        };
        if last {
            self.teardown();
        }
    }

    /// The work of teardown. The caller won the `torn` swap, so it runs once. In order: take the
    /// notify capability (nothing is sent after the process is gone), unmap what the space owns,
    /// refund what it charged. A space that does not own its `vm` unmaps nothing and refunds
    /// nothing: the pages it charged are still mapped, by the caller.
    fn teardown(&self) {
        drop(self.notify.lock().take());
        if self.owns_vm {
            let guard = disable_preempt();
            // `unmap` flushes and dispatches the TLB itself, and frees the frames once no CPU can
            // still hold a stale translation.
            let cursor = self.vm.cursor_mut(&guard, &(0..USER_TOP));
            // OSTD refusing the whole user range would leave the pages mapped and charged, a
            // leak; the ktests assert the range is mapped away, so it fails there, loudly.
            debug_assert!(
                cursor.is_ok(),
                "teardown: cursor over the user range refused"
            );
            if let Ok(mut c) = cursor {
                c.unmap(USER_TOP);
            }
            let pages = self.charged_pages.swap(0, Ordering::AcqRel);
            self.owner.uncharge_pages(pages);
        }
    }

    /// Sets the notify capability, replacing any earlier one. `cap` is stored as given; whether
    /// it still resolves is decided at each use.
    pub fn set_notify_cap(&self, cap: Cap) {
        *self.notify.lock() = Some(cap);
    }

    /// Sets the notify capability only if none is set, `Denied` otherwise. The test and the store
    /// share one lock acquisition.
    fn set_notify_cap_once(&self, cap: Cap) -> Result<(), SpaceError> {
        let mut notify = self.notify.lock();
        if notify.is_some() {
            return Err(SpaceError::Denied);
        }
        *notify = Some(cap);
        Ok(())
    }

    /// The endpoint notes go to: `None` when none is set, when the capability was revoked, or
    /// when it no longer resolves to an endpoint with SEND. Resolved at each use.
    pub fn notify_ep(&self) -> Option<Arc<Endpoint>> {
        let cap = self.notify.lock().clone()?;
        match cap.resolve(Rights::SEND) {
            Ok(Object::Endpoint(ep)) => Some(ep),
            _ => None,
        }
    }

    /// Tells the notify endpoint that `th` exited as `exit`: one `NOTE_EXIT` message stamped with
    /// the thread's credential, tid and tag, words `[kind, value, 0, 0]`. Nothing is sent when no
    /// endpoint resolves. The result is ignored: the exit has happened either way.
    ///
    /// Called after `set_exit` and before `reap`, so the note leaves while the space still counts
    /// the thread.
    pub fn note_exit(&self, th: &Thread, exit: Exit) {
        let Some(ep) = self.notify_ep() else {
            return;
        };
        let (kind, value) = match exit {
            Exit::Code(c) => (NOTE_KIND_CODE, c as u64),
            Exit::Fault(a) => (NOTE_KIND_FAULT, a as u64),
            Exit::Budget => (NOTE_KIND_BUDGET, 0),
            Exit::Exception(v) => (NOTE_KIND_EXCEPTION, v),
            Exit::Killed => (NOTE_KIND_KILLED, 0),
        };
        let _ = ep.push_note(th, NOTE_EXIT, [kind, value, 0, 0], None);
    }

    /// Threads registered and still referenced.
    pub fn live_threads(&self) -> usize {
        self.threads
            .lock()
            .iter()
            .filter(|(_, w)| w.strong_count() > 0)
            .count()
    }
}

/// Counts what a clone charges through `payer`, so `from_clone` can seed the new space's ledger.
struct Tally<'a> {
    payer: &'a dyn Payer,
    pages: AtomicUsize,
}

impl Payer for Tally<'_> {
    fn charge_pages(&self, pages: usize) -> Result<(), AgentError> {
        self.payer.charge_pages(pages)?;
        self.pages.fetch_add(pages, Ordering::AcqRel);
        Ok(())
    }

    fn uncharge_pages(&self, pages: usize) {
        self.pages.fetch_sub(pages, Ordering::AcqRel);
        self.payer.uncharge_pages(pages);
    }
}

impl Payer for Space {
    /// Charges the owner, then records the pages in the ledger teardown refunds.
    fn charge_pages(&self, pages: usize) -> Result<(), AgentError> {
        self.owner.charge_pages(pages)?;
        self.charged_pages.fetch_add(pages, Ordering::AcqRel);
        Ok(())
    }

    /// Takes `pages` out of the ledger, then refunds the owner. A refund larger than the ledger
    /// holds is cut to the ledger, so one space never uncharges the owner for another's pages.
    fn uncharge_pages(&self, pages: usize) {
        let taken = self
            .charged_pages
            .try_update(Ordering::AcqRel, Ordering::Acquire, |c| {
                Some(c.saturating_sub(pages))
            })
            .map_or(0, |before| before.min(pages));
        self.owner.uncharge_pages(taken);
    }
}

impl Drop for Space {
    /// A space that never started a thread still frees what it holds when its last reference
    /// drops. `torn` makes this a no-op after a real teardown.
    fn drop(&mut self) {
        if !self.torn.swap(true, Ordering::AcqRel) {
            self.teardown();
        }
    }
}

/// Where a new thread starts, as the caller asked. Values are unchecked until `thread_create`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadStart {
    pub rip: u64,
    pub rsp: u64,
    /// Lands in `rdi`.
    pub arg: u64,
    pub fs_base: u64,
    /// Must be 0 until a thread's exit has a futex wake to deliver (plan 02-10).
    pub clear_tid: u64,
}

/// Which space a new thread joins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The caller's own space; the tag is inherited.
    SelfSpace,
    /// The Space capability in this slot of the caller's agent, which needs CREATE.
    Slot(usize),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SpaceError {
    Cap(CapError),
    /// The slot holds another kind of object.
    WrongType,
    /// The caller has no space, or the thread has exited or no longer exists.
    Dead,
    /// The space already holds `MAX_THREADS_PER_SPACE` threads.
    Limit,
    Invalid,
    /// A thread named its own space for a notify endpoint, and the space already has one.
    Denied,
    /// The caller's capability space is full.
    NoRoom,
    /// The thread id could not be written back to the caller.
    Fault,
}

impl From<CapError> for SpaceError {
    fn from(e: CapError) -> Self {
        SpaceError::Cap(e)
    }
}

fn user_addr(a: u64) -> bool {
    (PAGE_SIZE as u64..USER_TOP as u64).contains(&a)
}

/// Starts a thread in the space named by `target`, as that space's owner, with `tag`. Returns the
/// slot of a new Thread capability (SIGNAL) in the caller's agent and the thread.
///
/// Checked in this order, so the error for a call with several faults is fixed: the fields of
/// `start`, the tag rule, the target, the space's thread limit, room in the caller's capability
/// space. `publish` then receives the new tid; if it fails nothing has been started and the
/// capability is taken back. A tag is chosen by a holder of a Space capability, never by a thread
/// for its own space (OD-13): `SelfSpace` inherits the caller's tag and refuses a non-zero one.
pub fn thread_create(
    caller: &Thread,
    target: Target,
    start: ThreadStart,
    tag: u64,
    publish: impl FnOnce(u64) -> Result<(), SpaceError>,
) -> Result<(usize, Arc<Thread>), SpaceError> {
    if !user_addr(start.rip)
        || !user_addr(start.rsp)
        || start.fs_base >= USER_TOP as u64
        || start.clear_tid != 0
    {
        return Err(SpaceError::Invalid);
    }
    let tag = match target {
        Target::SelfSpace if tag != 0 => return Err(SpaceError::Invalid),
        Target::SelfSpace => caller.tag(),
        Target::Slot(_) => tag,
    };
    let space = match target {
        Target::SelfSpace => caller.process().ok_or(SpaceError::Dead)?,
        Target::Slot(slot) => {
            // The CSpace lock ends with this statement, before the space is touched.
            let obj = caller
                .agent()
                .cspace
                .lock()
                .get(slot)?
                .resolve(Rights::CREATE)?;
            match obj {
                Object::Space(s) => s,
                _ => return Err(SpaceError::WrongType),
            }
        }
    };

    // The owner's agent, not the caller's: the credential follows the space.
    let th = Thread::new_tagged(space.owner().clone(), tag);
    space.register(&th)?;
    let cap = Cap::new(Object::Thread(th.downgrade()), Rights::SIGNAL);
    let slot = match caller.agent().cspace.lock().insert(cap) {
        Ok(slot) => slot,
        Err(_) => {
            space.unregister(th.tid());
            return Err(SpaceError::NoRoom);
        }
    };
    if let Err(e) = publish(th.tid()) {
        let _ = caller.agent().cspace.lock().remove(slot);
        space.unregister(th.tid());
        return Err(e);
    }
    user::spawn_thread(&space, th.clone(), start);
    Ok((slot, th))
}

/// Redirects the thread named by the Thread capability in `slot` to `entry`, aborting a blocking
/// IPC with EINTR as `Thread::interrupt` does. Needs SIGNAL. `Dead` once the thread has exited.
pub fn thread_interrupt(caller: &Thread, slot: usize, entry: usize) -> Result<(), SpaceError> {
    if !(PAGE_SIZE..USER_TOP).contains(&entry) {
        return Err(SpaceError::Invalid);
    }
    let obj = caller
        .agent()
        .cspace
        .lock()
        .get(slot)?
        .resolve(Rights::SIGNAL)?;
    let Object::Thread(weak) = obj else {
        return Err(SpaceError::WrongType);
    };
    let th = weak.upgrade().ok_or(SpaceError::Dead)?;
    if th.exit_status().is_some() {
        return Err(SpaceError::Dead);
    }
    th.interrupt(entry);
    Ok(())
}

/// Makes the Endpoint in `ep_slot` of the caller's agent (needs SEND) the notify endpoint of the
/// space named by `target`; the capability is kept, so revoking it later stops the notes.
///
/// Checked in this order: the target (`SelfSpace` is the caller's own space, `Dead` without one;
/// `Slot` needs SIGNAL and a Space), the endpoint slot (SEND and an Endpoint), then the self rule.
/// A holder of a Space capability may replace the endpoint at any time. A thread naming its own
/// space may set it only while none is set (`Denied`), so it cannot redirect the notes its parent
/// arranged (OD-18); the test and the store happen under one lock, so two racing self-sets cannot
/// both succeed.
pub fn set_notify(caller: &Thread, target: Target, ep_slot: usize) -> Result<(), SpaceError> {
    let space = match target {
        Target::SelfSpace => caller.process().ok_or(SpaceError::Dead)?,
        Target::Slot(slot) => {
            let obj = caller
                .agent()
                .cspace
                .lock()
                .get(slot)?
                .resolve(Rights::SIGNAL)?;
            match obj {
                Object::Space(s) => s,
                _ => return Err(SpaceError::WrongType),
            }
        }
    };
    let cap = caller.agent().cspace.lock().get(ep_slot)?.clone();
    match cap.resolve(Rights::SEND)? {
        Object::Endpoint(_) => {}
        _ => return Err(SpaceError::WrongType),
    }
    match target {
        Target::SelfSpace => space.set_notify_cap_once(cap),
        Target::Slot(_) => {
            space.set_notify_cap(cap);
            Ok(())
        }
    }
}

#[cfg(ktest)]
mod test {
    use abi::{IMAGE_BASE, NOTE_FAULT, TEST_DATA, USER_STACK_TOP, errno};
    use ostd::{
        mm::{FrameAllocOptions, PageFlags, UFrame, VmIo},
        prelude::*,
        task::{Task, TaskOptions, disable_preempt},
        timer::Jiffies,
    };

    use super::*;
    use crate::{
        ipc::{IpcError, Message},
        sched::install_space_switch,
        syscall::Errno,
        user::{
            Image, load_image, map_stack, map_test_page, spawn_thread,
            testkit::{self, join, join_thread, recv_note, spawn_image, spawn_image_in},
            user_image,
        },
        vm::cow_clone,
    };

    /// A space running the `spinner` image under `owner`, with the test page it reads.
    fn spinner_space(owner: &Arc<Agent>) -> (Arc<Space>, Image) {
        let img = load_image(user_image!("spinner")).unwrap();
        map_test_page(
            &img.space,
            FrameAllocOptions::new().alloc_frame().unwrap().into(),
            PageFlags::RW,
        );
        (Space::adopt(img.space.clone(), owner.clone()), img)
    }

    /// Start at the spinner's entry. Every thread of one space shares the id `arg`, which the
    /// spinner keeps in the shared test page.
    fn spin_at(img: &Image) -> ThreadStart {
        ThreadStart {
            rip: img.entry as u64,
            rsp: USER_STACK_TOP as u64,
            arg: 1,
            fs_base: 0,
            clear_tid: 0,
        }
    }

    fn ok(_: u64) -> Result<(), SpaceError> {
        Ok(())
    }

    #[ktest]
    fn thread_create_kernel_path_runs_in_other_space() {
        install_space_switch();
        let a = Agent::new_root(1, 0);
        let b_owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&b_owner);
        let space_slot = a
            .cspace
            .lock()
            .insert(Cap::new(
                Object::Space(b.clone()),
                Rights::CREATE.union(Rights::SIGNAL),
            ))
            .unwrap();
        // The caller never runs: it only names the agent whose capability space is used.
        let caller = Thread::new(a.clone());

        let (slot, th) =
            thread_create(&caller, Target::Slot(space_slot), spin_at(&img), 0x77, ok).unwrap();
        assert_eq!(th.tag(), 0x77);
        assert_eq!(th.agent().credential(), b.owner().credential());
        assert_ne!(th.agent().credential(), a.credential());
        assert_eq!(b.live_threads(), 1);

        thread_interrupt(&caller, slot, img.handler).unwrap();
        let Exit::Code(pc) = join_thread(&th) else {
            panic!("the thread must exit through its handler");
        };
        assert!(
            img.text.contains(&pc),
            "interrupted inside the image: {pc:#x}"
        );
        assert_eq!(b.live_threads(), 0);
    }

    #[ktest]
    fn thread_create_self_inherits_tag() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&owner);
        let caller = Thread::new_tagged(owner.clone(), 5);
        caller.set_process(b.clone());

        let (slot, th) = thread_create(&caller, Target::SelfSpace, spin_at(&img), 0, ok).unwrap();
        assert_eq!(th.tag(), 5, "inherited from the caller");
        assert_eq!(th.agent().credential(), owner.credential());
        assert_eq!(b.live_threads(), 1);

        // A thread cannot mint a tag for its own space.
        assert_eq!(
            thread_create(&caller, Target::SelfSpace, spin_at(&img), 9, ok).err(),
            Some(SpaceError::Invalid)
        );
        assert_eq!(b.live_threads(), 1, "the refused call registered nothing");

        thread_interrupt(&caller, slot, img.handler).unwrap();
        join_thread(&th);
        assert_eq!(b.live_threads(), 0);
    }

    type Reply = Arc<SpinLock<Option<[u64; 4]>>>;

    /// A ring-3 holder `A` (the `thread_create` program, agent `a`) with the endpoint in slot 0 and
    /// a Space capability for `b` in slot 1. A kernel task calls the endpoint with `label` and
    /// `[1, b.entry, USER_STACK_TOP, b.handler]`; the reply words arrive in the returned slot.
    fn ring3_holder(
        a: &Arc<Agent>,
        a_img: &Image,
        b_img: &Image,
        label: u64,
        ep: &Arc<Endpoint>,
    ) -> (Reply, testkit::ExitSlot) {
        let exit = spawn_image(&Thread::new(a.clone()), a_img, 0);
        let reply: Reply = Arc::new(SpinLock::new(None));
        let (ep, r2) = (ep.clone(), reply.clone());
        let words = [
            1,
            b_img.entry as u64,
            USER_STACK_TOP as u64,
            b_img.handler as u64,
        ];
        let caller = Thread::new(Agent::new_root(3, 0));
        TaskOptions::new(move || {
            *r2.lock() = Some(ep.call(&caller, label, words).unwrap());
        })
        .spawn()
        .unwrap();
        (reply, exit)
    }

    fn holder_agent(ep: &Arc<Endpoint>, b: &Arc<Space>) -> Arc<Agent> {
        let a = Agent::new_root(42, 64);
        let mut cs = a.cspace.lock();
        assert_eq!(
            cs.insert(Cap::new(Object::Endpoint(ep.clone()), Rights::ALL)),
            Ok(0)
        );
        let space = Cap::new(
            Object::Space(b.clone()),
            Rights::CREATE.union(Rights::SIGNAL),
        );
        assert_eq!(cs.insert(space), Ok(1));
        drop(cs);
        a
    }

    fn wait_reply(r: &Reply) -> [u64; 4] {
        loop {
            if let Some(w) = r.lock().take() {
                return w;
            }
            Task::yield_now();
        }
    }

    #[ktest]
    fn thread_create_ring3_holder_runs_thread_in_other_space() {
        install_space_switch();
        let ep = Endpoint::new();
        let b_owner = Agent::new_root(2, 0);
        let (b, b_img) = spinner_space(&b_owner);
        let a = holder_agent(&ep, &b);
        let a_img = load_image(user_image!("thread_create")).unwrap();

        let (reply, exit) = ring3_holder(&a, &a_img, &b_img, 0, &ep);
        let [slot, tid, interrupted, refused] = wait_reply(&reply);
        assert_eq!(join(&exit), Exit::Code(0));

        // Slot 0 is the endpoint and slot 1 the Space capability: the new capability is slot 2.
        assert_eq!(slot, 2);
        assert_ne!(
            tid, 0,
            "the kernel wrote the tid back before the thread ran"
        );
        assert_eq!(interrupted, 0);
        assert_eq!(refused as isize, -abi::errno::EBADF);
        let cap = a.cspace.lock().get(2).unwrap().resolve(Rights::SIGNAL);
        assert!(matches!(cap, Ok(Object::Thread(_))));
        assert_eq!(
            a.cspace.lock().get(2).unwrap().rights(),
            Rights::SIGNAL,
            "the Thread capability carries SIGNAL and nothing else"
        );

        // The interrupt sent it to the spinner's handler, which exits; the space is empty again.
        let mut spins = 0usize;
        while b.live_threads() != 0 {
            spins += 1;
            assert!(spins < 1_000_000, "the thread never exited");
            Task::yield_now();
        }
    }

    #[ktest]
    fn thread_create_writes_tid_or_starts_nothing() {
        install_space_switch();
        let ep = Endpoint::new();
        let b_owner = Agent::new_root(2, 0);
        let (b, b_img) = spinner_space(&b_owner);
        let a = holder_agent(&ep, &b);
        let a_img = load_image(user_image!("thread_create")).unwrap();

        // A valid ThreadCtx in a page the caller can read but the kernel may not write.
        let frame = FrameAllocOptions::new().alloc_frame().unwrap();
        let ctx = abi::ThreadCtx {
            rip: b_img.entry as u64,
            rsp: USER_STACK_TOP as u64,
            arg: 1,
            ..Default::default()
        };
        frame.write_bytes(0, &ctx.to_bytes()).unwrap();
        map_test_page(&a_img.space, frame.into(), PageFlags::R);

        let (reply, exit) = ring3_holder(&a, &a_img, &b_img, 1, &ep);
        let [result, ..] = wait_reply(&reply);
        assert_eq!(join(&exit), Exit::Code(0));

        assert_eq!(result as isize, -abi::errno::EFAULT);
        assert_eq!(b.live_threads(), 0, "nothing started");
        assert!(
            a.cspace.lock().get(2).is_err(),
            "the capability was taken back"
        );
    }

    /// An agent holding a Space capability for `b` with `rights` in slot 0, and a thread of it
    /// that never runs and only names the agent whose capability space is used.
    fn holder_of(b: &Arc<Space>, rights: Rights) -> (Arc<Agent>, Arc<Thread>) {
        let a = Agent::new_root(1, 0);
        let slot = a
            .cspace
            .lock()
            .insert(Cap::new(Object::Space(b.clone()), rights))
            .unwrap();
        assert_eq!(slot, 0);
        let caller = Thread::new(a.clone());
        (a, caller)
    }

    /// Interrupts every thread, then waits for each to exit. All are interrupted first: the
    /// ktest scheduler is OSTD's FIFO, which never preempts, so a thread still spinning that comes
    /// ahead of the caller in the queue would keep the CPU for good.
    fn stop_all(caller: &Thread, started: &[(usize, Arc<Thread>)], handler: usize) {
        for (slot, _) in started {
            thread_interrupt(caller, *slot, handler).unwrap();
        }
        for (_, th) in started {
            join_thread(th);
        }
    }

    #[ktest]
    fn thread_create_refuses_without_create() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&owner);
        // SIGNAL without CREATE.
        let (a, caller) = holder_of(&b, Rights::SIGNAL.union(Rights::GRANT));
        let room = a.cspace.lock().room();
        let try_slot =
            |slot| thread_create(&caller, Target::Slot(slot), spin_at(&img), 0, ok).err();

        assert_eq!(try_slot(0), Some(SpaceError::Cap(CapError::Rights)));
        // An empty slot.
        assert_eq!(try_slot(5), Some(SpaceError::Cap(CapError::NoSlot)));
        // A capability that is not a Space, with every right.
        let ep = a
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))
            .unwrap();
        assert_eq!(try_slot(ep), Some(SpaceError::WrongType));
        // Attenuated away: a derived capability without CREATE is refused as well.
        let derived = {
            let mut cs = a.cspace.lock();
            let d = cs.get(0).unwrap().derive(Rights::SIGNAL).unwrap();
            cs.insert(d).unwrap()
        };
        assert_eq!(try_slot(derived), Some(SpaceError::Cap(CapError::Rights)));

        assert_eq!(b.live_threads(), 0, "no refusal started a thread");
        assert_eq!(
            a.cspace.lock().room(),
            room - 2,
            "only the two capabilities this test inserted"
        );
    }

    #[ktest]
    fn thread_create_refuses_bad_ctx() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&owner);
        let (a, caller) = holder_of(&b, Rights::ALL);
        let room = a.cspace.lock().room();
        let good = spin_at(&img);
        let bad = |s: ThreadStart| thread_create(&caller, Target::Slot(0), s, 0, ok).err();

        let top = USER_TOP as u64;
        for (what, s) in [
            ("rip 0", ThreadStart { rip: 0, ..good }),
            ("rip in page 0", ThreadStart { rip: 8, ..good }),
            ("rip at USER_TOP", ThreadStart { rip: top, ..good }),
            ("rsp 0", ThreadStart { rsp: 0, ..good }),
            ("rsp at USER_TOP", ThreadStart { rsp: top, ..good }),
            (
                "fs_base at USER_TOP",
                ThreadStart {
                    fs_base: top,
                    ..good
                },
            ),
            (
                "clear_tid set",
                ThreadStart {
                    clear_tid: 1,
                    ..good
                },
            ),
        ] {
            assert_eq!(bad(s), Some(SpaceError::Invalid), "{what}");
        }

        // A thread cannot pick a tag for its own space, and a caller with no space has none.
        let me = Thread::new_tagged(owner.clone(), 5);
        assert_eq!(
            thread_create(&me, Target::SelfSpace, good, 9, ok).err(),
            Some(SpaceError::Invalid)
        );
        assert_eq!(
            thread_create(&me, Target::SelfSpace, good, 0, ok).err(),
            Some(SpaceError::Dead)
        );
        // The publish step failing takes the capability back and starts nothing.
        assert_eq!(
            thread_create(&caller, Target::Slot(0), good, 0, |_| Err(
                SpaceError::Fault
            ))
            .err(),
            Some(SpaceError::Fault)
        );

        assert_eq!(b.live_threads(), 0);
        assert_eq!(a.cspace.lock().room(), room, "no capability was kept");
    }

    #[ktest]
    fn thread_create_limit_is_64() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&owner);
        let (a, caller) = holder_of(&b, Rights::ALL);

        let mut started = Vec::new();
        for i in 0..MAX_THREADS_PER_SPACE {
            // Each thread gets its own slice of the shared stack; all share the spinner's id.
            let s = ThreadStart {
                rsp: USER_STACK_TOP as u64 - 0x400 * i as u64,
                ..spin_at(&img)
            };
            started.push(thread_create(&caller, Target::Slot(0), s, 0, ok).unwrap());
        }
        assert_eq!(b.live_threads(), MAX_THREADS_PER_SPACE);

        let room = a.cspace.lock().room();
        assert_eq!(
            thread_create(&caller, Target::Slot(0), spin_at(&img), 0, ok).err(),
            Some(SpaceError::Limit)
        );
        assert_eq!(b.live_threads(), MAX_THREADS_PER_SPACE);
        assert_eq!(
            a.cspace.lock().room(),
            room,
            "the refused call kept nothing"
        );

        // Exits give the room back. Every thread is stopped at once (see `stop_all`), so room is
        // observed after all 64 are gone, not after one.
        stop_all(&caller, &started, img.handler);
        assert_eq!(b.live_threads(), 0);
        // The last exit tore the process down: its slots are free but it takes no new thread.
        assert!(b.is_dying());
        assert_eq!(
            thread_create(&caller, Target::Slot(0), spin_at(&img), 0, ok).err(),
            Some(SpaceError::Dead)
        );
        assert_eq!(
            a.cspace.lock().room(),
            room,
            "the refused call kept nothing"
        );
    }

    #[ktest]
    fn thread_create_full_cspace_is_emfile() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&owner);
        let (a, caller) = holder_of(&b, Rights::ALL);

        // Fill the capability space with a placeholder.
        let filler = Endpoint::new();
        let last = {
            let mut cs = a.cspace.lock();
            let mut last = 0;
            while cs.room() > 0 {
                last = cs
                    .insert(Cap::new(Object::Endpoint(filler.clone()), Rights::SEND))
                    .unwrap();
            }
            last
        };

        assert_eq!(
            thread_create(&caller, Target::Slot(0), spin_at(&img), 0, ok).err(),
            Some(SpaceError::NoRoom)
        );
        assert_eq!(b.live_threads(), 0, "the refused thread was reaped");

        // Freeing one slot lets the same call through.
        a.cspace.lock().remove(last).unwrap();
        let started = thread_create(&caller, Target::Slot(0), spin_at(&img), 0, ok).unwrap();
        assert_eq!(started.0, last);
        stop_all(&caller, &[started], img.handler);
    }

    #[ktest]
    fn thread_interrupt_refuses() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&owner);
        let (a, caller) = holder_of(&b, Rights::ALL);
        let int = |slot, entry| thread_interrupt(&caller, slot, entry).err();
        let insert = |cap: Cap| a.cspace.lock().insert(cap).unwrap();

        let started = thread_create(&caller, Target::Slot(0), spin_at(&img), 0, ok).unwrap();
        let (slot, th) = (started.0, started.1.clone());

        // Entry outside user space is refused before the slot is looked at.
        assert_eq!(int(slot, 0), Some(SpaceError::Invalid));
        assert_eq!(int(slot, USER_TOP), Some(SpaceError::Invalid));
        assert_eq!(int(99, 0), Some(SpaceError::Invalid));
        // An empty slot.
        assert_eq!(
            int(99, img.handler),
            Some(SpaceError::Cap(CapError::NoSlot))
        );
        // A Thread capability without SIGNAL.
        let weak = Object::Thread(th.downgrade());
        let no_signal = insert(Cap::new(weak, Rights::SEND));
        assert_eq!(
            int(no_signal, img.handler),
            Some(SpaceError::Cap(CapError::Rights))
        );
        // Not a Thread at all, with every right (slot 0 is the Space capability).
        assert_eq!(int(0, img.handler), Some(SpaceError::WrongType));
        // A thread that no longer exists.
        let gone = Thread::new(owner.clone());
        let dead = insert(Cap::new(Object::Thread(gone.downgrade()), Rights::SIGNAL));
        drop(gone);
        assert_eq!(int(dead, img.handler), Some(SpaceError::Dead));

        // None of that disturbed the running thread; stopping it works once, then it is Dead.
        assert_eq!(b.live_threads(), 1);
        stop_all(&caller, &[started], img.handler);
        assert_eq!(int(slot, img.handler), Some(SpaceError::Dead));
    }

    #[ktest]
    fn space_credential_matches_owner() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img) = spinner_space(&owner);
        assert!(Arc::ptr_eq(b.owner(), &owner));

        // Two creators with different authority both produce threads that run as the owner.
        let (a1, c1) = holder_of(&b, Rights::ALL);
        let (a2, c2) = holder_of(&b, Rights::CREATE.union(Rights::SIGNAL));
        assert_ne!(a1.credential(), owner.credential());
        assert_ne!(a2.credential(), owner.credential());
        let t1 = thread_create(&c1, Target::Slot(0), spin_at(&img), 1, ok).unwrap();
        let t2 = thread_create(&c2, Target::Slot(0), spin_at(&img), 2, ok).unwrap();
        for (t, a) in [(&t1, &a1), (&t2, &a2)] {
            assert!(
                Arc::ptr_eq(t.1.agent(), &owner),
                "the owner, not the creator"
            );
            assert_eq!(t.1.agent().credential(), owner.credential());
            assert_ne!(t.1.agent().credential(), a.credential());
        }
        // The tag is the holder's choice; it is not part of the credential.
        assert_eq!((t1.1.tag(), t2.1.tag()), (1, 2));

        // A thread of the space that makes another one stays the owner too.
        let me = Thread::new_tagged(owner.clone(), 5);
        me.set_process(b.clone());
        let t3 = thread_create(&me, Target::SelfSpace, spin_at(&img), 0, ok).unwrap();
        assert!(Arc::ptr_eq(t3.1.agent(), &owner));
        assert_eq!(t3.1.tag(), 5);

        // Each thread's capability sits in its creator's space, so each creator interrupts its own;
        // all are interrupted before any is joined (see `stop_all`).
        for (creator, t) in [(&c1, &t1), (&c2, &t2), (&me, &t3)] {
            thread_interrupt(creator, t.0, img.handler).unwrap();
        }
        for t in [&t1, &t2, &t3] {
            join_thread(&t.1);
        }
    }

    /// The `fault_sibling` image over a test page mapped with `data`.
    fn sibling_parent(data: PageFlags) -> Image {
        let img = load_image(user_image!("fault_sibling")).unwrap();
        map_test_page(
            &img.space,
            FrameAllocOptions::new().alloc_frame().unwrap().into(),
            data,
        );
        img
    }

    /// A sibling of `parent`: its own space over a copy-on-write clone of the image range with a
    /// fresh stack, owned by `owner`, whose notes go to `ep` (a SEND capability) when given. One
    /// thread tagged `tag` starts at the entry with `arg`.
    fn start_sibling(
        parent: &Image,
        owner: &Arc<Agent>,
        ep: Option<&Arc<Endpoint>>,
        tag: u64,
        arg: u64,
    ) -> (Arc<Space>, Arc<Thread>) {
        install_space_switch();
        let vm = Arc::new(
            cow_clone(
                &parent.space,
                IMAGE_BASE..TEST_DATA + PAGE_SIZE,
                &*Agent::new_root(1, 64),
            )
            .expect("cow_clone"),
        );
        map_stack(&vm).unwrap();
        let space = Space::new(vm, owner.clone());
        if let Some(ep) = ep {
            space.set_notify_cap(Cap::new(Object::Endpoint(ep.clone()), Rights::SEND));
        }
        let th = Thread::new_tagged(owner.clone(), tag);
        space.register(&th).unwrap();
        let start = ThreadStart {
            rip: parent.entry as u64,
            rsp: USER_STACK_TOP as u64,
            arg,
            fs_base: 0,
            clear_tid: 0,
        };
        spawn_thread(&space, th.clone(), start);
        (space, th)
    }

    /// A kernel thread with no authority, to receive on an endpoint.
    fn receiver() -> Arc<Thread> {
        Thread::new(Agent::new_root(99, 0))
    }

    /// Waits, yielding, until `ep` holds `n` messages.
    fn wait_queued(ep: &Endpoint, n: usize) {
        let mut spins = 0usize;
        while ep.queued() < n {
            spins += 1;
            assert!(spins < 1_000_000, "the notes never arrived");
            Task::yield_now();
        }
    }

    /// What every note must be: the kernel's label, no capability, no data, no reply to wait on.
    fn assert_note_shape(m: &Message, label: u64) {
        assert_eq!(m.label, label);
        assert!(m.caps.is_empty(), "a note carries no capability");
        assert!(m.data.is_empty(), "a note carries no data");
    }

    #[ktest]
    fn note_exit_names_tid_and_tag_of_each_sibling() {
        let owner = Agent::new_root(5, 0);
        let parent = sibling_parent(PageFlags::RW);
        let e = Endpoint::new();
        let (_s0, t0) = start_sibling(&parent, &owner, Some(&e), 0x10, 0);
        let (_s1, t1) = start_sibling(&parent, &owner, Some(&e), 0x11, 0);

        let me = receiver();
        let mut seen: Vec<(u64, u64)> = Vec::new();
        for _ in 0..2 {
            let m = e.recv(&me).unwrap();
            assert_note_shape(&m, NOTE_EXIT);
            assert_eq!(m.words, [NOTE_KIND_CODE, 7, 0, 0]);
            assert_eq!(m.sender, owner.credential());
            assert!(m.reply.is_none(), "an exit note is not a call");
            seen.push((m.tid, m.tag));
        }
        seen.sort();
        let mut want = [(t0.tid(), 0x10), (t1.tid(), 0x11)];
        want.sort();
        assert_eq!(seen, want);
        assert_eq!(join_thread(&t0), Exit::Code(7));
        assert_eq!(join_thread(&t1), Exit::Code(7));
        assert_eq!(e.queued(), 0, "exactly two notes");
    }

    #[ktest]
    fn note_exit_without_notify_is_dropped() {
        let owner = Agent::new_root(5, 0);
        let parent = sibling_parent(PageFlags::RW);
        // An endpoint nothing points at, to show that nothing reaches it either.
        let bystander = Endpoint::new();
        let (s, t) = start_sibling(&parent, &owner, None, 0x10, 0);
        assert!(s.notify_ep().is_none());

        assert_eq!(join_thread(&t), Exit::Code(7));
        while s.live_threads() != 0 {
            Task::yield_now();
        }
        assert_eq!(bystander.queued(), 0);
    }

    #[ktest]
    fn note_labels_are_reserved() {
        let me = receiver();
        let ep = Endpoint::new();
        for label in [NOTE_EXIT, NOTE_FAULT] {
            assert_eq!(
                ep.push(&me, label, [0; 4], Vec::new(), Vec::new(), None),
                Err(IpcError::Invalid),
                "send path, label {label:#x}"
            );
            assert_eq!(
                ep.call(&me, label, [0; 4]),
                Err(IpcError::Invalid),
                "call path, label {label:#x}"
            );
        }
        assert_eq!(ep.queued(), 0, "nothing was queued");
    }

    #[ktest]
    fn note_exit_bypasses_queue_len() {
        let owner = Agent::new_root(5, 0);
        let parent = sibling_parent(PageFlags::RW);
        let e = Endpoint::new();
        let me = receiver();
        for i in 0..Endpoint::QUEUE_LEN {
            e.send(&me, [i as u64, 0, 0, 0]).unwrap();
        }
        assert_eq!(e.send(&me, [0; 4]), Err(IpcError::Full));

        let (_s, t) = start_sibling(&parent, &owner, Some(&e), 0x10, 0);
        wait_queued(&e, Endpoint::QUEUE_LEN + 1);
        assert_eq!(join_thread(&t), Exit::Code(7));
        assert_eq!(
            e.send(&me, [0; 4]),
            Err(IpcError::Full),
            "an ordinary send is still refused"
        );

        for i in 0..Endpoint::QUEUE_LEN {
            assert_eq!(e.recv(&me).unwrap().words[0], i as u64);
        }
        let note = e.recv(&me).unwrap();
        assert_note_shape(&note, NOTE_EXIT);
        assert_eq!(note.words, [NOTE_KIND_CODE, 7, 0, 0]);
    }

    #[ktest]
    fn note_fault_then_exit_from_one_sibling() {
        let owner = Agent::new_root(5, 0);
        // Read-only, not copy-on-write: the sibling's store is a fault, not a page copy.
        let parent = sibling_parent(PageFlags::R);
        let e = Endpoint::new();
        let me = receiver();

        // S0 exits first, so S1's notes are the only ones left to read.
        let (_s0, t0) = start_sibling(&parent, &owner, Some(&e), 0x10, 0);
        let m = recv_note(&e, &me);
        assert_note_shape(&m, NOTE_EXIT);
        assert_eq!(m.words, [NOTE_KIND_CODE, 7, 0, 0]);
        assert_eq!((m.tid, m.tag), (t0.tid(), 0x10));
        assert_eq!(join_thread(&t0), Exit::Code(7));

        let (_s1, t1) = start_sibling(&parent, &owner, Some(&e), 0x11, 1);
        let m = recv_note(&e, &me);
        assert_note_shape(&m, NOTE_FAULT);
        assert_eq!(m.sender, owner.credential());
        assert_eq!((m.tid, m.tag), (t1.tid(), 0x11));
        assert_eq!(m.words[..2], [14, TEST_DATA as u64]);
        let rip = m.words[2] as usize;
        assert!(
            parent.text.contains(&rip),
            "fault rip {rip:#x} outside the image text"
        );
        assert_eq!(m.words[3] & 0b110, 0b110, "write fault from user mode");
        assert!(t1.exit_status().is_none(), "S1 waits for the verdict");

        // Any non-zero verdict kills; the exit note then names the fault.
        m.reply
            .as_ref()
            .expect("a fault note is a call")
            .reply([1, 0, 0, 0]);
        let m = recv_note(&e, &me);
        assert_note_shape(&m, NOTE_EXIT);
        assert_eq!(m.words, [NOTE_KIND_FAULT, TEST_DATA as u64, 0, 0]);
        assert_eq!((m.tid, m.tag), (t1.tid(), 0x11));
        assert_eq!(join_thread(&t1), Exit::Fault(TEST_DATA));
        assert_eq!(e.queued(), 0, "three notes in all");
    }

    #[ktest]
    fn note_fault_interrupted_before_receive_resumes_redirect() {
        let owner = Agent::new_root(5, 0);
        let parent = sibling_parent(PageFlags::R);
        let e = Endpoint::new();
        let me = receiver();

        let (_s, t) = start_sibling(&parent, &owner, Some(&e), 0x11, 1);
        wait_queued(&e, 1);

        // Nobody has received the fault note, so the interrupt withdraws it and the thread runs
        // the redirect: the handler exits with the faulting pc.
        t.interrupt(parent.handler);
        let Exit::Code(pc) = join_thread(&t) else {
            panic!("the interrupted fault did not end in the redirect handler");
        };
        assert!(
            parent.text.contains(&pc),
            "pc {pc:#x} outside the image text"
        );

        // The only note left is the exit note: the fault note was taken out, not left queued.
        let m = recv_note(&e, &me);
        assert_note_shape(&m, NOTE_EXIT);
        assert_eq!(m.words, [NOTE_KIND_CODE, pc as u64, 0, 0]);
        assert_eq!(e.queued(), 0, "the withdrawn fault note is gone");
    }

    /// A capability for `ep` with `rights`, inserted in `agent`; returns the slot.
    fn give_endpoint(agent: &Agent, ep: &Arc<Endpoint>, rights: Rights) -> usize {
        agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(ep.clone()), rights))
            .unwrap()
    }

    #[ktest]
    fn space_notify_refuses() {
        let owner = Agent::new_root(2, 0);
        let b = Space::new(Arc::new(VmSpace::new()), owner.clone());
        let e = Endpoint::new();
        let refused = |caller: &Thread, target, slot| set_notify(caller, target, slot).err();

        // The space capability is the first thing looked at: without SIGNAL it is a rights error,
        // whatever the endpoint slot holds.
        let (a0, c0) = holder_of(&b, Rights::SEND.union(Rights::CREATE));
        let ep0 = give_endpoint(&a0, &e, Rights::SEND);
        assert_eq!(
            refused(&c0, Target::Slot(0), ep0),
            Some(SpaceError::Cap(CapError::Rights)),
            "no SIGNAL"
        );

        // With SIGNAL and SEND on the space capability itself, so the endpoint slot can name it.
        let (a, c) = holder_of(&b, Rights::ALL);
        let ep = give_endpoint(&a, &e, Rights::SEND);
        let recv_only = give_endpoint(&a, &e, Rights::RECV);
        let ep_as_space = give_endpoint(&a, &e, Rights::ALL);
        assert_eq!(
            refused(&c, Target::Slot(ep_as_space), ep),
            Some(SpaceError::WrongType),
            "an Endpoint where the Space belongs"
        );
        assert_eq!(
            refused(&c, Target::Slot(0), 0),
            Some(SpaceError::WrongType),
            "a Space where the Endpoint belongs"
        );
        assert_eq!(
            refused(&c, Target::Slot(0), recv_only),
            Some(SpaceError::Cap(CapError::Rights)),
            "an endpoint slot without SEND"
        );
        assert_eq!(
            refused(&c, Target::Slot(0), 9),
            Some(SpaceError::Cap(CapError::NoSlot)),
            "an empty endpoint slot"
        );
        assert_eq!(
            refused(&c, Target::Slot(9), ep),
            Some(SpaceError::Cap(CapError::NoSlot)),
            "an empty space slot"
        );

        // A thread with no Space has no "own space".
        assert_eq!(refused(&c, Target::SelfSpace, ep), Some(SpaceError::Dead));

        assert!(b.notify_ep().is_none(), "no refusal set an endpoint");
    }

    #[ktest]
    fn space_notify_self_only_while_unset() {
        let owner = Agent::new_root(2, 0);
        let b = Space::new(Arc::new(VmSpace::new()), owner.clone());
        let (e1, e2) = (Endpoint::new(), Endpoint::new());
        let s1 = give_endpoint(&owner, &e1, Rights::SEND);
        let s2 = give_endpoint(&owner, &e2, Rights::SEND);
        let me = Thread::new_tagged(owner.clone(), 5);
        me.set_process(b.clone());

        assert_eq!(set_notify(&me, Target::SelfSpace, s1), Ok(()));
        assert!(Arc::ptr_eq(&b.notify_ep().unwrap(), &e1));
        assert_eq!(
            set_notify(&me, Target::SelfSpace, s2),
            Err(SpaceError::Denied),
            "a thread cannot redirect what is already set"
        );
        assert!(Arc::ptr_eq(&b.notify_ep().unwrap(), &e1), "still E1");
        // The endpoint is checked before the self rule, so a bad slot is still its own error.
        assert_eq!(
            set_notify(&me, Target::SelfSpace, 9),
            Err(SpaceError::Cap(CapError::NoSlot))
        );

        // A holder with SIGNAL may replace it, and the next note goes to the new endpoint.
        let (a, c) = holder_of(&b, Rights::SIGNAL);
        let holder_ep = give_endpoint(&a, &e2, Rights::SEND);
        assert_eq!(set_notify(&c, Target::Slot(0), holder_ep), Ok(()));
        assert!(Arc::ptr_eq(&b.notify_ep().unwrap(), &e2));
        b.note_exit(&me, Exit::Code(3));
        assert_eq!((e1.queued(), e2.queued()), (0, 1));
        let m = e2.recv(&receiver()).unwrap();
        assert_note_shape(&m, NOTE_EXIT);
        assert_eq!(m.words, [NOTE_KIND_CODE, 3, 0, 0]);
        assert_eq!((m.tid, m.tag), (me.tid(), 5));
    }

    #[ktest]
    fn space_notify_ring3_self_sets_once() {
        let owner = Agent::new_root(5, 0);
        let parent = sibling_parent(PageFlags::RW);
        let e = Endpoint::new();
        // The thread runs as the owner, so slot 0 of the owner's capability space is its slot 0.
        assert_eq!(give_endpoint(&owner, &e, Rights::SEND), 0);
        let me = receiver();

        // No endpoint yet: the first self-set works, the second is refused, and the bin exits with
        // the refusal. The note reaching E at all shows the first call set it.
        let (s, t) = start_sibling(&parent, &owner, None, 0x12, 2);
        let m = recv_note(&e, &me);
        assert_note_shape(&m, NOTE_EXIT);
        assert_eq!(m.words, [NOTE_KIND_CODE, (-errno::EPERM) as u64, 0, 0]);
        assert_eq!((m.tid, m.tag), (t.tid(), 0x12));
        assert_eq!(join_thread(&t), Exit::Code((-errno::EPERM) as usize));
        // The note left before the process went away; once the last thread is reaped the
        // capability set by the first self-call is dropped with it.
        while s.live_threads() != 0 {
            Task::yield_now();
        }
        assert!(s.is_dying());
        assert!(s.notify_ep().is_none());
    }

    /// Whether a page is mapped at `va` in `vm`.
    fn mapped(vm: &VmSpace, va: usize) -> bool {
        let guard = disable_preempt();
        let mut c = vm.cursor(&guard, &(va..va + PAGE_SIZE)).unwrap();
        c.query().unwrap().1.is_some()
    }

    /// Waits, yielding, until the last thread of `space` has been reaped. A thread's exit is
    /// visible a moment before its reap (see `thread_create_limit_is_64`).
    fn wait_reaped(space: &Space) {
        let mut spins = 0usize;
        while space.live_threads() != 0 || !space.is_dying() {
            spins += 1;
            assert!(spins < 1_000_000, "the last thread was never reaped");
            Task::yield_now();
        }
    }

    /// Iterations of the fork-and-exit loop. A module constant so a slow run can be shortened in
    /// one place; the loop prints its wall time (1 jiffy is 1 ms at the 1 kHz tick).
    const ITERATIONS: usize = 10_000;

    /// A child space over a copy-on-write clone of `parent`'s image range, with a fresh stack.
    fn cow_space(parent: &Image, owner: &Arc<Agent>) -> Arc<Space> {
        let space = Space::from_clone(
            &parent.space,
            IMAGE_BASE..TEST_DATA + PAGE_SIZE,
            owner.clone(),
        )
        .expect("from_clone");
        map_stack(space.vm()).unwrap();
        space
    }

    #[ktest]
    fn teardown_loop_same_agent_10000x() {
        install_space_switch();
        let a = Agent::new_root(1, 64);
        let holder = Thread::new(a.clone());
        let parent = sibling_parent(PageFlags::RW);
        let (used, room) = (a.mem_used_pages(), a.cspace.lock().room());

        let t0 = Jiffies::elapsed().as_u64();
        for i in 0..ITERATIONS {
            let child = cow_space(&parent, &a);
            let weak = Arc::downgrade(&child);
            let slot = a
                .cspace
                .lock()
                .insert(Cap::new(Object::Space(child), Rights::CREATE))
                .unwrap();
            // arg 1: store into the test page, which is copy-on-write in the child, then exit 8.
            let start = ThreadStart {
                arg: 1,
                ..spin_at(&parent)
            };
            let (tslot, th) = thread_create(&holder, Target::Slot(slot), start, 0, ok).unwrap();
            assert_eq!(join_thread(&th), Exit::Code(8), "iteration {i}");
            a.cspace.lock().remove(tslot).unwrap();
            a.cspace.lock().remove(slot).unwrap();
            drop(th);
            let mut yields = 0;
            while weak.strong_count() > 0 {
                yields += 1;
                assert!(yields < 1000, "iteration {i}: the space never dropped");
                Task::yield_now();
            }
        }
        let t1 = Jiffies::elapsed().as_u64();
        println!(
            "teardown_loop: iterations={} jiffies={}",
            ITERATIONS,
            t1 - t0
        );

        assert_eq!(a.mem_used_pages(), used, "the loop leaked budget");
        assert_eq!(a.cspace.lock().room(), room, "the loop leaked capabilities");
    }

    #[ktest]
    fn teardown_charges_image_and_stack_pages() {
        install_space_switch();
        let img = load_image(user_image!("fault_sibling")).unwrap();

        // A budget one page short refuses the image and charges nothing.
        let small = Agent::new_root(2, img.pages - 1);
        assert_eq!(
            Space::from_image(&img, small.clone()).err(),
            Some(AgentError::Budget)
        );
        assert_eq!(small.mem_used_pages(), 0);

        let a = Agent::new_root(1, img.pages + 4);
        let before = a.mem_used_pages();
        let space = Space::from_image(&img, a.clone()).unwrap();
        assert_eq!(a.mem_used_pages(), before + img.pages);
        assert_eq!(space.charged(), img.pages);

        let exit = spawn_image_in(&Thread::new(a.clone()), &space, &img, 0);
        assert_eq!(join(&exit), Exit::Code(7));
        wait_reaped(&space);
        assert_eq!(a.mem_used_pages(), before, "the exit gave the pages back");
        assert_eq!(space.charged(), 0);
    }

    #[ktest]
    fn teardown_unmaps_owned_space_and_leaves_a_tombstone() {
        install_space_switch();
        let img = load_image(user_image!("fault_sibling")).unwrap();
        let a = Agent::new_root(1, img.pages);
        let space = Space::from_image(&img, a.clone()).unwrap();
        let ep = Endpoint::new();
        space.set_notify_cap(Cap::new(Object::Endpoint(ep.clone()), Rights::SEND));
        assert!(mapped(space.vm(), IMAGE_BASE));
        assert!(space.notify_ep().is_some());

        let exit = spawn_image_in(&Thread::new(a.clone()), &space, &img, 0);
        assert_eq!(join(&exit), Exit::Code(7));
        wait_reaped(&space);

        assert!(!mapped(space.vm(), IMAGE_BASE), "the image is unmapped");
        assert!(
            !mapped(space.vm(), USER_STACK_TOP - PAGE_SIZE),
            "so is the stack"
        );
        assert_eq!(ep.queued(), 1, "the exit note left before the process went");
        assert!(space.notify_ep().is_none(), "the notify capability is gone");

        // A Space capability still names the tombstone, and it takes no more threads.
        let (h, holder) = holder_of(&space, Rights::ALL);
        let room = h.cspace.lock().room();
        assert_eq!(
            thread_create(&holder, Target::Slot(0), spin_at(&img), 0, ok).err(),
            Some(SpaceError::Dead)
        );
        assert_eq!(
            h.cspace.lock().room(),
            room,
            "the refused call kept nothing"
        );
    }

    #[ktest]
    fn teardown_keeps_an_adopted_space_mapped() {
        install_space_switch();
        let owner = Agent::new_root(1, 4);
        let parent = sibling_parent(PageFlags::RW);
        let vm = Arc::new(
            cow_clone(
                &parent.space,
                IMAGE_BASE..TEST_DATA + PAGE_SIZE,
                &*Agent::new_root(1, 64),
            )
            .expect("cow_clone"),
        );
        map_stack(&vm).unwrap();
        let child = Image {
            space: vm,
            ..parent.clone()
        };
        let space = Space::adopt(child.space.clone(), owner.clone());

        // arg 1 stores into the copy-on-write test page: one copy, charged through the space.
        let exit = spawn_image_in(&Thread::new(owner.clone()), &space, &child, 1);
        assert_eq!(join(&exit), Exit::Code(8));
        wait_reaped(&space);

        // The copy is still mapped in the caller's space, so it stays charged.
        assert_eq!(owner.mem_used_pages(), 1, "the copy stays charged");
        assert_eq!(space.charged(), 1);
        for va in [IMAGE_BASE, TEST_DATA, USER_STACK_TOP - PAGE_SIZE] {
            assert!(mapped(&child.space, va), "the caller still maps {va:#x}");
        }
    }

    /// A space that owns a loaded `spinner` image over a test page the harness keeps a handle on.
    fn owned_spinner(owner: &Arc<Agent>) -> (Arc<Space>, Image, UFrame) {
        let img = load_image(user_image!("spinner")).unwrap();
        let page: UFrame = FrameAllocOptions::new().alloc_frame().unwrap().into();
        map_test_page(&img.space, page.clone(), PageFlags::RW);
        (Space::new(img.space.clone(), owner.clone()), img, page)
    }

    /// Registers a thread tagged `tag` in `space` and starts it at `start`.
    fn start_in(
        space: &Arc<Space>,
        owner: &Arc<Agent>,
        tag: u64,
        start: ThreadStart,
    ) -> Arc<Thread> {
        let th = Thread::new_tagged(owner.clone(), tag);
        space.register(&th).unwrap();
        spawn_thread(space, th.clone(), start);
        th
    }

    #[ktest]
    fn kill_stops_a_running_spinner() {
        install_space_switch();
        let owner = Agent::new_root(1, 0);
        let (space, img, _page) = owned_spinner(&owner);
        let ep = Endpoint::new();
        space.set_notify_cap(Cap::new(Object::Endpoint(ep.clone()), Rights::SEND));
        let th = start_in(&space, &owner, 0x21, spin_at(&img));

        space.kill();
        assert_eq!(join_thread(&th), Exit::Killed);
        let m = recv_note(&ep, &receiver());
        assert_note_shape(&m, NOTE_EXIT);
        assert_eq!(m.words, [NOTE_KIND_KILLED, 0, 0, 0]);
        assert_eq!((m.tid, m.tag), (th.tid(), 0x21));
        wait_reaped(&space);
        assert!(!mapped(&img.space, IMAGE_BASE), "the process is torn down");
        assert_eq!(space.live_threads(), 0);
    }

    #[ktest]
    fn kill_wakes_the_dead_process_futex_waiters() {
        install_space_switch();
        let owner = Agent::new_root(1, 0);
        let img = load_image(user_image!("futex_waiter")).unwrap();
        let word: UFrame = FrameAllocOptions::new().alloc_frame().unwrap().into();
        word.write_val(0, &0x11u32).unwrap();
        map_test_page(&img.space, word.clone(), PageFlags::RW);
        let space = Space::new(img.space.clone(), owner.clone());
        // Q maps the same frame, so a wake from Q reaches whatever S left in the futex table.
        let q = VmSpace::new();
        map_test_page(&q, word, PageFlags::RW);

        let at = |back: usize| ThreadStart {
            rip: img.entry as u64,
            rsp: (USER_STACK_TOP - back) as u64,
            arg: 0,
            fs_base: 0,
            clear_tid: 0,
        };
        let t1 = start_in(&space, &owner, 0, at(0));
        let t2 = start_in(&space, &owner, 0, at(PAGE_SIZE));
        let mut spins = 0usize;
        while !(t1.is_blocked() && t2.is_blocked()) {
            spins += 1;
            assert!(spins < 1_000_000, "the waiters never reached the futex");
            Task::yield_now();
        }

        space.kill();
        assert_eq!(join_thread(&t1), Exit::Killed);
        assert_eq!(join_thread(&t2), Exit::Killed);
        let waker = Thread::new(Agent::new_root(2, 0));
        assert_eq!(
            crate::futex::wake(&waker, &q, TEST_DATA, 10),
            Ok(0),
            "no waiter was left in the table"
        );
    }

    #[ktest]
    fn kill_frees_a_thread_in_an_uninterruptible_reply_wait() {
        install_space_switch();
        let ep = Endpoint::new();
        let agent = Agent::new_root(3, 0);
        let slot = give_endpoint(&agent, &ep, Rights::SEND);
        // `nullipc` calls the endpoint in slot `arg` until a call fails.
        let img = load_image(user_image!("nullipc")).unwrap();
        let space = Space::new(img.space.clone(), agent.clone());
        let caller = Thread::new(agent);
        let exit = spawn_image_in(&caller, &space, &img, slot as u64);

        // A kernel task takes the call and never answers it.
        let server = receiver();
        let m = ep.recv(&server).unwrap();
        let reply = m.reply.clone().expect("a call carries its reply");
        testkit::wait_blocked(&caller, &exit);
        caller.interrupt(img.handler);
        for _ in 0..50 {
            Task::yield_now();
        }
        assert!(
            caller.is_blocked() && exit.lock().is_none(),
            "an interrupted caller whose request a server holds waits for the reply"
        );

        space.kill();
        assert_eq!(join(&exit), Exit::Killed);
        assert!(!caller.is_blocked());

        // The server's late answer has nobody to go to and must not panic.
        reply.reply([1, 2, 3, 4]);
        assert_eq!(ep.queued(), 0);
        assert!(!caller.is_blocked());
    }

    #[ktest]
    fn dead_server_answers_parked_callers_esrch() {
        install_space_switch();
        let ep = Endpoint::new();
        let agent = Agent::new_root(4, 0);
        let rx = give_endpoint(&agent, &ep, Rights::RECV);
        // `park_srv` receives a call, parks it, then waits for a second one.
        let img = load_image(user_image!("park_srv")).unwrap();
        let space = Space::new(img.space.clone(), agent.clone());
        let server = Thread::new(agent);
        let exit = spawn_image_in(&server, &space, &img, rx as u64);

        let caller = Thread::new(Agent::new_root(5, 0));
        let out: Arc<SpinLock<Option<Result<[u64; 4], IpcError>>>> = Arc::new(SpinLock::new(None));
        let (e2, c2, o2) = (ep.clone(), caller.clone(), out.clone());
        TaskOptions::new(move || *o2.lock() = Some(e2.call(&c2, 7, [7, 0, 0, 0])))
            .spawn()
            .unwrap();
        let mut spins = 0usize;
        while server.replies.lock().len() < 1 {
            spins += 1;
            assert!(spins < 1_000_000, "the server never parked the call");
            Task::yield_now();
        }
        assert!(out.lock().is_none(), "the caller is still waiting");

        space.kill();
        assert_eq!(join(&exit), Exit::Killed);
        let answer = loop {
            if let Some(r) = out.lock().take() {
                break r;
            }
            Task::yield_now();
        };
        assert_eq!(answer, Err(IpcError::Dead));
        assert_eq!(Errno::from(IpcError::Dead), Errno::ESRCH);
        assert_eq!(
            server.replies.lock().len(),
            0,
            "the dead server holds nothing"
        );
    }

    #[ktest]
    fn kill_before_first_run_never_enters_user_mode() {
        install_space_switch();
        let owner = Agent::new_root(1, 0);
        let (space, img, page) = owned_spinner(&owner);
        let th = Thread::new(owner.clone());
        space.register(&th).unwrap();
        space.kill();
        spawn_thread(&space, th.clone(), spin_at(&img));

        assert_eq!(join_thread(&th), Exit::Killed);
        // The spinner stores its id in word 1 and counts in word 0 as soon as it runs.
        assert_eq!(page.read_val::<u64>(0).unwrap(), 0, "it never ran");
    }

    #[ktest]
    fn killed_space_refuses_thread_create() {
        install_space_switch();
        let owner = Agent::new_root(2, 0);
        let (b, img, _page) = owned_spinner(&owner);
        let (_a, caller) = holder_of(&b, Rights::CREATE.union(Rights::SIGNAL));

        b.kill();
        assert!(b.is_dying());
        assert!(
            !mapped(&img.space, IMAGE_BASE),
            "killing a space with no live thread tears it down"
        );
        let r = thread_create(&caller, Target::Slot(0), spin_at(&img), 0, ok);
        assert_eq!(r.err(), Some(SpaceError::Dead));
        assert_eq!(Errno::from(SpaceError::Dead), Errno::ESRCH);
        assert_eq!(b.live_threads(), 0);
    }
}
