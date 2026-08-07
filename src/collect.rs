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
use crate::app::next_poll_deadline;

/// How often the loop wakes up when nothing is scheduled, so it can react to
/// cadence/pause changes and shutdown promptly without spinning a hot loop.
const TICK: Duration = Duration::from_millis(100);

/// Mutable cell holding the newest snapshot the collector has produced along
/// with a monotonic production counter used to tell *new* from *already seen*.
struct State {
    snapshot: Option<Snapshot>,
    produced: u64,
    // Incremented whenever collection crosses a pause boundary. A fetch keeps
    // the epoch it started in and may only publish into that same epoch, which
    // prevents a pre-pause request from becoming the post-resume rate baseline.
    epoch: u64,
}

/// Shared, thread-safe snapshot collector.
#[derive(Clone)]
pub struct Collector {
    url: String,
    update_ms: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    // Set when the UI wants the loop to fetch right away (e.g. on resume from
    // pause) instead of waiting out a stale deadline.
    wakeup: Arc<AtomicBool>,
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
            wakeup: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                snapshot: None,
                produced: 0,
                epoch: 0,
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
        self.update_ms.store(update_ms, Ordering::Release);
        // The existing deadline was calculated from the old cadence and may be
        // minutes or hours away. Invalidate it so the new cadence takes effect
        // immediately instead of waiting for that stale deadline.
        self.wakeup.store(true, Ordering::Release);
    }

    /// Pause/unpause background collection. Paused collection stops issuing
    /// network/GPU work but still wakes to notice the change.
    ///
    /// Resuming from a pause requests an immediate fetch so the dashboard gets
    /// fresh data right away instead of waiting out the (possibly far-off)
    /// deadline that was scheduled while paused.
    pub fn set_paused(&self, paused: bool) {
        let was_paused = self.paused.swap(paused, Ordering::AcqRel);
        if was_paused == paused {
            return;
        }

        // A request already in flight cannot be cancelled. Advancing the epoch
        // both discards any pending snapshot and prevents that request from
        // publishing after a pause/resume transition.
        let mut state = self.state.lock().unwrap();
        state.epoch = state.epoch.wrapping_add(1);
        state.snapshot = None;
        drop(state);

        if !paused {
            self.wakeup.store(true, Ordering::Release);
        }
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

    fn current_epoch(&self) -> u64 {
        self.state.lock().unwrap().epoch
    }

    /// Publish only if the collection cycle still belongs to the current
    /// pause epoch. Returns whether the snapshot became visible to the UI.
    fn publish(&self, snapshot: Snapshot, epoch: u64) -> bool {
        let mut state = self.state.lock().unwrap();
        if self.paused.load(Ordering::Acquire) || state.epoch != epoch {
            return false;
        }
        state.snapshot = Some(snapshot);
        state.produced = state.produced.wrapping_add(1);
        true
    }

    fn run_loop(&self) {
        // First snapshot is due immediately, before the first draw.
        let mut next_fetch = Instant::now();
        // Continuity for deltas/carry-over between consecutive fetches. Kept on
        // this thread so it never races with the UI thread's reads.
        let mut previous: Option<Snapshot> = None;

        while !self.is_stopping() {
            // A resume (or any explicit wakeup, e.g. an interval change) should
            // fetch immediately rather than wait out a stale deadline that may
            // have been scheduled far in the future while paused.
            if self.wakeup.swap(false, Ordering::AcqRel) {
                next_fetch = Instant::now();
            }

            let now = Instant::now();
            if now < next_fetch {
                std::thread::sleep((next_fetch - now).min(TICK));
                continue;
            }

            let interval = Duration::from_millis(self.update_ms.load(Ordering::Relaxed));
            let started = now;
            let epoch = self.current_epoch();
            if !self.paused.load(Ordering::Relaxed) {
                let snapshot = api::fetch_snapshot(&self.url, &previous);
                if self.publish(snapshot.clone(), epoch) {
                    previous = Some(snapshot);
                }
            }
            let finished = Instant::now();

            // Advance to the first interval boundary *after* the cycle actually
            // finished. If a fetch overruns its interval (e.g. a slow request
            // near its timeout) the deadline is no longer in the past, so we
            // won't immediately start another back-to-back request against an
            // already slow server (the missed-tick behavior).
            next_fetch = next_poll_deadline(started, finished, interval);
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

        let epoch = collector.current_epoch();
        assert!(collector.publish(Snapshot::new(), epoch));

        let generation = collector.take_newer_than(0).map(|(_, g)| g).unwrap();
        assert_eq!(generation, 1);
        // Already consumed at generation 1 -> no new data.
        assert!(collector.take_newer_than(generation).is_none());

        // A later publication bumps the generation and is visible again.
        assert!(collector.publish(Snapshot::new(), epoch));
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

    #[test]
    fn interval_changes_invalidate_the_old_deadline() {
        let collector = Collector::new("http://localhost:8080".to_string(), 1000);

        collector.set_update_ms(500);

        assert_eq!(collector.update_ms.load(Ordering::Acquire), 500);
        assert!(collector.wakeup.swap(false, Ordering::AcqRel));
        assert!(!collector.wakeup.load(Ordering::Acquire));
    }

    #[test]
    fn pause_boundaries_discard_pending_and_inflight_snapshots() {
        let collector = Collector::new("http://localhost:8080".to_string(), 1000);
        let pre_pause_epoch = collector.current_epoch();

        // Model a completed snapshot that the UI has not consumed yet.
        assert!(collector.publish(Snapshot::new(), pre_pause_epoch));
        assert!(collector.take_newer_than(0).is_some());

        collector.set_paused(true);
        assert!(collector.take_newer_than(0).is_none());
        collector.set_paused(false);

        // Model a slow request that began before pausing and only completed
        // after resuming. It must not become the new generation-rate baseline.
        assert!(!collector.publish(Snapshot::new(), pre_pause_epoch));
        assert!(collector.take_newer_than(0).is_none());

        let post_resume_epoch = collector.current_epoch();
        assert_ne!(post_resume_epoch, pre_pause_epoch);
        assert!(collector.publish(Snapshot::new(), post_resume_epoch));
        assert!(collector.take_newer_than(0).is_some());
    }

    #[test]
    fn resuming_from_pause_requests_an_immediate_fetch() {
        let collector = Collector::new("http://localhost:8080".to_string(), 1000);

        collector.set_paused(true);
        collector.set_paused(false);

        // Unpausing latches a wakeup so the loop fetches right away instead of
        // waiting out the deadline scheduled while paused.
        assert!(collector.wakeup.swap(false, Ordering::AcqRel));
        // The wakeup is consumed (one-shot), so a second resume is not needed
        // to clear it and it won't fire spuriously later.
        assert!(!collector.wakeup.load(Ordering::Acquire));
    }

    #[test]
    fn not_paused_does_not_spuriously_latch_a_wakeup() {
        let collector = Collector::new("http://localhost:8080".to_string(), 1000);

        // Already running: toggling to unpaused must not schedule a fetch.
        collector.set_paused(false);
        assert!(!collector.wakeup.load(Ordering::Acquire));
    }

    #[test]
    fn overrun_deadline_advances_past_the_finished_time() {
        // A fetch that outlives its interval must schedule the next fetch at
        // the first interval boundary *after* completion, not one that already
        // elapsed (which would trigger an immediate back-to-back request).
        let started = Instant::now();
        let finished = started + Duration::from_millis(2_500);
        let interval = Duration::from_secs(1);

        assert_eq!(
            next_poll_deadline(started, finished, interval),
            started + Duration::from_secs(3)
        );
    }
}
