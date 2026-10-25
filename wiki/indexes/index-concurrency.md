# Index: Concurrency

Threads, locks and synchronization. Back to [[index]].

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
