//! Optional per-thread record of the HID transfers a driver performs.
//!
//! OFF by default: every hook first checks one relaxed atomic, so normal
//! refreshes pay a single load per transfer. `report` switches it on for the
//! worker thread that builds a device report, to show exactly which bytes went
//! out and came back during one driver read.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::Instant;

/// Upper bound so a chatty driver cannot grow the trace without limit.
const MAX_ENTRIES: usize = 400;

static ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// Host to device.
    Out,
    /// Device to host.
    In,
}

#[derive(Clone, Debug)]
pub struct Entry {
    /// Milliseconds since [`begin`].
    pub t_ms: u32,
    pub dir: Dir,
    /// "GET_FEATURE", "SET_FEATURE", "OUTPUT", "SET_OUTPUT", "INPUT", "GET_INPUT".
    pub kind: &'static str,
    /// Which interface: "mi=01 page FF00/0001".
    pub iface: String,
    /// False when the transfer failed (for a GET: no answer; bytes = the request).
    pub ok: bool,
    pub bytes: Vec<u8>,
}

struct State {
    start: Instant,
    entries: Vec<Entry>,
    dropped: usize,
}

thread_local! {
    static TRACE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Starts recording on the calling thread (restarts if already recording).
pub fn begin() {
    TRACE.with(|t| *t.borrow_mut() = Some(State { start: Instant::now(), entries: Vec::new(), dropped: 0 }));
    ACTIVE.store(true, Relaxed);
}

/// Stops recording on the calling thread and returns the entries
/// plus how many were dropped over the size cap.
pub fn end() -> (Vec<Entry>, usize) {
    let st = TRACE.with(|t| t.borrow_mut().take());
    ACTIVE.store(false, Relaxed);
    st.map_or((Vec::new(), 0), |s| (s.entries, s.dropped))
}

#[inline]
pub fn enabled() -> bool {
    ACTIVE.load(Relaxed)
}

/// Records one transfer if the calling thread is tracing. Callers check
/// [`enabled`] first so no bytes are copied otherwise.
pub fn record(dir: Dir, kind: &'static str, iface: String, ok: bool, bytes: &[u8]) {
    TRACE.with(|t| {
        if let Some(s) = t.borrow_mut().as_mut() {
            if s.entries.len() >= MAX_ENTRIES {
                s.dropped += 1;
                return;
            }
            s.entries.push(Entry {
                t_ms: s.start.elapsed().as_millis().min(u32::MAX as u128) as u32,
                dir,
                kind,
                iface,
                ok,
                bytes: bytes.to_vec(),
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_only_between_begin_and_end() {
        record(Dir::Out, "X", String::new(), true, &[1]); // not tracing: ignored
        begin();
        record(Dir::Out, "SET_FEATURE", "mi=00".into(), true, &[1, 2]);
        record(Dir::In, "GET_FEATURE", "mi=00".into(), false, &[1]);
        let (e, dropped) = end();
        assert_eq!((e.len(), dropped), (2, 0));
        assert_eq!(e[0].bytes, [1, 2]);
        assert!(!e[1].ok);
        assert!(!enabled());
        assert!(end().0.is_empty());
    }
}
