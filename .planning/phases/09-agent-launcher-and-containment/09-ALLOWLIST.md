# Phase 9 seccomp allowlist record (09-ALLOWLIST.md)

Written by 09-06 from the `src/seccomp.rs` module doc and the Allowlist record in
`09-03-SUMMARY.md`, then checked against the committed `TABLE`, `DENY`, `allow_ranges()` and
`program()` in `tools/aok-launch/src/seccomp.rs`. Decisions: D-01 to D-05.

## Check against the code

The numbers below were recomputed from the committed table, not copied. A script parsed `TABLE` and
`DENY` out of `seccomp.rs` and printed: 383 entries, no duplicate numbers, minimum 0, maximum 469,
one gap (337 to 423, 87 numbers), 23 deny entries whose names all match the table, 14 coalesced allow
ranges covering 360 numbers, and a program length of 71 instructions (6 header, 40 for the ranges,
23 deny compares, 2 returns). Every figure equals the record. No mismatch between the record and the
code was found, so nothing is reconciled.

The crate's own tests also ran green in the 09-06 full suite: `seccomp::tests::table_shape_matches_the_record`,
`allow_ranges_match_the_record`, `deny_set_names_match_the_table`,
`every_number_gets_the_documented_action` (an in-test BPF interpreter over every number 0..0x1000),
`x32_and_foreign_arch_are_eperm_for_every_number` and `program_reads_only_arch_and_nr_and_never_kills`.

## Derivation (D-01)

- Source: `arch/x86/entry/syscalls/syscall_64.tbl` at the Linux v6.18 tag. Rows with ABI `common`
  and `64` are kept; `x32` rows are not.
- The file is GPL and is not vendored. The derived Rust table (`TABLE`, `(number, name)` pairs) is the
  committed artifact.
- Table maximum: 469 (`file_setattr`). Numbers 0 to 336 and 424 to 469 are present; 337 to 423 is the
  one gap.
- 09-03 reported an independent comparison of the table against the v6.18 tbl with zero diffs. 09-06
  did not repeat that comparison; it checked the table against the record only.

## Coalesced allow ranges (inclusive)

0-100, 102-154, 156-164, 167-174, 177-245, 247-297, 299-312, 314-319, 322, 324-336, 424, 434-441,
443-466, 468-469. That is 14 ranges and 360 allowed numbers (the 383 table entries minus 23 denied).

`clone3` (435) is in 434-441 and is allowed (D-02). The launcher and glibc spawn with it, and its
flags sit in a user struct that the filter never reads (D-04).

## Deny set (23 numbers, EPERM)

| Family | Numbers | Rationale |
|---|---|---|
| ptrace | 101 | cross-process memory and register access |
| perf_event_open | 298 | hardware counters, a side channel and a large kernel attack surface |
| bpf | 321 | loads programs into the kernel |
| userfaultfd | 323 | lets user space stall the kernel at a chosen page fault, an exploit primitive |
| mount family | 165 mount, 166 umount2, 155 pivot_root, 428 open_tree, 429 move_mount, 430 fsopen, 431 fsconfig, 432 fsmount, 433 fspick, 442 mount_setattr, 467 open_tree_attr | change mount topology, which Landlock path rules do not model |
| kexec | 246 kexec_load, 320 kexec_file_load | replace the running kernel |
| kernel modules | 175 init_module, 176 delete_module, 313 finit_module | load or unload kernel code |
| io_uring | 425 io_uring_setup, 426 io_uring_enter, 427 io_uring_register | operations submitted through a ring do not pass this filter as syscalls |

## Program order and the errno per branch (D-01, D-05)

The filter loads only `seccomp_data.arch` (offset 4) and `seccomp_data.nr` (offset 0). It never loads
`args` or `instruction_pointer` (D-04), so it dereferences no pointer argument (LNCH-06). It returns
only `ALLOW` or `ERRNO` actions, never `KILL_PROCESS` or `KILL_THREAD`. It is assembled by hand because
`seccompiler`'s high-level constructor emits `KILL_PROCESS` on an arch mismatch.

| Step | Test | Result |
|---|---|---|
| 1 | `arch != AUDIT_ARCH_X86_64` (0xc000003e): the i386 `int 0x80` path and any other ABI | EPERM |
| 2 | `nr & 0x40000000` set (the x32 marker bit): an x32 call arrives under `AUDIT_ARCH_X86_64`, so step 1 passes it and this test catches it | EPERM |
| 3 | `nr` inside an allow range: one JGE/JGE/RET triple per range, JEQ/RET for a one-number range (322, 424) | ALLOW |
| 4 | `nr` equal to one of the 23 deny numbers: 23 JEQ compares to a shared return | EPERM |
| 5 | anything else: a table gap (337-423) or any number above 469, not x32 | ENOSYS |

ENOSYS for gaps and above-table numbers is what an older kernel returns, so a libc probe for a newer
syscall degrades as it would on that kernel. EPERM for the denied families is what an unprivileged
`bpf` call and `io_uring_disabled=2` also return on a stock kernel. Every jump offset is under 255,
checked by `u8::try_from` (a `try_into().expect` in `off()`).

## statmount and listmount (discretion)

`statmount` (457) and `listmount` (458) stay allowed. They only read the mount table visible to the
caller's own mount namespace. The topology-changing calls (the mount family above) are the denied ones.

## Seccomp cache (D-04)

That the kernel serves this filter from its per-syscall action cache (Linux 5.11 and later) is ASSUMED.
The filter reads only `arch` and `nr`, which is the shape the cache needs, but
`CONFIG_SECCOMP_CACHE_DEBUG` is unset on this host, so `/proc/PID/seccomp_cache` is absent and no test
asserts engagement.

## A1: do libc probes degrade on ENOSYS?

- The launcher's own probes are `clone3` (falls back to `fork` on ENOSYS), `ioctl(PIDFD_GET_INFO)`
  (falls back on ENOTTY) and `landlock_create_ruleset` with the version flag (ENOSYS and EOPNOTSUPP both
  mean ABI 0). All three numbers are in allow ranges, so the filter never changes their result.
  MEASURED: every launch in the crate suite runs through the filter and the spawn path works.
- glibc, the host libc, MEASURED in 09-03: `strace -f` over the launcher with `sh -c true`,
  `python3 -I -B -c pass`, `cat /proc/self/status` and `sleep 0.01` shows the 71-instruction filter
  installed and then no syscall in any of the four processes returning EPERM or ENOSYS after the
  install. The only post-install failures were ENOENT and Landlock EACCES from the shell's `PATH`
  search, which are not the filter. 09-06 did not repeat the strace run.
- musl 1.2.6: NOT MEASURED. No musl toolchain is on this host. Reasoned only: its probes are also
  ordinary syscalls inside the allow ranges, and a probe above 469 gets the same ENOSYS an older kernel
  gives. This is an open verification item for the first musl-linked agent (phase B3).
- No probe was found that branches on a non-ENOSYS errno, so no allow-list decision checkpoint was needed.

## Not covered by this record

- `/proc` is granted read-only wholesale by the default Landlock grant. A sibling agent's
  `/proc/PID/environ` was not tested (ptrace access-mode checks and the ptrace denial are the reasoned
  protection).
- Behaviour on a kernel with `CONFIG_IA32_EMULATION` off: `arch_mismatch_eperm` asserts a positive pid
  outside the agent and would fail there, not skip.
