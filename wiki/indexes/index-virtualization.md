# Index: Virtualization

CPU and memory virtualization. Back to [[index]].

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
