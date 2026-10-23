# Wiki Index

Catalog of every page. Start with [[aios-ostep-mapping]]; it bridges the two sources and lists the open design problems.

## Sources

- [[aios]]: AIOS: LLM Agent Operating System (arXiv 2403.16971v2, 2024)
- [[ostep]]: Operating Systems: Three Easy Pieces (Arpaci-Dusseau)

## Synthesis (1)

- [[aios-ostep-mapping|AIOS to OSTEP Concept Mapping]]: The bridge page of the wiki.

## AIOS (agentic) (13)

- [[access-manager|Access Manager]]: Enforces access control between agents using a privilege group per agent, with audit logging.
- [[agent-as-process|Agent as Process Analogy]]: Agents are treated like applications/processes: they have IDs, status, priority, can be suspended/resumed, and get allocated memory and storage.
- [[agent-scheduler|Agent Scheduler]]: Kernel module that queues and orders agent requests to the LLM so steps from different agents interleave instead of running agent-by-agent.
- [[aios-architecture|AIOS Architecture and Layers]]: A three-layer design (application, kernel, hardware) in which an LLM-specific kernel sits beside the ordinary OS kernel and exposes LLM services to agents.
- [[aios-sdk|AIOS SDK]]: Higher-level library that wraps LLM syscalls for agent developers.
- [[context-manager|Context Manager]]: Kernel module responsible for the context given to the LLM and for the generation process under that context.
- [[context-snapshot-restore|Context Snapshot and Restore]]: Mechanism to save the state of an LLM's partially complete generation when the scheduler suspends a request, and reload it to resume exactly where it stopped.
- [[context-window-management|Context Window Management]]: Handling contexts longer than the LLM's window.
- [[evaluation-aios|AIOS Evaluation (Consistency and Scheduling)]]: Two experiments on whether suspend/resume preserves outputs and whether scheduling improves wait/turnaround balance.
- [[llm-system-calls|LLM System Calls]]: Interface in the LLM kernel exposing basic LLM-call functions across kernel modules, bridging agent requests and module execution.
- [[memory-manager|Memory Manager]]: Holds each agent's short-term interaction history, available only while the agent is active.
- [[storage-manager|Storage Manager]]: Persists agent data (interaction logs, user preferences) beyond any agent's lifetime.
- [[tool-manager|Tool Manager]]: Manages the external API tools agents may call, spanning text and image modalities.

## Virtualization (28)

- [[address-space|Address Space]]: The OS abstraction of memory: each process sees its own large, private, contiguous space containing code, heap and stack, while the OS maps it onto shared physical memory.
- [[address-translation|Hardware Address Translation (Base and Bounds)]]: Hardware rewrites each virtual address to a physical address on every memory reference, using a base register for relocation and a bounds register for protection.
- [[context-switch|Context Switch]]: Saving the running process's register state and restoring another's so the CPU resumes the second as if it had never stopped.
- [[copy-on-write|Copy-on-Write]]: Share a page between address spaces read-only and copy it only when one side writes.
- [[demand-paging|Demand Paging, Prefetching, and Clustering]]: Page-selection and write-out policies: bring pages in only when touched, optionally guess ahead, and batch writes.
- [[isolation-and-protection|Isolation and Protection]]: The principle that faulty or hostile entities must be unable to read or damage each other or the OS, enforced by hardware and by killing offenders.
- [[limited-direct-execution|Limited Direct Execution (LDE)]]: Run programs directly on the CPU for speed, but with hardware-enforced limits and OS intervention at key points so the OS keeps control.
- [[linux-virtual-memory|Linux Virtual Memory System]]: A production VM combining hardware multi-level tables, huge pages, a unified page cache with 2Q replacement, and layered security defenses.
- [[mechanism-vs-policy|Mechanism versus Policy]]: Design rule that low-level machinery (how to do something) is kept separate from the decision logic (which thing to do), so policies can change without rebuilding mechanisms.
- [[memory-api|Memory API (malloc, free, brk, mmap)]]: The user-level interface for dynamic memory: stack memory is automatic, heap memory is allocated and freed explicitly via library calls built on a few system calls.
- [[mlfq|Multi-Level Feedback Queue]]: A scheduler with several priority queues that infers job behaviour from history, approximating SJF for short interactive jobs while staying fair to long ones.
- [[multi-level-page-table|Multi-Level and Alternative Page Tables]]: Structures that shrink page tables by allocating table space only for address-space regions in use.
- [[multiprocessor-scheduling|Multiprocessor Scheduling]]: Scheduling across several CPUs, where hardware caches, shared-data synchronisation and load balance create problems a single-CPU scheduler never sees.
- [[page-replacement-policy|Page Replacement Policies]]: Rules for choosing which resident page to evict when memory is full, evaluated by hit rate against a cost model.
- [[paging|Paging]]: Divide virtual memory into fixed-size pages and physical memory into same-size frames, mapping pages to frames through a per-process page table.
- [[process-api|Process API (fork, exec, wait, signals)]]: The UNIX interface for creating and controlling processes: fork clones the caller, exec replaces its image, wait blocks until a child exits.
- [[process|Process]]: A running program, defined by its machine state: address space, registers (PC, stack pointer), and open files/I/O information.
- [[proportional-share-scheduling|Proportional-Share Scheduling (Lottery, Stride, CFS)]]: Schedulers that guarantee each job a target share of the resource rather than optimising turnaround or response.
- [[scheduling|CPU Scheduling Policies and Metrics]]: Policies that choose which ready job runs next, judged mainly by turnaround time and response time, with fairness trading against both.
- [[segmentation-and-free-space|Segmentation and Free-Space Management]]: Variable-size allocation of memory regions and the allocator strategies for tracking holes; the source of external fragmentation that paging was designed to remove.
- [[segmented-fifo-and-vms|VAX/VMS: Segmented FIFO and Second-Chance Lists]]: VMS's replacement scheme for hardware without a reference bit: per-process FIFO with global clean and dirty free lists as a second chance.
- [[swapping|Swapping and Page Faults]]: Treating disk as a larger, slower backing store so the sum of address spaces can exceed physical memory.
- [[system-call|System Call]]: A controlled entry from user code into the kernel to perform a privileged operation; it looks like a procedure call but is implemented as a trap.
- [[tlb|Translation Lookaside Buffer]]: A small hardware cache of recent VPN-to-PFN translations that makes paging fast by exploiting locality.
- [[trap-and-interrupt|Traps, Interrupts and User/Kernel Mode]]: Hardware mechanisms that transfer control to the OS: a trap on system call or exception, an interrupt on a device or timer event, with a privilege bit separating user mode from kernel mode.
- [[virtual-machine-monitor|Virtual machine monitor (hypervisor)]]: A layer beneath the OS that multiplexes the hardware among whole operating systems while each believes it controls the machine; transparency is the goal.
- [[virtualization-and-resource-management|Virtualization and the OS as Resource Manager]]: The OS turns a physical resource (CPU, memory) into a more general, easy-to-use virtual form, and manages the resource among competing programs.
- [[working-set-and-thrashing|Working Set, Thrashing, and Admission Control]]: When the combined active pages of running processes exceed physical memory, the system pages constantly and makes little progress.

## Concurrency (15)

- [[atomic-instructions|Atomic Hardware Instructions]]: Single indivisible CPU instructions (test-and-set, compare-and-swap, load-linked/store-conditional, fetch-and-add) that are the foundation of working locks on multiprocessors.
- [[atomicity-and-ordering-violations|Atomicity and Order Violations]]: The two patterns making up 97% of non-deadlock concurrency bugs in the Lu et al.
- [[condition-variable|Condition Variable]]: A queue where a thread sleeps, releasing its lock, until another thread signals that a state condition may have changed; always paired with a lock and a state variable.
- [[deadlock|Deadlock]]: A cycle of threads each holding a lock another needs; occurs only when mutual exclusion, hold-and-wait, no preemption, and circular wait all hold.
- [[dining-philosophers|Dining Philosophers]]: Dijkstra's problem of five philosophers each needing two shared forks to eat, used to show how naive resource acquisition deadlocks.
- [[event-based-concurrency|Event-Based Concurrency]]: Handling many concurrent activities in a single-threaded event loop that waits for events and runs short handlers, giving explicit control over scheduling without locks.
- [[lock-based-data-structures|Lock-Based Concurrent Data Structures]]: Making counters, lists, queues, and hash tables thread-safe by adding locks; where and how many locks determines correctness and scalability.
- [[lock-free-synchronization|Lock-Free Synchronization]]: Updating shared structures with atomic compare-and-swap retry loops so no lock is held, removing deadlock but not livelock.
- [[lock|Lock (Mutex)]]: A variable that is either free or held, giving mutual exclusion over a critical section; judged by correctness, fairness, and performance.
- [[pthreads-api|POSIX Threads API]]: The standard C interface for thread creation, joining, mutex locking, and condition-variable signaling; the vocabulary the rest of the concurrency chapters assume.
- [[race-condition|Race Condition and Critical Section]]: A data race where results depend on the timing of thread execution; the code that touches shared state this way is a critical section needing mutual exclusion.
- [[reader-writer-lock|Reader-Writer Lock]]: A lock that permits many concurrent readers or exactly one writer, exploiting that lookups do not change data structures.
- [[semaphore|Semaphore]]: An integer-valued object with sem_wait (decrement, block if negative) and sem_post (increment, wake a waiter) that works as both lock and condition variable.
- [[spin-lock|Spin Lock (and Ticket Lock)]]: A lock whose waiters busy-loop on an atomic instruction; correct but wasteful when the holder is descheduled, and unfair unless built as a ticket lock.
- [[thread|Thread]]: A second point of execution inside one process: own PC, registers and stack, but a shared address space and heap.

## Persistence (15)

- [[atomic-rename|Atomic File Update via rename()]]: rename(old, new) is atomic with respect to crashes, so a file reads as the old or new version, never a mix; combined with write-to-temp and fsync it gives atomic whole-file replacement.
- [[crash-consistency|Crash Consistency]]: The problem of updating multiple on-disk structures when a crash can land between any two writes, leaving metadata inconsistent, space leaked, or garbage data visible.
- [[data-integrity|Data Integrity and Protection]]: Techniques to detect and recover from partial disk failures (latent sector errors and silent corruption) so data read equals data written.
- [[fast-file-system|Fast File System (FFS) and Locality]]: Berkeley redesign that keeps the file-system API but makes layout disk-aware: group related data and metadata near each other to avoid long seeks.
- [[file-and-directory-api|File and Directory API (POSIX)]]: The system-call interface for persistent storage: files are byte arrays named by an inode number, directories map names to inode numbers, and a single namespace is assembled by mounting.
- [[file-descriptor|File Descriptor and Open File Table]]: A small per-process integer indexing an array of pointers to entries in a system-wide open file table, each holding the current offset, flags, and a pointer to the inode.
- [[file-system-implementation|File System Implementation (vsfs)]]: The very simple file system: disk divided into superblock, inode and data bitmaps, inode table, and data blocks, plus the read/write paths and caching that operate on them.
- [[flash-ssd|Flash-based SSD and the FTL]]: NAND flash storage with no moving parts but asymmetric operations; a flash translation layer presents a normal block interface while hiding erase-before-program and wear-out.
- [[fsync-and-durability|fsync and Durability]]: write() only hands data to a buffering file system; fsync(fd) forces the file's dirty data to storage and returns when done, and is the application's lever on the durability/performance trade-off.
- [[hard-disk|Hard Disk Drive]]: A rotating-platter block device whose I/O time is seek plus rotational delay plus transfer, so sequential access is far cheaper than random access.
- [[inode|Inode]]: The on-disk structure holding a file's metadata and block pointers, addressed by inode number; directories supply the name-to-number mapping.
- [[io-devices-and-drivers|I/O Devices and Device Drivers]]: The OS talks to devices through a status/command/data register interface, hidden behind drivers so the rest of the kernel stays device-neutral.
- [[journaling|Journaling (Write-Ahead Logging)]]: Before updating structures in place, write a description of the update to a log; after a crash, replay committed transactions.
- [[log-structured-file-system|Log-Structured File System (LFS)]]: Buffer all updates, including metadata, into large in-memory segments and write them sequentially to free disk space; never overwrite in place.
- [[raid|RAID]]: Redundant Array of Independent Disks: combine many disks behind one block interface for capacity, performance, and reliability.

## Distribution (6)

- [[afs|AFS (Andrew File System)]]: A distributed file system built for scale, using whole-file caching on client disks and server callbacks instead of polling.
- [[cache-consistency|Cache consistency]]: When clients cache remote data, writers' updates must become visible and readers must not use stale copies; protocols differ in when they guarantee this.
- [[distributed-systems-communication|Distributed systems communication]]: Machines cooperate only by exchanging messages over a network that can lose, corrupt, or reorder packets.
- [[idempotency|Idempotent operations and retry safety]]: An operation that gives the same result when applied repeatedly can be retried blindly after a lost reply, which removes the need for server-side memory of requests.
- [[nfs|NFS (v2), a stateless network file system]]: A client/server file protocol designed so the server keeps no per-client state and therefore recovers from a crash by simply restarting.
- [[rpc|Remote procedure call]]: Makes a remote function call look like a local one by generating stubs that marshal arguments and a run-time library that moves the bytes.

## Security (12)

- [[access-control-lists|Access control lists]]: Each object stores a list of which principals may do what, checked on each access.
- [[access-control|Access control and the reference monitor]]: Decides whether a subject may perform an access on an object by consulting a policy at a single mediating point.
- [[authentication|Authentication]]: Establishing which principal is acting, from something it knows, has, or is, and binding that identity to a process for its lifetime.
- [[capabilities|Capabilities]]: An unforgeable token that both names an object and grants specific rights to it; possession is the authorization.
- [[cryptography|Cryptography]]: Transforms data with a key so that confidentiality, integrity, and authenticity can be provided over untrusted channels and storage; all benefit rests on key secrecy.
- [[distributed-security|Distributed system security (authentication, certificates, TLS)]]: Across machines the OS controls only its own host, so security rests on authenticating remote parties and protecting messages on a network the adversary can read, forge, replay, and delay.
- [[password-storage|Password storage and guessing resistance]]: Servers keep a salted one-way hash so a stolen file does not reveal passwords, and slow or limit guessing.
- [[principle-of-least-privilege|Security design principles (Saltzer-Schroeder) and least privilege]]: A checklist of design rules for protection mechanisms, of which least privilege (grant only the rights needed, only for as long as needed) is the most operationally important.
- [[rbac-and-mac|RBAC, mandatory access control, and type enforcement]]: Policy attached to roles or labels set by an administrator, not by object owners, so users cannot loosen it.
- [[security-goals-and-threat-model|Security goals, policy, and threat model]]: Security is stated as goals (confidentiality, integrity, availability, plus authenticity and non-repudiation), turned into policies, and enforced by mechanisms; the OS is inside the trusted base.
- [[setuid-and-privilege-escalation|Setuid, sudo, and privilege escalation (nearest material to confused deputy)]]: A program can run with its owner's rights instead of the caller's, which is needed for controlled elevation and is a prime attack target.
- [[trusted-computing-base|Trusted computing base]]: The OS cannot hide data from itself: whatever must be trusted to enforce security is the base, and it must be small and correct.
