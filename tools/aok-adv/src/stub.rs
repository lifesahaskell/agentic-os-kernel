//! The stub broker (D-15): answers the kernel-stamped identity protocol so
//! the adversarial suite has a target before the real Phase 10 broker
//! exists (D-14). Framing is a 4-byte little-endian length prefix with a
//! payload of at most `MAX_PAYLOAD` bytes (V5 bounds checks). Request
//! `HELLO <claimed_id>`; reply `OK cgroupid=<u64> pid=<i32>`. The claimed id
//! is read but never trusted: the reply's cgroupid always comes from
//! `sys::peer_identity`, read fresh for this request (route iii's control).
//! This module is safe Rust: every syscall it needs lives in `sys` (D-13).

use crate::sys;
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};

pub const MAX_PAYLOAD: usize = 4096;

/// Read one length-prefixed frame: 4 bytes little-endian length, then that
/// many bytes of payload. Rejects any length over `MAX_PAYLOAD` before
/// allocating or reading the payload (V5).
fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame length {len} exceeds MAX_PAYLOAD {MAX_PAYLOAD}"),
        ));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;
    Ok(payload)
}

fn write_frame(stream: &mut UnixStream, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "reply length {} exceeds MAX_PAYLOAD {MAX_PAYLOAD}",
                payload.len()
            ),
        ));
    }
    let len = (payload.len() as u32).to_le_bytes();
    stream.write_all(&len)?;
    stream.write_all(payload)
}

/// Pure request -> reply logic, split out from the socket I/O so the
/// protocol can be unit-tested without depending on frame encoding.
fn handle_request(peer_fd: std::os::unix::io::RawFd, request: &str) -> String {
    let request = request.trim_end_matches(['\n', '\r', '\0']);
    let mut parts = request.splitn(2, ' ');
    match parts.next() {
        Some("HELLO") => {
            // The claimed id (parts.next()) is read and discarded: route
            // (iii) is a control proving the server never trusts it.
            let _claimed_id = parts.next();
            match sys::peer_identity(peer_fd) {
                Ok(id) => format!("OK cgroupid={} pid={}", id.cgroupid, id.pid),
                Err(e) => format!("ERR identity_failed {e}"),
            }
        }
        _ => "ERR unknown_verb".to_string(),
    }
}

/// Handle exactly one request on an already-accepted connection.
pub fn handle_one(stream: &mut UnixStream) -> io::Result<()> {
    let payload = read_frame(stream)?;
    let request = String::from_utf8_lossy(&payload);
    let reply = handle_request(stream.as_raw_fd(), &request);
    write_frame(stream, reply.as_bytes())
}

/// Run the stub as a long-lived server: bind `socket_path` (removing a
/// stale pathname socket first) or an abstract-namespace name, then accept
/// and serve requests one at a time until the process is killed.
pub fn serve(socket_path: &str, abstract_name: Option<&str>) -> io::Result<()> {
    let listener = if let Some(name) = abstract_name {
        use std::os::linux::net::SocketAddrExt;
        use std::os::unix::net::SocketAddr;
        let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
        UnixListener::bind_addr(&addr)?
    } else {
        let _ = std::fs::remove_file(socket_path);
        UnixListener::bind(socket_path)?
    };

    for stream in listener.incoming() {
        let mut stream = stream?;
        if let Err(e) = handle_one(&mut stream) {
            eprintln!("aok-adv stub: request error: {e}");
        }
    }
    Ok(())
}

/// CLI entry point for `aok-adv stub --socket PATH [--abstract NAME]`.
pub fn run_cli(args: &[String]) -> io::Result<()> {
    let mut socket_path: Option<String> = None;
    let mut abstract_name: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                socket_path = args.get(i + 1).cloned();
                i += 2;
            }
            "--abstract" => {
                abstract_name = args.get(i + 1).cloned();
                i += 2;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("stub: unknown argument '{other}'"),
                ));
            }
        }
    }
    let socket_path = socket_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "stub: --socket PATH is required",
        )
    })?;
    serve(&socket_path, abstract_name.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_gets_ok_reply_with_kernel_identity() {
        let (mut a, mut b) = UnixStream::pair().expect("socketpair");
        let server = std::thread::spawn(move || {
            handle_one(&mut b).expect("handle one request");
        });

        write_frame(&mut a, b"HELLO attacker-claims-to-be-root").expect("write request");
        let reply_bytes = read_frame(&mut a).expect("read reply");
        server.join().unwrap();

        let reply = String::from_utf8(reply_bytes).expect("reply is utf8");
        assert!(
            reply.starts_with("OK cgroupid="),
            "unexpected reply: {reply}"
        );
        assert!(reply.contains(" pid="), "reply missing pid field: {reply}");
    }

    #[test]
    fn unknown_verb_is_rejected() {
        let (mut a, mut b) = UnixStream::pair().expect("socketpair");
        let server = std::thread::spawn(move || {
            handle_one(&mut b).expect("handle one request");
        });

        write_frame(&mut a, b"GOODBYE now").expect("write request");
        let reply_bytes = read_frame(&mut a).expect("read reply");
        server.join().unwrap();

        let reply = String::from_utf8(reply_bytes).expect("reply is utf8");
        assert_eq!(reply, "ERR unknown_verb");
    }

    #[test]
    fn claimed_id_in_payload_is_ignored() {
        // Route (iii) control: the claimed id never changes the reply.
        let (mut a, mut b) = UnixStream::pair().expect("socketpair");
        let server = std::thread::spawn(move || {
            handle_one(&mut b).expect("handle one request");
        });
        write_frame(&mut a, b"HELLO 1").expect("write request");
        let reply_a = String::from_utf8(read_frame(&mut a).expect("read reply")).unwrap();
        server.join().unwrap();

        let (mut c, mut d) = UnixStream::pair().expect("socketpair");
        let server2 = std::thread::spawn(move || {
            handle_one(&mut d).expect("handle one request");
        });
        write_frame(&mut c, b"HELLO 999999").expect("write request");
        let reply_c = String::from_utf8(read_frame(&mut c).expect("read reply")).unwrap();
        server2.join().unwrap();

        // Both requests come from this same test process, so the kernel-read
        // identity (and hence the reply) is identical regardless of the
        // wildly different claimed id in the payload.
        assert_eq!(reply_a, reply_c);
    }

    #[test]
    fn oversized_length_prefix_is_rejected_before_reading() {
        let (mut a, mut b) = UnixStream::pair().expect("socketpair");
        let bogus_len = (MAX_PAYLOAD as u32 + 1).to_le_bytes();
        a.write_all(&bogus_len)
            .expect("write oversized length prefix");
        let err = read_frame(&mut b).expect_err("oversized frame must be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
