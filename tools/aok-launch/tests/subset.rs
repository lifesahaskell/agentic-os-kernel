//! Subset and confinement tests (LNCH-03). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test subset -- --test-threads=1`
//!
//! Every agent here runs under the real Landlock layer. A sub launch is made
//! by running the launcher inside an agent; it execs inside the parent's
//! domain, so the kernel ANDs the parent's layers with the new one.

mod common;

use std::fs;

use common::*;

fn exe() -> &'static str {
    env!("CARGO_BIN_EXE_aok-launch")
}

#[test]
fn nnp() {
    let a = Agent::new("nnp");
    let o = launch(&["run", "--name", &a.name, "--", "cat", "/proc/self/status"]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(
        o.stdout.contains("NoNewPrivs:\t1\n"),
        "NoNewPrivs not set in the agent:\n{}",
        o.stdout
    );
    // The test process itself is not confined: the layers are the agent's.
    let own = fs::read_to_string("/proc/self/status").unwrap();
    assert!(
        own.contains("NoNewPrivs:\t0\n"),
        "test process was confined"
    );
}

#[test]
fn subset_landlock() {
    let outer = Agent::new("sub");
    let s = Scratch::new("subset");
    let a = s.path().join("A");
    let b = s.path().join("B");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    fs::write(a.join("f"), "content-of-a\n").unwrap();
    fs::write(b.join("f"), "content-of-b\n").unwrap();
    let (a_s, b_s) = (a.to_str().unwrap(), b.to_str().unwrap());
    let (rx_a, rx_b) = (format!("rx:{a_s}"), format!("rx:{b_s}"));
    let script = format!("cat {a_s}/f; echo arc=$?; cat {b_s}/f; echo brc=$?");

    // The inner launch asks for A and B; the outer holds only A. The inner
    // agent's authority is the AND of the two layers.
    let o = launch(&[
        "run",
        "--name",
        &outer.name,
        "--grant",
        &rx_a,
        "--",
        exe(),
        "run",
        "--name",
        "inner",
        "--grant",
        &rx_a,
        "--grant",
        &rx_b,
        "--",
        "sh",
        "-c",
        &script,
    ]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    assert!(o.stderr.contains("role=sub"), "no sub launch: {}", o.stderr);
    assert!(
        o.stdout.contains("content-of-a"),
        "A unreadable: {}",
        o.stdout
    );
    assert!(o.stdout.contains("arc=0"), "stdout: {}", o.stdout);
    assert!(
        !o.stdout.contains("content-of-b"),
        "the sub-agent read B, which its parent lacks: {}",
        o.stdout
    );
    assert!(o.stdout.contains("brc=1"), "stdout: {}", o.stdout);
    assert!(
        o.stderr.contains("Permission denied"),
        "B denial was not EACCES: {}",
        o.stderr
    );
    // The control: the test itself can read B.
    assert_eq!(fs::read_to_string(b.join("f")).unwrap(), "content-of-b\n");
}

#[test]
fn zero_grant_denies() {
    let a = Agent::new("zero");
    let s = Scratch::new("zero");
    let f = s.path().join("secret");
    fs::write(&f, "secret\n").unwrap();
    let f_s = f.to_str().unwrap();

    // The control: the test reads the file.
    assert_eq!(fs::read_to_string(&f).unwrap(), "secret\n");

    // No --grant: only the default and implicit grants apply, and /tmp is
    // in neither.
    let o = launch(&[
        "run",
        "--name",
        &a.name,
        "--",
        "sh",
        "-c",
        &format!("cat {f_s}"),
    ]);
    assert_ne!(o.code, 0, "an ungranted /tmp read succeeded: {}", o.stdout);
    assert!(
        o.stderr.contains("Permission denied"),
        "expected EACCES: {}",
        o.stderr
    );
    assert!(!o.stdout.contains("secret"), "leaked: {}", o.stdout);
}

/// Five steps, each printing `name ret=R errno=E`. Run inside an agent, none of
/// them may remove or widen what the agent inherited.
const WIDEN_PROBE: &str = r#"
import ctypes, os, struct, sys
libc = ctypes.CDLL(None, use_errno=True)
libc.syscall.restype = ctypes.c_long
libc.prctl.restype = ctypes.c_int

def show(name, ret):
    print("%s ret=%d errno=%d" % (name, ret, ctypes.get_errno()))

def sc(n, *a):
    ctypes.set_errno(0)
    return libc.syscall(ctypes.c_long(n), *[ctypes.c_long(x) for x in a])

def prctl(*a):
    ctypes.set_errno(0)
    return libc.prctl(*[ctypes.c_ulong(x) for x in a])

def try_open(path):
    ctypes.set_errno(0)
    try:
        fd = os.open(path, os.O_RDONLY)
    except OSError as e:
        return -1, e.errno
    os.close(fd)
    return 0, 0

def show_open(name, path):
    r, e = try_open(path)
    print("%s ret=%d errno=%d" % (name, r, e))

file_a, file_b = sys.argv[1], sys.argv[2]

# (0) set NoNewPrivs so the later unprivileged installs are allowed.
show("s0_nnp_set", prctl(38, 1, 0, 0, 0))
# (1) clearing NoNewPrivs.
show("s1_nnp_clear", prctl(38, 0, 0, 0, 0))
# (2) seccomp mode disabled.
show("s2_seccomp_off", prctl(22, 0, 0, 0, 0))

# (3) stack an allow-all filter, then a denied syscall.
class Fprog(ctypes.Structure):
    _fields_ = [("len", ctypes.c_ushort), ("filter", ctypes.c_void_p)]
insn = ctypes.create_string_buffer(struct.pack("HBBI", 0x06, 0, 0, 0x7fff0000), 8)
prog = Fprog(1, ctypes.addressof(insn))
show("s3_allow_all_install", sc(317, 1, 0, ctypes.addressof(prog)))
params = ctypes.create_string_buffer(120)
r = sc(425, 1, ctypes.addressof(params))
show("s3_io_uring", r)
if r >= 0:
    os.close(r)

# (4) a new Landlock ruleset granting every ABI-1 fs right beneath /.
ALL = 0x1FFF
attr = ctypes.c_uint64(ALL)
rs = sc(444, ctypes.addressof(attr), 8, 0)
show("s4_create", rs)
class Rule(ctypes.Structure):
    _pack_ = 1
    _fields_ = [("allowed_access", ctypes.c_uint64), ("parent_fd", ctypes.c_int32)]
root = os.open("/", os.O_PATH | os.O_CLOEXEC)
rule = Rule(ALL, root)
show("s4_add_rule", sc(445, rs, 1, ctypes.addressof(rule), 0))
show("s4_restrict", sc(446, rs, 0))
show_open("s4_open_b", file_b)
show_open("s4_open_a", file_a)
"#;

#[test]
fn cannot_remove_or_widen() {
    let a = Agent::new("widenp");
    let s = Scratch::new("widenp");
    let da = s.path().join("A");
    let db = s.path().join("B");
    let p = s.path().join("P");
    for d in [&da, &db, &p] {
        fs::create_dir(d).unwrap();
    }
    fs::write(da.join("f"), "content-of-a\n").unwrap();
    fs::write(db.join("f"), "content-of-b\n").unwrap();
    let probe = p.join("probe.py");
    fs::write(&probe, WIDEN_PROBE).unwrap();
    let (fa, fb) = (da.join("f"), db.join("f"));
    let argv = [
        "python3",
        "-I",
        "-B",
        probe.to_str().unwrap(),
        fa.to_str().unwrap(),
        fb.to_str().unwrap(),
    ];

    // The control: outside any agent every install succeeds, and since the
    // probe's own filter and ruleset allow everything, io_uring_setup returns
    // an fd and B opens. The inside denials are therefore the inherited
    // layers, not the probe.
    let c = std::process::Command::new(argv[0])
        .args(&argv[1..])
        .output()
        .expect("control probe");
    let cout = String::from_utf8_lossy(&c.stdout);
    assert!(
        c.status.success(),
        "control: {cout}{}",
        String::from_utf8_lossy(&c.stderr)
    );
    let ctl = parse_probe(&cout);
    for k in [
        "s0_nnp_set",
        "s3_allow_all_install",
        "s4_add_rule",
        "s4_restrict",
    ] {
        assert_eq!(probe_line(&ctl, k).ret, 0, "control {k}: {cout}");
    }
    assert!(probe_line(&ctl, "s3_io_uring").ret >= 0, "control: {cout}");
    for k in ["s1_nnp_clear", "s2_seccomp_off"] {
        let l = probe_line(&ctl, k);
        assert_eq!((l.ret, l.errno), (-1, libc::EINVAL), "control {k}: {cout}");
    }
    assert!(probe_line(&ctl, "s4_create").ret >= 0, "control: {cout}");
    assert_eq!(probe_line(&ctl, "s4_open_b").ret, 0, "control: {cout}");
    assert_eq!(probe_line(&ctl, "s4_open_a").ret, 0, "control: {cout}");

    // The same probe inside an agent granted A and P but not B.
    let (rx_a, rx_p) = (
        format!("rx:{}", da.display()),
        format!("rx:{}", p.display()),
    );
    let mut args = vec![
        "run",
        "--name",
        a.name.as_str(),
        "--grant",
        &rx_a,
        "--grant",
        &rx_p,
        "--",
    ];
    args.extend_from_slice(&argv);
    let o = launch(&args);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = parse_probe(&o.stdout);
    let is = |k: &str, ret: i64, errno: i32| {
        let l = probe_line(&m, k);
        assert_eq!((l.ret, l.errno), (ret, errno), "{k}: {}", o.stdout);
    };
    is("s0_nnp_set", 0, 0);
    is("s1_nnp_clear", -1, libc::EINVAL);
    is("s2_seccomp_off", -1, libc::EINVAL);
    // Stacking an allow-all filter succeeds but widens nothing.
    is("s3_allow_all_install", 0, 0);
    is("s3_io_uring", -1, libc::EPERM);
    // A new Landlock layer is created, filled and self-applied, and still
    // cannot reach B; A stays readable.
    assert!(probe_line(&m, "s4_create").ret >= 0, "{}", o.stdout);
    is("s4_add_rule", 0, 0);
    is("s4_restrict", 0, 0);
    is("s4_open_b", -1, libc::EACCES);
    is("s4_open_a", 0, 0);
}

#[test]
fn depth_cap() {
    let a = Agent::new("depth0");
    let o = launch(&["nest", "--name", &a.name, "--depth", "0", "--", "true"]);
    assert_eq!(o.code, 64, "depth 0 is a usage error; stderr: {}", o.stderr);

    // Each level applies one Landlock ruleset, so depth 16 is exactly the
    // kernel's stacking limit and the deepest child still runs.
    let a = Agent::new("depth16");
    let o = launch(&[
        "nest",
        "--name",
        &a.name,
        "--depth",
        "16",
        "--",
        "sh",
        "-c",
        "echo deep-ok",
    ]);
    assert_eq!(o.code, 0, "depth 16 stderr: {}", o.stderr);
    assert!(o.stdout.contains("deep-ok"), "stdout: {}", o.stdout);

    // The 17th stacked ruleset fails with the kernel's E2BIG: refusal (125),
    // errno named.
    let a = Agent::new("depth17");
    let o = launch(&[
        "nest",
        "--name",
        &a.name,
        "--depth",
        "17",
        "--",
        "sh",
        "-c",
        "echo deep-ok",
    ]);
    assert_eq!(o.code, 125, "depth 17 stderr: {}", o.stderr);
    assert!(o.stderr.contains("E2BIG"), "stderr: {}", o.stderr);
    assert!(!o.stdout.contains("deep-ok"), "stdout: {}", o.stdout);
}
