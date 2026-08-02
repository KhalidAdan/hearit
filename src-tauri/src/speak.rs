//! Stage 4: the voice out loud. One thread owns the audio device for the
//! app's lifetime (same shape as sayit's sounds.rs, same reason: rodio's
//! OutputStream can't leave the thread that created it). Everyone else
//! holds an Arc<Sink> — append and stop are thread-safe, so "silence on
//! demand" is one native call, never an IPC round-trip.
//!
//! The pill's honesty also lives here: every sample bound for the sink
//! passes through a Tap that keeps the most recent slice in a ring buffer.
//! A monitor thread runs a small FFT over that ring ~30 times a second and
//! emits 32 quantized band heights — the pill draws what is actually
//! leaving the speaker, not an animation that looks alive.

use rustfft::{num_complex::Complex, FftPlanner};
use std::collections::VecDeque;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::synth;

/// The pill's grid — must match ROWS/BANDS in src/pill.ts.
const BANDS: usize = 32;
const ROWS: u8 = 6;

/// Log-spaced band edges, tuned for speech (see the spectrum-grid handoff
/// notes): energy concentrates low, and Kokoro emits little of interest
/// above 5kHz. Linear bins would leave the top of the display dark.
const MIN_HZ: f32 = 80.0;
const MAX_HZ: f32 = 5000.0;

/// FFT window: 1024 samples at 24kHz ≈ 43ms of audio per frame.
const FFT_N: usize = 1024;

/// ~30Hz emit rate. Anything faster is invisible at 6 rows of resolution.
const FRAME: Duration = Duration::from_millis(33);

/// Exponential smoothing between frames — the AnalyserNode default the
/// prototypes were tuned against, reimplemented honestly.
const SMOOTHING: f32 = 0.75;

/// Hysteresis as a fraction of one row: a band parked on a cell boundary
/// would otherwise strobe, and at 6 rows that's a sixth of the display.
const HYSTERESIS: f32 = 0.12;

/// The dB window mapped onto the rows. AnalyserNode maps [-100,-30]dB;
/// our synth output is hotter and cleaner, so the window sits higher.
/// Tune by ear against real speech, not by theory.
const DB_FLOOR: f32 = -55.0;
const DB_CEIL: f32 = -10.0;

/// Consecutive empty frames before the monitor declares playback done —
/// ~330ms of grace, so a between-sentences underrun doesn't flicker the
/// pill or end the take early.
const DONE_FRAMES: u32 = 10;

/// Tap bookkeeping: samples are pushed to the ring in batches so the audio
/// callback never takes the lock per-sample, and never *waits* on it at all.
const TAP_BATCH: usize = 256;
const RING_CAP: usize = 4096;

pub struct Speaker {
    sink: Arc<rodio::Sink>,
    ring: Arc<Mutex<VecDeque<f32>>>,
}

impl Speaker {
    /// Queue one synthesized sentence. Returns true if this append started
    /// playback (the sink was empty) so the caller can stamp first-audio.
    pub fn enqueue(&self, samples: Vec<f32>) -> bool {
        let was_empty = self.sink.empty();
        self.sink.append(Tap {
            samples,
            pos: 0,
            pending: Vec::with_capacity(TAP_BATCH),
            ring: self.ring.clone(),
        });
        // stop() leaves some rodio versions paused; play() is idempotent.
        self.sink.play();
        was_empty
    }

    /// Silence on demand: clears everything queued, instantly.
    pub fn stop(&self) {
        self.sink.stop();
        self.sink.play();
        self.ring.lock().unwrap().clear();
    }
}

/// Opens the audio device on its own thread and starts the FFT monitor.
/// hearit without a speaker is not degraded, it is pointless — so unlike
/// sayit's soundpack, failure here is an error, not silence.
pub fn start(app: &AppHandle) -> Result<Speaker, String> {
    let (tx, rx) = channel();
    std::thread::spawn(move || match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => match rodio::Sink::try_new(&handle) {
            Ok(sink) => {
                let _ = tx.send(Ok(Arc::new(sink)));
                // Park forever: the OutputStream dies with this thread,
                // and this thread dies with the process.
                let _stream = stream;
                loop {
                    std::thread::park();
                }
            }
            Err(e) => {
                let _ = tx.send(Err(format!("audio sink failed: {e}")));
            }
        },
        Err(e) => {
            let _ = tx.send(Err(format!("no audio output device: {e}")));
        }
    });
    let sink = rx
        .recv()
        .map_err(|_| "audio thread died before answering".to_string())??;
    let ring = Arc::new(Mutex::new(VecDeque::with_capacity(RING_CAP)));
    monitor(app.clone(), sink.clone(), ring.clone());
    Ok(Speaker { sink, ring })
}

/// A pass-through Source that copies what it plays into the ring buffer.
/// The audio path is untouched: same samples in, same samples out.
struct Tap {
    samples: Vec<f32>,
    pos: usize,
    pending: Vec<f32>,
    ring: Arc<Mutex<VecDeque<f32>>>,
}

impl Tap {
    fn flush(&mut self) {
        // try_lock: the audio callback must NEVER wait on the monitor
        // thread. If the lock is busy we keep accumulating; the ring is a
        // visualization aid, not the audio path.
        if let Ok(mut ring) = self.ring.try_lock() {
            for &s in &self.pending {
                if ring.len() >= RING_CAP {
                    ring.pop_front();
                }
                ring.push_back(s);
            }
            self.pending.clear();
        } else if self.pending.len() > RING_CAP {
            self.pending.clear(); // monitor wedged? drop, never grow forever
        }
    }
}

impl Iterator for Tap {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = *self.samples.get(self.pos)?;
        self.pos += 1;
        self.pending.push(s);
        if self.pending.len() >= TAP_BATCH {
            self.flush();
        }
        Some(s)
    }
}

impl rodio::Source for Tap {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        synth::SAMPLE_RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.samples.len() as f64 / synth::SAMPLE_RATE as f64,
        ))
    }
}

/// The pill's data feed. Runs for the app's lifetime; idles at one cheap
/// `sink.empty()` check per frame when nothing is playing.
fn monitor(app: AppHandle, sink: Arc<rodio::Sink>, ring: Arc<Mutex<VecDeque<f32>>>) {
    std::thread::spawn(move || {
        let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_N);
        // Hann window, precomputed: tapers the frame's edges so the FFT
        // sees a tone, not the click of a rectangular cut.
        let hann: Vec<f32> = (0..FFT_N)
            .map(|i| 0.5 * (1.0 - (std::f32::consts::TAU * i as f32 / FFT_N as f32).cos()))
            .collect();
        let bands = band_map();
        let mut levels = [0f32; BANDS]; // smoothed 0..1
        let mut heights = [0u8; BANDS]; // quantized, with hysteresis
        let mut buf = vec![Complex::default(); FFT_N];
        let mut speaking = false;
        let mut empty_frames = 0u32;

        loop {
            std::thread::sleep(FRAME);
            if sink.empty() {
                if speaking {
                    empty_frames += 1;
                    if empty_frames >= DONE_FRAMES {
                        speaking = false;
                        levels = [0.0; BANDS];
                        heights = [0u8; BANDS];
                        let _ = app.emit("viz_heights", heights.to_vec());
                        let _ = app.emit("playback_done", ());
                    }
                }
                continue;
            }
            speaking = true;
            empty_frames = 0;

            // Snapshot the newest FFT_N samples; an underfull ring is
            // padded with leading silence.
            {
                let r = ring.lock().unwrap();
                let n = r.len().min(FFT_N);
                let pad = FFT_N - n;
                let skip = r.len() - n;
                for i in 0..FFT_N {
                    let s = if i < pad { 0.0 } else { r[skip + i - pad] };
                    buf[i] = Complex { re: s * hann[i], im: 0.0 };
                }
            }
            fft.process(&mut buf);

            for (b, &(lo, hi)) in bands.iter().enumerate() {
                // MAX across the band's bins, not mean — six rows have no
                // headroom to average consonants away (handoff notes).
                let mut peak = 0f32;
                for bin in lo..=hi {
                    let m = buf[bin].norm() / (FFT_N as f32 / 2.0);
                    if m > peak {
                        peak = m;
                    }
                }
                let db = 20.0 * peak.max(1e-9).log10();
                let raw = ((db - DB_FLOOR) / (DB_CEIL - DB_FLOOR)).clamp(0.0, 1.0);
                levels[b] = SMOOTHING * levels[b] + (1.0 - SMOOTHING) * raw;
                heights[b] = quantize(levels[b], heights[b]);
            }
            let _ = app.emit("viz_heights", heights.to_vec());
        }
    });
}

/// Log-spaced FFT-bin ranges for each of the 32 bands.
fn band_map() -> [(usize, usize); BANDS] {
    let nyquist = synth::SAMPLE_RATE as f32 / 2.0;
    let bins = FFT_N / 2;
    let ratio = MAX_HZ / MIN_HZ;
    let mut map = [(0usize, 0usize); BANDS];
    for (i, slot) in map.iter_mut().enumerate() {
        let lo_hz = MIN_HZ * ratio.powf(i as f32 / BANDS as f32);
        let hi_hz = MIN_HZ * ratio.powf((i + 1) as f32 / BANDS as f32);
        let lo = (((lo_hz / nyquist) * bins as f32).floor() as usize).min(bins - 1);
        let hi = ((((hi_hz / nyquist) * bins as f32).ceil() as usize).max(lo)).min(bins - 1);
        *slot = (lo, hi);
    }
    map
}

/// Quantize a 0..1 level onto the rows, refusing to move off the previous
/// row unless the value clears the boundary by the hysteresis margin.
fn quantize(level: f32, prev: u8) -> u8 {
    let scaled = level * ROWS as f32;
    let mut row = scaled.round() as i32;
    let prev_i = prev as i32;
    let prev_f = prev as f32;
    if row > prev_i && scaled < prev_f + 0.5 + HYSTERESIS {
        row = prev_i;
    }
    if row < prev_i && scaled > prev_f - 0.5 - HYSTERESIS {
        row = prev_i;
    }
    row.clamp(0, ROWS as i32) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_map_is_monotonic_and_in_range() {
        let map = band_map();
        let bins = FFT_N / 2;
        for i in 0..BANDS {
            let (lo, hi) = map[i];
            assert!(lo <= hi, "band {i}: lo {lo} > hi {hi}");
            assert!(hi < bins, "band {i}: hi {hi} out of range");
            if i > 0 {
                assert!(map[i - 1].0 <= lo, "band {i} starts before band {}", i - 1);
            }
        }
    }

    #[test]
    fn quantize_commits_through_hysteresis() {
        // A level parked exactly on a boundary must not strobe: from row 3,
        // 3.5+ε stays at 3 until it clears the margin.
        assert_eq!(quantize(3.55 / ROWS as f32, 3), 3);
        assert_eq!(quantize(3.75 / ROWS as f32, 3), 4);
    }

    #[test]
    fn quantize_clamps_to_grid() {
        assert_eq!(quantize(2.0, 0), ROWS); // over-hot input can't overflow
        assert_eq!(quantize(0.0, 0), 0);
    }

    #[test]
    fn tap_passes_samples_through_untouched() {
        let ring = Arc::new(Mutex::new(VecDeque::new()));
        let samples = vec![0.1f32, -0.2, 0.3];
        let tap = Tap {
            samples: samples.clone(),
            pos: 0,
            pending: Vec::new(),
            ring,
        };
        let out: Vec<f32> = tap.collect();
        assert_eq!(out, samples);
    }
}
