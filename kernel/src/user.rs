//! Runs a thread in user mode: syscalls, CoW faults, and redirects applied between `execute`
//! calls. The spike's syscalls are `exit(code)`, `sched_yield()` and `futex(2)`.

use alloc::sync::Arc;

use ostd::{
    arch::cpu::context::{CpuException, PageFaultErrorCode, UserContext},
    mm::{
        CachePolicy, FrameAllocOptions, PAGE_SIZE, PageFlags, PageProperty, Vaddr, VmIo, VmSpace,
    },
    task::{Task, disable_preempt},
    user::{ReturnReason, UserMode},
};

use crate::{
    cap::{Object, Rights},
    futex,
    thread::{ExcReply, Interrupted, Thread},
    vm::handle_cow_fault,
};

const SYS_SCHED_YIELD: usize = 24;
const SYS_EXIT: usize = 60;
const SYS_FUTEX: usize = 202;
const ENOSYS: isize = 38;

const FUTEX_WAIT: usize = 0;
const FUTEX_WAKE: usize = 1;
const FUTEX_REQUEUE: usize = 3;
const FUTEX_CMP_REQUEUE: usize = 4;
/// Ignored: futexes are keyed by physical address, so private and shared behave the same.
const FUTEX_PRIVATE_FLAG: usize = 128;

#[derive(Debug, PartialEq, Eq)]
pub enum Exit {
    Code(usize),
    /// A fault that is not copy-on-write.
    Fault(Vaddr),
    /// A copy-on-write fault the agent's memory budget could not pay for.
    Budget,
    /// A non-page-fault exception (x86 vector) that no handler took.
    Exception(u64),
}

/// Position-independent test programs. Each touches only the counter page at `DATA`.
/// Hand-assembled; `objdump -D -b binary -mi386:x86-64` reproduces the listing.
pub mod prog {
    use ostd::mm::Vaddr;

    pub const DATA: Vaddr = 0x50_0000;
    /// `do { [DATA] += 1; sched_yield(); } while [DATA] < 100; exit(0)`
    pub const YIELDER: usize = 0x00;
    /// `loop { [DATA] += 1 }`, 10 bytes long.
    pub const SPINNER: usize = 0x30;
    /// `exit(rdi)`: a redirect target that exits with the interrupted pc.
    pub const HANDLER: usize = 0x40;

    #[rustfmt::skip]
    pub const BYTES: [u8; 0x47] = [
        // 0x00 YIELDER: inc qword [0x500000]; mov eax, 24; syscall
        0x48, 0xff, 0x04, 0x25, 0x00, 0x00, 0x50, 0x00, 0xb8, 0x18, 0, 0, 0, 0x0f, 0x05,
        //               cmp qword [0x500000], 100; jb YIELDER
        0x48, 0x83, 0x3c, 0x25, 0x00, 0x00, 0x50, 0x00, 0x64, 0x72, 0xe6,
        //               xor edi, edi; mov eax, 60; syscall
        0x31, 0xff, 0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // 0x30 SPINNER: inc qword [0x500000]; jmp SPINNER
        0x48, 0xff, 0x04, 0x25, 0x00, 0x00, 0x50, 0x00, 0xeb, 0xf6,
        0, 0, 0, 0, 0, 0,
        // 0x40 HANDLER: mov eax, 60; syscall
        0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05,
    ];
}

/// A new space with `prog::BYTES` mapped RX at `code` and a zeroed RW page at `prog::DATA`.
pub fn load(code: Vaddr) -> Arc<VmSpace> {
    let text = FrameAllocOptions::new().alloc_frame().unwrap();
    text.write_bytes(0, &prog::BYTES).unwrap();
    let data = FrameAllocOptions::new().alloc_frame().unwrap();
    let space = VmSpace::new();
    let guard = disable_preempt();
    for (va, frame, flags) in [
        (code, text, PageFlags::RX),
        (prog::DATA, data, PageFlags::RW),
    ] {
        let mut c = space.cursor_mut(&guard, &(va..va + PAGE_SIZE)).unwrap();
        c.map(
            frame.into(),
            PageProperty::new_user(flags, CachePolicy::Writeback),
        );
    }
    Arc::new(space)
}

fn sys_futex(t: &Thread, space: &VmSpace, ctx: &UserContext) -> isize {
    let (uaddr, op, val) = (ctx.rdi(), ctx.rsi() & !FUTEX_PRIVATE_FLAG, ctx.rdx());
    let (val2, uaddr2, val3) = (ctx.r10(), ctx.r8(), ctx.r9());
    let n = val as u32 as usize;
    let r = match op {
        // shortcut: no timeouts; add one with a timer callback when pthread_cond_timedwait needs it.
        FUTEX_WAIT if val2 != 0 => return -ENOSYS,
        FUTEX_WAIT => futex::wait(t, space, uaddr, val as u32).map(|()| 0),
        FUTEX_WAKE => futex::wake(t, space, uaddr, n),
        // Return values follow futex(2): REQUEUE reports woken, CMP_REQUEUE woken plus moved.
        FUTEX_REQUEUE => {
            futex::requeue(t, space, uaddr, n, uaddr2, val2, None).map(|(woken, _)| woken)
        }
        FUTEX_CMP_REQUEUE => futex::requeue(t, space, uaddr, n, uaddr2, val2, Some(val3 as u32))
            .map(|(woken, moved)| woken + moved),
        _ => return -ENOSYS,
    };
    match r {
        Ok(n) => n as isize,
        Err(e) => -e.errno(),
    }
}

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

/// Sends the fault to the thread's handler endpoint as `[vector, fault address, rip, page-fault
/// error code]` (kernel-stamped credential) and waits for the verdict. `None`: carry on, the
/// handler resumed the thread or redirected it (the caller applies the redirect). `Some`: the
/// fault is fatal, either by the handler's verdict or because no handler could take it.
// shortcut: `cap::Object` has no thread type, so the message does not name the faulting thread
// and the handler replies through an `Arc<Thread>`; procmgr needs `Object::Thread` first.
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
    let Some(Object::Endpoint(ep)) = thread
        .exception_handler()
        .and_then(|cap| cap.resolve(Rights::SEND).ok())
    else {
        return fatal();
    };
    thread.clear_exception_reply();
    if ep
        .send(thread, [vector, addr as u64, rip as u64, err as u64])
        .is_err()
    {
        return fatal();
    }
    match thread.wait_exception_reply() {
        Ok(ExcReply::Resume) | Err(Interrupted) => None,
        Ok(ExcReply::Kill) => fatal(),
    }
}

pub fn run(thread: &Thread, space: &Arc<VmSpace>, entry: Vaddr) -> Exit {
    thread.set_space(space.clone());
    space.activate();
    let mut ctx = UserContext::default();
    ctx.set_rip(entry);
    let mut um = UserMode::new(ctx);
    loop {
        let reason = um.execute(thread);
        let ctx = um.context_mut();
        match reason {
            ReturnReason::UserSyscall => match ctx.rax() {
                SYS_EXIT => return Exit::Code(ctx.rdi()),
                SYS_SCHED_YIELD => {
                    Task::yield_now();
                    ctx.set_rax(0);
                }
                SYS_FUTEX => ctx.set_rax(sys_futex(thread, space, ctx) as usize),
                _ => ctx.set_rax(-ENOSYS as usize),
            },
            ReturnReason::UserException => match ctx.take_exception() {
                Some(exc) => {
                    if let CpuException::PageFault(pf) = exc {
                        let write = pf.error_code.contains(PageFaultErrorCode::WRITE);
                        match write.then(|| handle_cow_fault(space, thread.agent(), pf.addr)) {
                            Some(Ok(true)) => {}
                            Some(Err(_)) => return Exit::Budget,
                            _ => {
                                if let Some(exit) = deliver(thread, exc, ctx.rip()) {
                                    return exit;
                                }
                            }
                        }
                    } else if let Some(exit) = deliver(thread, exc, ctx.rip()) {
                        return exit;
                    }
                }
                None => panic!("user exception without exception info"),
            },
            ReturnReason::KernelEvent => {}
        }
        // Signal-style redirect: resume at the handler with the interrupted pc in rdi.
        if let Some(handler) = thread.take_redirect() {
            ctx.set_rdi(ctx.rip());
            ctx.set_rip(handler);
        }
    }
}

#[cfg(ktest)]
mod test {
    use alloc::{vec, vec::Vec};

    use ostd::{
        mm::{
            CachePolicy, FallibleVmRead, FrameAllocOptions, PAGE_SIZE, PageFlags, PageProperty,
            UFrame, VmIo, VmWriter,
        },
        prelude::*,
        sync::{LocalIrqDisabled, SpinLock},
        task::{TaskOptions, disable_preempt},
        timer::{Jiffies, register_callback_on_cpu},
    };

    use super::*;
    use crate::{
        agent::Agent,
        cap::Cap,
        ipc::{Endpoint, sys_recv, sys_set_exception_handler},
        sched::install_space_switch,
        vm::cow_clone,
    };

    const CODE: Vaddr = 0x40_0000;
    const DATA: Vaddr = 0x50_0000;
    const SPIN: Vaddr = CODE;
    const HANDLER: Vaddr = CODE + 0x10;
    const COW_WRITER: Vaddr = CODE + 0x20;

    const UD2: Vaddr = CODE + 0x40;
    const FUTEX_WAITER: Vaddr = CODE + 0x50;
    /// The pc a redirect sees after FUTEX_WAITER's futex syscall.
    const FUTEX_WAITER_AFTER_SYSCALL: Vaddr = CODE + 0x66;
    const FUTEX_WAKER: Vaddr = CODE + 0x70;

    /// Hand-assembled; `objdump -D -b binary -mi386:x86-64` reproduces the listing.
    #[rustfmt::skip]
    const PROGRAM: [u8; 0x9b] = [
        // 0x00 SPIN:       jmp .
        0xeb, 0xfe, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // 0x10 HANDLER:    mov eax, 60; syscall          (exit(rdi), rdi = interrupted pc)
        0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // 0x20 COW_WRITER: mov byte [0x500000], 0x42; xor edi, edi; mov eax, 60; syscall
        0xc6, 0x04, 0x25, 0x00, 0x00, 0x50, 0x00, 0x42,
        0x31, 0xff, 0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // 0x40 UD2:        ud2
        0x0f, 0x0b, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // 0x50 FUTEX_WAITER: mov edi, 0x500000; xor esi, esi; mov edx, 0x11; xor r10d, r10d
        0xbf, 0x00, 0x00, 0x50, 0x00, 0x31, 0xf6, 0xba, 0x11, 0, 0, 0, 0x45, 0x31, 0xd2,
        //                  mov eax, 202; syscall           (futex(DATA, WAIT, 0x11, NULL))
        0xb8, 0xca, 0, 0, 0, 0x0f, 0x05,
        //                  mov rdi, rax; mov eax, 60; syscall   (exit(result))
        0x48, 0x89, 0xc7, 0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05,
        // 0x70 FUTEX_WAKER:  mov dword [0x500000], 1; mov edi, 0x500000; mov esi, 1; mov edx, 1
        0xc7, 0x04, 0x25, 0x00, 0x00, 0x50, 0x00, 0x01, 0, 0, 0,
        0xbf, 0x00, 0x00, 0x50, 0x00, 0xbe, 0x01, 0, 0, 0, 0xba, 0x01, 0, 0, 0,
        //                  mov eax, 202; syscall           (futex(DATA, WAKE, 1))
        0xb8, 0xca, 0, 0, 0, 0x0f, 0x05,
        //                  mov rdi, rax; mov eax, 60; syscall   (exit(result))
        0x48, 0x89, 0xc7, 0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05,
    ];

    /// Code page RX at CODE; one data page at DATA holding 0x11.
    fn new_space(data_flags: PageFlags) -> Arc<VmSpace> {
        let code = FrameAllocOptions::new().alloc_frame().unwrap();
        code.write_bytes(0, &PROGRAM).unwrap();
        let data = FrameAllocOptions::new().alloc_frame().unwrap();
        data.write_bytes(0, &[0x11u8]).unwrap();

        let space = VmSpace::new();
        let guard = disable_preempt();
        let mut c = space.cursor_mut(&guard, &(CODE..DATA + PAGE_SIZE)).unwrap();
        c.map(
            code.into(),
            PageProperty::new_user(PageFlags::RX, CachePolicy::Writeback),
        );
        c.jump(DATA).unwrap();
        c.map(
            data.into(),
            PageProperty::new_user(data_flags, CachePolicy::Writeback),
        );
        drop(c);
        Arc::new(space)
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
        let parent = new_space(PageFlags::RW);
        let child = Arc::new(cow_clone(&parent, CODE..DATA + PAGE_SIZE));
        assert!(
            !data_flags(&parent).contains(PageFlags::W),
            "parent write-protected"
        );
        assert!(!data_flags(&child).contains(PageFlags::W));

        let t = Thread::new(agent.clone());
        assert_eq!(run(&t, &child, COW_WRITER), Exit::Code(0));
        assert_eq!(data_byte(&child), 0x42);
        assert_eq!(data_byte(&parent), 0x11, "parent unchanged");
        assert_eq!(
            agent.mem_used_pages(),
            1,
            "one page copied, charged to the writer"
        );
        assert!(data_flags(&child).contains(PageFlags::W));

        // The parent still faults once, then owns a writable page again.
        assert_eq!(run(&t, &parent, COW_WRITER), Exit::Code(0));
        assert_eq!(data_byte(&parent), 0x42);
        assert!(data_flags(&parent).contains(PageFlags::W));
    }

    #[ktest]
    fn cow_fault_over_budget_stops_the_thread() {
        let parent = new_space(PageFlags::RW);
        let child = Arc::new(cow_clone(&parent, CODE..DATA + PAGE_SIZE));
        let t = Thread::new(Agent::new_root(1, 0));
        assert_eq!(run(&t, &child, COW_WRITER), Exit::Budget);
        assert_eq!(data_byte(&child), 0x11);
    }

    #[ktest]
    fn write_to_read_only_page_is_a_fault_not_cow() {
        let space = new_space(PageFlags::R);
        let t = Thread::new(Agent::new_root(1, 4));
        assert_eq!(run(&t, &space, COW_WRITER), Exit::Fault(DATA));
    }

    #[ktest]
    fn timer_interrupt_redirects_a_running_thread() {
        let space = new_space(PageFlags::RW);
        let t = Thread::new(Agent::new_root(1, 0));
        let alarm: Arc<SpinLock<Option<Arc<Thread>>, LocalIrqDisabled>> =
            Arc::new(SpinLock::new(Some(t.clone())));
        let deadline = Jiffies::elapsed().as_u64() + 5;
        let a = alarm.clone();
        register_callback_on_cpu(move || {
            if Jiffies::elapsed().as_u64() >= deadline {
                if let Some(t) = a.lock().take() {
                    t.interrupt(HANDLER);
                }
            }
        });

        // SPIN never traps on its own; only the redirect can make it exit, with its pc in rdi.
        assert_eq!(run(&t, &space, SPIN), Exit::Code(SPIN));
        assert!(alarm.lock().is_none());
    }

    /// A new space with the code page RX at CODE and `data` (shared, not copied) at DATA.
    fn space_over(data: &UFrame, data_flags: PageFlags) -> Arc<VmSpace> {
        let code = FrameAllocOptions::new().alloc_frame().unwrap();
        code.write_bytes(0, &PROGRAM).unwrap();
        let space = VmSpace::new();
        let guard = disable_preempt();
        let mut c = space.cursor_mut(&guard, &(CODE..DATA + PAGE_SIZE)).unwrap();
        c.map(
            code.into(),
            PageProperty::new_user(PageFlags::RX, CachePolicy::Writeback),
        );
        c.jump(DATA).unwrap();
        c.map(
            data.clone(),
            PageProperty::new_user(data_flags, CachePolicy::Writeback),
        );
        drop(c);
        Arc::new(space)
    }

    /// A data frame whose first byte is `first`.
    fn data_frame(first: u8) -> UFrame {
        let f = FrameAllocOptions::new().alloc_frame().unwrap();
        f.write_bytes(0, &[first]).unwrap();
        f.into()
    }

    /// Code page only: DATA is unmapped.
    fn code_only_space() -> Arc<VmSpace> {
        let code = FrameAllocOptions::new().alloc_frame().unwrap();
        code.write_bytes(0, &PROGRAM).unwrap();
        let space = VmSpace::new();
        let guard = disable_preempt();
        let mut c = space.cursor_mut(&guard, &(CODE..CODE + PAGE_SIZE)).unwrap();
        c.map(
            code.into(),
            PageProperty::new_user(PageFlags::RX, CachePolicy::Writeback),
        );
        drop(c);
        Arc::new(space)
    }

    type ExitSlot = Arc<SpinLock<Option<Exit>>>;

    /// Runs `entry` in `space` on a new task and returns where its exit will appear.
    fn spawn_user(t: &Arc<Thread>, space: &Arc<VmSpace>, entry: Vaddr) -> ExitSlot {
        install_space_switch();
        let exit: ExitSlot = Arc::new(SpinLock::new(None));
        let (t2, space, e) = (t.clone(), space.clone(), exit.clone());
        TaskOptions::new(move || *e.lock() = Some(run(&t2, &space, entry)))
            .data(t.clone())
            .spawn()
            .unwrap();
        exit
    }

    fn join(e: &ExitSlot) -> Exit {
        loop {
            if let Some(x) = e.lock().take() {
                return x;
            }
            Task::yield_now();
        }
    }

    /// Returns once `t` is blocked in the kernel, or its run already ended.
    fn wait_blocked(t: &Thread, e: &ExitSlot) {
        while !t.is_blocked() && e.lock().is_none() {
            Task::yield_now();
        }
    }

    /// A handler thread holding RECV on an endpoint, and a client thread whose exceptions go
    /// there.
    fn handler_pair() -> (Arc<Thread>, Arc<Thread>) {
        let server = Agent::new_root(42, 16);
        let slot = server
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))
            .unwrap();
        let client = server.spawn_child(&[(slot, Rights::SEND)], 4, 1).unwrap();
        let client = Thread::new(client);
        sys_set_exception_handler(&client, slot).unwrap();
        (Thread::new(server), client)
    }

    #[ktest]
    fn fault_message_carries_vector_addr_rip_and_credential() {
        let (server, client) = handler_pair();
        let space = new_space(PageFlags::R);
        let exit = spawn_user(&client, &space, COW_WRITER);

        let m = sys_recv(&server, 0).unwrap();
        assert_eq!(m.sender, client.agent().credential());
        assert_eq!(m.words[..3], [14, DATA as u64, COW_WRITER as u64]);
        assert_eq!(m.words[3] & 0b110, 0b110, "write fault from user mode");

        // Redirecting is the SIGSEGV path: the handler runs with the faulting pc in rdi.
        client.interrupt(HANDLER);
        assert_eq!(join(&exit), Exit::Code(COW_WRITER));
    }

    #[ktest]
    fn pager_maps_the_page_and_resumes() {
        let (server, client) = handler_pair();
        let space = code_only_space();
        let exit = spawn_user(&client, &space, COW_WRITER);

        let m = sys_recv(&server, 0).unwrap();
        assert_eq!(m.words[1], DATA as u64);
        {
            let page = FrameAllocOptions::new().alloc_frame().unwrap();
            let guard = disable_preempt();
            let mut c = space.cursor_mut(&guard, &(DATA..DATA + PAGE_SIZE)).unwrap();
            c.map(
                page.into(),
                PageProperty::new_user(PageFlags::RW, CachePolicy::Writeback),
            );
        }
        client.reply_exception(ExcReply::Resume);

        assert_eq!(join(&exit), Exit::Code(0));
        assert_eq!(data_byte(&space), 0x42);
    }

    #[ktest]
    fn kill_reply_makes_the_fault_fatal() {
        let (server, client) = handler_pair();
        let space = code_only_space();
        let exit = spawn_user(&client, &space, COW_WRITER);
        sys_recv(&server, 0).unwrap();
        client.reply_exception(ExcReply::Kill);
        assert_eq!(join(&exit), Exit::Fault(DATA));
    }

    #[ktest]
    fn revoked_or_missing_handler_is_fatal() {
        let space = code_only_space();

        let none = Thread::new(Agent::new_root(1, 0));
        assert_eq!(run(&none, &space, UD2), Exit::Exception(6));

        let agent = Agent::new_root(1, 0);
        agent
            .cspace
            .lock()
            .insert(Cap::new(Object::Endpoint(Endpoint::new()), Rights::ALL))
            .unwrap();
        let (grant, fwd) = {
            let cs = agent.cspace.lock();
            cs.get(0).unwrap().grant_revocable(Rights::SEND).unwrap()
        };
        let slot = agent.cspace.lock().insert(grant).unwrap();
        let t = Thread::new(agent);
        sys_set_exception_handler(&t, slot).unwrap();
        fwd.revoke();
        assert_eq!(run(&t, &space, UD2), Exit::Exception(6));
    }

    #[ktest]
    fn futex_wakes_across_spaces_sharing_a_frame() {
        let data = data_frame(0x11);
        let (s1, s2) = (
            space_over(&data, PageFlags::RW),
            space_over(&data, PageFlags::RW),
        );
        let (waiter, waker) = (
            Thread::new(Agent::new_root(1, 0)),
            Thread::new(Agent::new_root(1, 0)),
        );
        let we = spawn_user(&waiter, &s1, FUTEX_WAITER);
        wait_blocked(&waiter, &we);
        assert!(waiter.is_blocked(), "the waiter did not reach the futex");

        let ke = spawn_user(&waker, &s2, FUTEX_WAKER);
        assert_eq!(join(&ke), Exit::Code(1));
        assert_eq!(join(&we), Exit::Code(0));
    }

    #[ktest]
    fn futex_wait_with_stale_value_is_eagain() {
        let space = space_over(&data_frame(1), PageFlags::RW);
        let t = Thread::new(Agent::new_root(1, 0));
        assert_eq!(run(&t, &space, FUTEX_WAITER), Exit::Code(-11isize as usize));
    }

    #[ktest]
    fn futex_key_survives_cow_break() {
        let agent = Agent::new_root(1, 4);
        let s1 = new_space(PageFlags::RW);
        // The clone keeps the frame shared, so s1's next touch of it must copy.
        let _clone = cow_clone(&s1, CODE..DATA + PAGE_SIZE);
        let (waiter, waker) = (Thread::new(agent.clone()), Thread::new(agent));

        let we = spawn_user(&waiter, &s1, FUTEX_WAITER);
        wait_blocked(&waiter, &we);
        let ke = spawn_user(&waker, &s1, FUTEX_WAKER);
        let woke = join(&ke);
        if woke != Exit::Code(1) {
            // Lost wakeup: free the waiter so the failure shows up as a failure, not a hang.
            waiter.interrupt(HANDLER);
            join(&we);
        }
        assert_eq!(woke, Exit::Code(1));
        assert_eq!(join(&we), Exit::Code(0));
    }

    #[ktest]
    fn interrupt_aborts_futex_wait() {
        let space = space_over(&data_frame(0x11), PageFlags::RW);
        let t = Thread::new(Agent::new_root(1, 0));
        let exit = spawn_user(&t, &space, FUTEX_WAITER);
        wait_blocked(&t, &exit);
        t.interrupt(HANDLER);
        assert_eq!(join(&exit), Exit::Code(FUTEX_WAITER_AFTER_SYSCALL));
        assert_eq!(
            futex::wake(&t, &space, DATA, 10),
            Ok(0),
            "waiter was removed"
        );
    }

    #[ktest]
    fn requeue_moves_waiters() {
        let space = new_space(PageFlags::RW);
        let results: Vec<Arc<SpinLock<Option<Result<(), futex::FutexError>>>>> =
            (0..2).map(|_| Arc::new(SpinLock::new(None))).collect();
        let waiters: Vec<Arc<Thread>> =
            (0..2).map(|_| Thread::new(Agent::new_root(1, 0))).collect();
        for (t, r) in waiters.iter().zip(&results) {
            let (t, r, space) = (t.clone(), r.clone(), space.clone());
            TaskOptions::new(move || *r.lock() = Some(futex::wait(&t, &space, DATA, 0x11)))
                .spawn()
                .unwrap();
            while !waiters.iter().any(|w| w.is_blocked()) {
                Task::yield_now();
            }
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
}
