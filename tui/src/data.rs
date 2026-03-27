use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use dmxp_mpmc::Core::alloc::SharedMemoryAllocator;

/// How many history samples to keep per channel for sparkline graphs.
pub const HISTORY_LEN: usize = 120;

/// A point-in-time snapshot of a single channel.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChannelSnapshot {
    pub id: u32,
    /// Total slot capacity of the ring buffer.
    pub capacity: u64,
    /// Number of slots currently occupied (tail - head).
    pub fill: u64,
    /// Fill percentage 0.0 – 100.0.
    pub fill_pct: f64,
    /// Messages produced per second (delta tail / delta time).
    pub msgs_per_sec: f64,
    /// Whether any messages in the last sample were overflowed to SFB.
    pub has_overflow: bool,
}

/// A point-in-time snapshot of the SFB (Stable Fragmented Buffer).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SfbSnapshot {
    pub active_pages: usize,
    pub active_capacity_bytes: u64,
    pub active_data_bytes: u64,
    pub free_space_bytes: u64,
    /// Fragmentation ratio 0.0 – 1.0.
    pub fragmentation_ratio: f64,
    pub total_appends: u64,
    pub appends_per_sec: f64,
}

/// Full state snapshot used by the UI on each render tick.
#[derive(Debug, Clone)]
pub struct AppSnapshot {
    pub channels: Vec<ChannelSnapshot>,
    pub sfb: SfbSnapshot,
    /// Rolling throughput history per channel (indexed by position in channels vec).
    pub throughput_history: Vec<VecDeque<f64>>,
    /// Total shared memory size in bytes.
    pub shm_total_bytes: usize,
    /// Total shared memory used in bytes.
    pub shm_used_bytes: usize,
}

/// Internal per-channel state for computing deltas between polls.
struct ChannelPrev {
    tail: u64,
    instant: Instant,
}

/// Internal SFB state for computing deltas between polls.
struct SfbPrev {
    total_appends: u64,
    instant: Instant,
}

/// Background poller. Runs in its own thread and deposits snapshots into a
/// shared `Mutex<AppSnapshot>` that the UI reads on each render tick.
pub struct Poller {
    snapshot: Arc<Mutex<AppSnapshot>>,
}

impl Poller {
    pub fn new(
        allocator: Arc<SharedMemoryAllocator>,
        interval: Duration,
    ) -> Self {
        let snapshot = Arc::new(Mutex::new(AppSnapshot {
            channels: vec![],
            sfb: SfbSnapshot {
                active_pages: 0,
                active_capacity_bytes: 0,
                active_data_bytes: 0,
                free_space_bytes: 0,
                fragmentation_ratio: 0.0,
                total_appends: 0,
                appends_per_sec: 0.0,
            },
            throughput_history: vec![],
            shm_total_bytes: 0,
            shm_used_bytes: 0,
        }));

        let snapshot_clone = snapshot.clone();

        thread::spawn(move || {
            // Per-channel previous values for rate computation.
            let mut prev_channels: Vec<Option<ChannelPrev>> = Vec::new();
            let mut prev_sfb = SfbPrev {
                total_appends: 0,
                instant: Instant::now(),
            };
            // Throughput history indexed by channel order.
            let mut history: Vec<VecDeque<f64>> = Vec::new();

            loop {
                let channels = allocator.get_channels();
                let sfb = allocator.sfu();
                let prof = sfb.profiler().stats();

                // Grow prev/history vecs if new channels appeared.
                while prev_channels.len() < channels.len() {
                    prev_channels.push(None);
                    history.push(VecDeque::with_capacity(HISTORY_LEN));
                }

                let now = Instant::now();

                // Build per-channel snapshots.
                let channel_snaps: Vec<ChannelSnapshot> = channels
                    .iter()
                    .enumerate()
                    .map(|(i, ch)| {
                        let buf = ch.buffer();
                        let tail = buf.tail();
                        let head = buf.head();
                        let capacity = buf.capacity() as u64;
                        let fill = tail.saturating_sub(head).min(capacity);
                        let fill_pct = if capacity > 0 {
                            fill as f64 / capacity as f64 * 100.0
                        } else {
                            0.0
                        };

                        let msgs_per_sec = if let Some(prev) = &prev_channels[i] {
                            let delta_msgs = tail.saturating_sub(prev.tail) as f64;
                            let delta_secs = now.duration_since(prev.instant).as_secs_f64();
                            if delta_secs > 0.0 { delta_msgs / delta_secs } else { 0.0 }
                        } else {
                            0.0
                        };

                        prev_channels[i] = Some(ChannelPrev { tail, instant: now });

                        // Update sparkline history.
                        history[i].push_back(msgs_per_sec);
                        if history[i].len() > HISTORY_LEN {
                            history[i].pop_front();
                        }

                        ChannelSnapshot {
                            id: ch.channel_id,
                            capacity,
                            fill,
                            fill_pct,
                            msgs_per_sec,
                            has_overflow: false, // updated below via SFB appends delta
                        }
                    })
                    .collect();

                // SFB snapshot.
                let sfb_appends_per_sec = {
                    let delta = prof.total_appends.saturating_sub(prev_sfb.total_appends) as f64;
                    let secs = now.duration_since(prev_sfb.instant).as_secs_f64();
                    if secs > 0.0 { delta / secs } else { 0.0 }
                };
                prev_sfb = SfbPrev {
                    total_appends: prof.total_appends,
                    instant: now,
                };

                let sfb_snap = SfbSnapshot {
                    active_pages: prof.active_pages,
                    active_capacity_bytes: prof.active_capacity_bytes,
                    active_data_bytes: prof.active_data_bytes,
                    free_space_bytes: prof.free_space_bytes,
                    fragmentation_ratio: prof.fragmentation_ratio,
                    total_appends: prof.total_appends,
                    appends_per_sec: sfb_appends_per_sec,
                };

                // Write into shared snapshot.
                if let Ok(mut snap) = snapshot_clone.lock() {
                    snap.channels = channel_snaps;
                    snap.sfb = sfb_snap;
                    snap.throughput_history = history.iter().map(|h| h.clone()).collect();
                    snap.shm_total_bytes = allocator.used_memory() + allocator.available_memory();
                    snap.shm_used_bytes = allocator.used_memory();
                }

                thread::sleep(interval);
            }
        });

        Self { snapshot }
    }

    /// Get a clone of the latest snapshot. Cheap — only called on UI render ticks.
    pub fn latest(&self) -> AppSnapshot {
        self.snapshot.lock().unwrap().clone()
    }
}
