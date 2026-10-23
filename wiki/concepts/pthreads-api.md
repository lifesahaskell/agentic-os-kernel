---
title: POSIX Threads API
type: concept
sources: [ostep]
tags: [concurrency]
---

# POSIX Threads API

The standard C interface for thread creation, joining, mutex locking, and condition-variable signaling; the vocabulary the rest of the concurrency chapters assume.

## Mechanisms

- pthread_create(thread, attr, start_routine, arg) takes a void* argument and returns a void* result; pthread_join(thread, retval) waits and collects it (ostep p. 329-332)
- Returning pointers to a thread's stack variable from the start routine is a bug; pass data via heap or caller-owned memory (ostep p. 332)
- pthread_mutex_lock/unlock bracket a critical section; initialize with PTHREAD_MUTEX_INITIALIZER or pthread_mutex_init, and check return codes via wrappers (ostep p. 333-335)
- pthread_mutex_trylock and timedlock exist but should generally be avoided (ostep p. 335)
- pthread_cond_wait(cond, mutex) / pthread_cond_signal: waiting needs a held lock and a re-check loop (ostep p. 335-336)
- Practical advice: keep it simple, minimize thread interaction, init locks and conditions, check return codes, use man pages (ostep p. 337-338)

## Trade-offs

- A small, portable primitive set that pushes all correctness (init, error checks, loops) onto the programmer.

## Agentic OS relevance

- (interpretation) Tool-call runtimes expose the same create/join/lock/wait shape (spawn agent, await result, mutex on a resource); the same discipline about return values and ownership applies.

## Related

- [[thread]]
- [[lock]]
- [[condition-variable]]
