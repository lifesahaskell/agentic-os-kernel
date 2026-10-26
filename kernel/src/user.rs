//! Runs a thread in user mode: syscalls, CoW faults, and redirects applied between `execute`
//! calls. The syscall numbers live in the shared `abi` crate.

use core::ops::Range;

use abi::{
    IMAGE_BASE, IMAGE_HEADER_LEN, IMAGE_MAGIC, MAX_IMAGE_SIZE, NOTE_FAULT, TEST_DATA,
    USER_STACK_PAGES, USER_STACK_TOP,
};
use alloc::sync::Arc;

use ostd::{
    arch::cpu::context::{CpuException, FsBase, PageFaultErrorCode, UserContext},
    mm::{
        CachePolicy, FrameAllocOptions, PAGE_SIZE, PageFlags, PageProperty, UFrame, Vaddr, VmIo,
        VmSpace,
    },
    task::{TaskOptions, disable_preempt},
    user::{ReturnReason, UserMode},
};

use crate::{
    ipc::IpcError,
    space::{Space, ThreadStart},
    syscall::{self, Outcome},
    thread::Thread,
    vm::handle_cow_fault,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Code(usize),
    /// A fault that is not copy-on-write.
    Fault(Vaddr),
    /// A copy-on-write fault the agent's memory budget could not pay for.
    Budget,
    /// A non-page-fault exception (x86 vector) that no handler took.
    Exception(u64),
    /// Killed from outside (`Space::kill`), not by anything the thread did.
    Killed,
}

/// A loaded flat image: the space holds the mapped image and the user stack.
#[derive(Clone)]
pub struct Image {
    pub space: Arc<VmSpace>,
    pub entry: Vaddr,
    pub handler: Vaddr,
    pub text: Range<Vaddr>,
    /// Frames `load_image` allocated: the segments and the stack. What `Space::from_image`
    /// charges.
    pub pages: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    /// Too short for the header, or the magic is wrong.
    Header,
    /// The header's addresses are inconsistent or exceed the limits.
    Layout,
    /// A frame allocation failed.
    NoMemory,
}

/// Maps a flat image (see `abi::IMAGE_MAGIC`) into a new space: every page of
/// `[IMAGE_BASE, mem_end)` with the matching bytes (zero beyond the file), RX below `rx_end` and
/// RW above, plus `USER_STACK_PAGES` zeroed RW pages ending at `USER_STACK_TOP`. Nothing is
/// mapped until the header has passed every check.
pub fn load_image(bytes: &[u8]) -> Result<Image, LoadError> {
    if bytes.len() < IMAGE_HEADER_LEN {
        return Err(LoadError::Header);
    }
    let quad = |i: usize| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
    if quad(0) != IMAGE_MAGIC {
        return Err(LoadError::Header);
    }
    let [entry, handler, rx_end, mem_end] = [1, 2, 3, 4].map(|i| quad(i) as usize);
    let text = IMAGE_BASE..rx_end;
    let layout_ok = IMAGE_BASE < rx_end
        && rx_end <= mem_end
        && rx_end % PAGE_SIZE == 0
        && mem_end - IMAGE_BASE <= MAX_IMAGE_SIZE
        && bytes.len() <= mem_end - IMAGE_BASE
        && text.contains(&entry)
        && text.contains(&handler);
    // The header values are u64 on a 64-bit target, so the `as usize` casts above are lossless.
    if !layout_ok {
        return Err(LoadError::Layout);
    }

    let space = VmSpace::new();
    let guard = disable_preempt();
    let image_end = mem_end.next_multiple_of(PAGE_SIZE);
    let mut c = space
        .cursor_mut(&guard, &(IMAGE_BASE..image_end))
        .map_err(|_| LoadError::Layout)?;
    for va in (IMAGE_BASE..image_end).step_by(PAGE_SIZE) {
        let frame = FrameAllocOptions::new()
            .alloc_frame()
            .map_err(|_| LoadError::NoMemory)?;
        let off = va - IMAGE_BASE;
        if off < bytes.len() {
            frame
                .write_bytes(0, &bytes[off..bytes.len().min(off + PAGE_SIZE)])
                .unwrap();
        }
        let flags = if va < rx_end {
            PageFlags::RX
        } else {
            PageFlags::RW
        };
        c.map(
            frame.into(),
            PageProperty::new_user(flags, CachePolicy::Writeback),
        );
    }
    drop(c);
    drop(guard);
    map_stack(&space)?;
    Ok(Image {
        space: Arc::new(space),
        entry,
        handler,
        text,
        pages: (image_end - IMAGE_BASE) / PAGE_SIZE + USER_STACK_PAGES,
    })
}

/// Maps `USER_STACK_PAGES` zeroed RW pages ending at `USER_STACK_TOP`.
pub fn map_stack(space: &VmSpace) -> Result<(), LoadError> {
    let guard = disable_preempt();
    let stack = USER_STACK_TOP - USER_STACK_PAGES * PAGE_SIZE..USER_STACK_TOP;
    let mut c = space
        .cursor_mut(&guard, &stack)
        .map_err(|_| LoadError::Layout)?;
    for _ in 0..USER_STACK_PAGES {
        let frame = FrameAllocOptions::new()
            .alloc_frame()
            .map_err(|_| LoadError::NoMemory)?;
        c.map(
            frame.into(),
            PageProperty::new_user(PageFlags::RW, CachePolicy::Writeback),
        );
    }
    Ok(())
}

/// Maps `frame` (shared, not copied) at `abi::TEST_DATA`, the page `libsys::test_page` reads.
/// Test programs and the boot checks use it for memory the harness can inspect.
pub fn map_test_page(space: &VmSpace, frame: UFrame, flags: PageFlags) {
    let guard = disable_preempt();
    let at = TEST_DATA..TEST_DATA + PAGE_SIZE;
    let mut c = space.cursor_mut(&guard, &at).unwrap();
    c.map(frame, PageProperty::new_user(flags, CachePolicy::Writeback));
}

/// The registers for running `img` from its entry point with `arg` in `rdi` and the stack at the
/// top of the mapped stack region.
fn entry_ctx(img: &Image, arg: u64) -> UserContext {
    let mut ctx = UserContext::default();
    ctx.set_rip(img.entry);
    ctx.set_rsp(USER_STACK_TOP);
    ctx.set_rdi(arg as usize);
    ctx
}

/// Runs `img` on `thread` from its entry point with `arg` in `rdi` and the stack at the top of
/// the mapped stack region.
pub fn start(thread: &Thread, img: &Image, arg: u64) -> Exit {
    start_ctx(thread, &img.space, entry_ctx(img, arg))
}

/// Runs `ctx` on `thread` in a new process that wraps `vm` and is owned by the thread's agent. The
/// caller keeps `vm` and its memory, so the process does not unmap it at teardown (`adopt`).
fn start_ctx(thread: &Thread, vm: &Arc<VmSpace>, ctx: UserContext) -> Exit {
    let space = Space::adopt(vm.clone(), thread.agent().clone());
    run_registered(thread, &space, ctx)
}

/// Runs `ctx` on `thread` as a thread of `space`, from registering it to reaping it.
fn run_registered(thread: &Thread, space: &Arc<Space>, ctx: UserContext) -> Exit {
    thread.clear_exit();
    space
        .register(thread)
        .expect("the space has room for this thread");
    let exit = run_ctx(thread, space, ctx);
    // Recorded, then announced while the space still counts the thread, then reaped.
    thread.set_exit(exit);
    space.note_exit(thread, exit);
    thread.retire();
    space.reap(thread.tid());
    exit
}

/// Starts a thread that `space::thread_create` made: it runs `start` in `space` on its own task
/// and, when its run ends, leaves the space's thread list and records its exit.
pub fn spawn_thread(space: &Arc<Space>, th: Arc<Thread>, start: ThreadStart) {
    let mut ctx = UserContext::default();
    ctx.set_rip(start.rip as usize);
    ctx.set_rsp(start.rsp as usize);
    ctx.set_rdi(start.arg as usize);
    th.set_fs_base(start.fs_base as usize);
    let (space, t) = (space.clone(), th.clone());
    TaskOptions::new(move || {
        let exit = run_ctx(&t, &space, ctx);
        // Record the exit, announce it while the space still counts the thread, then reap. A
        // joiner that sees the exit may therefore still see the thread registered for an instant.
        t.set_exit(exit);
        space.note_exit(&t, exit);
        t.retire();
        space.reap(t.tid());
    })
    .data(th)
    .spawn()
    .expect("spawning a thread task");
}

/// The bytes of `userland/out/<name>.bin`, built by `userland/build.sh` before `cargo osdk`.
macro_rules! user_image {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../userland/out/",
            $name,
            ".bin"
        ))
    };
}
pub(crate) use user_image;

/// x86 vector of an exception, for the fault message.
fn vector(exc: &CpuException) -> u64 {
    use CpuException::*;
    match exc {
        DivisionError => 0,
        Debug => 1,
        NonMaskableInterrupt => 2,
        BreakPoint => 3,
        Overflow => 4,
        BoundRangeExceeded => 5,
        InvalidOpcode => 6,
        DeviceNotAvailable => 7,
        DoubleFault => 8,
        CoprocessorSegmentOverrun => 9,
        InvalidTss(_) => 10,
        SegmentNotPresent(_) => 11,
        StackSegmentFault(_) => 12,
        GeneralProtectionFault(_) => 13,
        PageFault(_) => 14,
        X87FloatingPointException => 16,
        AlignmentCheck => 17,
        MachineCheck => 18,
        SIMDFloatingPointException => 19,
        VirtualizationException => 20,
        ControlProtectionException => 21,
        HypervisorInjectionException => 28,
        VMMCommunicationException => 29,
        SecurityException => 30,
        _ => u64::MAX,
    }
}

/// Calls the thread's space's notify endpoint with a `NOTE_FAULT` note, `[vector, fault address,
/// rip, page-fault error code]`, stamped with the thread's credential, tid and tag, and waits for
/// the reply: the verdict. `None`: carry on, the verdict was zero (resume) or the thread was
/// interrupted (the caller applies the redirect). `Some`: the fault is fatal, by a non-zero
/// verdict or because no endpoint could take the note (OD-06).
fn deliver(thread: &Thread, exc: CpuException, rip: usize) -> Option<Exit> {
    let vector = vector(&exc);
    let (addr, err) = match exc {
        CpuException::PageFault(pf) => (pf.addr, pf.error_code.bits()),
        _ => (0, 0),
    };
    let fatal = || {
        Some(match exc {
            CpuException::PageFault(pf) => Exit::Fault(pf.addr),
            _ => Exit::Exception(vector),
        })
    };
    let Some(ep) = thread.process().and_then(|p| p.notify_ep()) else {
        return fatal();
    };
    let words = [vector, addr as u64, rip as u64, err as u64];
    match ep.call_note(thread, NOTE_FAULT, words) {
        Ok(verdict) if verdict[0] == 0 => None,
        Ok(_) => fatal(),
        // The thread was interrupted: the note is withdrawn or already answered, and the caller
        // applies the redirect.
        Err(IpcError::Interrupted) => None,
        Err(_) => fatal(),
    }
}

fn run_ctx(thread: &Thread, process: &Arc<Space>, ctx: UserContext) -> Exit {
    thread.set_process(process.clone());
    let space = process.vm();
    space.activate();
    let mut um = UserMode::new(ctx);
    loop {
        // Before every entry to ring 3, and so after every syscall, fault and kernel event: a
        // killed thread never runs user code again.
        if thread.is_killed() {
            return Exit::Killed;
        }
        let reason = um.execute(thread);
        // Ring 3 may have moved FS (OSTD turns on FSGSBASE): keep it for the next entry.
        let mut fs = FsBase::default();
        fs.save();
        thread.set_fs_base(fs.addr());
        // Killed while in ring 3: whatever brought it back (a kernel event, a syscall it issued
        // as the kill landed) is not served.
        if thread.is_killed() {
            return Exit::Killed;
        }
        let ctx = um.context_mut();
        match reason {
            ReturnReason::UserSyscall => {
                let a = [
                    ctx.rdi(),
                    ctx.rsi(),
                    ctx.rdx(),
                    ctx.r10(),
                    ctx.r8(),
                    ctx.r9(),
                ];
                match syscall::decode(ctx.rax(), a) {
                    Err(e) => ctx.set_rax(-e.0 as usize),
                    Ok(call) => match syscall::dispatch(thread, space, call) {
                        Outcome::Return(v) => ctx.set_rax(v as usize),
                        Outcome::Exit(code) => return Exit::Code(code as usize),
                        // The frame replaces the whole context, `rax` included: the interrupted
                        // code resumes exactly as the redirect found it.
                        Outcome::Sigreturn(f) => thread.restore_frame(ctx, f),
                    },
                }
            }
            ReturnReason::UserException => match ctx.take_exception() {
                Some(exc) => {
                    let cow = match exc {
                        CpuException::PageFault(pf)
                            if pf.error_code.contains(PageFaultErrorCode::WRITE) =>
                        {
                            Some(handle_cow_fault(space, &**process, pf.addr))
                        }
                        _ => None,
                    };
                    match cow {
                        Some(Ok(true)) => {}
                        // shortcut: Exit has no out-of-memory cause, so a CoW break the allocator
                        // cannot serve also ends as Exit::Budget; split it when exit causes reach
                        // procmgr.
                        Some(Err(_)) => return Exit::Budget,
                        _ => {
                            if let Some(exit) = deliver(thread, exc, ctx.rip()) {
                                return exit;
                            }
                        }
                    }
                }
                None => panic!("user exception without exception info"),
            },
            ReturnReason::KernelEvent => {}
        }
        // Signal-style redirect: the interrupted context goes on the thread's kernel-held frame
        // stack and the thread resumes at the handler with the interrupted pc in rdi. Nothing is
        // written to user memory; `SYS_SIGRETURN` pops the frame.
        thread.apply_redirect(ctx);
    }
}

#[cfg(ktest)]
pub(crate) mod testkit {
    use ostd::{
        sync::SpinLock,
        task::{Task, TaskOptions},
        timer::Jiffies,
    };

    use super::*;
    use crate::{
        ipc::{Endpoint, Message},
        sched::install_space_switch,
    };

    pub type ExitSlot = Arc<SpinLock<Option<Exit>>>;

    /// Runs `img` with `arg` on a new task and returns where its exit will appear.
    pub fn spawn_image(t: &Arc<Thread>, img: &Image, arg: u64) -> ExitSlot {
        install_space_switch();
        let exit: ExitSlot = Arc::new(SpinLock::new(None));
        let (t2, img, e) = (t.clone(), img.clone(), exit.clone());
        TaskOptions::new(move || *e.lock() = Some(start(&t2, &img, arg)))
            .data(t.clone())
            .spawn()
            .unwrap();
        exit
    }

    /// Like `spawn_image`, but `t` runs as the first thread of `space`, so a notify capability set
    /// on `space` is the one its faults and its exit go to. `img` supplies the entry point only;
    /// `space` must map it.
    pub fn spawn_image_in(t: &Arc<Thread>, space: &Arc<Space>, img: &Image, arg: u64) -> ExitSlot {
        install_space_switch();
        let exit: ExitSlot = Arc::new(SpinLock::new(None));
        let (t2, space, e) = (t.clone(), space.clone(), exit.clone());
        let ctx = entry_ctx(img, arg);
        TaskOptions::new(move || *e.lock() = Some(run_registered(&t2, &space, ctx)))
            .data(t.clone())
            .spawn()
            .unwrap();
        exit
    }

    /// The next message on `ep`. A note that never comes ends the test with a panic, not a hang.
    pub fn recv_note(ep: &Endpoint, me: &Thread) -> Message {
        let deadline = Jiffies::elapsed().as_u64() + 1000;
        ep.recv_until(me, Some(deadline))
            .expect("no note arrived within the wait")
    }

    pub fn join(e: &ExitSlot) -> Exit {
        loop {
            if let Some(x) = e.lock().take() {
                return x;
            }
            Task::yield_now();
        }
    }

    /// Waits, yielding, until `t`'s run has ended, and returns how. For a thread whose task
    /// `space::thread_create` spawned, so there is no `ExitSlot` to join.
    pub fn join_thread(t: &Arc<Thread>) -> Exit {
        loop {
            if let Some(x) = t.exit_status() {
                return x;
            }
            Task::yield_now();
        }
    }

    /// Returns once `t` is blocked in the kernel, or its run already ended.
    pub fn wait_blocked(t: &Thread, e: &ExitSlot) {
        while !t.is_blocked() && e.lock().is_none() {
            Task::yield_now();
        }
    }
}

#[cfg(ktest)]
mod test {
    use alloc::{vec, vec::Vec};

    use ostd::{
        mm::{FallibleVmRead, FrameAllocOptions, PAGE_SIZE, PageFlags, UFrame, VmIo, VmWriter},
        prelude::*,
        sync::{LocalIrqDisabled, SpinLock},
        task::{Task, TaskOptions, disable_preempt},
        timer::{Jiffies, register_callback_on_cpu},
    };

    use abi::NOTE_FAULT;

    use super::*;
    use crate::{
        agent::Agent,
        cap::{Cap, Object, Rights},
        futex,
        ipc::{Endpoint, Message, sys_recv},
        space::{SpaceError, thread_interrupt},
        syscall::Errno,
        user::testkit::{join, join_thread, recv_note, spawn_image, spawn_image_in, wait_blocked},
        vm::cow_clone,
    };

    const DATA: Vaddr = TEST_DATA;

    /// `bytes` loaded, with `data` (shared, not copied) at `DATA`, or `DATA` unmapped.
    fn image_over(bytes: &[u8], data: Option<(&UFrame, PageFlags)>) -> Image {
        let img = load_image(bytes).unwrap();
        if let Some((frame, flags)) = data {
            map_test_page(&img.space, frame.clone(), flags);
        }
        img
    }

    /// `bytes` loaded, with a data page holding 0x11 at `DATA`.
    fn new_image(bytes: &[u8], data_flags: PageFlags) -> Image {
        image_over(bytes, Some((&data_frame(0x11), data_flags)))
    }

    /// Clones the image part of `parent` (text, data and the test page) copy-on-write and gives the
    /// clone a fresh stack, so the only pages the child can copy are the ones it writes in the
    /// cloned range.
    fn cow_child(parent: &Image) -> Image {
        let child = Arc::new(
            cow_clone(
                &parent.space,
                abi::IMAGE_BASE..DATA + PAGE_SIZE,
                &*Agent::new_root(1, 64),
            )
            .expect("cow_clone"),
        );
        map_stack(&child).unwrap();
        Image {
            space: child,
            ..parent.clone()
        }
    }

    fn data_byte(space: &Arc<VmSpace>) -> u8 {
        space.activate();
        let mut buf = vec![0u8; 1];
        let mut r = space.reader(DATA, 1).unwrap();
        r.read_fallible(&mut VmWriter::from(&mut buf[..])).unwrap();
        buf[0]
    }

    fn data_flags(space: &VmSpace) -> PageFlags {
        let guard = disable_preempt();
        let mut c = space.cursor(&guard, &(DATA..DATA + PAGE_SIZE)).unwrap();
        c.query().unwrap().1.unwrap().prop().flags
    }

    #[ktest]
    fn cow_clone_isolates_writes_and_charges_the_writer() {
        let agent = Agent::new_root(1, 4);
        let parent = new_image(user_image!("cow_writer"), PageFlags::RW);
        let child = cow_child(&parent);
        assert!(
            !data_flags(&parent.space).contains(PageFlags::W),
            "parent write-protected"
        );
        assert!(!data_flags(&child.space).contains(PageFlags::W));

        let t = Thread::new(agent.clone());
        assert_eq!(start(&t, &child, 0), Exit::Code(0));
        assert_eq!(data_byte(&child.space), 0x42);
        assert_eq!(data_byte(&parent.space), 0x11, "parent unchanged");
        assert_eq!(
            agent.mem_used_pages(),
            1,
            "one page copied, charged to the writer"
        );
        assert!(data_flags(&child.space).contains(PageFlags::W));

        // The parent still faults once, then owns a writable page again.
        assert_eq!(start(&t, &parent, 0), Exit::Code(0));
        assert_eq!(data_byte(&parent.space), 0x42);
        assert!(data_flags(&parent.space).contains(PageFlags::W));
    }

    #[ktest]
    fn cow_fault_over_budget_stops_the_thread() {
        let parent = new_image(user_image!("cow_writer"), PageFlags::RW);
        let child = cow_child(&parent);
        let t = Thread::new(Agent::new_root(1, 0));
        assert_eq!(start(&t, &child, 0), Exit::Budget);
        assert_eq!(data_byte(&child.space), 0x11);
        assert_eq!(t.agent().mem_used_pages(), 0);
    }

    #[ktest]
    fn write_to_read_only_page_is_a_fault_not_cow() {
        let img = new_image(user_image!("cow_writer"), PageFlags::R);
        let t = Thread::new(Agent::new_root(1, 4));
        assert_eq!(start(&t, &img, 0), Exit::Fault(DATA));
    }

    /// Like `start`, but with the initial `rsp` chosen by the caller.
    fn start_with_sp(thread: &Thread, img: &Image, sp: usize) -> Exit {
        let mut ctx = UserContext::default();
        ctx.set_rip(img.entry);
        ctx.set_rsp(sp);
        ctx.set_rdi(0);
        start_ctx(thread, &img.space, ctx)
    }

    /// CR-01: `_start` aligns `rsp` and `call`s, and the call's return-address push is an 8-byte
    /// ring-3 store to `rsp - 8`. With `rsp` in the kernel half that store is a write fault the
    /// kernel must report as `Exit::Fault`, not die on.
    #[ktest]
    fn ring3_store_to_kernel_half_is_a_fault() {
        let img = load_image(user_image!("kpush")).unwrap();
        let t = Thread::new(Agent::new_root(1, 4));
        assert_eq!(
            start_with_sp(&t, &img, 0xffff_8000_0000_0010),
            Exit::Fault(0xffff_8000_0000_0008)
        );
        // The top page, where `page + PAGE_SIZE` overflows.
        assert_eq!(
            start_with_sp(&t, &img, 0),
            Exit::Fault(0xffff_ffff_ffff_fff8)
        );
        // The kernel survived and still runs ring 3.
        assert_eq!(start(&t, &img, 0), Exit::Code(0x5afe));
        assert_eq!(t.agent().mem_used_pages(), 0);
    }

    #[ktest]
    fn timer_interrupt_redirects_a_running_thread() {
        let img = load_image(user_image!("spin")).unwrap();
        let t = Thread::new(Agent::new_root(1, 0));
        let alarm: Arc<SpinLock<Option<Arc<Thread>>, LocalIrqDisabled>> =
            Arc::new(SpinLock::new(Some(t.clone())));
        let deadline = Jiffies::elapsed().as_u64() + 5;
        let a = alarm.clone();
        let handler = img.handler;
        register_callback_on_cpu(move || {
            if Jiffies::elapsed().as_u64() >= deadline {
                if let Some(t) = a.lock().take() {
                    t.interrupt(handler);
                }
            }
        });

        // `spin` never traps on its own; only the redirect can make it exit, with its pc in rdi.
        let Exit::Code(pc) = start(&t, &img, 0) else {
            panic!("spin did not exit through the redirect handler");
        };
        assert!(img.text.contains(&pc), "pc {pc:#x} outside the image text");
        assert!(alarm.lock().is_none());
    }

    /// `sigreturn` mode 1 spins until its handler runs; the handler returns through
    /// `SYS_SIGRETURN`, so the loop resumes, sees the flag and exits 42.
    #[ktest]
    fn sigreturn_resumes_the_interrupted_loop() {
        let img = load_image(user_image!("sigreturn")).unwrap();
        let t = Thread::new(Agent::new_root(1, 0));
        let exit = spawn_image(&t, &img, 1);
        for _ in 0..50 {
            Task::yield_now();
        }
        t.interrupt(img.handler);
        assert_eq!(join(&exit), Exit::Code(42));
        assert_eq!(t.frame_depth(), 0);
    }

    #[ktest]
    fn sigreturn_on_an_empty_stack_is_einval() {
        let img = load_image(user_image!("sigreturn")).unwrap();
        let t = Thread::new(Agent::new_root(1, 0));
        let exit = spawn_image(&t, &img, 3);
        assert_eq!(join(&exit), Exit::Code(abi::errno::EINVAL as usize));
        assert_eq!(t.frame_depth(), 0);
    }

    /// Runs the `sigframe` probe with a timer-driven interrupt: the probe has no syscalls to
    /// yield in, and the scheduler does not preempt, so the interrupt must come from the tick.
    /// The delay is long enough that the probe is spinning with every register loaded.
    fn run_sigframe() -> Exit {
        let img = load_image(user_image!("sigframe")).unwrap();
        let t = Thread::new(Agent::new_root(1, 0));
        let alarm: Arc<SpinLock<Option<Arc<Thread>>, LocalIrqDisabled>> =
            Arc::new(SpinLock::new(Some(t.clone())));
        let deadline = Jiffies::elapsed().as_u64() + 30;
        let a = alarm.clone();
        let handler = img.handler;
        register_callback_on_cpu(move || {
            if Jiffies::elapsed().as_u64() >= deadline {
                if let Some(t) = a.lock().take() {
                    t.interrupt(handler);
                }
            }
        });
        let exit = start(&t, &img, 0);
        assert!(alarm.lock().is_none(), "the interrupt never fired");
        assert_eq!(t.frame_depth(), 0);
        exit
    }

    /// Resets `CORRUPT_FRAME` when a control test ends, however it ends.
    struct Uncorrupt;
    impl Drop for Uncorrupt {
        fn drop(&mut self) {
            crate::thread::CORRUPT_FRAME.store(0, core::sync::atomic::Ordering::Relaxed);
        }
    }

    #[ktest]
    fn sigreturn_restores_all_gprs_fpu_and_leaves_the_user_stack_untouched() {
        assert_eq!(run_sigframe(), Exit::Code(0));
    }

    /// Negative control: a restore that corrupts rbx must make the probe report rbx (1 + 1).
    #[ktest]
    fn sigreturn_probe_detects_a_corrupted_rbx() {
        let _guard = Uncorrupt;
        crate::thread::CORRUPT_FRAME.store(1, core::sync::atomic::Ordering::Relaxed);
        assert_eq!(run_sigframe(), Exit::Code(2));
    }

    /// Negative control: a restore that skips the FPU write leaves the handler's zeroed xmm0.
    #[ktest]
    fn sigreturn_probe_detects_a_skipped_fpu_restore() {
        let _guard = Uncorrupt;
        crate::thread::CORRUPT_FRAME.store(2, core::sync::atomic::Ordering::Relaxed);
        assert_eq!(run_sigframe(), Exit::Code(20));
    }

    /// A `sigreturn` mode 4 thread in a space of its own, and a caller whose capability space
    /// holds a Thread capability (SIGNAL) for it, as `SYS_THREAD_INTERRUPT` needs.
    fn mode4_rig() -> (Arc<Space>, Arc<Thread>, Arc<Thread>, usize, Image) {
        crate::sched::install_space_switch();
        let img = load_image(user_image!("sigreturn")).unwrap();
        let owner = Agent::new_root(1, 0);
        let space = Space::new(img.space.clone(), owner.clone());
        let th = Thread::new(owner);
        space.register(&th).unwrap();
        let start = ThreadStart {
            rip: img.entry as u64,
            rsp: USER_STACK_TOP as u64,
            arg: 4,
            fs_base: 0,
            clear_tid: 0,
        };
        spawn_thread(&space, th.clone(), start);
        let holder = Agent::new_root(2, 0);
        let slot = holder
            .cspace
            .lock()
            .insert(Cap::new(Object::Thread(th.downgrade()), Rights::SIGNAL))
            .unwrap();
        (space, th, Thread::new(holder), slot, img)
    }

    /// Interrupts `th` `n` times, waiting for each handler to start before the next.
    fn nest(th: &Arc<Thread>, caller: &Thread, slot: usize, handler: usize, n: usize) {
        for i in 1..=n {
            thread_interrupt(caller, slot, handler).unwrap();
            let mut spins = 0usize;
            while th.frame_depth() < i {
                spins += 1;
                assert!(spins < 1000, "redirect {i} never reached its handler");
                Task::yield_now();
            }
        }
    }

    #[ktest]
    fn sigreturn_nests_to_depth_8_then_eagain() {
        let (space, th, caller, slot, img) = mode4_rig();
        nest(&th, &caller, slot, img.handler, crate::thread::MAX_FRAMES);
        assert_eq!(th.frame_depth(), crate::thread::MAX_FRAMES);

        let full = thread_interrupt(&caller, slot, img.handler).err();
        assert_eq!(full, Some(SpaceError::Limit));
        assert_eq!(Errno::from(SpaceError::Limit), Errno::EAGAIN);
        assert_eq!(th.frame_depth(), crate::thread::MAX_FRAMES, "the refusal pushed nothing");

        space.kill();
        assert_eq!(join_thread(&th), Exit::Killed);
    }

    /// A blocked call interrupted and then returned from sees `EINTR` in `rax`: the frame saved
    /// the syscall result, and `SYS_SIGRETURN` gives it back.
    #[ktest]
    fn sigreturn_after_a_blocked_call_returns_eintr() {
        let img = load_image(user_image!("sigreturn")).unwrap();
        let t = Thread::new(Agent::new_root(1, 0));
        let exit = spawn_image(&t, &img, 2);
        wait_blocked(&t, &exit);
        t.interrupt(img.handler);
        assert_eq!(join(&exit), Exit::Code(abi::errno::EINTR as usize));
        assert_eq!(t.frame_depth(), 0);
    }

    #[ktest]
    fn sigreturn_frames_dropped_at_kill() {
        let (space, th, caller, slot, img) = mode4_rig();
        nest(&th, &caller, slot, img.handler, 3);
        assert_eq!(th.frame_depth(), 3);

        space.kill();
        assert_eq!(join_thread(&th), Exit::Killed);
        // `retire` runs after the exit is recorded; the reap that follows empties the space.
        let mut spins = 0usize;
        while space.live_threads() != 0 {
            spins += 1;
            assert!(spins < 1_000_000, "the thread was never reaped");
            Task::yield_now();
        }
        assert_eq!(th.frame_depth(), 0, "a dead thread holds no frames");
    }

    /// A data frame whose first byte is `first`.
    fn data_frame(first: u8) -> UFrame {
        let f = FrameAllocOptions::new().alloc_frame().unwrap();
        f.write_bytes(0, &[first]).unwrap();
        f.into()
    }

    /// A space holding only a data page at `DATA`, for tests that never run ring-3 code in it.
    fn data_space(first: u8) -> Arc<VmSpace> {
        let space = VmSpace::new();
        map_test_page(&space, data_frame(first), PageFlags::RW);
        Arc::new(space)
    }

    /// A server thread holding RECV on an endpoint, and a client thread whose cspace is `{0: SEND
    /// on it}`.
    fn endpoint_pair() -> (Arc<Thread>, Arc<Thread>) {
        let server = Agent::new_root(42, 16);
        let slot = server
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))
            .unwrap();
        let client = server.spawn_child(&[(slot, Rights::SEND)], 4, 1).unwrap();
        (Thread::new(server), Thread::new(client))
    }

    /// Everything a fault-note test needs: `client` runs as the first thread of `space`, over
    /// `img`'s address space, and the space's notes go to `ep`, which the kernel thread `server`
    /// reads.
    struct NotifyRig {
        server: Arc<Thread>,
        client: Arc<Thread>,
        space: Arc<Space>,
        ep: Arc<Endpoint>,
    }

    /// The client carries tag `TAG`, so a test can tell the note names it.
    const TAG: u64 = 0x2a;

    fn notify_rig(img: &Image) -> NotifyRig {
        let agent = Agent::new_root(42, 4);
        let ep = Endpoint::new();
        let space = Space::adopt(img.space.clone(), agent.clone());
        space.set_notify_cap(Cap::new(Object::Endpoint(ep.clone()), Rights::SEND));
        NotifyRig {
            server: Thread::new(Agent::new_root(99, 0)),
            client: Thread::new_tagged(agent, TAG),
            space,
            ep,
        }
    }

    #[ktest]
    fn fault_note_carries_vector_addr_rip_tid_and_tag() {
        let img = new_image(user_image!("cow_writer"), PageFlags::R);
        let r = notify_rig(&img);
        let exit = spawn_image_in(&r.client, &r.space, &img, 0);

        let m = recv_note(&r.ep, &r.server);
        assert_eq!(m.label, NOTE_FAULT);
        assert_eq!(m.sender, r.client.agent().credential());
        assert_eq!((m.tid, m.tag), (r.client.tid(), TAG));
        assert!(m.caps.is_empty() && m.data.is_empty());
        assert_eq!(m.words[..2], [14, DATA as u64]);
        let rip = m.words[2] as usize;
        assert!(
            img.text.contains(&rip),
            "fault rip {rip:#x} outside the image text"
        );
        assert_eq!(m.words[3] & 0b110, 0b110, "write fault from user mode");

        // Redirecting is the SIGSEGV path: interrupt to the handler, then answer 0. The thread
        // runs the handler with the faulting pc in rdi.
        r.client.interrupt(img.handler);
        m.reply
            .as_ref()
            .expect("a fault note is a call")
            .reply([0; 4]);
        assert_eq!(join(&exit), Exit::Code(rip));
    }

    #[ktest]
    fn pager_maps_the_page_and_resumes() {
        let img = image_over(user_image!("cow_writer"), None);
        let r = notify_rig(&img);
        let exit = spawn_image_in(&r.client, &r.space, &img, 0);

        let m = recv_note(&r.ep, &r.server);
        assert_eq!(m.words[1], DATA as u64);
        map_test_page(
            &img.space,
            FrameAllocOptions::new().alloc_frame().unwrap().into(),
            PageFlags::RW,
        );
        m.reply.as_ref().unwrap().reply([0; 4]);

        assert_eq!(join(&exit), Exit::Code(0));
        assert_eq!(data_byte(&img.space), 0x42);
    }

    #[ktest]
    fn kill_verdict_makes_the_fault_fatal() {
        // Zero resumes; every other verdict kills.
        for verdict in [1, 7] {
            let img = image_over(user_image!("cow_writer"), None);
            let r = notify_rig(&img);
            let exit = spawn_image_in(&r.client, &r.space, &img, 0);
            let m = recv_note(&r.ep, &r.server);
            m.reply.as_ref().unwrap().reply([verdict, 0, 0, 0]);
            assert_eq!(join(&exit), Exit::Fault(DATA), "verdict {verdict}");
        }
    }

    #[ktest]
    fn revoked_or_missing_notify_is_fatal() {
        let img = load_image(user_image!("ud2")).unwrap();

        // No notify endpoint at all.
        let none = Thread::new(Agent::new_root(1, 0));
        assert_eq!(start(&none, &img, 0), Exit::Exception(6));

        // A forwarded capability, revoked before the fault: the fault is fatal and nothing is queued.
        let agent = Agent::new_root(1, 0);
        let ep = Endpoint::new();
        let (grant, fwd) = Cap::new(Object::Endpoint(ep.clone()), Rights::ALL)
            .grant_revocable(Rights::SEND)
            .unwrap();
        let space = Space::adopt(img.space.clone(), agent.clone());
        space.set_notify_cap(grant);
        fwd.revoke();
        assert!(space.notify_ep().is_none());
        let t = Thread::new(agent);
        let exit = run_registered(&t, &space, entry_ctx(&img, 0));
        assert_eq!(exit, Exit::Exception(6));
        assert_eq!(ep.queued(), 0, "no note was queued");
    }

    #[ktest]
    fn futex_wakes_across_spaces_sharing_a_frame() {
        let data = data_frame(0x11);
        let (s1, s2) = (
            image_over(user_image!("futex_waiter"), Some((&data, PageFlags::RW))),
            image_over(user_image!("futex_waker"), Some((&data, PageFlags::RW))),
        );
        let (waiter, waker) = (
            Thread::new(Agent::new_root(1, 0)),
            Thread::new(Agent::new_root(1, 0)),
        );
        let we = spawn_image(&waiter, &s1, 0);
        wait_blocked(&waiter, &we);
        assert!(waiter.is_blocked(), "the waiter did not reach the futex");

        let ke = spawn_image(&waker, &s2, 0);
        assert_eq!(join(&ke), Exit::Code(1));
        assert_eq!(join(&we), Exit::Code(0));
    }

    #[ktest]
    fn futex_wait_with_stale_value_is_eagain() {
        let img = image_over(
            user_image!("futex_waiter"),
            Some((&data_frame(1), PageFlags::RW)),
        );
        let t = Thread::new(Agent::new_root(1, 0));
        assert_eq!(start(&t, &img, 0), Exit::Code(-11isize as usize));
    }

    #[ktest]
    fn futex_key_survives_cow_break() {
        let agent = Agent::new_root(1, 4);
        let img = new_image(user_image!("futex_waiter"), PageFlags::RW);
        // The clone keeps the frame shared, so the waiter's next touch of it must copy.
        let _clone = cow_clone(
            &img.space,
            abi::IMAGE_BASE..DATA + PAGE_SIZE,
            &*Agent::new_root(1, 64),
        )
        .expect("cow_clone");
        let (waiter, waker) = (Thread::new(agent.clone()), Thread::new(agent));

        let we = spawn_image(&waiter, &img, 0);
        wait_blocked(&waiter, &we);
        // The waker is a kernel call in the waiter's own space: two ring-3 programs cannot share
        // one space, since every image is linked at `IMAGE_BASE`.
        let woke = futex::wake(&waker, &img.space, DATA, 1);
        if woke != Ok(1) {
            // Lost wakeup: free the waiter so the failure shows up as a failure, not a hang.
            waiter.interrupt(img.handler);
            join(&we);
        }
        assert_eq!(woke, Ok(1));
        assert_eq!(join(&we), Exit::Code(0));
    }

    /// A `futex_waiter` image whose data frame is shared with a clone, so a futex lookup on `DATA`
    /// must copy it. The clone is returned to keep the frame shared.
    fn shared_data_image() -> (Image, Arc<VmSpace>) {
        let img = new_image(user_image!("futex_waiter"), PageFlags::RW);
        let spare = Agent::new_root(1, 64);
        let clone =
            cow_clone(&img.space, abi::IMAGE_BASE..DATA + PAGE_SIZE, &*spare).expect("cow_clone");
        (img, Arc::new(clone))
    }

    #[ktest]
    fn futex_cow_budget_is_enomem() {
        let (img, _clone) = shared_data_image();
        // Its payer is its agent, which has no budget for the copy.
        let t = Thread::new(Agent::new_root(1, 0));
        let err = futex::wake(&t, &img.space, DATA, 1).unwrap_err();
        assert_eq!(err, futex::FutexError::Budget);
        assert_eq!(err.errno(), 12);
        assert_eq!(
            crate::syscall::Errno::from(err),
            crate::syscall::Errno::ENOMEM
        );
        assert_eq!(t.agent().mem_used_pages(), 0);
    }

    #[ktest]
    fn futex_cow_bad_addresses_are_fault() {
        let (img, _clone) = shared_data_image();
        let t = Thread::new(Agent::new_root(1, 64));
        for va in [
            abi::USER_TOP - 4,
            ostd::mm::MAX_USERSPACE_VADDR - 4,
            abi::IMAGE_BASE - PAGE_SIZE,
        ] {
            assert_eq!(
                futex::wake(&t, &img.space, va, 1),
                Err(futex::FutexError::Fault),
                "address {va:#x}"
            );
        }
    }

    #[ktest]
    fn futex_cow_no_memory_maps_through_from() {
        use crate::{agent::AgentError, cap::CapError, futex::FutexError};
        // OSTD 0.19.0 cannot make a frame allocation fail, so the path from a real break that
        // finds the allocator empty is covered by this mapping and by inspection (D-05).
        let no_memory = FutexError::from(AgentError::NoMemory);
        assert_eq!(no_memory, FutexError::NoMemory);
        assert_eq!(no_memory.errno(), 12);
        assert_eq!(FutexError::from(AgentError::Budget), FutexError::Budget);
        assert_eq!(
            FutexError::from(AgentError::Cap(CapError::Full)),
            FutexError::Fault
        );
    }

    #[ktest]
    fn interrupt_aborts_futex_wait() {
        let img = image_over(
            user_image!("futex_waiter"),
            Some((&data_frame(0x11), PageFlags::RW)),
        );
        let t = Thread::new(Agent::new_root(1, 0));
        let exit = spawn_image(&t, &img, 0);
        wait_blocked(&t, &exit);
        t.interrupt(img.handler);
        // The handler exits with the pc the futex syscall was interrupted at.
        let Exit::Code(pc) = join(&exit) else {
            panic!("the waiter did not exit through the redirect handler");
        };
        assert!(img.text.contains(&pc), "pc {pc:#x} outside the image text");
        assert_eq!(
            futex::wake(&t, &img.space, DATA, 10),
            Ok(0),
            "waiter was removed"
        );
    }

    #[ktest]
    fn requeue_moves_waiters() {
        let space = data_space(0x11);
        let results: Vec<Arc<SpinLock<Option<Result<(), futex::FutexError>>>>> =
            (0..2).map(|_| Arc::new(SpinLock::new(None))).collect();
        let waiters: Vec<Arc<Thread>> =
            (0..2).map(|_| Thread::new(Agent::new_root(1, 0))).collect();
        for (t, r) in waiters.iter().zip(&results) {
            let (t, r, space) = (t.clone(), r.clone(), space.clone());
            TaskOptions::new(move || *r.lock() = Some(futex::wait(&t, &space, DATA, 0x11)))
                .spawn()
                .unwrap();
        }
        while !waiters.iter().all(|w| w.is_blocked()) {
            Task::yield_now();
        }

        let me = Thread::new(Agent::new_root(1, 0));
        assert_eq!(
            futex::requeue(&me, &space, DATA, 1, DATA + 4, 1, None),
            Ok((1, 1))
        );
        assert_eq!(futex::wake(&me, &space, DATA + 4, 10), Ok(1));
        for r in &results {
            while r.lock().is_none() {
                Task::yield_now();
            }
            assert_eq!(r.lock().take(), Some(Ok(())));
        }
    }

    #[ktest]
    fn futex_on_a_kernel_address_is_efault_not_a_panic() {
        let space = data_space(0x11);
        let t = Thread::new(Agent::new_root(1, 0));
        for va in [usize::MAX & !3, 0xffff_8000_0000_0000] {
            assert_eq!(
                futex::wait(&t, &space, va, 0),
                Err(futex::FutexError::Fault)
            );
            assert_eq!(
                futex::wake(&t, &space, va, 1),
                Err(futex::FutexError::Fault)
            );
        }
    }

    #[ktest]
    fn tracer_runs_rust_ring3_image() {
        let (server, client) = endpoint_pair();
        let img = load_image(user_image!("tracer")).unwrap();
        // The kernel-side server answers one call: reply[0] = words[0] + 1, reply[1] = label.
        let seen: Arc<SpinLock<Option<Message>>> = Arc::new(SpinLock::new(None));
        let seen2 = seen.clone();
        TaskOptions::new(move || {
            let m = sys_recv(&server, 0).unwrap();
            m.reply
                .as_ref()
                .unwrap()
                .reply([m.words[0] + 1, m.label, 0, 0]);
            *seen2.lock() = Some(m);
        })
        .spawn()
        .unwrap();

        // The client's cspace is {0: SEND on the endpoint}; the tracer calls slot 0 and probes 63.
        let exit = spawn_image(&client, &img, 0);
        assert_eq!(join(&exit), Exit::Code(0));
        let m = seen.lock().take().expect("the server saw the call");
        assert_eq!(m.sender, client.agent().credential());
        assert_eq!((m.label, m.words[0]), (7, 41));
    }

    #[ktest]
    fn badsys_gets_errno_from_ring3() {
        let (_server, client) = endpoint_pair();
        let img = load_image(user_image!("badsys")).unwrap();
        // Every case fails before the endpoint is used, so no server is needed. Exit code 0 means
        // each hostile syscall came back as the expected errno.
        let exit = spawn_image(&client, &img, 0);
        assert_eq!(join(&exit), Exit::Code(0));
    }

    /// A header with the given quads, padded to `IMAGE_HEADER_LEN`.
    fn header(magic: u64, entry: usize, handler: usize, rx_end: usize, mem_end: usize) -> Vec<u8> {
        [
            magic,
            entry as u64,
            handler as u64,
            rx_end as u64,
            mem_end as u64,
        ]
        .iter()
        .flat_map(|q| q.to_le_bytes())
        .collect()
    }

    #[ktest]
    fn load_image_rejects_malformed_headers() {
        use abi::IMAGE_BASE as B;
        let ok = header(
            IMAGE_MAGIC,
            B + 0x30,
            B + 0x30,
            B + PAGE_SIZE,
            B + 2 * PAGE_SIZE,
        );
        assert!(load_image(&ok).is_ok(), "the baseline header must load");

        let bad_magic = header(
            !IMAGE_MAGIC,
            B + 0x30,
            B + 0x30,
            B + PAGE_SIZE,
            B + 2 * PAGE_SIZE,
        );
        assert_eq!(load_image(&bad_magic).err(), Some(LoadError::Header));
        assert_eq!(load_image(&ok[..39]).err(), Some(LoadError::Header));

        let layouts = [
            // entry == rx_end is outside [base, rx_end)
            header(
                IMAGE_MAGIC,
                B + PAGE_SIZE,
                B + 0x30,
                B + PAGE_SIZE,
                B + 2 * PAGE_SIZE,
            ),
            header(
                IMAGE_MAGIC,
                B + 0x30,
                B - 1,
                B + PAGE_SIZE,
                B + 2 * PAGE_SIZE,
            ),
            // rx_end beyond mem_end
            header(
                IMAGE_MAGIC,
                B + 0x30,
                B + 0x30,
                B + 2 * PAGE_SIZE,
                B + PAGE_SIZE,
            ),
            // rx_end not page aligned: a page would mix RX and RW
            header(IMAGE_MAGIC, B + 0x30, B + 0x30, B + 0x800, B + PAGE_SIZE),
            // larger than MAX_IMAGE_SIZE
            header(
                IMAGE_MAGIC,
                B + 0x30,
                B + 0x30,
                B + PAGE_SIZE,
                B + MAX_IMAGE_SIZE + PAGE_SIZE,
            ),
            header(IMAGE_MAGIC, 0, 0, 0, 0),
            header(IMAGE_MAGIC, usize::MAX, usize::MAX, usize::MAX, usize::MAX),
        ];
        for h in layouts {
            assert_eq!(load_image(&h).err(), Some(LoadError::Layout), "{h:x?}");
        }
        // The file may not be longer than the image it declares.
        let mut long = ok.clone();
        long.resize(2 * PAGE_SIZE + 1, 0);
        assert_eq!(load_image(&long).err(), Some(LoadError::Layout));
    }
}
