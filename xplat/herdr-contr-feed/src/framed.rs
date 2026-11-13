//! A framed hc connection that observes one terminal: `hello` with `framed`, then `terminal.attach` with `observe`.
//! It takes the first paint and detaches. It sends no input, no size and no focus: only the JSON-RPC calls on
//! channel 0 and the CLOSE frame that detaches.

// Written from herdr-core's wire layout; see NOTICE.

use serde_json::{json, Value};
use std::io::{self, Read};
use std::time::{Duration, Instant};

/// Length of a frame header: channel (u32), kind (u8), payload length (u32), little endian.
pub const HEADER_LEN: usize = 9;
/// The longest payload that is accepted.
pub const MAX_PAYLOAD: usize = 16 << 20;
/// The protocol generation of the `hello` call.
pub const GENERATION: u64 = 1;

pub const KIND_DATA: u8 = 0;
pub const KIND_CLOSE: u8 = 2;
pub const KIND_RESIZE: u8 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub channel: u32,
    pub kind: u8,
    pub payload: Vec<u8>,
}

pub fn encode(channel: u32, kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&channel.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// One frame from the front of `buf`: the frame and the bytes it took, `Ok(None)` when it is not complete yet, an
/// error when its length is more than `MAX_PAYLOAD`.
pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, String> {
    if buf.len() < HEADER_LEN {
        return Ok(None);
    }
    let n = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;
    if n > MAX_PAYLOAD {
        return Err(format!("frame too long: {n} bytes"));
    }
    if buf.len() < HEADER_LEN + n {
        return Ok(None);
    }
    let channel = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    Ok(Some((Frame { channel, kind: buf[4], payload: buf[HEADER_LEN..HEADER_LEN + n].to_vec() }, HEADER_LEN + n)))
}

/// What an observer saw: the terminal's bytes in order, with the sizes that the core sent between them.
#[derive(Debug, Default, PartialEq)]
pub struct Paint {
    pub events: Vec<PaintEvent>,
}

#[derive(Debug, PartialEq)]
pub enum PaintEvent {
    Data(Vec<u8>),
    Resize(u16, u16),
}

/// How long the observer waits.
#[derive(Debug, Clone, Copy)]
pub struct Waits {
    /// For the hello and the attach replies.
    pub reply: Duration,
    /// For the first paint, from the attach reply.
    pub paint: Duration,
    /// After paint bytes came, a pause this long ends the read early.
    pub quiet: Duration,
}

pub const WAITS: Waits = Waits { reply: Duration::from_secs(2), paint: Duration::from_millis(300), quiet: Duration::from_millis(60) };

/// Observes `terminal` through the core's socket at `socket` and returns its first paint.
#[cfg(unix)]
pub fn first_paint(socket: &str, terminal: &str, cols: u16, rows: u16, waits: Waits) -> Result<Paint, String> {
    use std::os::unix::net::UnixStream;
    let s = UnixStream::connect(socket).map_err(|e| format!("connect {socket}: {e}"))?;
    let mut conn = Conn { s, buf: Vec::new() };
    let out = conn.observe(terminal, cols, rows, waits);
    let _ = conn.s.shutdown(std::net::Shutdown::Both);
    out
}

#[cfg(not(unix))]
pub fn first_paint(_socket: &str, _terminal: &str, _cols: u16, _rows: u16, _waits: Waits) -> Result<Paint, String> {
    Err("herdr-core sockets are not supported on this OS yet".into())
}

#[cfg(unix)]
struct Conn {
    s: std::os::unix::net::UnixStream,
    buf: Vec<u8>,
}

#[cfg(unix)]
impl Conn {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write;
        self.s.write_all(bytes).map_err(|e| format!("write: {e}"))
    }

    /// Reads more bytes until `deadline`. `Ok(false)`: the time is over. An end of input is an error.
    fn fill(&mut self, deadline: Instant) -> Result<bool, String> {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(false);
        }
        self.s.set_read_timeout(Some(left)).map_err(|e| format!("set timeout: {e}"))?;
        let mut chunk = [0u8; 65536];
        match self.s.read(&mut chunk) {
            Ok(0) => Err("the core closed the connection".into()),
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                Ok(true)
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => Ok(!Instant::now().ge(&deadline)),
            Err(e) => Err(format!("read: {e}")),
        }
    }

    fn frame(&mut self, deadline: Instant) -> Result<Option<Frame>, String> {
        loop {
            if let Some((f, n)) = decode(&self.buf)? {
                self.buf.drain(..n);
                return Ok(Some(f));
            }
            if !self.fill(deadline)? {
                return Ok(None);
            }
        }
    }

    fn observe(&mut self, terminal: &str, cols: u16, rows: u16, waits: Waits) -> Result<Paint, String> {
        // hello is one JSON line; the frames start after its reply line.
        let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello", "params": {"generation": GENERATION, "framed": true}});
        self.write(format!("{hello}\n").as_bytes())?;
        let deadline = Instant::now() + waits.reply;
        let line = loop {
            if let Some(nl) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=nl).collect();
                break line;
            }
            if !self.fill(deadline)? {
                return Err("no hello reply".into());
            }
        };
        let reply: Value = serde_json::from_slice(&line).map_err(|e| format!("bad hello reply: {e}"))?;
        let r = result(&reply)?;
        if r.get("generation").and_then(Value::as_u64) != Some(GENERATION) {
            return Err("incompatible: another protocol generation".into());
        }
        if r.get("framed").and_then(Value::as_bool) != Some(true) {
            return Err("incompatible: the core did not switch to framed mode".into());
        }
        if !r.get("features").and_then(Value::as_array).is_some_and(|f| f.iter().any(|x| x == "attach")) {
            return Err("incompatible: the core has no attach feature".into());
        }

        let attach = json!({"jsonrpc": "2.0", "id": 1, "method": "terminal.attach", "params": {
            "terminal": terminal, "cols": cols, "rows": rows, "observe": true, "resize_notifications": true}});
        self.write(&encode(0, KIND_DATA, attach.to_string().as_bytes()))?;
        let deadline = Instant::now() + waits.reply;
        let channel = loop {
            let Some(f) = self.frame(deadline)? else { return Err("no attach reply".into()) };
            if f.channel != 0 || f.kind != KIND_DATA {
                continue;
            }
            let m: Value = serde_json::from_slice(&f.payload).map_err(|e| format!("bad message: {e}"))?;
            if m.get("id").and_then(Value::as_u64) != Some(1) {
                continue;
            }
            let ch = result(&m)?.get("channel").and_then(Value::as_u64).unwrap_or(0) as u32;
            if ch == 0 {
                return Err("attach reply has no channel".into());
            }
            break ch;
        };

        let mut paint = Paint::default();
        let end = Instant::now() + waits.paint;
        loop {
            let quiet = if paint.events.is_empty() { end } else { end.min(Instant::now() + waits.quiet) };
            let Some(f) = self.frame(quiet)? else { break };
            if f.channel != channel {
                continue;
            }
            match f.kind {
                KIND_DATA => paint.events.push(PaintEvent::Data(f.payload)),
                KIND_RESIZE if f.payload.len() == 4 => {
                    let p = &f.payload;
                    paint.events.push(PaintEvent::Resize(u16::from_le_bytes([p[0], p[1]]), u16::from_le_bytes([p[2], p[3]])));
                }
                KIND_CLOSE => return Ok(paint),
                _ => {}
            }
        }
        // Detach: the terminal keeps running.
        let _ = self.write(&encode(channel, KIND_CLOSE, &[]));
        Ok(paint)
    }
}

fn result(m: &Value) -> Result<&Value, String> {
    if let Some(e) = m.get("error") {
        return Err(e.get("message").and_then(Value::as_str).unwrap_or("hc error").to_string());
    }
    m.get("result").ok_or_else(|| "reply has no result".to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    #[test]
    fn codec_round_trip_and_partial_frames() {
        let bytes = encode(7, KIND_DATA, b"hi");
        assert_eq!(bytes, [7, 0, 0, 0, 0, 2, 0, 0, 0, b'h', b'i']);
        assert_eq!(decode(&bytes[..8]), Ok(None), "a part of the header");
        assert_eq!(decode(&bytes[..10]), Ok(None), "a part of the payload");
        assert_eq!(decode(&bytes), Ok(Some((Frame { channel: 7, kind: KIND_DATA, payload: b"hi".to_vec() }, 11))));
        let mut long = encode(1, KIND_DATA, &[]);
        long[5..9].copy_from_slice(&((MAX_PAYLOAD as u32) + 1).to_le_bytes());
        assert!(decode(&long).is_err());
    }

    /// A fake core: answers hello and the attach, sends `paint` on channel 5, then records what the client sent.
    fn fake(hello: Value, paint: Vec<Vec<u8>>) -> (String, mpsc::Receiver<Vec<Frame>>, tempdir::Dir) {
        let dir = tempdir::Dir::new();
        let path = dir.0.join("hc.sock").to_string_lossy().into_owned();
        let l = UnixListener::bind(&path).unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut w = s;
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let req: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(req["params"]["framed"], true);
            w.write_all(format!("{}\n", json!({"jsonrpc": "2.0", "id": 0, "result": hello})).as_bytes()).unwrap();
            let mut got = Vec::new();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                while let Ok(Some((f, n))) = decode(&buf) {
                    buf.drain(..n);
                    if f.channel == 0 {
                        let m: Value = serde_json::from_slice(&f.payload).unwrap();
                        assert_eq!(m["method"], "terminal.attach");
                        assert_eq!(m["params"]["observe"], true);
                        w.write_all(&encode(0, KIND_DATA, json!({"jsonrpc": "2.0", "id": m["id"], "result": {"channel": 5}}).to_string().as_bytes())).unwrap();
                        w.write_all(&encode(5, KIND_RESIZE, &[80, 0, 24, 0])).unwrap();
                        for p in &paint {
                            w.write_all(&encode(5, KIND_DATA, p)).unwrap();
                        }
                    }
                    got.push(f);
                }
                match r.get_mut().read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
            tx.send(got).unwrap();
        });
        (path, rx, dir)
    }

    mod tempdir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn new() -> Dir {
                let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
                // A short path: Unix socket paths have a length limit.
                let p = std::env::temp_dir().join(format!("hcf{}{}", std::process::id(), n % 1_000_000_000));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn ok_hello() -> Value {
        json!({"generation": 1, "framed": true, "features": ["attach"]})
    }

    #[test]
    fn observes_the_first_paint_and_detaches_without_input() {
        let (path, rx, _d) = fake(ok_hello(), vec![b"ab".to_vec(), b"cd".to_vec()]);
        let p = first_paint(&path, "t1", 200, 50, WAITS).unwrap();
        assert_eq!(p.events, vec![PaintEvent::Resize(80, 24), PaintEvent::Data(b"ab".to_vec()), PaintEvent::Data(b"cd".to_vec())]);
        let sent = rx.recv().unwrap();
        // Only the attach call and the CLOSE that detaches: no DATA, RESIZE or FOCUS on the terminal's channel.
        assert_eq!(sent.len(), 2);
        assert_eq!((sent[0].channel, sent[0].kind), (0, KIND_DATA));
        assert_eq!(sent[1], Frame { channel: 5, kind: KIND_CLOSE, payload: vec![] });
    }

    #[test]
    fn no_paint_gives_an_empty_paint_after_the_wait() {
        let (path, _rx, _d) = fake(ok_hello(), vec![]);
        let waits = Waits { paint: Duration::from_millis(50), ..WAITS };
        let p = first_paint(&path, "t1", 80, 24, waits).unwrap();
        assert_eq!(p.events, vec![PaintEvent::Resize(80, 24)]);
    }

    #[test]
    fn a_core_without_framed_attach_is_an_error() {
        let (path, _rx, _d) = fake(json!({"generation": 1, "framed": false, "features": ["attach"]}), vec![]);
        assert!(first_paint(&path, "t1", 80, 24, WAITS).unwrap_err().contains("framed"));
        let (path, _rx, _d) = fake(json!({"generation": 1, "framed": true, "features": []}), vec![]);
        assert!(first_paint(&path, "t1", 80, 24, WAITS).unwrap_err().contains("attach"));
        let (path, _rx, _d) = fake(json!({"generation": 2, "framed": true, "features": ["attach"]}), vec![]);
        assert!(first_paint(&path, "t1", 80, 24, WAITS).unwrap_err().contains("generation"));
    }

    #[test]
    fn a_missing_socket_is_an_error() {
        assert!(first_paint("/nonexistent/hc.sock", "t1", 80, 24, WAITS).is_err());
    }
}
