//! hearit — the key that speaks.
//!
//! sayit's pipeline run backwards: where sayit ends by injecting text at
//! your cursor, hearit begins by lifting text from your selection. Rust
//! owns the four pipeline stages (hotkey, grab, synthesize, play) because
//! that's where the OS is. Rust touches the OS, TS makes decisions, the
//! sidecar thinks.

mod diag;
mod grab;
mod hotkey;
mod paths;
mod settings;
mod sidecar;
mod speak;
mod synth;
mod tray;
mod update;

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// The take counter — Rust's half of cancellation. speak_begin and
/// speak_stop bump it; speak_sentence refuses to enqueue audio for any
/// take that is no longer current. This is what makes "the new selection
/// wins, instantly" true even when an old sentence is mid-synthesis: its
/// audio comes back, matches a dead token, and lands on the floor.
#[derive(Default)]
pub struct Takes(pub AtomicU64);

/// The tray's speed pick, applied to every synthesis request. A Mutex
/// around an f32 is heavier than an atomic, but it reads as what it is.
pub struct Speed(pub Mutex<f32>);

#[tauri::command]
async fn grab_selection() -> Result<grab::Grab, String> {
    // spawn_blocking: grab sleeps while it polls the clipboard, and a
    // command that blocks the async runtime would stall every other IPC.
    tauri::async_runtime::spawn_blocking(grab::grab)
        .await
        .map_err(|e| e.to_string())?
}

/// Stops whatever is speaking and opens a new take, returning its token.
#[tauri::command]
fn speak_begin(
    speaker: tauri::State<'_, speak::Speaker>,
    takes: tauri::State<'_, Takes>,
) -> u64 {
    speaker.stop();
    takes.0.fetch_add(1, Ordering::SeqCst) + 1
}

/// One sentence's journey: synthesized by the sidecar, then queued for
/// playback — unless the take died while the sidecar was thinking.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Spoken {
    engine_wait_ms: u64,
    attempts: u32,
    http_ms: u64,
    decode_ms: u64,
    /// Length of the synthesized audio itself — the denominator the other
    /// numbers should be read against.
    audio_ms: u64,
    /// False = superseded mid-synthesis; the audio was dropped, not played.
    queued: bool,
}

#[tauri::command]
async fn speak_sentence(
    app: tauri::AppHandle,
    speaker: tauri::State<'_, speak::Speaker>,
    takes: tauri::State<'_, Takes>,
    speed: tauri::State<'_, Speed>,
    text: String,
    take: u64,
) -> Result<Spoken, String> {
    let speed = *speed.0.lock().unwrap();
    // Patient: if this take raced the engine's warmup, wait for warmth.
    // 30s covers a cold first-ever start; real presses cost milliseconds.
    let (samples, t) =
        synth::synth_waiting(&text, speed, std::time::Duration::from_secs(30)).await?;
    let audio_ms = samples.len() as u64 * 1000 / synth::SAMPLE_RATE as u64;

    let queued = takes.0.load(Ordering::SeqCst) == take;
    if queued {
        let started = speaker.enqueue(samples);
        if started {
            // The sink just went from empty to fed: first audio of a take
            // (or a recovery from an underrun — the coordinator sorts it).
            let _ = app.emit("playback_started", hotkey::now_ms());
        }
    }
    println!(
        "[timing] speak_sentence: wait {}ms · synth {}ms · decode {}ms → {:.1}s of audio for {} chars{}",
        t.engine_wait_ms,
        t.http_ms,
        t.decode_ms,
        audio_ms as f32 / 1000.0,
        text.len(),
        if queued { "" } else { " (stale, dropped)" }
    );
    Ok(Spoken {
        engine_wait_ms: t.engine_wait_ms,
        attempts: t.attempts,
        http_ms: t.http_ms,
        decode_ms: t.decode_ms,
        audio_ms,
        queued,
    })
}

/// Silence on demand. Also kills the take token, so in-flight synthesis
/// can't resurrect the voice.
#[tauri::command]
fn speak_stop(
    speaker: tauri::State<'_, speak::Speaker>,
    takes: tauri::State<'_, Takes>,
) {
    takes.0.fetch_add(1, Ordering::SeqCst);
    speaker.stop();
}

#[tauri::command]
fn is_ready(ready: tauri::State<sidecar::Ready>) -> bool {
    ready.0.load(std::sync::atomic::Ordering::Relaxed)
}

#[tauri::command]
fn engine_start(app: tauri::AppHandle) -> Result<(), String> {
    sidecar::start(&app)
}

/// The coordinator's idle timer lands here: kill the engine, free the
/// VRAM. Same path as the tray's "Free VRAM"; the next press wakes it.
#[tauri::command]
fn engine_sleep(app: tauri::AppHandle) {
    sidecar::sleep(&app);
}

/// How long the engine may idle before it sleeps (settings.json
/// `idle_minutes`, default 5, 0 = never). Pulled by the coordinator at
/// boot.
#[tauri::command]
fn get_idle_minutes(app: tauri::AppHandle) -> u64 {
    settings::load(&app).idle_minutes
}

/// One take's dead-air breakdown, assembled by the coordinator (the only
/// place all three clocks meet: key stamp, Rust stage timings, playback
/// stamp). All milliseconds.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GapRow {
    /// Key press → first audible sample. THE number (north star).
    dead_air_ms: u64,
    chars: usize,
    sentences: usize,
    grab_ms: u64,
    engine_wait_ms: u64,
    first_synth_ms: u64,
    /// Length of the first sentence's audio — context, not latency.
    first_audio_ms: u64,
}

const GAP_HEADER: &str =
    "unix_ts,dead_air_ms,chars,sentences,grab_ms,engine_wait_ms,first_synth_ms,first_audio_ms";

/// Dead air, measured, not vibed — appended per take to a CSV in the app
/// config dir so tuning has a dataset. Same discipline as sayit's gap log:
/// a file with a foreign header gets shelved, never appended to.
#[tauri::command]
fn log_gap(app: tauri::AppHandle, row: GapRow) {
    println!(
        "[hearit] dead air: {}ms for {} chars in {} sentence(s)",
        row.dead_air_ms, row.chars, row.sentences
    );
    let Ok(dir) = app.path().app_config_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let csv = dir.join("dead-air-log.csv");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if let Ok(existing) = std::fs::File::open(&csv) {
        use std::io::BufRead;
        let first = std::io::BufReader::new(existing)
            .lines()
            .next()
            .and_then(|l| l.ok())
            .unwrap_or_default();
        if first.trim() != GAP_HEADER {
            let shelf = dir.join(format!("dead-air-log-old-{stamp}.csv"));
            match std::fs::rename(&csv, &shelf) {
                Ok(()) => println!("[hearit] dead-air-log: old schema shelved"),
                Err(e) => {
                    eprintln!("[hearit] dead-air-log: can't shelve old log ({e}); skipping row");
                    return;
                }
            }
        }
    }

    let new = !csv.exists();
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(csv) {
        if new {
            let _ = writeln!(file, "{GAP_HEADER}");
        }
        let _ = writeln!(
            file,
            "{stamp},{},{},{},{},{},{},{}",
            row.dead_air_ms,
            row.chars,
            row.sentences,
            row.grab_ms,
            row.engine_wait_ms,
            row.first_synth_ms,
            row.first_audio_ms
        );
    }
}

/// The pill appears bottom-center while speaking — same spot, same math
/// as sayit's waveform. focusable:false in config: it must NEVER take
/// focus, or the app the user is reading loses its selection. Show/hide
/// live on the Rust side so the webview needs no window capabilities.
#[tauri::command]
fn pill_show(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("pill") {
        if let (Ok(Some(monitor)), Ok(size)) = (w.primary_monitor(), w.outer_size()) {
            let screen = monitor.size();
            let x = screen.width.saturating_sub(size.width) / 2;
            let y = screen.height.saturating_sub(size.height + 96);
            let _ = w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
        }
        let _ = w.show();
    }
}

#[tauri::command]
fn pill_hide(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("pill") {
        let _ = w.hide();
    }
}

/// One hearit per machine. Two instances means two engines fighting over
/// one hotkey, one port, and — the expensive part — double the VRAM.
/// A named mutex is the classic Windows answer; the handle is deliberately
/// leaked so the claim lasts exactly as long as the process.
fn already_running() -> bool {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "Local\\hearit-single-instance\0".encode_utf16().collect();
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        !handle.is_null() && GetLastError() == ERROR_ALREADY_EXISTS
    }
}

pub fn run() {
    if already_running() {
        eprintln!("[hearit] another hearit is already running — not starting a second engine");
        return;
    }
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([hotkey::SPEAK_KEY])
                .expect("speak key is not a valid shortcut")
                .with_handler(|app, _shortcut, event| hotkey::on_shortcut(app, event.state()))
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(sidecar::Sidecar(Mutex::default()))
        .manage(sidecar::Ready::default())
        .manage(Takes::default())
        .manage(Speed(Mutex::new(1.0)))
        .manage(tray::Tray::default())
        .manage(update::Staged::default())
        .setup(|app| {
            // The first line of every boot, durably: version and exe path.
            // When an installer-launched instance misbehaves (2026-08-04:
            // one booted, slept, and couldn't wake its engine), engine.log
            // shows which binary booted, when, and what the engine did next.
            diag::log(
                app.handle(),
                &format!(
                    "[hearit] boot — v{} from {}",
                    app.package_info().version,
                    std::env::current_exe()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "?".into())
                ),
            );
            // The speaker is managed here, not before setup, because its
            // FFT monitor needs an AppHandle to emit viz_heights.
            app.manage(speak::start(app.handle())?);
            let saved = settings::load(app.handle());
            *app.state::<Speed>().0.lock().unwrap() = saved.speed;
            tray::build(app.handle(), saved.speed)?;
            // The update check runs BEFORE the sidecar: a broken sidecar
            // must never be able to block the update that fixes it.
            tauri::async_runtime::spawn(update::check_and_stage(app.handle().clone()));
            // A missing sidecar is a visible condition, not a boot
            // failure. The lesson of v0.1.0: installed away from its
            // companions, it died at setup with no console to say why.
            // Now the app lives in the tray either way; start() itself
            // writes the failure to engine.log and the tray tooltip, and
            // the next press retries via engine_start.
            // But first: a previous hearit that died uncleanly may have
            // left its engine behind, still holding VRAM and port 8880.
            // Clear it before spawning ours, or we'd run two.
            sidecar::reap_stale(app.handle());
            let _ = sidecar::start(app.handle());
            println!("[hearit] speak key on {}", hotkey::SPEAK_KEY);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            grab_selection,
            speak_begin,
            speak_sentence,
            speak_stop,
            is_ready,
            engine_start,
            engine_sleep,
            get_idle_minutes,
            log_gap,
            pill_show,
            pill_hide
        ])
        .build(tauri::generate_context!())
        .expect("error building hearit")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // One durable line per clean exit: in engine.log it
                // separates "the app quit" from "the app vanished" when
                // reading a timeline after the fact.
                diag::log(app, "[hearit] exiting");
                // The sidecar is our child; if we exit and leave it
                // running, it squats on the port and ~2GB forever. The
                // job object (sidecar.rs) would catch it anyway — this is
                // just the polite version that doesn't wait for the OS.
                if let Some(mut child) = app.state::<sidecar::Sidecar>().0.lock().unwrap().take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                // Any downloaded update applies now, while we're already
                // going down — never mid-session.
                update::install_if_staged(app);
            }
        });
}
