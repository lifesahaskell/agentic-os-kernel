# How microkernel and capability OSes provide a POSIX personality

Researched 2026-10-23 from primary sources: source trees (cloned or fetched raw into the scratchpad), project design docs, vendor manuals, one paper.
Convention: every factual line carries a URL. "not found" means I looked and did not find a primary source. "(interpretation)" marks my own synthesis.
Source trees were read at their default branch on 2026-10-23; line numbers refer to those checkouts and may drift.

## 0. Comparison matrix (one line per mechanism)

| System | fork | signals | fd table | futex | POSIX identity (pid/pgrp/uid) |
|---|---|---|---|---|---|
| Redox | Userspace in relibc; kernel CoW address-space clone via proc scheme | Kernel redirects user IP to a handler; control block shared with userspace | Kernel fd table + kernel FileDescription; schemes are the servers | Kernel, keyed by physical address | `procmgr` userspace server |
| Fuchsia native | ENOSYS | `sigaction` ENOSYS, `kill` EPERM | Client-side fdio table over FIDL channels | `zx_futex` kernel | none |
| Fuchsia Starnix | Userspace "Linux kernel" (executive) + VMO snapshot CoW | Zircon exception -> restricted exit -> Linux signal; `zx_thread_kick_restricted` | Executive owns the table | Userspace futex table in the executive | Executive owns it |
| Genode | Userspace libc, eager copy into a new component | Local to the process only | Libc-local table | Libc-local cooperative "kernel" | dummies; setsid/setpgid are stubs |
| seL4 muslcsys | none (ENOSYS) | none | none | none | none |
| LionsOS | none; static PDs | none | none (own API) | none | none |
| QNX Neutrino | Process manager `procnto`; address space is copied | Kernel-implemented, 64 signals, POSIX semantics | fd = connection ID (kernel channel/connection object) | Kernel (`Sync*` calls) | `procnto` + resource managers |
| MINIX 3 | PM + VM server + kernel `sys_fork`; shared physical blocks (CoW) | PM decides; kernel `SYS_SIGSEND` builds the frame | VFS server table | none; green threads only | PM (pid/pgrp/uid) + VFS (file access) |
| GNU Hurd | glibc `_Fork`: stop threads, Mach `task_create` (inherits memory), copy port rights | Per-process signal thread; suspends target thread, edits register state | Table of Mach port rights in libc | Mach `gsync` in kernel | `proc` server, `auth` server |

## 1. Redox (relibc + userspace schemes)

1. fork: supported. The runtime library does it in userspace: `fork_impl` in `redox-rt` builds the child by asking the kernel's `proc` scheme for a CoW duplicate of the address space, re-obtaining fds via `fmap`; doc comment: "Other mappings are kept but duplicated using CoW". https://gitlab.redox-os.org/redox-os/relibc/-/blob/master/redox-rt/src/proc.rs (`fork_impl` line ~959, "CoW-duplicate address space" line ~1031). Cost: not found (no measurement published).
2. exec, wait, pgrp, sessions, controlling tty: exec is in `redox-rt` (same file). wait/waitpid, setpgid, setsid, kill, job control, ControlTerm, credential changes (SetResugid) live in a userspace process manager. https://gitlab.redox-os.org/redox-os/base/-/blob/main/bootstrap/src/procmgr.rs . Pty server is userspace `ptyd`: https://gitlab.redox-os.org/redox-os/base (ptyd directory).
3. Signals: kernel redirects the target thread's instruction pointer to a user handler (`signal_handler`, with `INHIBIT_DELIVERY`) and routes CPU exceptions via `excp_handler`; the signal control block is shared between kernel and process. https://gitlab.redox-os.org/redox-os/kernel/-/blob/master/src/context/signal.rs and https://gitlab.redox-os.org/redox-os/relibc/-/blob/master/redox-rt/src/signal.rs . Async-signal-safety issues specific to Redox: not found.
4. fds: kernel keeps the fd table and a `FileDescription` (offset, scheme reference, number, flags) so dup/fork share offsets. https://gitlab.redox-os.org/redox-os/kernel/-/blob/master/src/context/file.rs . relibc wraps tables in `FdTbl`: https://gitlab.redox-os.org/redox-os/relibc/-/blob/master/redox-rt/src/sys.rs . `poll` and `select` are emulated over the epoll/event scheme: https://gitlab.redox-os.org/redox-os/relibc/-/blob/master/src/header/poll/mod.rs and https://gitlab.redox-os.org/redox-os/relibc/-/blob/master/src/header/sys_select/mod.rs .
5. mmap/shm/mprotect: `mmap` via `fmap`; `mprotect` implemented; `msync` and `mremap` return ENOSYS; `madvise` is a stub. https://gitlab.redox-os.org/redox-os/relibc/-/blob/master/src/platform/redox/mod.rs . POSIX shm is a userspace scheme in `ipcd`: https://gitlab.redox-os.org/redox-os/base (ipcd/src/shm.rs).
6. uid/gid: held and changed by `procmgr` (SetResugid), see item 2. Capability coexistence: namespaces act as capabilities, per https://doc.redox-os.org/book/security.html . Which server enforces file permission checks: not found.
7. pthreads/futex: kernel futex keyed by physical address, address space locked while translating. https://gitlab.redox-os.org/redox-os/kernel/-/blob/master/src/syscall/futex.rs . Wrappers: https://gitlab.redox-os.org/redox-os/relibc/-/blob/master/redox-rt/src/sys.rs .
8. Conformance (os-test, Redox 0.5.12): overall 84% (3842/4557); basic 76%; process 83% (20/24); signal 80% (25/31); pty 6% (2/29); namespace 21% (15/70); io 94%; XSI 52%. https://sortix.org/os-test/ . libc-test, LTP, Open POSIX Test Suite, VSX-PCTS results for Redox: not found.
9. Performance cost of the POSIX layer: not found.

## 2. Fuchsia / Zircon

1. fork: not supported in the native libc. The musl fork in the Fuchsia tree returns ENOSYS. https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/third_party/ulib/musl/src/process/fork.c . Starnix (a Linux-ABI executive in userspace) implements fork itself, using VMO snapshot (copy on write) semantics for private mappings: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/starnix/kernel/core/mm/memory_manager.rs .
2. exec/wait/pgrp/sessions: `execve` and `waitpid` are likewise ENOSYS in native musl (https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/third_party/ulib/musl/src/process/execve.c , .../waitpid.c). Process creation is by the launcher/`fdio_spawn`: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/docs/concepts/process/process_creation.md . Process groups, sessions, controlling terminals: not found in native; Starnix emulates Linux ones: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/docs/contribute/governance/rfcs/0082_starnix.md .
3. Signals: native `sigaction` is ENOSYS and `kill` returns EPERM (https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/third_party/ulib/musl/src/signal/sigaction.c , .../signal/kill.c). Starnix: Zircon exceptions from restricted-mode threads arrive as restricted exits and are translated to Linux signals; delivery has three phases (delivery, processing, execution): https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/docs/concepts/starnix/signals.md . Restricted mode design: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/docs/contribute/governance/rfcs/0261_fast_and_efficient_user_space_kernel_emulation.md . `zx_thread_kick_restricted` to interrupt a thread running restricted code: same RFC.
4. fds: client-side fd table in `fdio` over FIDL channels; `fdio_t` is the opaque per-fd object; `dup()` of a fd that must become a handle can be "cloned"; `fdio_handle_fd()` creates a fd specifically for waiting via `select()`/`epoll()`. https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/lib/fdio/README.md . Open path: https://fuchsia.dev/fuchsia-src/concepts/filesystems/life_of_an_open . The design deliberately excludes mutable process-global state ("POSIX Lite"): same fdio README.
5. mmap/mprotect: native VMAR/VMO APIs; Starnix memory manager emulates mmap/mprotect/mremap in a userspace memory manager over VMARs/VMOs: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/starnix/kernel/core/mm/memory_manager.rs .
6. uid/gid: no uids in native Zircon (authority = handles). Starnix keeps Linux credentials in the executive: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/docs/concepts/starnix/README.md (mirrored as starnix_README in scratchpad). How Starnix coexists with capability filesystems beyond that: not found.
7. pthreads/futex: kernel `zx_futex`; Starnix has a userspace futex table in the executive to give Linux futex semantics: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/starnix/kernel/core/mm/futex_table.rs . The Starnix glossary says overall system performance is quite sensitive to futex performance: https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/starnix/glossary.md .
8. Conformance: os-test does not cover Fuchsia (listed OSes: https://sortix.org/os-test/). Open POSIX/LTP/libc-test results: not found.
9. Performance: not found (no published Starnix numbers).

## 3. seL4-based systems

1. fork: seL4 has no processes, no fork. The kernel API exposes threads, CSpaces, VSpaces, and untyped memory: https://sel4.systems/Info/Docs/seL4-manual-latest.pdf (v16.0.0; no POSIX, fork or futex content found by grep). `libsel4muslcsys` installs a vsyscall table whose default for unimplemented syscalls is -ENOSYS: https://github.com/seL4/seL4_libs/blob/master/libsel4muslcsys/src/vsyscall.c . Genode on seL4 and LionsOS differ, below.
2. exec/wait/pgrp/sessions/ctty: none in seL4 itself. CAmkES builds static component assemblies at build time: https://docs.sel4.systems/projects/camkes/manual.html . LionsOS is a static Microkit system of protection domains: https://arxiv.org/pdf/2501.06234 . POSIX-style process management: not found for either.
3. Signals: no signal concept; faults are IPC messages to a fault handler (VM fault, user exception, unknown syscall, capability fault) and replying restarts the thread: https://sel4.systems/Info/Docs/seL4-manual-latest.pdf sections 6.2.1 to 6.2.7. Notifications (`seL4_Signal`/`seL4_Wait`, section 5.2 of the same manual) are the asynchronous primitive. Linux-style signal delivery on seL4: not found.
4. fds: libraries only. LionsOS offers its own coroutine-based blocking API instead of an fd table (arXiv 2501.06234). Select/poll/epoll: not found.
5. mmap/shm/mprotect: userspace allocators over Untyped/Frame/Page caps; page-fault handling is a userspace fault handler per the manual section 6.2.7. https://sel4.systems/Info/Docs/seL4-manual-latest.pdf . A CoW pager built on it: not found in primary docs.
6. uid/gid: none; authority is capabilities: https://sel4.systems/Info/Docs/seL4-manual-latest.pdf .
7. pthreads/futex: `libsel4muslcsys` has a `sys_thread.c` for its thread-related syscalls (https://github.com/seL4/seL4_libs/blob/master/libsel4muslcsys/src/sys_thread.c); contents not analysed. The kernel has no futex; notifications (manual section 5.2) are the signalling primitive. Mapping of pthread locks onto them: not found.
8. Conformance: not found for seL4, CAmkES, LionsOS. os-test excludes them (https://sortix.org/os-test/).
9. Performance: LionsOS paper reports performance of its native system servers, not of a POSIX layer (arXiv 2501.06234). POSIX-layer cost: not found.

### Genode (generic, and on seL4)
1. fork: supported by an optional libc. Eager copy: the libc asks a Clone session to start a new component from the same binary and copies the parent's memory into it. https://github.com/genodelabs/genode/blob/master/repos/libports/src/lib/libc/fork.cc ; release notes https://genode.org/documentation/release-notes/19.08 .
2. exec: `execve` goes through the dynamic linker and starts a new component: https://github.com/genodelabs/genode/blob/master/repos/libports/src/lib/libc/execve.cc . Process groups/sessions: `setsid`, `setpgid` are dummies: https://github.com/genodelabs/genode/blob/master/repos/libports/src/lib/libc/dummies.cc (macro at line ~50 sets ENOSYS for unimplemented ones).
3. Signals: local to the process only (no cross-process kill): https://github.com/genodelabs/genode/blob/master/repos/libports/src/lib/libc/signal.cc .
4. fds: libc-local table; inherited fd info passed to the child through its config on fork (fork.cc "config too deeply nested" path). Plugins back individual fds (vfs).
5. mmap: libc-level over dataspaces. Not verified further.
6. uid/gid: not found.
7. Threads: libc has a cooperative "kernel" that serialises libc code: https://github.com/genodelabs/genode/blob/master/repos/libports/src/lib/libc/kernel.cc ; pthreads: .../pthread.cc.
8-9. Conformance and performance: not found. Note: Noux was the earlier POSIX runtime; history in release notes above.

## 4. QNX Neutrino

Sources are QNX 7.1 and 8.0 documentation, vendor primary.
1. fork: supported. The process manager `procnto` creates processes; "fork() creates a new child process by sharing the same code as the calling process and duplicating the calling process's data". https://www.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.sys_arch/topic/proc_Process_primitives.html . Details from the reference page: child is single-threaded; MAP_PRIVATE mappings are private copies after fork; pending signals, timers, channels, connection IDs, interrupt handlers, aio and resource-manager state are not inherited; caller needs the `PROCMGR_AID_FORK` ability; after a multithreaded fork only async-signal-safe functions are allowed until exec, with `forksafe_mutex` and `pthread_atfork` provided. https://www.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.lib_ref/topic/f/fork.html . Whether the copy is lazy (CoW) or eager: not found in these pages ("duplicating" only). Cost: not found.
2. exec/wait/pgrp: exec, spawn, posix_spawn all in procnto (process_primitives page above); ELF loading in procnto. Sessions and controlling terminals: not found in what I read.
3. Signals: kernel-implemented, 64 signals (32 standard and POSIX realtime) with uniform functionality; queueing selectable per signal; actions per process, mask per thread; CPU-exception signals go to the faulting thread and cannot be blocked; the kernel saves and restores FPU context around handlers (7.1+). Signals and pulses share kernel code. https://www.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.sys_arch/topic/ipc_Signals.html . Kernel calls: `SignalKill`, `SignalAction`, `SignalProcmask`.
4. fds: a file descriptor is a connection ID (`ConnectAttach`) to a resource manager's channel; `open()` obtains it from procnto's pathname space then sends `_IO_CONNECT`. https://qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.resmgr/topic/skeleton_ClientSide.html and https://www.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.lib_ref/topic/c/connectattach.html . Pathname space is a prefix tree in procnto that resource managers adopt: https://qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.sys_arch/topic/proc_PATHMAN.html . Inheritance on fork: child gets its own copy of the fds, each referring to the same open file description (fork reference page). Select/poll: not found.
5. mmap/mprotect: "The virtual memory system implements the POSIX model for memory management along with some QNX Neutrino extensions", in procnto (process_primitives page above). Shared memory object mechanism: not found.
6. uid/gid: procnto manages uid/gid as process attributes (https://china.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.sys_arch/topic/proc_Process_primitives.html summary text: "process attributes such as user ID and group ID"). Fine-grained abilities (`procmgr_ability`) gate privileged operations, e.g. fork needs `PROCMGR_AID_FORK` (fork reference page). Where file permission checks happen: not found; (interpretation) they would be in the resource managers that own the files.
7. pthreads/futex: mutexes, condvars, barriers, semaphores are "implemented directly by the kernel", except sleepon locks and rwlocks (built from mutexes and condvars); atomics in CPU or emulated. https://www.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.sys_arch/topic/kernel_SYNCHRO.html .
8. Conformance: QNX Neutrino certified to POSIX PSE52 (1003.13-2003 realtime controller profile) in 2008; QNX RTOS v4 to 1003.1-1988 in 1993. https://www.qnx.com/news/pr_2862_1.html (via search result summaries; PSE52 statement) and https://www.qnx.com/products/certifications.html . A current POSIX.1-2017 certification for QNX 8.0: not found. Open POSIX/LTP results: not found.
9. Performance: not found.

## 5. MINIX 3

Source: Stichting-MINIX-Research-Foundation/minix, cloned at depth 1. https://github.com/Stichting-MINIX-Research-Foundation/minix
1. fork: PM `do_fork` calls `vm_fork`; the VM server copies regions with `map_proc_copy` and calls kernel `sys_fork`; physical blocks are shared between parent and child (assertion `orig_ph->ph == new_ph->ph` in `map_proc_copy_range`), which is copy-on-write. https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/minix/servers/pm/forkexit.c and .../minix/servers/vm/fork.c and .../minix/servers/vm/region.c (`map_proc_copy`, line ~931). PM must not fail the fork after `vm_fork` (comment in forkexit.c). Cost: not found. VFS is told afterwards via `VFS_PM_FORK`.
2. exec/wait/pgrp/sessions/ctty: PM owns them: exec.c calls `sys_exec` (line ~197), `mp_procgrp` for process groups, setsid/setpgid in getset.c. VFS tracks the controlling tty per process (`fp_tty` in fproc.h). https://github.com/Stichting-MINIX-Research-Foundation/minix/tree/master/minix/servers/pm and .../minix/servers/vfs/fproc.h .
3. Signals: PM runs sigaction, sigprocmask, kill, etc. (`do_sigaction`, `do_kill`, signal.c); delivery calls kernel `sys_sigsend`, which builds the sigcontext frame on the user stack and redirects registers to the handler; kernel signals arrive via `process_ksig`. https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/minix/servers/pm/signal.c and .../minix/kernel/system/do_sigsend.c .
4. fds: VFS holds a per-process table `fp_filp[OPEN_MAX]` pointing at a global `struct filp` (mode, flags, count, vnode, pos); select state lives in the filp; `pm_fork` copies the table. https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/minix/servers/vfs/fproc.h , .../file.h , .../misc.c (`pm_fork`), .../select.c .
5. mmap: VM server (`mmap.c`). Writable MAP_SHARED file mappings are refused (ENXIO); file mapping can be disabled (`enable_filemap`). https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/minix/servers/vm/mmap.c (lines ~254-262). `mprotect`: no implementation found in `minix/servers` (only a man page and a NetBSD stub list); treat as not implemented. SysV shm/sem live in a userspace `ipc` server: .../minix/servers/ipc/shm.c .
6. uid/gid: PM keeps real/effective ids (PM_SETUID, PM_SETEUID and friends in `callnr.h`); VFS enforces file permissions in `forbidden()`. https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/minix/servers/vfs/protect.c .
7. Threads/futex: userland has `thread-stub` in libc (no threads); `libmthread` is cooperative green threads used inside servers, and its own source warns it assumes no preemption. https://github.com/Stichting-MINIX-Research-Foundation/minix/blob/master/minix/lib/libmthread/pthread_compat.c and .../lib/libc/thread-stub/thread-stub.c . No futex found.
8. Conformance (os-test, Minix 3.4.0): overall 73% (3357/4560); basic 66%; process 16% (4/24); signal 64% (20/31); pty 34%; namespace 35%; io 29%; SHM 12%; threads (TPS) 0/23. https://sortix.org/os-test/ . Other suites: not found.
9. Performance: not found.

## 6. GNU Hurd

Sources: glibc `sysdeps/mach/hurd` and `hurd/` (https://github.com/bminor/glibc), Hurd docs, LWN.
1. fork: glibc `_Fork`: stops all other threads (`proc_dostop`), calls `task_create` so the child inherits a copy of the address space (Mach does the VM copy), then copies port rights and thread state, creates the child's threads, and finishes with `proc_child`. A comment documents an avoided deadlock with the signal thread faulting on the same page. https://github.com/bminor/glibc/blob/master/sysdeps/mach/hurd/_Fork.c . The Hurd project itself warns POSIX compatibility layers "may" cause "severe performance degradation", "in this fork system call's case": https://www.gnu.org/s/hurd/open_issues/performance.html (no numbers on that page).
2. exec/wait/pgrp/sessions/ctty: exec goes through the exec server (`file_exec_paths`/`file_exec` in `_hurd_exec`): https://github.com/bminor/glibc/blob/master/hurd/hurdexec.c . PIDs, process groups and sessions belong to the `proc` server, which a fork'd child is introduced to; users can run their own proc server: https://lwn.net/Articles/492853/ . glibc `setsid` asks the proc server and handles the controlling terminal: https://github.com/bminor/glibc/blob/master/sysdeps/mach/hurd/setsid.c .
3. Signals: each process has a signal thread and message port (`_hurd_msgport_thread`); `sig_post` RPCs arrive there. To run a handler it suspends the target thread, `thread_abort`s pending RPCs, reads and edits register state ("thread_get_state is never kosher before thread_abort"). https://github.com/bminor/glibc/blob/master/hurd/hurdsig.c (lines ~50, ~307-325, ~975-1000). Async-signal-safety is therefore the usual one, plus interruptible-RPC logic ("intr-msg").
4. fds: a fd is a Mach port to an io server; table in libc (`hurd/dtable.c`). Select sends `io_select` request messages to every fd's server and waits for replies on one port: https://github.com/bminor/glibc/blob/master/hurd/hurdselect.c (lines ~289-368). Inheritance on fork: port rights copied by `_Fork` (above).
5. mmap/mprotect: `mmap` asks the file server for a memory object with `io_map` then calls `vm_map`; fds that cannot do this return ENODEV. https://github.com/bminor/glibc/blob/master/sysdeps/mach/hurd/mmap.c . `mprotect` is a direct kernel `vm_protect`: https://github.com/bminor/glibc/blob/master/sysdeps/mach/mprotect.c .
6. uid/gid: credentials are auth-server ports; libc holds id sets and builds ports with `auth_makeauth`/`auth_getids`. https://github.com/bminor/glibc/blob/master/hurd/hurdid.c and .../hurd/hurdauth.c . Enforcement point per file: not found in what I read ((interpretation): done by the server that owns the object, using the presented auth port).
7. Threads/futex: Mach `gsync` (`__gsync_wait`) backs the low-level lock. https://github.com/bminor/glibc/blob/master/mach/lowlevellock.h and .../hurd/hurdlock.c .
8. Conformance (os-test, GNU 0.9): overall 91% (4169/4575); basic 89%; process 83% (20/24); signal 87% (27/31); pty 68% (20/29); io 36% (20/55); namespace 84%; threads TPS basic 66%; MSG basic 0/10. https://sortix.org/os-test/ . Linux on the same page: overall 92%, process 87%, signal 93%, io 96%. Other suites: not found.
9. Performance: no measured numbers on the Hurd page; a Phoronix comparison is cited there but the Hurd page criticises it as not measuring the OS: https://www.gnu.org/s/hurd/open_issues/performance.html . Härtig et al. report microkernel overhead for Linux-on-L4/Mach of 0 to about 2.5x: https://cs.uwaterloo.ca/~brecht/servers/readings/Summaries/Seltzer-OS/readings/hartig-1997.html (secondary summary; not Hurd).

## 7. Cross-system conformance data (os-test)

Source: https://sortix.org/os-test/ (only cross-system published data found; no QNX, Fuchsia, seL4, Genode rows; the page says QNX access is still sought).

| Suite | Hurd | MINIX 3.4 | Redox 0.5.12 | Managarm | Linux (reference) |
|---|---|---|---|---|---|
| overall | 91% | 73% | 84% | 94% | 92% |
| basic | 89% | 66% | 76% | 87% | 91% |
| process | 83% (20/24) | 16% (4/24) | 83% (20/24) | 100% (24/24) | 87% (21/24) |
| signal | 87% (27/31) | 64% (20/31) | 80% (25/31) | 93% (29/31) | 93% (29/31) |
| pty | 68% (20/29) | 34% (10/29) | 6% (2/29) | 100% (29/29) | 89% (25/28) |
| io | 36% | 29% | 94% | 63% | 96% |
| namespace | 84% | 35% | 21% | 98% | 84% |
| XSI overall | 86% | 73% | 52% | 79% | 88% |
| TPS (thread process shared sync) | 86% | 0% | 69% | 86% | 100% |
| TSH (thread shared memory) | 76% | 0% | 94% | 94% | 100% |

Caveat: os-test is a comparison tool built by one author, not a certification suite; scores count tests as written and say nothing about severity.
Other suites (Open POSIX Test Suite, LTP, VSX-PCTS) for these systems: not found.

## 8. Lessons for a new Rust capability microkernel that wants full POSIX.1 Base semantics

Everything in this section is (interpretation) except where a URL is attached to the evidence.

### 8.1 What the evidence says directly
- Systems that kept fork out of the design (Fuchsia native, seL4, LionsOS) get ENOSYS and later bolt on a separate Linux executive (Starnix) to get it back. Evidence: musl fork.c ENOSYS (Fuchsia), RFC-0082, vsyscall.c.
- Systems with working fork have a kernel primitive that clones an address space lazily: Redox (proc scheme CoW clone), MINIX 3 (VM server + `sys_fork`, shared physical blocks), Hurd (Mach `task_create` inherits memory), Starnix (VMO snapshot). Genode is the exception: eager copy by a userspace clone service, and it accepts the limits (local signals, dummy sessions). Evidence: sections 1, 2, 3, 5, 6.
- Every system that passes the os-test "process" suite above 80% (Hurd, Redox, Managarm) has a userspace process manager holding pid, parent, pgrp and session state, and the kernel does not. MINIX 3 has the same split (PM) yet scores 16% there, so the split alone is not what decides the score. Evidence: sections 1, 5, 6, 7.
- Signal delivery is a kernel-assisted operation in all working designs: Redox redirects the user IP, MINIX `sys_sigsend` builds the frame, QNX has kernel signals, Hurd's signal thread must suspend and rewrite another thread's registers via kernel calls, Starnix uses exception plus kick. No system delivers signals purely from userspace with no kernel help to stop or redirect a running thread. Evidence: sections 1, 2, 4, 5, 6.
- Futex-like blocking is a kernel primitive in every system with real pthreads: Redox futex (physical-address keyed), QNX Sync calls, Hurd gsync, Zircon `zx_futex`. MINIX 3 has no threads and also no futex. Starnix re-implements Linux futex semantics in userspace on top of the kernel's. Evidence: sections 1, 2, 4, 5, 6.
- fd tables split three ways: kernel-resident (Redox), server-resident (MINIX VFS, QNX connection IDs backed by kernel objects), client-resident (fdio, Hurd ports, Genode libc). Shared open file description semantics across fork (shared offset) needs one shared object: Redox keeps `FileDescription` in the kernel; MINIX keeps `filp` in VFS; QNX documents shared open file description. Evidence: sections 1, 2, 4, 5, 6.

### 8.2 Must be native to the kernel
1. Lazy address-space clone (CoW) with an API a userspace process manager can call on behalf of another process. Evidence: Redox, MINIX, Hurd, Starnix all rely on one; Genode's eager copy is the only alternative and it limits the personality.
2. A way to interrupt, stop and redirect any thread, and to do so atomically with respect to syscall entry: signal delivery with "abort the blocking operation, then change registers" (Hurd's thread_abort/get_state ordering, Redox INHIBIT_DELIVERY, Zircon kick). Plus CPU exceptions delivered as fault IPC (seL4 fault handler model) or exception ports (Zircon) so SIGSEGV/SIGFPE can be synthesised.
3. A futex (wait/wake/requeue on an address, keyed so shared memory works across processes; Redox uses the physical address). Pure userspace blocking via notifications is possible but every system that did it for real pthreads put the primitive in the kernel.
4. Per-thread signal-mask-safe syscall restart/EINTR plumbing (interrupted blocking IPC must return cleanly). Hurd needs a whole `intr-msg` layer for this; a kernel that designs "cancel pending IPC" in from the start avoids it.
5. Fault/pager hooks (seL4-style VM-fault to userspace handler) if `mmap` of files and CoW policy are to live outside the kernel.

### 8.3 Can live in userspace servers
1. Process table: pid, parent/child, wait, zombie reaping, pgrp, sessions, controlling tty, credentials (Redox procmgr, MINIX PM, Hurd proc). Evidence above.
2. fd-to-object table and open-file descriptions, provided there is a shared object for them (MINIX VFS, Redox kernel-resident variant shows it can also be in the kernel). select/poll/epoll over the server's event interface (Redox emulates poll/select over epoll; Hurd sends io_select RPCs).
3. exec (Hurd exec server, Redox redox-rt, MINIX PM + VM), pty (Redox ptyd), SysV and POSIX shm (Redox ipcd, MINIX ipc server), pathname resolution (QNX procnto prefix tree, Hurd translators), uid/gid policy checks in each server.

### 8.4 Risks the evidence flags (interpretation)
- Process-global state in a distributed place is where conformance leaks: Fuchsia native deliberately has none and so cannot do kill/sigaction; Genode keeps signals local; Redox scores 6% on pty and 21% on namespace in os-test; MINIX scores 16% on process. Budget testing on pgrp/session/ctty/pty early.
- fork in multithreaded processes needs the child-side library state to be rebuilt (QNX documents forksafe mutexes and atfork; Hurd `_Fork` has hooks and a deadlock workaround). Design the libc fork path with the kernel clone semantics (single thread in child, locks held by other threads) in mind.
- Capability authority vs POSIX identity: no source examined documents a clean answer. Redox uses namespaces as capabilities plus a procmgr uid; Hurd passes auth ports; QNX uses abilities plus uids; Fuchsia keeps uids only in Starnix. Decide early that uid/gid is a server-side attribute checked by servers and never a kernel authority.
- Measured cost of the POSIX layer: not found for any system. A new project should measure fork+exec, signal round trip and pipe latency itself before committing to a server-heavy split; the Hurd project's own warning about fork cost (https://www.gnu.org/s/hurd/open_issues/performance.html) is the only primary statement, without numbers.

## 9. Gaps
- QNX: sessions/controlling tty, select/poll, shm, permission-check location, lazy vs eager fork copy: not found in pages read.
- Hurd: proc server and exec server internals read only via glibc client side and LWN, not the Hurd server source.
- MINIX 3 user pthreads: none found; mprotect not found in servers.
- Genode on seL4 specifics and LionsOS POSIX behaviour: not found beyond the sources above.
- No source for conformance of QNX, Fuchsia, Genode, seL4 from LTP/Open POSIX/VSX-PCTS: not found.
