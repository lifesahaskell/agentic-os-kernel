---
phase: 02-process-objects-teardown-and-timers
plan: 09
subsystem: kernel-memory
tags: [mmap, munmap, mprotect, brk, space-ledger, agent-budget, ostd, vmspace]

requires:
  - phase: 02-process-objects-teardown-and-timers
    provides: "Space charge ledger (02-06), teardown ordering (02-07, 02-08), process lookup from a thread (02-11)"
provides:
  - "Cap-less self-memory syscalls mmap(9), mprotect(10), munmap(11), brk(12) with Linux numbers, argument order and results"
  - "Eager anonymous private mappings charged through the space ledger to the owner agent; a refused charge is ENOMEM, maps nothing and charges nothing"
  - "PROT_NONE reservations that cost no frames until an mprotect grants access"
  - "Copy-on-write on a shared frame when mprotect grants write"
  - "Program break per space, starting at the first page past the image, capped at BRK_MAX"
affects: [02-12, SIG-03, libaok, procmgr, exec]

actuals:
  tokens: 15800
  tasks: 3
  commits: 5

commits: 5
plan_head_before: aa45594da2b3015bba75f53318d4ff16db191eba
plan_head_after: ab985711798a1842838d2e44755c00350e98e802

tech-stack:
  added: []
  patterns:
    - "Space::mm (ostd Mutex over Maps) orders every mapping call and teardown; the page-table cursor is made after it and dropped inside each helper"
    - "Records cover mmap'd regions only; the image, stack and brk pages are found by walking the page table"
    - "Charge before map; uncharge exactly what unmap removed"

key-files:
  created:
    - kernel/src/space/mmap.rs
    - userland/tests/src/bin/mmap.rs
  modified:
    - userland/abi/src/lib.rs
    - kernel/src/syscall.rs
    - kernel/src/space.rs
    - kernel/src/futex.rs
    - kernel/src/user.rs
    - kernel/src/ipc.rs
    - kernel/src/vm.rs

key-decisions:
  - "Anonymous private mappings are populated eagerly so the budget refusal happens at the call, not at a later fault"
  - "brk is the raw Linux call: any failure returns the old break, and musl's wrapper turns that into ENOMEM"
  - "MAP_FIXED replaces what is there, as on Linux; if the replacement then cannot be paid for, the old pages stay gone"
  - "Mapping records are bounded at MAX_MAPS_PER_SPACE (1024); a call that would exceed it, including a munmap or mprotect that splits a record, is ENOMEM"
  - "futex lookup now treats a page without PageFlags::R as a fault, so a PROT_NONE page is unreadable through FUTEX_WAIT as well as from ring 3"

requirements-completed: [AGNT-02]

coverage:
  - id: D1
    description: "A ring-3 anonymous mmap past the owner's budget returns ENOMEM, maps nothing and charges nothing"
    requirement: AGNT-02
    verification:
      - kind: integration
        ref: "kernel/src/space/mmap.rs#vmem_enomem_past_budget_from_ring3"
        status: pass
    human_judgment: false
  - id: D2
    description: "munmap refunds exactly the pages it removed; a 1,000 turn mmap+munmap loop leaves budget, ledger, records, object counts and the frame probe where they started"
    requirement: AGNT-02
    verification:
      - kind: unit
        ref: "kernel/src/space/mmap.rs#vmem_mmap_munmap_loop_keeps_pages_flat"
        status: pass
    human_judgment: false
  - id: D3
    description: "mprotect and PROT_NONE reservations, including copy-on-write on a shared frame"
    requirement: AGNT-02
    verification:
      - kind: unit
        ref: "kernel/src/space/mmap.rs#vmem_prot_none_reserves_without_charge, vmem_mprotect_write_on_a_shared_frame_sets_cow, vmem_ring3_protect_and_unmap"
        status: pass
    human_judgment: false
  - id: D4
    description: "brk grows, shrinks, refunds and fails soft with the old break; ring-3 brk probe"
    requirement: AGNT-02
    verification:
      - kind: unit
        ref: "kernel/src/space/mmap.rs#vmem_brk_grows_shrinks_and_fails_softly, vmem_brk_refuses_an_occupied_range, vmem_brk_without_an_image_is_zero, vmem_ring3_brk"
        status: pass
    human_judgment: false
  - id: D5
    description: "The syscall fuzz covers numbers 9 to 12 and every decode refusal returns its exact errno"
    verification:
      - kind: unit
        ref: "kernel/src/syscall.rs#fuzz_syscalls_never_panic"
        status: pass
    human_judgment: false

duration: "about 30 min measured from the first task commit to the final run; setup before the first commit was not timed"
completed: 2026-10-26
status: complete
---

# Phase 2 Plan 09: Self-Memory Syscalls Summary

**mmap, mprotect, munmap and brk on the caller's own space, charged page for page through the space ledger to the owner agent: a ring-3 mmap past the budget gets ENOMEM and leaves no mapping and no charge.**

## Performance

- **Duration:** about 30 min from first task commit to the full-suite run (earlier setup not timed)
- **Tasks:** 3 of 3
- **Files created:** 2, **modified:** 7

## Accomplishments

- Task 1 (tracer, `1d7abab`): `abi` numbers, `ENODEV` and `ENOTSUP`, `Space::mmap` with eager charged population, decode and dispatch for number 9, and the `mmap` ring-3 bin (mode 1 maps until ENOMEM and names the errno and the page count). Four `vmem_` ktests ran green end to end before any expansion.
- Task 2 (`522d409` RED, `6b8d8cd` GREEN): `munmap`, `mprotect`, `PROT_NONE` reservations, `MAP_FIXED`, copy-on-write on a shared frame, the record limit, and the futex `R` check so an inaccessible page faults through `FUTEX_WAIT`.
- Task 3 (`2ae70e1` RED, `ab98571` GREEN): `brk` and `set_brk_base`, `Image::brk_base` carried through `load_image`, `from_image`, `start_ctx` and both `ipc.rs` image literals; decode and dispatch for number 12; fuzz extended with the four numbers and eight targeted cases; map-limit and 1,000-turn leak-loop tests.

## Verification

Run under QEMU TCG only. There is no `/dev/kvm` in this environment, so nothing here was run with KVM.

- Full unfiltered `cargo osdk test`: `test result: ok. 171 passed; 0 failed; 0 filtered out.` Log: `/tmp/claude-1000/-home-rakdos-repos-agentic-os-kernel/551cda72-80af-415e-95df-8a64bb7a2418/scratchpad/full3.log`.
- Named results in that run, all `ok`: `vmem_brk_grows_shrinks_and_fails_softly`, `vmem_brk_refuses_an_occupied_range`, `vmem_brk_without_an_image_is_zero`, `vmem_ring3_brk`, `vmem_map_count_limit_is_enomem`, `vmem_mmap_munmap_loop_keeps_pages_flat`, `vmem_enomem_past_budget_from_ring3`, `fuzz_syscalls_never_panic`, `teardown_loop_same_agent_10000x`. The futex `R` check caused no regression in the existing futex tests.
- `sh scripts/check.sh --no-kvm` ends `check: static gates passed; LOCAL-ONLY (needs /dev/kvm): cargo osdk test, cargo osdk run boot checks`.
- Frame probe over the 1,000-turn loop: `frame_probe: loop=vmem_mmap_munmap before=2090315776 after=2090315776 drop=0 bound=688128`. A drop of 0 bytes against a one-process-footprint tolerance of 688,128 bytes.
- Acceptance greps from all three tasks pass (counts taken with comment lines excluded where the plan says so).
- key_links: `process()` appears 4 times in `syscall.rs` dispatch (cap-less path); `charge_pages` 5 times in `mmap.rs`; `mm.lock` 7 times in `mmap.rs` and once in `space.rs` teardown (line 325).

Not verified: anything under KVM, and timing-sensitive behaviour (the TCG scheduler is slower and the ktest scheduler is non-preemptive). The deferred KVM re-run should cover the full suite, in particular `vmem_enomem_past_budget_from_ring3`, `vmem_ring3_brk`, `vmem_ring3_protect_and_unmap`, the 1,000-turn loop and `teardown_loop_same_agent_10000x`.

## SIGXCPU

AGNT-02's SIGXCPU leg is not built. OD-12 assigns it to SIG-03; that item stays open. This plan covers the memory leg only.

## libsys futex wrapper

None exists: the `mmap` bin probes pages with a raw `syscall6(SYS_FUTEX, ...)` call, using `abi::FUTEX_WAIT`.

## TDD Gate Compliance

- Task 1 is a tracer (`type="tracer"`): committed as `feat` after its tests passed end to end, then the tracer gate (re-run `<verify>`) passed before expansion.
- Task 2: RED `522d409` (tests fail against ENOSYS stubs), GREEN `6b8d8cd`.
- Task 3: RED `2ae70e1`, GREEN `ab98571`. At the RED commit three tests failed against the stub `brk` (`vmem_brk_grows_shrinks_and_fails_softly`, `vmem_brk_refuses_an_occupied_range`, `vmem_ring3_brk`; for example `left: 0, right: 4198400`, and `Code(1)` against `Code(0)`). Three others passed at RED: `vmem_brk_without_an_image_is_zero` (the stub already returns 0; it guards the real implementation), `vmem_map_count_limit_is_enomem` and `vmem_mmap_munmap_loop_keeps_pages_flat` (both characterise behaviour Task 2 already built and Task 3 only needed to lock in). The fuzz test also passed at RED because decode for 9 to 11 already existed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ktest filters are exact paths**
- **Found during:** Task 1
- **Issue:** the plan's `cargo osdk test vmem_` filter matches 0 tests; OSTD's ktest filter takes the exact full path (`aok::space::mmap::test::<name>`) and one test per invocation.
- **Fix:** ran each test by full path, and the unfiltered suite for the final gate. The unfiltered run covers every `vmem_` test.

**2. [Rule 3 - Blocking] `<automated>` commands name main-checkout paths**
- **Found during:** Tasks 1 to 3
- **Issue:** the plan's `<automated>` blocks `cd` into `/home/rakdos/repos/agentic-os-kernel/kernel`, the main checkout, not this worktree.
- **Fix:** ran the same commands from this worktree's `kernel/`.

**3. [Rule 1 - Plan inaccuracy] `abi::FUTEX_WAIT` already existed**
- **Issue:** the plan lists adding it; it was already in `userland/abi/src/lib.rs`. Nothing to add.

**4. [Rule 1 - Plan inaccuracy] stale acceptance line**
- **Issue:** Task 1's acceptance says `space.rs` "does not exist at HEAD". It does. The criterion (`mod mmap` count at least 1) holds.

**5. [Rule 3 - Blocking] `vm::COW` made `pub(crate)`**
- **Found during:** Task 2
- **Issue:** `mprotect` must set the copy-on-write flag by the same rule `vm::cow_clone` keeps, and `COW` was private to `vm.rs`. `kernel/src/vm.rs` was not in the plan's `files_modified`.
- **Fix:** one-word visibility change, no behaviour change. Commit `6b8d8cd`.

**6. [Rule 2 - Correctness] decode-time EINVAL for munmap and mprotect**
- **Issue:** an unaligned address, `len` 0 for munmap, or `prot` outside `PROT_BITS` for mprotect is refused in `decode`, before dispatch, so it is EINVAL even from a thread with no process. This matches Linux's errno and keeps the fuzz thread (which has no process) deterministic.

**7. [Rule 3 - Sequencing] Task 1 syscall wiring trimmed, then restored**
- **Issue:** Task 1 wired only number 9; wiring 10 and 11 before `Space::mprotect` and `Space::munmap` existed broke the build (E0599). They were wired in Task 2.

**8. [Rule 3 - Sequencing] temporary `#[allow(dead_code)]`**
- **Issue:** `brk_base` and `brk_cur` were declared in Task 1 and not read until Task 3. They carried `#[allow(dead_code, reason = ...)]` for two commits; Task 3 removed both allows.

**9. [Rule 1 - Bug] clippy `manual_is_multiple_of` in the bin**
- **Found during:** Task 3 gate (`scripts/check.sh --no-kvm`)
- **Fix:** `b0 % PAGE != 0` became `!b0.is_multiple_of(PAGE)` in `userland/tests/src/bin/mmap.rs`. Commit `ab98571`.

### Not changed

- `ipc.rs` was not in the plan's `files_modified`. Two explicit `Image { .. }` literals there needed the new `brk_base` field (Rule 3, two lines, commit `2ae70e1`). `cow_child` in `user.rs` and the space.rs clone path use `..parent.clone()` and needed nothing.

## Known Stubs

None.

## Threat Flags

None. All four calls act on the caller's own space with no capability argument, and every one is bounded: `MAX_MAPS_PER_SPACE` records, `BRK_MAX` for the break, the owner's page budget for frames.

## Issues Encountered

- A stray `touch kernel/src/main.rs` created an empty file and broke the build; removed. No commit carried it.

## Self-Check: PASSED

- Created files exist: `kernel/src/space/mmap.rs`, `userland/tests/src/bin/mmap.rs`, this SUMMARY.
- Commits are ancestors of HEAD: `1d7abab`, `522d409`, `6b8d8cd`, `2ae70e1`, `ab98571`.
- `git rev-list --count aa45594..HEAD` is 5, matching `commits: 5`.
