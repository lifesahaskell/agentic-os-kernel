# POSIX Certification: What It Takes (as of 2026-10-23)

Research summary for the "formally POSIX certified" requirement. Every claim has a URL. Claims
marked **[verified]** were re-checked against the primary page on 2026-10-23. The rest come from
a research pass whose fetches went through a summarizing model; re-check them before relying on
them for a commitment.

## Bottom line

1. **POSIX.1-2024 cannot be certified today.** The Open Group's test-suite page (updated
   4 Sep 2026) lists no 2024 / Issue 8 suite **[verified]**
   (https://posix.opengroup.org/testsuites.html). The newest certifiable POSIX targets are
   **IEEE 1003.1-2016 Base** and **PSE52/PSE54 (1003.13-2003)**
   (https://posix.opengroup.org/certification_guide.html). The UNIX brand's newest product
   standard is UNIX V7 (https://www.opengroup.org/openbrand/testing/prodstds.htm).
2. **Money is not the constraint.** A new 1003.1-2016 Base certification costs US$5,000 for one
   platform or $12,000 for a product family. Renewal is $1,000/yr. The POSIX trademark licence
   is optional: $2,000/yr under $100m revenue, $5,000/yr above. Fee schedule dated 12 Jan 2023
   **[verified]** (https://posix.opengroup.org/docs/posix-fee-schedule-1.3.PDF). The latest
   test suites are free to organisations certifying a product, and open-source projects get a
   free 12-month licence **[verified]** (https://posix.opengroup.org/testsuites.html).
3. **The constraint is the surface area.** 1003.1-2016 Base means passing both the System
   Interfaces suite (VSX-PCTS2016, ~29,600 tests) and the Shell & Utilities suite (VSC-PCTS2016,
   ~5,000 tests) (https://posix.opengroup.org/posixfaq.html). That requires a POSIX shell, job
   control, saved IDs, fork/exec/wait, signals and uid/gid semantics. Issue 8 makes threads,
   semaphores, timers, async I/O, mapped files, memory protection and realtime signals mandatory,
   where they had been options
   (https://pubs.opengroup.org/onlinepubs/9799919799/xrat/V4_xbd_chap01.html).
4. **Almost nobody does it.** The POSIX register has 7 products: Green Hills INTEGRITY
   (2004-2012), VxWorks 7 (PSE52, 2019), ZTE Automotive OS (PSE52, 2022) and Xiaomi Vela OS 5.0
   (PSE52, NuttX-based, 2025). None is certified to 1003.1-2016 Base
   (https://posix.opengroup.org/register.html). On the UNIX side, only AIX holds UNIX V7. Huawei
   EulerOS, which is Linux-based, held UNIX 03 from 2016, which shows that a Linux kernel can pass
   (https://www.opengroup.org/openbrand/register/brand3622.htm).
5. **No published time-to-certify exists** for any vendor. The only figures are process SLAs:
   ~6 working days for a clean POSIX submission, and 60 days to fix and resubmit
   (https://posix.opengroup.org/certification_guide.html).

## Targets, ranked for this project

| Target | Fit | Notes |
|---|---|---|
| **1003.1-2016 Base** (SI + Shell & Utilities) | **Recommended** | Full general-purpose POSIX. Free suites, $5k fee. Move to the Issue 8 standard when the Open Group publishes a suite for it. |
| PSE52 (1003.13-2003) | Fallback / stepping stone | Realtime-controller profile for single-process systems. It is what the recent novel kernels passed. It does not cover multi-user, multi-agent POSIX. |
| UNIX V7 brand | Not recommended | Adds XSI: vi, c17, curses, locale creation, plus a third-party commercial C-compiler suite (Plum Hall / Perennial). Trademark fees are published only in a schedule dated 2003, at $25k-$110k/yr (https://www.opengroup.org/openbrand/brandfees.htm). There is no Issue 8 product standard yet. |

## Process (POSIX program)

These steps are summarized from https://posix.opengroup.org/certification_guide.html and
https://posix.opengroup.org/posixfaq.html:

1. Informal runs with authorized suite versions.
2. Sign the certification agreement for the product.
3. Register the product.
4. Submit the conformance statement (an HTML questionnaire) plus a POSIX Conformance Document of
   implementation-defined behavior.
5. Formal run, uploading the full journals. FAIL and UNRESOLVED results are resolved through
   approved Problem Reports.
6. Pay.
7. Get the result.

Certification lasts 12 months. Recompiling for another CPU architecture counts as a new
product.

## Architectural constraints that fall out

- Every **non-agent POSIX process must see unmodified POSIX behavior.** Extensions are allowed,
  but they cannot change what conforming applications observe. The agent layer therefore has to
  be additive: new principals, devices and syscalls. It cannot alter `open(2)` semantics for
  ordinary processes.
- When mediation denies an agent's operation, the denial must surface as a standard errno
  (`EACCES`/`EPERM`), so POSIX tools running *as* agents behave predictably.
- The certified product is a specific build on a specific architecture. The test harness (TET /
  VSXgen) must build and run on it. Redox found cross-compiling it hard
  (https://www.redox-os.org/news/posix-conformance-testing/).
- Free pre-certification signal: Open POSIX Test Suite inside LTP (POSIX.1-2001), musl
  libc-test, Sortix os-test (partial 2024 coverage).

## Open items

- Is there a timetable for POSIX.1-2024 suites? Ask posix-cert@opengroup.org.
- The exact PSE52 option list (https://posix.opengroup.org/docs/pse52-2003.html, not read).
- The macOS UNIX 03 entries are absent from the register today. The only explanation found is a
  secondary blog (https://www.remio.ai/post/apple-macos-silently-removed-from-official-unix-registry-but-the-reason-is-still),
  unverified. This does not affect the plan.
