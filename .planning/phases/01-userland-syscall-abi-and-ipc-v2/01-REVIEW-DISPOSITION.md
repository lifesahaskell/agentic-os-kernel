---
phase: 01
review: 01-REVIEW.md
titles: json
findings:
  - id: CR-01
    severity: critical
    disposition: fixed
    title: "Ring-3 write to a kernel-half address panics the kernel"
  - id: WR-01
    severity: warning
    disposition: open
    title: "`recv_parked` can consume a wakeup without consuming a message"
  - id: WR-02
    severity: warning
    disposition: open
    title: "`unreceive` pushes past `QUEUE_LEN` and can hand a stranded caller a second chance only by luck"
  - id: WR-03
    severity: warning
    disposition: fixed
    title: "Frame allocation `unwrap`s turn memory pressure into a kernel panic"
  - id: WR-04
    severity: warning
    disposition: open
    title: "CPU budget is sampled at the tick, so a thread can run uncharged"
  - id: WR-05
    severity: warning
    disposition: open
    title: "The CPU-budget boot check cannot detect starvation or under-charging"
  - id: WR-06
    severity: warning
    disposition: open
    title: "The syscall fuzz test asserts almost nothing"
  - id: WR-07
    severity: warning
    disposition: open
    title: "The boot gate in `check.sh` has no timeout and discards the exit status"
  - id: WR-08
    severity: warning
    disposition: open
    title: "`unsafe_check.py` misses multi-line `allow` and files outside `src/`"
  - id: IN-01
    severity: info
    disposition: open
    title: "Capability delegation depth is documented but not enforced"
  - id: IN-02
    severity: info
    disposition: open
    title: "`load_image` accepts an entry inside the header and an image that overlaps `TEST_DATA`"
  - id: IN-03
    severity: info
    disposition: open
    title: "Hardcoded quota in the boot log line"
  - id: IN-04
    severity: info
    disposition: open
    title: "`reader` handler relies on redirect timing"
  - id: IN-05
    severity: info
    disposition: open
    title: "`build.sh` leaves stale images in `out/`"
  - id: IN-06
    severity: info
    disposition: open
    title: "`osdk.sh` can leave root-owned files after an interrupted run"
open: 13
total: 15
---

# Phase 01: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| CR-01 | critical | fixed | 40a9da1 (plan 01-08) |
| WR-01 | warning | open | - |
| WR-02 | warning | open | - |
| WR-03 | warning | fixed | 2ee2116 (plan 01-08) |
| WR-04 | warning | open | - |
| WR-05 | warning | open | - |
| WR-06 | warning | open | - |
| WR-07 | warning | open | - |
| WR-08 | warning | open | - |
| IN-01 | info | open | - |
| IN-02 | info | open | - |
| IN-03 | info | open | - |
| IN-04 | info | open | - |
| IN-05 | info | open | - |
| IN-06 | info | open | - |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved.
