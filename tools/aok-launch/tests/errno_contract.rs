//! Seccomp errno contract (LNCH-06). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test errno_contract -- --test-threads=1`
//!
//! The filter's decisions are asserted three ways: through the real launcher
//! (`errno_contract`, `arch_mismatch_eperm`), by inspecting the assembled
//! program (`filter_no_ptr`), and by stacking a second filter in a child stage
//! (`subset_seccomp`). The syscalls are issued from python3 ctypes or a C
//! probe, so this file needs no `unsafe`.

mod common;

use std::fs;
use std::process::Command;

use aok_launch::seccomp::{self, RET_ACTION_MASK, RET_ALLOW, RET_ERRNO};
use common::*;
use seccompiler::sock_filter;

/// Issues raw syscalls through `CDLL(None).syscall` and prints
/// `name ret=R errno=E` per line. Takes no argument and never exits non-zero
/// because of what it observes.
const PROBE: &str = r#"
import ctypes, os
libc = ctypes.CDLL(None, use_errno=True)
libc.syscall.restype = ctypes.c_long

def sc(n, *a):
    ctypes.set_errno(0)
    r = libc.syscall(ctypes.c_long(n), *[ctypes.c_long(x) for x in a])
    return r, ctypes.get_errno()

def show(name, res):
    print("%s ret=%d errno=%d" % (name, res[0], res[1]))

params = ctypes.create_string_buffer(120)
r = sc(425, 1, ctypes.addressof(params))
show("uring_setup", r)
if r[0] >= 0:
    os.close(r[0])
show("uring_enter", sc(426, -1, 0, 0, 0))
show("uring_register", sc(427, -1, 0, 0, 0))
show("x32", sc(0x40000000 | 39))
show("gap", sc(400))
show("gap_edge_low", sc(337))
show("gap_edge_high", sc(423))
show("above", sc(470))
show("far_above", sc(5000))
show("getpid", sc(39))
show("clone3", sc(435, 0, 0))
"#;

#[test]
fn errno_contract() {
    let a = Agent::new("errno");
    let s = Scratch::new("errno");
    let script = s.path().join("probe.py");
    fs::write(&script, PROBE).unwrap();
    let script_s = script.to_str().unwrap();
    let grant = format!("rx:{}", s.s());

    // The control: outside any agent, the witness succeeds. io_uring is
    // enabled on this host (io_uring_disabled 0, MEASURED), so this is what
    // tells the filter apart from the kernel. init_module is not a witness:
    // an unprivileged caller gets EPERM from the capability check anyway.
    let c = Command::new("python3")
        .args(["-I", "-B", script_s])
        .output()
        .expect("control probe");
    assert!(c.status.success());
    let control = parse_probe(&String::from_utf8_lossy(&c.stdout));
    let up = probe_line(&control, "uring_setup");
    assert!(
        up.ret >= 0,
        "control io_uring_setup failed (errno {}); is io_uring_disabled non-zero on this host?",
        up.errno
    );
    // Without the filter a bad ring fd is rejected by the ring code itself
    // (MEASURED: EINVAL here), never with EPERM.
    for k in ["uring_enter", "uring_register"] {
        let l = probe_line(&control, k);
        assert_eq!(l.ret, -1, "control {k}");
        assert_ne!(
            l.errno,
            libc::EPERM,
            "control {k} already EPERM without the filter"
        );
    }

    // The same probe through the real filter.
    let o = launch(&[
        "run", "--name", &a.name, "--grant", &grant, "--", "python3", "-I", "-B", script_s,
    ]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = parse_probe(&o.stdout);
    let eperm = |k: &str| {
        let l = probe_line(&m, k);
        assert_eq!((l.ret, l.errno), (-1, libc::EPERM), "{k}: {}", o.stdout);
    };
    let enosys = |k: &str| {
        let l = probe_line(&m, k);
        assert_eq!((l.ret, l.errno), (-1, libc::ENOSYS), "{k}: {}", o.stdout);
    };
    // Known-denied: EPERM where the control saw success or EBADF.
    eperm("uring_setup");
    eperm("uring_enter");
    eperm("uring_register");
    // x32 numbers: EPERM (D-05).
    eperm("x32");
    // Gap and above-table numbers: ENOSYS (D-01), including both gap edges.
    enosys("gap");
    enosys("gap_edge_low");
    enosys("gap_edge_high");
    enosys("above");
    enosys("far_above");
    // Allowed numbers behave normally: getpid works and clone3 (D-02) got
    // past the filter to its own argument check (size 0 is EINVAL).
    assert!(probe_line(&m, "getpid").ret > 0, "{}", o.stdout);
    let c3 = probe_line(&m, "clone3");
    assert_eq!((c3.ret, c3.errno), (-1, libc::EINVAL), "{}", o.stdout);
}

/// i386 `getpid` through `int 0x80`: the kernel reports AUDIT_ARCH_I386.
const C_PROBE: &str = r#"
#include <stdio.h>
int main(void) {
    long r;
    __asm__ volatile("int $0x80" : "=a"(r) : "0"(20L) : "memory", "r8", "r9", "r10", "r11");
    printf("%ld\n", r);
    return 0;
}
"#;

#[test]
fn arch_mismatch_eperm() {
    assert!(
        Command::new("cc").arg("--version").output().is_ok(),
        "arch_mismatch_eperm needs a host C compiler (cc); install one, this test must not skip"
    );
    let a = Agent::new("arch");
    let s = Scratch::new("arch");
    let src = s.path().join("probe.c");
    let bin = s.path().join("probe");
    fs::write(&src, C_PROBE).unwrap();
    let cc = Command::new("cc")
        .args(["-O0", "-o"])
        .arg(&bin)
        .arg(&src)
        .output()
        .expect("run cc");
    assert!(
        cc.status.success(),
        "cc failed: {}",
        String::from_utf8_lossy(&cc.stderr)
    );
    let bin_s = bin.to_str().unwrap();

    // Outside an agent: the compat entry works and returns a pid.
    let c = Command::new(&bin).output().expect("control probe");
    assert!(c.status.success(), "control died: {:?}", c.status);
    let pid: i64 = String::from_utf8_lossy(&c.stdout).trim().parse().unwrap();
    assert!(
        pid > 0,
        "int 0x80 getpid returned {pid} outside the agent; IA32 emulation is off on this host"
    );

    // Inside the agent: -EPERM, not a kill (D-05).
    let grant = format!("rx:{}", s.s());
    let o = launch(&["run", "--name", &a.name, "--grant", &grant, "--", bin_s]);
    // Exit 0 is the wait status: a SIGSYS or a kill would surface as 128+n.
    assert_eq!(o.code, 0, "agent died; stderr: {}", o.stderr);
    assert_eq!(o.stdout.trim(), "-1", "stdout: {}", o.stdout);
    assert!(!o.stderr.contains("SIGSYS"), "stderr: {}", o.stderr);
}

#[test]
fn filter_no_ptr() {
    let prog = seccomp::program();
    assert!(!prog.is_empty());
    let mut loads = 0;
    for (pc, i) in prog.iter().enumerate() {
        match i.code & 0x07 {
            // BPF_LD: only `A = seccomp_data[k]` (BPF_W | BPF_ABS = 0x20).
            0x00 => {
                assert_eq!(i.code, 0x20, "insn {pc}: LD that is not W|ABS");
                assert!(i.k == 0 || i.k == 4, "insn {pc}: load at offset {}", i.k);
                loads += 1;
            }
            // BPF_JMP: JEQ, JGE or JSET with a constant.
            0x05 => assert!(
                matches!(i.code, 0x15 | 0x35 | 0x45),
                "insn {pc}: unexpected jump {:#x}",
                i.code
            ),
            // BPF_RET with a constant.
            0x06 => {
                assert_eq!(i.code, 0x06, "insn {pc}: RET not constant");
                let action = i.k & RET_ACTION_MASK;
                assert!(
                    action == RET_ALLOW || action == RET_ERRNO,
                    "insn {pc}: return {:#x} is not ALLOW or ERRNO (no KILL)",
                    i.k
                );
            }
            // LDX, ST, STX, ALU, MISC: scratch memory, index and arithmetic
            // are how a filter would chase a pointer argument.
            c => panic!(
                "insn {pc}: class {c:#x} (code {:#x}) is not allowed",
                i.code
            ),
        }
    }
    assert!(loads >= 2, "expected an arch load and an nr load");
    assert_eq!(
        prog.iter().filter(|i| i.code == 0x20).count(),
        loads,
        "every load is W|ABS"
    );
}

const STAGE: &str = "AOK_SECCOMP_STAGE";

/// Runs only inside the child that `subset_seccomp` spawns: installs the
/// launcher filter on this thread, then an allow-everything second filter, and
/// checks a denied syscall is still denied. The probe is a grandchild so the
/// filters apply to a fresh process, as they do to an agent.
fn subset_seccomp_stage() {
    seccomp::apply().expect("install the launcher filter");
    let allow_all = [sock_filter {
        code: 0x06,
        jt: 0,
        jf: 0,
        k: RET_ALLOW,
    }];
    seccompiler::apply_filter(&allow_all).expect("stacking an allow-all filter succeeds");

    let s = Scratch::new("subsec");
    let script = s.path().join("probe.py");
    fs::write(&script, PROBE).unwrap();
    let o = Command::new("python3")
        .args(["-I", "-B"])
        .arg(&script)
        .output()
        .expect("spawn the probe under both filters");
    assert!(o.status.success(), "probe died: {:?}", o.status);
    let m = parse_probe(&String::from_utf8_lossy(&o.stdout));
    let up = probe_line(&m, "uring_setup");
    assert_eq!(
        (up.ret, up.errno),
        (-1, libc::EPERM),
        "the allow-all filter lifted a denial: {:?}",
        String::from_utf8_lossy(&o.stdout)
    );
    let gap = probe_line(&m, "gap");
    assert_eq!((gap.ret, gap.errno), (-1, libc::ENOSYS));
    println!("subset_seccomp stage ok");
}

#[test]
fn subset_seccomp() {
    if std::env::var_os(STAGE).is_some() {
        subset_seccomp_stage();
        return;
    }
    // Re-run this test binary for just this test, so the filters confine the
    // child and not the test process.
    let me = std::env::current_exe().unwrap();
    let o = Command::new(me)
        .args([
            "--exact",
            "subset_seccomp",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(STAGE, "1")
        .output()
        .expect("spawn the stage");
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "stage failed: {:?}\nstdout: {out}\nstderr: {}",
        o.status,
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        out.contains("subset_seccomp stage ok"),
        "stage did not run: {out}"
    );
}
