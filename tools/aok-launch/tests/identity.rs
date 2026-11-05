//! Identity tests (LNCH-02). Run inside a delegated scope:
//! `systemd-run --user --scope -q -p Delegate=yes -- cargo test --test identity -- --test-threads=1`

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use common::*;

/// Parse `key=value` lines from `whois`.
fn kv(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn whois(args: &[&str]) -> Out {
    let mut a = vec!["whois"];
    a.extend_from_slice(args);
    launch(&a)
}

/// Start a long-lived root agent and wait for its first process.
struct Live {
    child: std::process::Child,
    pid: i32,
}

fn start_root(a: &Agent, extra: &[&str]) -> Live {
    let mut args = vec!["run", "--name", &a.name];
    args.extend_from_slice(extra);
    args.extend_from_slice(&["--", "sleep", "60"]);
    let child = launch_bg(&args);
    let pid = wait_for_pid(&a.leaf(), 5);
    Live { child, pid }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn identity_from_pidfd() {
    let a = Agent::new("ident");
    let live = start_root(&a, &["--on-behalf-of", "alice@example.org"]);

    let o = whois(&["--pid", &live.pid.to_string()]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = kv(&o.stdout);
    assert_eq!(m["agent_id"], a.name, "{}", o.stdout);
    assert_eq!(m["on_behalf_of"], "alice@example.org");
    assert_eq!(m["parent"], "none");
    assert_eq!(m["path"], "pidfd");
    // The key is the cgroup id the kernel stamped: the main leaf's inode.
    assert_eq!(
        m["cgroup_id"],
        fs::metadata(a.leaf()).unwrap().ino().to_string()
    );

    // Usage errors are 64 and name the problem.
    assert_eq!(whois(&[]).code, 64);
    assert_eq!(whois(&["--pid", "0"]).code, 64);
    assert_eq!(whois(&["--bogus", "1"]).code, 64);

    // A pid that is not in any registered agent is refused, not guessed.
    let o = whois(&["--pid", "1"]);
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
}

/// RESEARCH item 9, recorded as a note: moving a task into a sibling leaf of
/// the agent's own subtree re-attributes its agent_id. That is an ADV
/// candidate (an agent can re-label itself within its own subtree), not a
/// defect in resolution: the id follows the cgroup, which is the point.
#[test]
fn moving_a_task_within_the_subtree_reattributes_it() {
    let a = Agent::new("move");
    let live = start_root(&a, &[]);

    let sibling = a.dir.join("elsewhere");
    fs::create_dir(&sibling).expect("mkdir sibling leaf");
    fs::write(sibling.join("cgroup.procs"), live.pid.to_string()).expect("move task");

    let o = whois(&["--pid", &live.pid.to_string()]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = kv(&o.stdout);
    assert_eq!(
        m["agent_id"],
        format!("{}/elsewhere", a.name),
        "{}",
        o.stdout
    );
    assert_eq!(
        m["cgroup_id"],
        fs::metadata(&sibling).unwrap().ino().to_string()
    );

    // Put it back so teardown is plain.
    fs::write(a.leaf().join("cgroup.procs"), live.pid.to_string()).expect("move back");
    let o = whois(&["--pid", &live.pid.to_string()]);
    assert_eq!(kv(&o.stdout)["agent_id"], a.name);
}

/// RESEARCH item 8: renaming a cgroup2 directory is EPERM, so an agent_id,
/// which is a path, cannot change by rename. This confirms D-06.
#[test]
fn cgroup_directory_rename_is_eperm() {
    let a = Agent::new("rename");
    let _live = start_root(&a, &[]);
    let to = a.dir.with_file_name(format!("{}-renamed", a.name));
    let e = fs::rename(&a.dir, &to).expect_err("cgroup2 rename must fail");
    assert_eq!(e.raw_os_error(), Some(libc::EPERM), "{e}");
    assert!(a.dir.exists() && !to.exists());
}

// ---------------------------------------------------------------------------
// Task 2: the fallback, the registry, the listening mode.
// ---------------------------------------------------------------------------

/// Run the launcher through the D-13 shim so `PIDFD_GET_INFO` returns ENOTTY.
/// This is simulated: the ioctl returns ENOTTY on a kernel that has the
/// feature.
fn shim_command(fakes: &[&str], launcher_args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_aok-shim"));
    for f in fakes {
        c.arg("--fake").arg(f);
    }
    c.arg("--").arg(env!("CARGO_BIN_EXE_aok-launch"));
    c.args(launcher_args);
    c
}

fn collect(o: std::process::Output) -> Out {
    Out {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn shimmed(fakes: &[&str], launcher_args: &[&str]) -> Out {
    collect(
        shim_command(fakes, launcher_args)
            .output()
            .expect("spawn aok-shim"),
    )
}

/// The real uid without `unsafe`: the owner of `/proc/self`.
fn real_uid() -> u32 {
    fs::metadata("/proc/self").unwrap().uid()
}

/// The registry file for this boot, built from the real uid.
fn registry_file() -> PathBuf {
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap();
    PathBuf::from(format!(
        "/run/user/{}/aok-launch/registry.{}",
        real_uid(),
        boot.trim()
    ))
}

fn registry_text() -> String {
    fs::read_to_string(registry_file()).unwrap_or_default()
}

fn write_registry(text: &str) {
    let path = registry_file();
    let tmp = path.with_extension("tmp-test");
    fs::write(&tmp, text).unwrap();
    fs::rename(&tmp, &path).unwrap();
}

/// Lines a test appended to the registry. Dropping removes exactly those
/// lines and nothing else.
struct Added(Vec<String>);

fn append_registry(lines: &[&str]) -> Added {
    let mut text = registry_text();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for l in lines {
        text.push_str(l);
        text.push('\n');
    }
    write_registry(&text);
    Added(lines.iter().map(|l| l.to_string()).collect())
}

impl Drop for Added {
    fn drop(&mut self) {
        let mut keep: Vec<String> = registry_text().lines().map(str::to_string).collect();
        for add in &self.0 {
            if let Some(i) = keep.iter().position(|l| l == add) {
                keep.remove(i);
            }
        }
        let mut out = keep.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        write_registry(&out);
    }
}

fn leaf_id(dir: &Path) -> String {
    fs::metadata(dir).unwrap().ino().to_string()
}

fn wait_until(secs: u64, mut f: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        sleep(Duration::from_millis(10));
    }
    false
}

/// Both routes must resolve the same key (assumption-delta no-change): same
/// cgroup_id, agent_id, parent and on_behalf_of. The fallback half is
/// simulated: the ioctl returns ENOTTY on a kernel that has the feature.
fn assert_same_key(pid: i32, want_agent: &str, want_parent: &str, want_obo: &str, want_id: &str) {
    let p = pid.to_string();
    let primary = whois(&["--pid", &p]);
    assert_eq!(primary.code, 0, "stderr: {}", primary.stderr);
    let fallback = shimmed(&["ioctl-enotty"], &["whois", "--pid", &p]);
    assert_eq!(fallback.code, 0, "stderr: {}", fallback.stderr);

    let (a, b) = (kv(&primary.stdout), kv(&fallback.stdout));
    assert_eq!(a["path"], "pidfd", "{}", primary.stdout);
    assert_eq!(b["path"], "fallback", "{}", fallback.stdout);
    // The window is the race the fallback cannot close; it is reported, not
    // hidden, and it is a real elapsed time.
    let window: u128 = b["window_ns"].parse().expect("window_ns is a number");
    assert!(window > 0 && window < 5_000_000_000, "window_ns={window}");
    assert!(!a.contains_key("window_ns"));

    for k in ["agent_id", "parent", "on_behalf_of", "cgroup_id"] {
        assert_eq!(a[k], b[k], "{k} differs between routes");
    }
    assert_eq!(a["agent_id"], want_agent);
    assert_eq!(a["parent"], want_parent);
    assert_eq!(a["on_behalf_of"], want_obo);
    assert_eq!(a["cgroup_id"], want_id);
}

#[test]
fn fallback_equivalence() {
    let a = Agent::new("equiv");
    let live = start_root(&a, &["--on-behalf-of", "alice@example.org"]);
    assert_same_key(
        live.pid,
        &a.name,
        "none",
        "alice@example.org",
        &leaf_id(&a.leaf()),
    );
}

#[test]
fn fallback_equivalence_for_a_sub_agent() {
    // The root agent's command is a launcher, so the second launch runs inside
    // the root agent's main leaf and is a sub launch: the principal comes from
    // the root, the parent is the launching agent.
    let a = Agent::new("equivsub");
    let launcher = env!("CARGO_BIN_EXE_aok-launch");
    let _outer = Live {
        child: launch_bg(&[
            "run",
            "--name",
            &a.name,
            "--on-behalf-of",
            "alice@example.org",
            "--",
            launcher,
            "run",
            "--name",
            "s1",
            "--",
            "sleep",
            "60",
        ]),
        pid: 0,
    };
    let sub_leaf = a.dir.join("s1").join("main");
    let pid = wait_for_pid(&sub_leaf, 5);
    assert_same_key(
        pid,
        &format!("{}/s1", a.name),
        &a.name,
        "alice@example.org",
        &leaf_id(&sub_leaf),
    );
}

#[test]
fn fallback_refuses_a_task_outside_every_agent() {
    // Simulated ENOTTY, then the pid route: pid 1 is in no registered agent,
    // so the fallback refuses rather than inventing an identity.
    let o = shimmed(&["ioctl-enotty"], &["whois", "--pid", "1"]);
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
    assert!(
        o.stderr.contains("not under a registered root"),
        "{}",
        o.stderr
    );
}

#[test]
fn sub_launch_cannot_set_on_behalf_of() {
    let a = Agent::new("subobo");
    let launcher = env!("CARGO_BIN_EXE_aok-launch");
    let o = launch(&[
        "run",
        "--name",
        &a.name,
        "--on-behalf-of",
        "alice@example.org",
        "--",
        launcher,
        "run",
        "--name",
        "s1",
        "--on-behalf-of",
        "mallory",
        "--",
        "true",
    ]);
    // The inner launcher's exit status is the agent's, passed through.
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
    assert!(o.stderr.contains("EACCES"), "stderr: {}", o.stderr);
    assert!(!a.dir.join("s1").exists(), "refused before any mkdir");
}

/// Run `cmd` from a shell that first moves itself into `cgroup_dir`, so the
/// launcher sees an own cgroup that is not a `main` leaf.
fn launch_from(cgroup_dir: &Path, cmd: &[&str]) -> Out {
    let procs = cgroup_dir.join("cgroup.procs");
    let o = Command::new("sh")
        .args(["-c", "echo $$ > \"$1\" && shift && exec \"$@\""])
        .arg("sh")
        .arg(&procs)
        .args(cmd)
        .output()
        .expect("spawn sh");
    collect(o)
}

#[test]
fn registry_root_from_inside_agent_is_eacces_a() {
    // (a) A process inside a registered agent's tree but outside a main leaf
    // is a Root launch from under a registered root: refused EACCES, and
    // nothing is registered or created.
    let a = Agent::new("regtree");
    let live = start_root(&a, &["--on-behalf-of", "alice@example.org"]);
    let esc = a.dir.join("esc");
    fs::create_dir(&esc).unwrap();
    let before = registry_text();

    let other = format!("{}-other", a.name);
    let o = launch_from(
        &esc,
        &[
            env!("CARGO_BIN_EXE_aok-launch"),
            "run",
            "--name",
            &other,
            "--on-behalf-of",
            "mallory",
            "--",
            "true",
        ],
    );
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
    assert!(o.stderr.contains("EACCES"), "stderr: {}", o.stderr);
    assert!(
        o.stderr.contains("registered agent tree"),
        "stderr: {}",
        o.stderr
    );
    assert!(!scope_root().join(&other).exists());
    assert!(!a.dir.join(&other).exists());
    assert_eq!(registry_text(), before, "the refused launch wrote nothing");

    // The legitimate principal is untouched.
    let m = kv(&whois(&["--pid", &live.pid.to_string()]).stdout);
    assert_eq!(m["on_behalf_of"], "alice@example.org");
}

/// Applies a Landlock ruleset that handles every ABI-1 filesystem right and
/// grants read and execute on the listed directories only, then execs the
/// rest of argv. Anything not listed, `/run` included, is denied with EACCES.
const LANDLOCK_EXEC: &str = r#"
import ctypes, os, struct, sys
libc = ctypes.CDLL(None, use_errno=True)
def sc(n, *a):
    r = libc.syscall(n, *a)
    if r < 0:
        e = ctypes.get_errno()
        sys.stderr.write("landlock setup failed: %s\n" % os.strerror(e)); sys.exit(70)
    return r
HANDLED = 0x1FFF
READ_EXEC = (1 << 0) | (1 << 2) | (1 << 3)
attr = struct.pack("Q", HANDLED)
fd = sc(444, attr, len(attr), 0)
sep = sys.argv.index("--")
for d in sys.argv[1:sep]:
    if not os.path.isdir(d):
        continue
    pfd = os.open(d, os.O_PATH | os.O_DIRECTORY)
    rule = struct.pack("Qi", READ_EXEC, pfd)
    buf = ctypes.create_string_buffer(rule, len(rule))
    sc(445, fd, 1, buf, 0)
    os.close(pfd)
sc(157, 38, 1, 0, 0, 0)
sc(446, fd, 0)
os.execv(sys.argv[sep + 1], sys.argv[sep + 1:])
"#;

fn launcher_dir() -> PathBuf {
    Path::new(env!("CARGO_BIN_EXE_aok-launch"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn landlocked(cmd: &[&str]) -> Out {
    let dir = launcher_dir();
    let o = Command::new("python3")
        .args(["-I", "-B", "-c", LANDLOCK_EXEC])
        .args(["/usr", "/lib", "/lib64", "/bin", "/etc", "/proc", "/sys"])
        .arg(&dir)
        .arg("--")
        .args(cmd)
        .output()
        .expect("spawn python3");
    collect(o)
}

#[test]
fn registry_root_from_inside_agent_is_eacces_b() {
    // (b) The registry sits outside any agent's Landlock grant, so a
    // Landlocked process that reaches a Root launch gets EACCES reading it.
    // Control first: the same launch with no Landlock succeeds.
    let a = Agent::new("regll");
    let b = Agent::new("regllc");
    let launcher = env!("CARGO_BIN_EXE_aok-launch");

    let c = launch(&["run", "--name", &b.name, "--", "true"]);
    assert_eq!(c.code, 0, "control, stderr: {}", c.stderr);

    let before = registry_text();
    let o = landlocked(&[launcher, "run", "--name", &a.name, "--", "true"]);
    if o.stderr.contains("landlock setup failed") {
        panic!("Landlock unavailable on this host: {}", o.stderr);
    }
    assert_eq!(o.code, 125, "stderr: {}", o.stderr);
    assert!(o.stderr.contains("EACCES"), "stderr: {}", o.stderr);
    assert!(o.stderr.contains("registry"), "stderr: {}", o.stderr);
    assert!(!a.dir.exists(), "refused before any mkdir");
    assert_eq!(registry_text(), before);
}

#[test]
fn registry_malformed_line_skipped() {
    let a = Agent::new("regbad");
    let live = start_root(&a, &["--on-behalf-of", "alice@example.org"]);
    let id = leaf_id(&a.dir);
    let good_path = a.dir.display().to_string();

    // Malformed lines, one valid-looking duplicate for this agent's own id
    // with a spoofed principal, and one oversized line.
    let spoof = format!("{id}\t{good_path}\tmallory");
    let long = format!("1\t/sys/fs/cgroup/{}\tx", "z".repeat(4096));
    let _added = append_registry(&[
        "this is not a registry line",
        "12\t/etc/passwd\troot",
        "abc\t/sys/fs/cgroup/x\tbob",
        "7\t/sys/fs/cgroup/x\tbad principal with spaces",
        &spoof,
        &long,
    ]);

    let o = whois(&["--pid", &live.pid.to_string()]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = kv(&o.stdout);
    assert_eq!(m["agent_id"], a.name);
    assert_eq!(m["on_behalf_of"], "alice@example.org", "first entry wins");
    let warnings = o
        .stderr
        .lines()
        .filter(|l| l.contains("registry: warning:") && l.contains("skipped"))
        .count();
    assert!(warnings >= 6, "warnings={warnings}, stderr: {}", o.stderr);
}

#[test]
fn registry_is_private_and_pruned() {
    let a = Agent::new("regprune");
    let b = Agent::new("regkeep");
    let first = launch(&[
        "run",
        "--name",
        &a.name,
        "--on-behalf-of",
        "p1",
        "--",
        "true",
    ]);
    assert_eq!(first.code, 0, "stderr: {}", first.stderr);
    let a_id = leaf_id(&a.dir);
    assert!(
        registry_text().contains(&format!("{a_id}\t{}\tp1", a.dir.display())),
        "{}",
        registry_text()
    );

    // File and directory modes: the registry is not readable by anyone else.
    let mode = |p: &Path| fs::metadata(p).unwrap().mode() & 0o777;
    assert_eq!(mode(&registry_file()), 0o600);
    assert_eq!(mode(registry_file().parent().unwrap()), 0o700);

    // Tear the agent down; the next root launch prunes its dead entry.
    drop(a);
    let second = launch(&["run", "--name", &b.name, "--", "true"]);
    assert_eq!(second.code, 0, "stderr: {}", second.stderr);
    let text = registry_text();
    assert!(
        !text.contains(&format!("{a_id}\t")),
        "dead root kept: {text}"
    );
    assert!(text.contains(&format!("{}\t{}\t", leaf_id(&b.dir), b.dir.display())));
}

static SOCK: AtomicU32 = AtomicU32::new(0);

/// `whois --listen` in a launcher (optionally shimmed), a client inside the
/// agent connects, and the record comes back for the client's task.
fn listen_whois(a: &Agent, fakes: &[&str]) -> Out {
    let sock = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "whois-{}-{}.sock",
        std::process::id(),
        SOCK.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_file(&sock);
    let sock_s = sock.display().to_string();

    let args = ["whois", "--listen", sock_s.as_str()];
    let mut cmd = if fakes.is_empty() {
        let mut c = Command::new(env!("CARGO_BIN_EXE_aok-launch"));
        c.args(args);
        c
    } else {
        shim_command(fakes, &args)
    };
    let server = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn whois --listen");
    assert!(wait_until(5, || sock.exists()), "socket never appeared");

    let client = launch(&[
        "run",
        "--name",
        &a.name,
        "--on-behalf-of",
        "alice@example.org",
        "--",
        "python3",
        "-I",
        "-B",
        "-c",
        "import socket, sys, time\ns = socket.socket(socket.AF_UNIX)\ns.connect(sys.argv[1])\ntime.sleep(1)",
        &sock_s,
    ]);
    assert_eq!(client.code, 0, "client stderr: {}", client.stderr);
    let out = collect(server.wait_with_output().expect("whois --listen"));
    assert!(!sock.exists(), "the listening socket is cleaned up");
    out
}

#[test]
fn whois_listen_names_the_connecting_task() {
    let a = Agent::new("listen");
    let o = listen_whois(&a, &[]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = kv(&o.stdout);
    assert_eq!(m["agent_id"], a.name, "{}", o.stdout);
    assert_eq!(m["on_behalf_of"], "alice@example.org");
    assert_eq!(m["path"], "pidfd");
    assert_eq!(m["cgroup_id"], leaf_id(&a.leaf()));
}

#[test]
fn whois_listen_falls_back_to_so_peercred_on_enotty() {
    // Simulated: the ioctl returns ENOTTY on a kernel that has the feature.
    let a = Agent::new("listenfb");
    let o = listen_whois(&a, &["ioctl-enotty"]);
    assert_eq!(o.code, 0, "stderr: {}", o.stderr);
    let m = kv(&o.stdout);
    assert_eq!(m["agent_id"], a.name, "{}", o.stdout);
    assert_eq!(m["path"], "fallback");
    assert!(m["window_ns"].parse::<u128>().is_ok());
    assert_eq!(m["cgroup_id"], leaf_id(&a.leaf()));
}
