//! A background snapshot collector.
//!
//! `ltop` renders and reads keys on the UI thread. Every network request to the
//! llama.cpp server and every `nvidia-smi` subprocess call is comparatively slow
//! and, worse, *blocking* (each HTTP fetch waits up to 5 seconds). If collection
//! happens inline on the UI thread, pressing a key like `t` cannot be *drawn*
//! until the collection cycle — and therefore nothing the UI does feels snappy
//! while the server is busy.
//!
//! [`Collector`] solves this by performing all I/O on a dedicated thread. The UI
//! thread only ever reads whatever snapshot the collector has already produced,
//! so drawing and key handling never wait on the network. The collector is cheap
//! to clone: configuration and the produced snapshot live behind `Arc`s shared
//! between the main `App` and the collection thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::api::{self, Snapshot};

/// How often the loop wakes up when nothing is scheduled, so it can react to
/// cadence/pause changes and shutdown promptly without spinning a hot loop.
const TICK: Duration = Duration::from_millis(100);

/// Mutable cell holding the newest snapshot the collector has produced along
/// with a monotonic production counter used to tell *new* from *already seen*.
struct State {
    snapshot: Option<Snapshot>,
    produced: u64,
}

/// Shared, thread-safe snapshot collector.
#[derive(Clone)]
pub struct Collector {
    url: String,
    update_ms: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
}

impl Collector {
    /// Build an *unstarted* collector for `url`. Nothing is fetched until
    /// [`Collector::start`] is called; this keeps constructing an [`App`] (as
    /// tests do) free of background threads.
    pub fn new(url: String, update_ms: u64) -> Self {
        Self {
            url,
            update_ms: Arc::new(AtomicU64::new(update_ms)),
            paused: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                snapshot: None,
                produced: 0,
            })),
        }
    }

    /// Spawn the collection thread. The returned `JoinHandle` lets the caller
    /// stop the collector and wait for it to finish on shutdown.
    pub fn start(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let collector = Arc::clone(self);
        std::thread::Builder::new()
            .name("ltop-collector".to_string())
            .spawn(move || collector.run_loop())
            .expect("failed to spawn the ltop collector thread")
    }

    /// Change the collection cadence for the background thread.
    pub fn set_update_ms(&self, update_ms: u64) {
        self.update_ms.store(update_ms, Ordering::Relaxed);
    }

    /// Pause/unpause background collection. Paused collection stops issuing
    /// network/GPU work but still wakes to notice the change.
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    /// Ask the collection thread to wind down. Safe to call repeatedly.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn is_stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Return the newest snapshot the collector has produced since generation
    /// `applied`, or `None` if there is nothing new. Never blocks.
    pub fn take_newer_than(&self, applied: u64) -> Option<(Snapshot, u64)> {
        let state = self.state.lock().unwrap();
        let produced = state.produced;
        state
            .snapshot
            .clone()
            .filter(|_| produced > applied)
            .map(|snapshot| (snapshot, produced))
    }

    fn publish(&self, snapshot: Snapshot) {
        let mut state = self.state.lock().unwrap();
        state.snapshot = Some(snapshot);
        state.produced = state.produced.wrapping_add(1);
    }

    fn run_loop(&self) {
        // First snapshot is due immediately, before the first draw.
        let mut next_fetch = Instant::now();
        // Continuity for deltas/carry-over between consecutive fetches. Kept on
        // this thread so it never races with the UI thread's reads.
        let mut previous: Option<Snapshot> = None;

        while !self.is_stopping() {
            let now = Instant::now();
            if now < next_fetch {
                std::thread::sleep((next_fetch - now).min(TICK));
                continue;
            }

            let interval = Duration::from_millis(self.update_ms.load(Ordering::Relaxed));
            if !self.paused.load(Ordering::Relaxed) {
                let snapshot = api::fetch_snapshot(&self.url, &previous);
                self.publish(snapshot.clone());
                previous = Some(snapshot);
            }

            next_fetch = now.checked_add(interval).unwrap_or(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_newer_than_only_reports_unconsumed_snapshots() {
        let collector = Collector::new("http://localhost:8080".to_string(), 1000);

        // Nothing produced yet.
        assert!(collector.take_newer_than(0).is_none());

        collector.publish(Snapshot::new());

        let generation = collector.take_newer_than(0).map(|(_, g)| g).unwrap();
        assert_eq!(generation, 1);
        // Already consumed at generation 1 -> no new data.
        assert!(collector.take_newer_than(generation).is_none());

        // A later publication bumps the generation and is visible again.
        collector.publish(Snapshot::new());
        assert!(collector.take_newer_than(generation).is_some());
    }

    #[test]
    fn config_updates_flow_through_to_the_collector() {
        let collector = Collector::new("http://localhost:8080".to_string(), 1000);

        collector.set_update_ms(500);
        collector.set_paused(true);

        assert_eq!(collector.update_ms.load(Ordering::Relaxed), 500);
        assert!(collector.paused.load(Ordering::Relaxed));
    }
}
