# The sidecar — Kokoro behind the boundary

hearit talks to its voice model the way sayit talks to whisper: a child
process on localhost, spawned at boot, warmed before first use, killed at
exit. Never linked against. The model may stay a black box; the plumbing
may not.

## The pick: kokoros (`koko.exe`)

[kokoros](https://github.com/lucasjinreal/Kokoros) — a Rust
implementation of Kokoro TTS with an OpenAI-compatible server mode. It is
the closest match to the whisper-server shape sayit already trusts: one
exe, one port, HTTP in, audio out.

No prebuilt Windows binaries exist, so it is built from source — a key is
bought once with effort. The clone lives at `E:\CODE\Kokoros` (pinned:
commit `b54354b`), built with `cargo build --release`, and the resulting
`koko.exe` is copied into `sidecar\`.

Verified against that commit's source, not memory:

- CLI: `koko -m <model.onnx> -d <voices.bin> openai --ip 127.0.0.1
  --port 8880` — model/voices are **global flags before the subcommand**;
  `--ip`/`--port` belong to `openai`. Defaults: port 3000, voice
  `af_sky`, format `mp3` — hearit overrides all three per request.
- Endpoint: `POST /v1/audio/speech` with
  `{model, input, voice, response_format: "pcm", speed}`.
- `"pcm"` returns raw 16-bit little-endian mono at **24kHz** — no header,
  no container, exactly what `synth.rs` decodes. (`wav` would be 32-bit
  float with a header; we don't want it.)
- `stream: true` exists and forces PCM over chunked HTTP in ~10-word
  windows — that is the intra-sentence streaming lever if the dead-air
  log ever demands it. v1 doesn't use it; the sentence is our unit.
- `GET /v1/audio/voices` lists voices; `GET /` is a health check.

Fallback if kokoros disappoints:
[Kokoro-FastAPI](https://github.com/remsky/Kokoro-FastAPI) — same
OpenAI-compatible surface, Python instead of Rust, heavier to ship but
battle-tested. `synth.rs` would not change; only paths.rs and the spawn
args in sidecar.rs would.

## Layout

paths.rs finds all three companions (env override → next to hearit.exe →
repo walk-up) and passes the paths explicitly — nothing depends on the
working directory:

```
sidecar\
  koko.exe                          (HEARIT_SIDECAR overrides)
  checkpoints\kokoro-v1.0.onnx      (HEARIT_MODEL overrides)
  data\voices-v1.0.bin              (HEARIT_VOICES overrides)
```

An installed hearit (e.g. `E:\hearit\`) can share the repo's sidecar
without copying 2GB — a junction costs nothing and needs no admin:

```
New-Item -ItemType Junction -Path E:\hearit\sidecar -Target E:\CODE\hearit\sidecar
```

If the sidecar can't be found, hearit still boots: the tray tooltip says
so, and the next press retries. (It didn't always — installed v0.1.0
died silently at setup. The update check now runs before the sidecar
start for the same reason: a broken sidecar must never block the update
that fixes it.)

When something goes wrong in an installed instance, the story survives:
every engine lifecycle event (boot, spawn with pid, warm, sleep, death
with exit status, update install) is appended to `engine.log` in the app
config dir, next to `dead-air-log.csv`, and koko's stderr from the most
recent spawn is kept as `koko-stderr.log`. An installer-launched hearit
has no console — the 2026-08-04 wake failure taught us that a tooltip
and a silent pill are not a diagnosis.

Model sources (what `download_all.sh` in the kokoros repo uses):

- model: `https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX-timestamped/resolve/main/onnx/model.onnx` (~310MB)
- voices: `https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin` (~27MB)

## CPU first, GPU when the log says so

The log spoke on day one. Measured on this machine (2026-08-01, release
build, warm server, `af_heart`):

| request | result |
|---|---|
| short sentence (2.0s audio), non-streaming | ~1.8–2.5s |
| 3-sentence paragraph (11.4s audio), `stream:true`, time-to-first-chunk | ~2.8s |
| same paragraph, total wall time | 6.6s (windows pipeline in parallel) |

This CPU synthesizes at roughly **1× realtime** — the "faster than
realtime on a modern CPU" claim did not survive contact with this CPU.
Dead air ≈ grab (~0.1–0.2s) + first-sentence synth (≈ audio length of
that sentence), so a typical press lands at 2–4s: over the one-second
budget, every time.

**The CUDA lever, pulled and measured the same day** (RTX 2070, driver
595.97, `--features kokoros/cuda` build, warm server):

| request | CPU | CUDA |
|---|---|---|
| short sentence (2.0s audio) | ~1.8–2.5s | **~0.65s** |
| 3-sentence paragraph (11s audio) | ~6.6s | **~1.9s** |

≈3× realtime on short sentences, ≈6× on paragraphs (per-request overhead
amortizes). Dead air ≈ grab (~0.1–0.2s) + first-sentence synth → **under
the one-second budget for typical sentences.** The "39 Memcpy nodes"
warning at load is expected: a few ops fall back to CPU per inference.

The CUDA runtime travels with the sidecar as plain DLLs — no system
CUDA install, same trick whisper-cublas uses (and three of the DLLs are
literally copied from it):

```
sidecar\
  koko.exe                      built with --features kokoros/cuda
  onnxruntime_providers_cuda.dll + _shared.dll   (from the koko build)
  cudart64_12.dll, cublas64_12.dll, cublasLt64_12.dll   (from whisper-cublas)
  cudnn*.dll (10 files)         cuDNN 9.25 cuda12 redist zip
  cufft64_11.dll                libcufft 11.3.3.83 redist zip
```

(~2GB total on disk. NVIDIA redist zips: developer.download.nvidia.com/
compute/{cudnn,cuda}/redist/ — plain archives, no installer, no account.)

Remaining levers if a long first sentence ever breaches the budget:
smaller first chunk (synthesize the first clause before the first full
sentence), or `stream:true` (server-side ~10-word windows, chunked PCM).

## Warmup

`sidecar.rs` probes with one throwaway synthesis per second until the
first success, then declares `sidecar_ready`. ONNX session creation and
first-inference costs are paid there, at boot, so the first real press of
the day speaks as fast as the hundredth.

## The engine has a metabolism (sayit's, ported)

After `idle_minutes` of idle (settings.json, default 5, 0 = never) the
coordinator kills koko, freeing ~2GB of working set — the CUDA runtime
is most of it. A press from sleep still works instantly on the grammar
side (grab needs no engine) while `engine_start` wakes it and
`synth_waiting`'s patience absorbs the warmup (seconds). The tray's
"Free VRAM" is the manual version of the same path. The key always
works; the user manages nothing.

## The engine cannot outlive hearit (added 2026-08-02)

Ported from sayit the day idle sidecars starved a nightly Ollama job
into 5-minute timeouts (WDDM paged the VRAM overflow to system RAM;
prompt processing fell ~250×, invisibly). Three layers:

- every spawned koko is assigned to a Windows **job object** with
  `KILL_ON_JOB_CLOSE` — if hearit dies for ANY reason (crash, Task
  Manager, dev-run teardown), the OS kills the engine with it;
- at boot, hearit **reaps stale** koko processes whose image path
  matches its own resolved sidecar exe (orphans from pre-job-object
  builds). sayit's whisper-server proved a stale sidecar can bind its
  port alongside a fresh one instead of failing, so nothing visibly
  breaks — both just squat VRAM;
- a named mutex makes hearit **single-instance**, so two app launches
  (installed autostart + a dev run) can't spawn two engines.

Verify: leave hearit idle past `idle_minutes`, then
`nvidia-smi --query-gpu=memory.used --format=csv` — hearit should
contribute ~0 MiB (and `tasklist | findstr koko` nothing).
