#![allow(dead_code)]
use crate::api::{Metrics, SlotInfo, Snapshot};
use crate::collect::Collector;
use crate::theme::{
    validate_update_ms, Theme, ThemeCatalog, ThemePreferences, DEFAULT_UPDATE_MS, MAX_UPDATE_MS,
    MIN_UPDATE_MS,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MAX_SAMPLES: usize = 120;

/// How long the UI waits between checks for a fresh snapshot when the collector
/// has not produced one yet. Small enough to stay responsive, large enough to
/// avoid a hot loop.
const COLLECTOR_POLL_BACKOFF: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SlotCounters {
    task_id: Option<i64>,
    decoded_tokens: i64,
}

impl From<&SlotInfo> for SlotCounters {
    fn from(slot: &SlotInfo) -> Self {
        Self {
            task_id: slot.task_id,
            decoded_tokens: slot.decoded_tokens,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheTotals {
    pub requests: u64,
    pub reused_tokens: u64,
    pub evaluated_tokens: u64,
}

impl CacheTotals {
    pub fn input_tokens(&self) -> u64 {
        self.reused_tokens.saturating_add(self.evaluated_tokens)
    }

    pub fn reuse_percent(&self) -> f64 {
        let input = self.input_tokens();
        if input == 0 {
            0.0
        } else {
            self.reused_tokens as f64 / input as f64 * 100.0
        }
    }

    fn include(&mut self, request: &CacheRequestObservation) {
        self.requests = self.requests.saturating_add(1);
        self.reused_tokens = self.reused_tokens.saturating_add(request.reused_tokens);
        self.evaluated_tokens = self
            .evaluated_tokens
            .saturating_add(request.evaluated_tokens);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheRequestObservation {
    pub slot_id: i64,
    pub task_id: Option<i64>,
    pub reused_tokens: u64,
    pub evaluated_tokens: u64,
    pub context_tokens: u64,
    pub context_capacity: u64,
    pub output_tokens: u64,
    pub phase: &'static str,
    pub last_seen: Instant,
}

impl CacheRequestObservation {
    fn from_slot(slot: &SlotInfo, now: Instant) -> Self {
        Self {
            slot_id: slot.id,
            task_id: slot.task_id,
            reused_tokens: slot.prompt_tokens_cached.max(0) as u64,
            evaluated_tokens: slot.prompt_tokens_processed.max(0) as u64,
            context_tokens: slot.context_tokens.max(0) as u64,
            context_capacity: slot.context_capacity.max(0) as u64,
            output_tokens: slot.current_output_tokens().max(0) as u64,
            phase: slot.phase(),
            last_seen: now,
        }
    }

    pub fn input_tokens(&self) -> u64 {
        self.reused_tokens.saturating_add(self.evaluated_tokens)
    }

    pub fn reuse_percent(&self) -> f64 {
        let input = self.input_tokens();
        if input == 0 {
            0.0
        } else {
            self.reused_tokens as f64 / input as f64 * 100.0
        }
    }

    pub fn context_headroom(&self) -> Option<u64> {
        (self.context_capacity > 0)
            .then(|| self.context_capacity.saturating_sub(self.context_tokens))
    }

    pub fn context_percent(&self) -> f64 {
        if self.context_capacity == 0 {
            0.0
        } else {
            self.context_tokens as f64 / self.context_capacity as f64 * 100.0
        }
    }

    pub fn provisional(&self) -> bool {
        self.phase == "prefill"
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PromptRateUpdate {
    measurement: Option<PromptRateMeasurement>,
    reset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PromptRateMeasurement {
    tokens_per_second: f64,
    basis: PromptRateBasis,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PromptRateBasis {
    #[default]
    Unavailable,
    Interval,
    ServerAverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Overview,
    Service,
    Throughput,
    Slots,
    Cache,
    Gpu,
    Help,
}

impl Section {
    pub fn next(&self) -> Self {
        match self {
            Section::Overview => Section::Service,
            Section::Service => Section::Throughput,
            Section::Throughput => Section::Slots,
            Section::Slots => Section::Cache,
            Section::Cache => Section::Gpu,
            Section::Gpu => Section::Overview,
            Section::Help => Section::Overview,
        }
    }
    pub fn prev(&self) -> Self {
        match self {
            Section::Overview => Section::Gpu,
            Section::Service => Section::Overview,
            Section::Throughput => Section::Service,
            Section::Slots => Section::Throughput,
            Section::Cache => Section::Slots,
            Section::Gpu => Section::Cache,
            Section::Help => Section::Overview,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Section::Overview => "Overview",
            Section::Service => "Service",
            Section::Throughput => "Throughput",
            Section::Slots => "Slots",
            Section::Cache => "Cache",
            Section::Gpu => "GPU",
            Section::Help => "Help",
        }
    }
}

pub struct App {
    pub url: String,
    pub snapshot: Snapshot,
    pub current_section: Section,
    pub scroll: u16,
    pub bits_mode: bool,
    pub paused: bool,
    pub show_help: bool,
    pub show_theme_picker: bool,
    pub theme: Theme,
    pub theme_background: bool,
    pub start_time: Instant,
    pub last_poll: Instant,
    pub update_ms: u64,
    pub prompt_rate_history: VecDeque<f64>,
    pub predict_rate_history: VecDeque<f64>,
    pub gpu_util_history: VecDeque<f64>,
    pub power_history: VecDeque<f64>,
    pub mem_history: VecDeque<f64>,
    pub requests_history: VecDeque<f64>,
    pub total_prompt_tokens: f64,
    pub total_predict_tokens: f64,
    pub prompt_rate: f64,
    pub prompt_rate_basis: PromptRateBasis,
    pub predict_rate: f64,
    theme_catalog: ThemeCatalog,
    active_theme_index: usize,
    picker_theme_index: usize,
    theme_before_picker: usize,
    background_before_picker: bool,
    // Previous state is tracked per lane so one slot resetting cannot erase
    // progress made by another slot in the same polling interval.
    prev_slot_counters: HashMap<i64, SlotCounters>,
    active_cache_requests: HashMap<i64, CacheRequestObservation>,
    completed_cache_totals: CacheTotals,
    last_cache_request: Option<CacheRequestObservation>,
    prev_rate_time: Option<Instant>,
    next_poll: Instant,
    // Background snapshot provider. All network/GPU work happens off the UI
    // thread so drawing and key handling never block on the server.
    collector: Arc<Collector>,
    // Generation of the newest snapshot we have incorporated into `snapshot`.
    applied_generation: u64,
}

impl App {
    pub fn new(url: String) -> Self {
        Self::with_update_ms(url, DEFAULT_UPDATE_MS)
    }

    pub fn with_update_ms(url: String, update_ms: u64) -> Self {
        Self::with_theme_catalog_and_update_ms(
            url,
            ThemeCatalog::builtin_only(),
            0,
            true,
            update_ms,
        )
    }

    pub fn with_theme_catalog(
        url: String,
        theme_catalog: ThemeCatalog,
        active_theme_index: usize,
        theme_background: bool,
    ) -> Self {
        Self::with_theme_catalog_and_update_ms(
            url,
            theme_catalog,
            active_theme_index,
            theme_background,
            DEFAULT_UPDATE_MS,
        )
    }

    pub fn with_theme_catalog_and_update_ms(
        url: String,
        theme_catalog: ThemeCatalog,
        active_theme_index: usize,
        theme_background: bool,
        update_ms: u64,
    ) -> Self {
        let now = Instant::now();
        let update_ms = update_ms.clamp(MIN_UPDATE_MS, MAX_UPDATE_MS);
        let active_theme_index = active_theme_index.min(theme_catalog.len().saturating_sub(1));
        let theme = theme_catalog.theme(active_theme_index).clone();
        let collector = Arc::new(Collector::new(url.clone(), update_ms));
        Self {
            url,
            snapshot: Snapshot::new(),
            current_section: Section::Overview,
            scroll: 0,
            bits_mode: false,
            paused: false,
            show_help: false,
            show_theme_picker: false,
            theme,
            theme_background,
            start_time: now,
            last_poll: now,
            update_ms,
            prompt_rate_history: VecDeque::with_capacity(MAX_SAMPLES),
            predict_rate_history: VecDeque::with_capacity(MAX_SAMPLES),
            gpu_util_history: VecDeque::with_capacity(MAX_SAMPLES),
            power_history: VecDeque::with_capacity(MAX_SAMPLES),
            mem_history: VecDeque::with_capacity(MAX_SAMPLES),
            requests_history: VecDeque::with_capacity(MAX_SAMPLES),
            total_prompt_tokens: 0.0,
            total_predict_tokens: 0.0,
            prompt_rate: 0.0,
            prompt_rate_basis: PromptRateBasis::Unavailable,
            predict_rate: 0.0,
            theme_catalog,
            active_theme_index,
            picker_theme_index: active_theme_index,
            theme_before_picker: active_theme_index,
            background_before_picker: theme_background,
            prev_slot_counters: HashMap::new(),
            active_cache_requests: HashMap::new(),
            completed_cache_totals: CacheTotals::default(),
            last_cache_request: None,
            prev_rate_time: None,
            // The first collection is due immediately, before the first draw.
            next_poll: now,
            collector,
            applied_generation: 0,
        }
    }

    pub fn next_section(&mut self) {
        self.current_section = self.current_section.next();
        self.scroll = 0;
    }

    pub fn prev_section(&mut self) {
        self.current_section = self.current_section.prev();
        self.scroll = 0;
    }

    pub fn select_section(&mut self, section: Section) {
        self.current_section = section;
        self.scroll = 0;
    }

    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }

    pub fn scroll_down(&mut self) {
        let max_scroll = match self.current_section {
            Section::Slots => self.snapshot.slots.len().saturating_sub(1),
            Section::Gpu => self.snapshot.gpus.len().saturating_sub(1),
            // The rich service view wraps differently with terminal width.
            // A generous bound keeps the bottom reachable on the minimum-size
            // layout; rendering safely clips any harmless overscroll.
            Section::Service | Section::Cache => 80,
            _ => 0,
        };
        self.scroll = (self.scroll as usize + 1).min(max_scroll) as u16;
    }

    pub fn toggle_rate_unit(&mut self) {
        self.bits_mode = !self.bits_mode;
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        // Keep the background collector in sync so it stops hammering the
        // server (and spending effort) while the UI is frozen.
        self.collector.set_paused(self.paused);
        if !self.paused {
            // Resume with a fresh universal snapshot. Slot counters cannot be
            // divided by time spent paused without producing a false rate.
            self.next_poll = Instant::now();
            self.prev_slot_counters.clear();
            self.prev_rate_time = None;
        }
    }

    pub fn poll_wait(&self) -> Duration {
        if self.paused {
            return Duration::from_secs(1);
        }
        self.next_poll.saturating_duration_since(Instant::now())
    }

    /// Start the background snapshot collector. Call once from the event loop
    /// before rendering. Returns the collection thread's join handle.
    pub fn start_collection(&self) -> std::thread::JoinHandle<()> {
        self.collector.start()
    }

    /// Ask the background collector to wind down. Asynchronous: it only takes
    /// effect between collection cycles and never waits on an in-flight fetch.
    pub fn stop_collection(&self) {
        self.collector.stop();
    }

    pub fn update_interval_label(&self) -> String {
        if self.update_ms >= 3_600_000 {
            let hours = self.update_ms as f64 / 3_600_000.0;
            if evenly_divisible(self.update_ms, 3_600_000) {
                format!("{hours:.0}h")
            } else {
                format!("{hours:.1}h")
            }
        } else if self.update_ms >= 60_000 {
            let minutes = self.update_ms as f64 / 60_000.0;
            if evenly_divisible(self.update_ms, 60_000) {
                format!("{minutes:.0}m")
            } else {
                format!("{minutes:.1}m")
            }
        } else if self.update_ms >= 1_000 {
            let seconds = self.update_ms as f64 / 1_000.0;
            if evenly_divisible(self.update_ms, 1_000) {
                format!("{seconds:.0}s")
            } else {
                format!("{seconds:.1}s")
            }
        } else {
            format!("{}ms", self.update_ms)
        }
    }

    pub fn increase_update_interval(&mut self) -> Result<(), String> {
        let update_ms = self.update_ms.saturating_add(100).min(MAX_UPDATE_MS);
        if update_ms == self.update_ms {
            return Ok(());
        }
        self.set_update_interval(update_ms)
    }

    pub fn decrease_update_interval(&mut self) -> Result<(), String> {
        let update_ms = self.update_ms.saturating_sub(100).max(MIN_UPDATE_MS);
        if update_ms == self.update_ms {
            return Ok(());
        }
        self.set_update_interval(update_ms)
    }

    fn set_update_interval(&mut self, update_ms: u64) -> Result<(), String> {
        self.update_ms = validate_update_ms(update_ms)?;
        self.collector.set_update_ms(self.update_ms);
        self.next_poll = Instant::now()
            .checked_add(Duration::from_millis(self.update_ms))
            .unwrap_or_else(Instant::now);
        self.save_preferences()
    }

    fn save_preferences(&self) -> Result<(), String> {
        ThemePreferences {
            theme: self.theme.selection().to_string(),
            theme_background: self.theme_background,
            update_ms: self.update_ms,
        }
        .save()
    }

    pub fn toggle_help(&mut self) {
        if self.show_theme_picker {
            self.cancel_theme_picker();
        }
        self.show_help = !self.show_help;
    }

    pub fn open_theme_picker(&mut self) {
        self.show_help = false;
        self.theme_before_picker = self.active_theme_index;
        self.background_before_picker = self.theme_background;
        self.picker_theme_index = self.active_theme_index;
        self.show_theme_picker = true;
    }

    pub fn theme_count(&self) -> usize {
        self.theme_catalog.len()
    }

    pub fn theme_name(&self, index: usize) -> &str {
        self.theme_catalog.theme(index).name()
    }

    pub fn theme_at(&self, index: usize) -> &Theme {
        self.theme_catalog.theme(index)
    }

    pub fn picker_theme_index(&self) -> usize {
        self.picker_theme_index
    }

    pub fn active_theme_index(&self) -> usize {
        self.active_theme_index
    }

    pub fn preview_next_theme(&mut self) {
        self.move_theme_preview(1);
    }

    pub fn preview_previous_theme(&mut self) {
        self.move_theme_preview(-1);
    }

    pub fn move_theme_preview(&mut self, delta: isize) {
        let len = self.theme_catalog.len();
        if len == 0 {
            return;
        }
        self.picker_theme_index =
            (self.picker_theme_index as isize + delta).rem_euclid(len as isize) as usize;
        self.theme = self.theme_catalog.theme(self.picker_theme_index).clone();
    }

    pub fn preview_first_theme(&mut self) {
        self.preview_theme(0);
    }

    pub fn preview_last_theme(&mut self) {
        self.preview_theme(self.theme_catalog.len().saturating_sub(1));
    }

    pub fn toggle_theme_background(&mut self) {
        self.theme_background = !self.theme_background;
    }

    pub fn commit_theme_picker(&mut self) -> Result<(), String> {
        self.active_theme_index = self.picker_theme_index;
        self.show_theme_picker = false;
        self.save_preferences()
    }

    pub fn cancel_theme_picker(&mut self) {
        self.active_theme_index = self.theme_before_picker;
        self.picker_theme_index = self.active_theme_index;
        self.theme = self.theme_catalog.theme(self.active_theme_index).clone();
        self.theme_background = self.background_before_picker;
        self.show_theme_picker = false;
    }

    fn preview_theme(&mut self, index: usize) {
        if self.theme_catalog.is_empty() {
            return;
        }
        self.picker_theme_index = index.min(self.theme_catalog.len() - 1);
        self.theme = self.theme_catalog.theme(self.picker_theme_index).clone();
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Fold the newest snapshot produced by the background collector into the
    /// running charts.
    ///
    /// This never performs I/O and therefore never blocks the UI thread: it
    /// reads whatever snapshot the [`Collector`] already fetched and folds it
    /// into the histories. Returns `true` when a fresh snapshot was applied.
    pub fn poll(&mut self) -> bool {
        let now = Instant::now();
        if self.paused {
            return false;
        }

        // The collector has not produced anything newer than what we last
        // applied. Schedule a short re-check so the loop wakes up as soon as a
        // snapshot lands instead of spinning on a zero-duration input wait.
        let Some((snap, generation)) = self.collector.take_newer_than(self.applied_generation)
        else {
            self.next_poll = now.checked_add(COLLECTOR_POLL_BACKOFF).unwrap_or(now);
            return false;
        };
        self.applied_generation = generation;
        self.last_poll = now;

        let mut prompt_history_sample = 0.0;

        // Prompt-processing speed is measured from llama.cpp's cumulative
        // evaluated-token and active prompt-time counters. Slot snapshots are
        // unsuitable here: a short prompt can start and finish between polls,
        // and a task transition resets its per-slot counters.
        if snap.metrics_available {
            let update = measured_prompt_rate(&snap.metrics, snap.prev_metrics.as_ref());
            prompt_history_sample = prompt_chart_sample(update, snap.prev_metrics.is_some());
            if update.reset {
                self.prompt_rate = 0.0;
                self.prompt_rate_basis = PromptRateBasis::Unavailable;
                self.prompt_rate_history.clear();
            }
            if let Some(measurement) = update.measurement {
                self.prompt_rate = measurement.tokens_per_second;
                self.prompt_rate_basis = measurement.basis;
            }
        }

        // Generation remains a live activity rate from the per-slot decoded
        // counters, which advance throughout generation.
        let dt = self
            .prev_rate_time
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);

        if dt > 0.0 {
            let any_processing = snap.slots.iter().any(|s| s.is_processing);

            if any_processing {
                let decoded_delta = slot_decoded_delta(&self.prev_slot_counters, &snap.slots);

                // Generation rate: new decoded tokens / time
                self.predict_rate = decoded_delta as f64 / dt;
            } else {
                // No slot is processing — reset rates to 0
                self.predict_rate = 0.0;
            }
        }

        if snap.slots_error.is_none() {
            self.observe_cache_slots(&snap.slots, now);
        }

        self.prev_slot_counters = snap
            .slots
            .iter()
            .map(|slot| (slot.id, SlotCounters::from(slot)))
            .collect();
        self.prev_rate_time = Some(now);

        // Every chart advances on the same universal polling tick. Prompt eval
        // uses zero for intervals without newly committed prefill work, while
        // the headline above retains the last measured speed.
        Self::push_history(&mut self.prompt_rate_history, prompt_history_sample);
        Self::push_history(&mut self.predict_rate_history, self.predict_rate);

        // GPU averages
        if !snap.gpus.is_empty() {
            let avg_util: f64 =
                snap.gpus.iter().map(|g| g.gpu_util).sum::<f64>() / snap.gpus.len() as f64;
            let total_power: f64 = snap.gpus.iter().map(|g| g.power_draw).sum();
            let avg_mem_pct: f64 = snap
                .gpus
                .iter()
                .map(|g| {
                    if g.mem_total > 0 {
                        (g.mem_used as f64 / g.mem_total as f64) * 100.0
                    } else {
                        0.0
                    }
                })
                .sum::<f64>()
                / snap.gpus.len() as f64;

            Self::push_history(&mut self.gpu_util_history, avg_util);
            Self::push_history(&mut self.power_history, total_power);
            Self::push_history(&mut self.mem_history, avg_mem_pct);
        }

        if snap.metrics_available {
            Self::push_history(&mut self.requests_history, snap.metrics.requests_processing);
            self.total_prompt_tokens = snap.metrics.prompt_tokens_total;
            self.total_predict_tokens = snap.metrics.tokens_predicted_total;
        }

        self.snapshot = snap;

        // Keep one cadence for the whole snapshot. If a slow source overruns
        // an interval, advance to the next future tick instead of issuing a
        // burst of catch-up polls.
        let finished = Instant::now();
        self.next_poll = next_poll_deadline(now, finished, Duration::from_millis(self.update_ms));
        true
    }

    fn push_history(history: &mut VecDeque<f64>, val: f64) {
        if history.len() >= MAX_SAMPLES {
            history.pop_front();
        }
        history.push_back(val);
    }

    pub fn history_as_points(&self, history: &VecDeque<f64>) -> Vec<(f64, f64)> {
        history
            .iter()
            .enumerate()
            .map(|(i, v)| (i as f64, *v))
            .collect()
    }

    pub fn max_history(&self, history: &VecDeque<f64>) -> f64 {
        let max = history.iter().cloned().fold(0.0f64, f64::max);
        if max <= 0.0 {
            return 1.0;
        }
        let scaled = max * 1.2;
        if scaled >= 100.0 {
            (scaled / 10.0).ceil() * 10.0
        } else if scaled >= 10.0 {
            (scaled).ceil()
        } else {
            (scaled * 10.0).ceil() / 10.0
        }
    }

    pub fn uptime_str(&self) -> String {
        let d = self.start_time.elapsed();
        let h = d.as_secs() / 3600;
        let m = (d.as_secs() % 3600) / 60;
        let s = d.as_secs() % 60;
        if h > 0 {
            format!("{}h {:02}m {:02}s", h, m, s)
        } else {
            format!("{:02}m {:02}s", m, s)
        }
    }

    pub fn cache_observed_totals(&self) -> CacheTotals {
        let mut totals = self.completed_cache_totals;
        for request in self.active_cache_requests.values() {
            totals.include(request);
        }
        totals
    }

    pub fn last_cache_request(&self) -> Option<&CacheRequestObservation> {
        self.last_cache_request.as_ref()
    }

    pub fn active_cache_requests(&self) -> impl Iterator<Item = &CacheRequestObservation> {
        self.active_cache_requests.values()
    }

    pub(crate) fn observe_cache_slots(&mut self, slots: &[SlotInfo], now: Instant) {
        let active_slot_ids: HashSet<i64> = slots
            .iter()
            .filter(|slot| slot.is_processing)
            .map(|slot| slot.id)
            .collect();

        for slot in slots.iter().filter(|slot| slot.is_processing) {
            let mut current = CacheRequestObservation::from_slot(slot, now);
            if let Some(previous) = self.active_cache_requests.remove(&slot.id) {
                let compatible_task_ids = previous.task_id == current.task_id
                    || previous.task_id.is_none()
                    || current.task_id.is_none();
                let counters_are_continuous = current.input_tokens() >= previous.input_tokens();

                if compatible_task_ids && counters_are_continuous {
                    current.task_id = current.task_id.or(previous.task_id);
                } else {
                    self.finish_cache_request(previous);
                }
            }
            self.active_cache_requests.insert(slot.id, current);
        }

        let finished_slots: Vec<i64> = self
            .active_cache_requests
            .keys()
            .filter(|slot_id| !active_slot_ids.contains(slot_id))
            .copied()
            .collect();
        for slot_id in finished_slots {
            if let Some(request) = self.active_cache_requests.remove(&slot_id) {
                self.finish_cache_request(request);
            }
        }
    }

    fn finish_cache_request(&mut self, request: CacheRequestObservation) {
        self.completed_cache_totals.include(&request);
        let is_newest = self
            .last_cache_request
            .as_ref()
            .is_none_or(|last| request.last_seen >= last.last_seen);
        if is_newest {
            self.last_cache_request = Some(request);
        }
    }
}

pub(crate) fn next_poll_deadline(
    started: Instant,
    finished: Instant,
    interval: Duration,
) -> Instant {
    let mut deadline = started.checked_add(interval).unwrap_or(finished);
    while deadline <= finished {
        let Some(next) = deadline.checked_add(interval) else {
            return finished;
        };
        deadline = next;
    }
    deadline
}

fn evenly_divisible(value: u64, divisor: u64) -> bool {
    value.checked_rem(divisor) == Some(0)
}

fn valid_counter(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn cumulative_prompt_average(metrics: &Metrics) -> Option<f64> {
    if !valid_counter(metrics.prompt_tokens_total)
        || !valid_counter(metrics.prompt_seconds_total)
        || metrics.prompt_tokens_total <= 0.0
    {
        return None;
    }

    if metrics.prompt_tokens_seconds.is_finite() && metrics.prompt_tokens_seconds > 0.0 {
        return Some(metrics.prompt_tokens_seconds);
    }

    if metrics.prompt_seconds_total > 0.0 {
        let rate = metrics.prompt_tokens_total / metrics.prompt_seconds_total;
        if rate.is_finite() && rate > 0.0 {
            return Some(rate);
        }
    }

    None
}

/// Converts a prompt measurement into activity for the current chart tick.
/// An initial scrape may supply a historical server average for the headline,
/// but only a delta against a previous scrape belongs on the live timeline.
fn prompt_chart_sample(update: PromptRateUpdate, has_previous_metrics: bool) -> f64 {
    if !has_previous_metrics {
        return 0.0;
    }

    update
        .measurement
        .map_or(0.0, |measurement| measurement.tokens_per_second)
}

/// Returns a server-timed prompt-evaluation measurement when new prompt work
/// has been committed to `/metrics`. The delta uses active prompt-processing
/// seconds rather than scrape wall time, so idle time and HTTP latency cannot
/// distort the value.
fn measured_prompt_rate(current: &Metrics, previous: Option<&Metrics>) -> PromptRateUpdate {
    if !valid_counter(current.prompt_tokens_total) || !valid_counter(current.prompt_seconds_total) {
        return PromptRateUpdate::default();
    }

    let Some(previous) = previous else {
        return PromptRateUpdate {
            measurement: cumulative_prompt_average(current).map(|tokens_per_second| {
                PromptRateMeasurement {
                    tokens_per_second,
                    basis: PromptRateBasis::ServerAverage,
                }
            }),
            reset: false,
        };
    };

    if !valid_counter(previous.prompt_tokens_total) || !valid_counter(previous.prompt_seconds_total)
    {
        return PromptRateUpdate {
            measurement: cumulative_prompt_average(current).map(|tokens_per_second| {
                PromptRateMeasurement {
                    tokens_per_second,
                    basis: PromptRateBasis::ServerAverage,
                }
            }),
            reset: false,
        };
    }

    let reset = current.prompt_tokens_total < previous.prompt_tokens_total
        || current.prompt_seconds_total < previous.prompt_seconds_total;
    if reset {
        return PromptRateUpdate {
            measurement: cumulative_prompt_average(current).map(|tokens_per_second| {
                PromptRateMeasurement {
                    tokens_per_second,
                    basis: PromptRateBasis::ServerAverage,
                }
            }),
            reset: true,
        };
    }

    let token_delta = current.prompt_tokens_total - previous.prompt_tokens_total;
    if token_delta <= 0.0 {
        return PromptRateUpdate::default();
    }

    let seconds_delta = current.prompt_seconds_total - previous.prompt_seconds_total;
    let measurement = if seconds_delta > 0.0 {
        let rate = token_delta / seconds_delta;
        (rate.is_finite() && rate > 0.0).then_some(PromptRateMeasurement {
            tokens_per_second: rate,
            basis: PromptRateBasis::Interval,
        })
    } else {
        // Millisecond serialization can round a very short prompt's cumulative
        // time to the same value. llama.cpp's own gauge retains the precise
        // internal timing and is the honest fallback in that case.
        cumulative_prompt_average(current).map(|tokens_per_second| PromptRateMeasurement {
            tokens_per_second,
            basis: PromptRateBasis::ServerAverage,
        })
    };

    PromptRateUpdate {
        measurement,
        reset: false,
    }
}

fn slot_decoded_delta(previous: &HashMap<i64, SlotCounters>, slots: &[SlotInfo]) -> i64 {
    slots
        .iter()
        .filter(|slot| slot.is_processing)
        .filter_map(|slot| {
            let before = previous.get(&slot.id)?;
            let current = SlotCounters::from(slot);

            if before.task_id != current.task_id {
                // Task transition. If the decoded counter reset (current <
                // before), the new task's count is the valid delta for this
                // interval — those tokens were genuinely generated since the
                // last poll. If the counter did not reset, the raw difference
                // is the real number of new tokens. In both cases the result
                // is clamped to non-negative.
                return Some(if current.decoded_tokens < before.decoded_tokens {
                    current.decoded_tokens.max(0)
                } else {
                    (current.decoded_tokens - before.decoded_tokens).max(0)
                });
            }

            Some((current.decoded_tokens - before.decoded_tokens).max(0))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_navigation_includes_service_details_in_both_directions() {
        assert_eq!(Section::Overview.next(), Section::Service);
        assert_eq!(Section::Service.next(), Section::Throughput);
        assert_eq!(Section::Throughput.prev(), Section::Service);
        assert_eq!(Section::Service.prev(), Section::Overview);
        assert_eq!(Section::Service.name(), "Service");
        assert_eq!(Section::Slots.next(), Section::Cache);
        assert_eq!(Section::Cache.next(), Section::Gpu);
        assert_eq!(Section::Gpu.prev(), Section::Cache);
        assert_eq!(Section::Cache.prev(), Section::Slots);
        assert_eq!(Section::Cache.name(), "Cache");
    }

    #[test]
    fn direct_section_selection_resets_scroll_position() {
        let mut app = App::new("http://127.0.0.1:8080".to_string());
        app.scroll = 12;

        app.select_section(Section::Cache);

        assert_eq!(app.current_section, Section::Cache);
        assert_eq!(app.scroll, 0);
    }

    fn cache_slot(
        id: i64,
        task_id: i64,
        reused_tokens: i64,
        evaluated_tokens: i64,
        output_tokens: i64,
    ) -> SlotInfo {
        SlotInfo {
            id,
            task_id: Some(task_id),
            context_capacity: 4096,
            is_processing: true,
            context_tokens: reused_tokens + evaluated_tokens + output_tokens,
            prompt_tokens_cached: reused_tokens,
            prompt_tokens_processed: evaluated_tokens,
            decoded_tokens: output_tokens,
            ..SlotInfo::default()
        }
    }

    #[test]
    fn cache_observations_update_a_task_without_double_counting_polls() {
        let mut app = App::new("http://127.0.0.1:8080".to_string());
        let now = Instant::now();

        app.observe_cache_slots(&[cache_slot(0, 7, 80, 20, 0)], now);
        app.observe_cache_slots(
            &[cache_slot(0, 7, 80, 40, 5)],
            now + Duration::from_millis(500),
        );

        let totals = app.cache_observed_totals();
        assert_eq!(totals.requests, 1);
        assert_eq!(totals.reused_tokens, 80);
        assert_eq!(totals.evaluated_tokens, 40);
        assert_eq!(totals.input_tokens(), 120);
        assert!((totals.reuse_percent() - 66.666).abs() < 0.01);
        assert!(app.last_cache_request().is_none());
    }

    #[test]
    fn cache_observations_finalize_task_transitions_and_idle_slots() {
        let mut app = App::new("http://127.0.0.1:8080".to_string());
        let now = Instant::now();

        app.observe_cache_slots(&[cache_slot(0, 7, 80, 40, 5)], now);
        app.observe_cache_slots(&[cache_slot(0, 8, 10, 90, 2)], now + Duration::from_secs(1));

        let totals = app.cache_observed_totals();
        assert_eq!(totals.requests, 2);
        assert_eq!(totals.reused_tokens, 90);
        assert_eq!(totals.evaluated_tokens, 130);
        assert_eq!(
            app.last_cache_request().and_then(|last| last.task_id),
            Some(7)
        );

        app.observe_cache_slots(&[], now + Duration::from_secs(2));
        let last = app.last_cache_request().expect("last observed request");
        assert_eq!(last.task_id, Some(8));
        assert_eq!(last.input_tokens(), 100);
        assert_eq!(app.cache_observed_totals().requests, 2);
    }

    #[test]
    fn cache_counter_regression_starts_a_new_observation_even_if_task_id_repeats() {
        let mut app = App::new("http://127.0.0.1:8080".to_string());
        let now = Instant::now();

        app.observe_cache_slots(&[cache_slot(0, 7, 80, 40, 5)], now);
        app.observe_cache_slots(&[cache_slot(0, 7, 5, 15, 0)], now + Duration::from_secs(1));

        let totals = app.cache_observed_totals();
        assert_eq!(totals.requests, 2);
        assert_eq!(totals.input_tokens(), 140);
    }

    fn slot(id: i64, task_id: i64, decoded_tokens: i64) -> SlotInfo {
        SlotInfo {
            id,
            task_id: Some(task_id),
            is_processing: true,
            decoded_tokens,
            ..SlotInfo::default()
        }
    }

    #[test]
    fn generation_slot_resets_do_not_cancel_progress_in_other_slots() {
        let previous = HashMap::from([
            (0, SlotCounters::from(&slot(0, 7, 20))),
            (1, SlotCounters::from(&slot(1, 8, 40))),
        ]);
        let current = vec![slot(0, 9, 1), slot(1, 8, 46)];

        // Slot 0: task changed 7->9, counter reset 20->1, delta = 1.
        // Slot 1: same task 8, 40->46, delta = 6.
        assert_eq!(slot_decoded_delta(&previous, &current), 7);
    }

    #[test]
    fn generation_changed_task_with_counter_reset_uses_current_count() {
        let previous = HashMap::from([(0, SlotCounters::from(&slot(0, 7, 20)))]);
        let current = vec![slot(0, 8, 5)];

        // Task changed, counter reset to 5 — those 5 tokens were generated
        // since the last poll on the new task.
        assert_eq!(slot_decoded_delta(&previous, &current), 5);
    }

    #[test]
    fn generation_changed_task_without_counter_reset_uses_difference() {
        let previous = HashMap::from([(0, SlotCounters::from(&slot(0, 7, 20)))]);
        let current = vec![slot(0, 8, 22)];

        // Task changed, counter continued 20->22, delta = 2.
        assert_eq!(slot_decoded_delta(&previous, &current), 2);
    }

    fn prompt_metrics(tokens: f64, seconds: f64, server_average: f64) -> Metrics {
        Metrics {
            prompt_tokens_total: tokens,
            prompt_seconds_total: seconds,
            prompt_tokens_seconds: server_average,
            ..Metrics::default()
        }
    }

    #[test]
    fn prompt_rate_uses_server_processing_time_not_poll_time() {
        let previous = prompt_metrics(1_000.0, 2.0, 500.0);
        let current = prompt_metrics(1_300.0, 2.75, 472.7);

        assert_eq!(
            measured_prompt_rate(&current, Some(&previous)),
            PromptRateUpdate {
                measurement: Some(PromptRateMeasurement {
                    tokens_per_second: 400.0,
                    basis: PromptRateBasis::Interval,
                }),
                reset: false,
            }
        );
    }

    #[test]
    fn idle_scrapes_do_not_invent_prompt_measurements() {
        let metrics = prompt_metrics(1_300.0, 2.75, 472.7);

        assert_eq!(
            measured_prompt_rate(&metrics, Some(&metrics)),
            PromptRateUpdate::default()
        );
    }

    #[test]
    fn prompt_chart_advances_with_zero_during_idle_polls() {
        let metrics = prompt_metrics(1_300.0, 2.75, 472.7);
        let idle_update = measured_prompt_rate(&metrics, Some(&metrics));
        let mut history = VecDeque::from([400.0]);

        App::push_history(&mut history, prompt_chart_sample(idle_update, true));

        assert_eq!(history, VecDeque::from([400.0, 0.0]));
    }

    #[test]
    fn initial_server_average_is_a_headline_not_current_activity() {
        let current = prompt_metrics(1_300.0, 2.75, 472.7);
        let initial_update = measured_prompt_rate(&current, None);

        assert_eq!(prompt_chart_sample(initial_update, false), 0.0);
        assert_eq!(
            initial_update.measurement,
            Some(PromptRateMeasurement {
                tokens_per_second: 472.7,
                basis: PromptRateBasis::ServerAverage,
            })
        );
    }

    #[test]
    fn first_scrape_uses_llama_server_average_when_history_exists() {
        let current = prompt_metrics(1_300.0, 2.75, 472.7);

        assert_eq!(
            measured_prompt_rate(&current, None),
            PromptRateUpdate {
                measurement: Some(PromptRateMeasurement {
                    tokens_per_second: 472.7,
                    basis: PromptRateBasis::ServerAverage,
                }),
                reset: false,
            }
        );
    }

    #[test]
    fn sub_millisecond_counter_rounding_uses_server_gauge() {
        let previous = prompt_metrics(1_000.0, 2.0, 500.0);
        let current = prompt_metrics(1_001.0, 2.0, 625.0);

        assert_eq!(
            measured_prompt_rate(&current, Some(&previous)),
            PromptRateUpdate {
                measurement: Some(PromptRateMeasurement {
                    tokens_per_second: 625.0,
                    basis: PromptRateBasis::ServerAverage,
                }),
                reset: false,
            }
        );
    }

    #[test]
    fn server_counter_reset_clears_the_previous_prompt_session() {
        let previous = prompt_metrics(1_000.0, 2.0, 500.0);
        let current = prompt_metrics(0.0, 0.0, 0.0);

        assert_eq!(
            measured_prompt_rate(&current, Some(&previous)),
            PromptRateUpdate {
                measurement: None,
                reset: true,
            }
        );
    }

    #[test]
    fn malformed_prompt_counters_are_ignored() {
        let previous = prompt_metrics(1_000.0, 2.0, 500.0);
        let current = prompt_metrics(f64::NAN, 3.0, 500.0);

        assert_eq!(
            measured_prompt_rate(&current, Some(&previous)),
            PromptRateUpdate::default()
        );
    }

    #[test]
    fn the_initial_poll_is_not_throttled() {
        let app = App::new("http://localhost:8080".to_string());

        assert_eq!(app.poll_wait(), Duration::ZERO);
    }

    #[test]
    fn one_deadline_skips_missed_ticks_without_catch_up_bursts() {
        let started = Instant::now();
        let finished = started + Duration::from_millis(2_500);

        assert_eq!(
            next_poll_deadline(started, finished, Duration::from_secs(1)),
            started + Duration::from_secs(3)
        );
    }

    #[test]
    fn update_interval_labels_stay_compact() {
        let milliseconds = App::with_update_ms("http://localhost:8080".to_string(), 750);
        let seconds = App::with_update_ms("http://localhost:8080".to_string(), 2_000);
        let fractional = App::with_update_ms("http://localhost:8080".to_string(), 1_500);
        let day = App::with_update_ms("http://localhost:8080".to_string(), MAX_UPDATE_MS);

        assert_eq!(milliseconds.update_interval_label(), "750ms");
        assert_eq!(seconds.update_interval_label(), "2s");
        assert_eq!(fractional.update_interval_label(), "1.5s");
        assert_eq!(day.update_interval_label(), "24h");
    }

    #[test]
    fn poll_does_not_block_on_a_slow_server() {
        use std::io::Read;
        use std::time::Duration as D;

        // A server thread that takes 500ms to answer every request, so any
        // blocking collection would measurably stall the UI thread.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let _ = stream.try_clone().map(|mut c| {
                        let mut buf = [0u8; 512];
                        let _ = c.read(&mut buf);
                    });
                    std::thread::sleep(D::from_millis(500));
                    let _ = write_ok(stream);
                });
            }
        });

        let mut app = App::with_update_ms(format!("http://127.0.0.1:{port}"), 100);
        let _handle = app.start_collection();
        // Let the collector begin its first (slow) fetch in the background.
        std::thread::sleep(D::from_millis(30));

        let start = std::time::Instant::now();
        for _ in 0..10 {
            app.poll();
        }
        let elapsed = start.elapsed();
        app.stop_collection();
        drop(_handle);

        assert!(
            elapsed < D::from_millis(200),
            "poll() blocked on the server for {elapsed:?}"
        );

        fn write_ok(mut stream: std::net::TcpStream) -> std::io::Result<()> {
            use std::io::Write as _;
            let body = br#"{"error":{"message":"x"}}"#;
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: ")?;
            stream.write_all(body.len().to_string().as_bytes())?;
            stream.write_all(b"\r\n\r\n")?;
            stream.write_all(body)
        }
    }

    #[test]
    fn cancelling_the_theme_picker_restores_the_applied_theme() {
        let mut app = App::new("http://localhost:8080".to_string());
        let original = app.theme.name().to_string();

        app.open_theme_picker();
        app.preview_next_theme();
        app.toggle_theme_background();
        assert_ne!(app.theme.name(), original);
        assert!(!app.theme_background);
        app.cancel_theme_picker();

        assert_eq!(app.theme.name(), original);
        assert!(app.theme_background);
        assert!(!app.show_theme_picker);
    }
}
