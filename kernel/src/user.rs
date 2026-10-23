//! Runs a thread in user mode: syscalls, CoW faults, and redirects applied between `execute`
//! calls. The spike's only syscall is `exit(code)`.

use alloc::sync::Arc;

use ostd::{
    arch::cpu::context::{CpuException, PageFaultErrorCode, UserContext},
    mm::{Vaddr, VmSpace},
    user::{ReturnReason, UserMode},
};

use crate::{thread::Thread, vm::handle_cow_fault};

const SYS_EXIT: usize = 60;
const ENOSYS: isize = 38;

#[derive(Debug, PartialEq, Eq)]
pub enum Exit {
    Code(usize),
    /// A fault that is not copy-on-write.
    Fault(Vaddr),
    /// A copy-on-write fault the agent's memory budget could not pay for.
    Budget,
}

pub fn run(thread: &Thread, space: &Arc<VmSpace>, entry: Vaddr) -> Exit {
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
                _ => ctx.set_rax(-ENOSYS as usize),
            },
            // shortcut: non-CoW exceptions end the run; B3 delivers them as messages to a
            // handler endpoint so procmgr can synthesize SIGSEGV.
            ReturnReason::UserException => match ctx.take_exception() {
                Some(CpuException::PageFault(pf)) => {
                    let write = pf.error_code.contains(PageFaultErrorCode::WRITE);
                    match write.then(|| handle_cow_fault(space, thread.agent(), pf.addr)) {
                        Some(Ok(true)) => {}
                        Some(Err(_)) => return Exit::Budget,
                        _ => return Exit::Fault(pf.addr),
                    }
                }
                other => panic!("unhandled user exception: {other:?}"),
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
    use alloc::vec;

    use ostd::{
        mm::{
            CachePolicy, FallibleVmRead, FrameAllocOptions, PAGE_SIZE, PageFlags, PageProperty,
            VmIo, VmWriter,
        },
        prelude::*,
        sync::{LocalIrqDisabled, SpinLock},
        task::disable_preempt,
        timer::{Jiffies, register_callback_on_cpu},
    };

    use super::*;
    use crate::{agent::Agent, vm::cow_clone};

    const CODE: Vaddr = 0x40_0000;
    const DATA: Vaddr = 0x50_0000;
    const SPIN: Vaddr = CODE;
    const HANDLER: Vaddr = CODE + 0x10;
    const COW_WRITER: Vaddr = CODE + 0x20;

    /// Hand-assembled; `objdump -D -b binary -mi386:x86-64` reproduces the listing.
    #[rustfmt::skip]
    const PROGRAM: [u8; 0x32] = [
        // 0x00 SPIN:       jmp .
        0xeb, 0xfe, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // 0x10 HANDLER:    mov eax, 60; syscall          (exit(rdi), rdi = interrupted pc)
        0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        // 0x20 COW_WRITER: mov byte [0x500000], 0x42; xor edi, edi; mov eax, 60; syscall
        0xc6, 0x04, 0x25, 0x00, 0x00, 0x50, 0x00, 0x42,
        0x31, 0xff, 0xb8, 0x3c, 0, 0, 0, 0x0f, 0x05,
        0,
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
}
