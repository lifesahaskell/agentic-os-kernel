//! The process as a kernel object: a `Space` is an address space plus the agent that owns it, and
//! the threads running in it. A Space capability names it; `thread_create` starts a thread in it
//! and `thread_interrupt` redirects one through a Thread capability.
//!
//! Every thread's credential is its space owner's, never its creator's: a thread started in
//! another space does not run with the creator's authority (OD-11).

use abi::USER_TOP;
use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};

use ostd::{
    mm::{PAGE_SIZE, VmSpace},
    sync::SpinLock,
};

use crate::{
    agent::Agent,
    cap::{Cap, CapError, Object, Rights},
    thread::Thread,
    user,
};

/// Most live threads one space holds. The 65th `thread_create` is refused (`Limit`).
pub const MAX_THREADS_PER_SPACE: usize = 64;

pub struct Space {
    vm: Arc<VmSpace>,
    owner: Arc<Agent>,
    /// `(tid, thread)` of every thread started in this space and not yet reaped. Weak, so a
    /// registered thread never outlives its own last reference because of this list.
    threads: SpinLock<Vec<(u64, Weak<Thread>)>>,
}

impl Space {
    pub fn new(vm: Arc<VmSpace>, owner: Arc<Agent>) -> Arc<Space> {
        Arc::new(Space {
            vm,
            owner,
            threads: SpinLock::new(Vec::new()),
        })
    }

    pub fn vm(&self) -> &Arc<VmSpace> {
        &self.vm
    }

    /// The agent every thread in this space runs as.
    pub fn owner(&self) -> &Arc<Agent> {
        &self.owner
    }

    /// Adds `th` to the space. Entries of threads that no longer exist are dropped first;
    /// `Limit` when `MAX_THREADS_PER_SPACE` threads are still registered.
    pub fn register(&self, th: &Thread) -> Result<(), SpaceError> {
        let mut threads = self.threads.lock();
        threads.retain(|(_, w)| w.strong_count() > 0);
        if threads.len() >= MAX_THREADS_PER_SPACE {
            return Err(SpaceError::Limit);
        }
        threads.push((th.tid(), th.downgrade()));
        Ok(())
    }

    /// Removes thread `tid` from the space.
    pub fn reap(&self, tid: u64) {
        self.threads.lock().retain(|(t, _)| *t != tid);
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
            space.reap(th.tid());
            return Err(SpaceError::NoRoom);
        }
    };
    if let Err(e) = publish(th.tid()) {
        let _ = caller.agent().cspace.lock().remove(slot);
        space.reap(th.tid());
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

#[cfg(ktest)]
mod test {
    use abi::USER_STACK_TOP;
    use ostd::{
        mm::{FrameAllocOptions, PageFlags, VmIo},
        prelude::*,
        task::{Task, TaskOptions},
    };

    use super::*;
    use crate::{
        ipc::Endpoint,
        sched::install_space_switch,
        user::{
            Exit, Image, load_image, map_test_page,
            testkit::{self, join, join_thread, spawn_image},
            user_image,
        },
    };

    /// A space running the `spinner` image under `owner`, with the test page it reads.
    fn spinner_space(owner: &Arc<Agent>) -> (Arc<Space>, Image) {
        let img = load_image(user_image!("spinner")).unwrap();
        map_test_page(
            &img.space,
            FrameAllocOptions::new().alloc_frame().unwrap().into(),
            PageFlags::RW,
        );
        (Space::new(img.space.clone(), owner.clone()), img)
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
        let again = thread_create(&caller, Target::Slot(0), spin_at(&img), 0, ok).unwrap();
        assert_eq!(b.live_threads(), 1);
        stop_all(&caller, &[again], img.handler);
        assert_eq!(b.live_threads(), 0);
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
}
