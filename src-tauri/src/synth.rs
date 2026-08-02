//! Stage 3: the voice. Talks to the Kokoro sidecar over localhost HTTP —
//! an OpenAI-compatible speech endpoint that answers with raw PCM. Talked
//! to, never linked against: the model may stay a black box; the plumbing
//! may not (north star, unchanged from sayit).

use std::time::{Duration, Instant};

/// Where the sidecar listens. sidecar.rs passes this on the command line.
pub const SIDECAR_PORT: u16 = 8880;

/// Kokoro's native output rate. speak.rs's playback and the pill's band
/// math both assume this.
pub const SAMPLE_RATE: u32 = 24_000;

/// One voice to start, chosen carefully; a small shelf someday. Speed
/// comes first (north star).
pub const VOICE: &str = "af_heart";

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthTiming {
    /// Time spent waiting for a warm engine, excluded from http_ms.
    pub engine_wait_ms: u64,
    pub attempts: u32,
    /// The successful request: synthesis + the localhost round-trip.
    pub http_ms: u64,
    /// s16le bytes → f32 samples.
    pub decode_ms: u64,
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

async fn synth_once(text: &str) -> Result<(Vec<f32>, u64, u64), String> {
    let t = Instant::now();
    let resp = client()
        .post(format!(
            "http://127.0.0.1:{SIDECAR_PORT}/v1/audio/speech"
        ))
        .json(&serde_json::json!({
            "model": "kokoro",
            "input": text,
            "voice": VOICE,
            "response_format": "pcm",
            "speed": 1.0,
        }))
        .send()
        .await
        .map_err(|e| format!("sidecar unreachable: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("sidecar answered {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let http_ms = t.elapsed().as_millis() as u64;

    // "pcm" means raw 16-bit little-endian mono at 24kHz — no header to
    // parse, which is exactly why we ask for it.
    let t = Instant::now();
    let samples: Vec<f32> = bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect();
    let decode_ms = t.elapsed().as_millis() as u64;
    if samples.is_empty() {
        return Err("sidecar returned no audio".into());
    }
    Ok((samples, http_ms, decode_ms))
}

/// Patient synth, mirroring sayit's transcribe_waiting: if the press raced
/// the engine's warmup, wait for warmth instead of failing the take.
pub async fn synth_waiting(
    text: &str,
    patience: Duration,
) -> Result<(Vec<f32>, SynthTiming), String> {
    let start = Instant::now();
    let mut attempts = 0u32;
    loop {
        attempts += 1;
        match synth_once(text).await {
            Ok((samples, http_ms, decode_ms)) => {
                return Ok((
                    samples,
                    SynthTiming {
                        engine_wait_ms: (start.elapsed().as_millis() as u64)
                            .saturating_sub(http_ms + decode_ms),
                        attempts,
                        http_ms,
                        decode_ms,
                    },
                ));
            }
            Err(_) if start.elapsed() < patience => {
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Warmup probe for sidecar.rs: success means "warm and listening". The
/// audio is discarded — this is a knock on the door, not a take.
pub async fn probe() -> Result<(), String> {
    synth_once("Ready.").await.map(|_| ())
}
