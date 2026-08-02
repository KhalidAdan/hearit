//! The engine's keeper: the Kokoro sidecar's lifecycle. Started at boot,
//! killed at exit. Unlike sayit's whisper engine there is no sleep cycle
//! in v1 — Kokoro is a fraction of whisper's footprint, and "the model
//! warm at boot" is a v2 promise we get almost for free by never letting
//! it cool. The user never manages any of this.

use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::synth;

/// Pullable readiness, same race-proofing as sayit: `sidecar_ready` alone
/// can fire before the webview has registered its listeners. TS asks via
/// `is_ready` at startup AND listens; whichever wins, wins.
#[derive(Default)]
pub struct Ready(pub AtomicBool);

pub struct Sidecar(pub Mutex<Option<Child>>);

/// Start the engine. Idempotent: a running engine is left alone.
pub fn start(app: &AppHandle) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let sidecar = app.state::<Sidecar>();
    let mut guard = sidecar.0.lock().unwrap();
    if guard.is_some() {
        return Ok(());
    }

    // Companions are found at runtime (env override, next-to-exe, or repo
    // layout) — see paths.rs. The same exe works everywhere.
    let server = crate::paths::sidecar_exe()?;
    let model = crate::paths::model()?;
    let voices = crate::paths::voices()?;
    // CLI verified against kokoros b54354b: model and voices are GLOBAL
    // flags and must come before the `openai` subcommand; --ip/--port
    // belong to the subcommand (docs/sidecar.md).
    let child = Command::new(&server)
        .arg("-m")
        .arg(&model)
        .arg("-d")
        .arg(&voices)
        .args([
            "openai",
            "--ip",
            "127.0.0.1",
            "--port",
            &synth::SIDECAR_PORT.to_string(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("failed to spawn kokoro sidecar: {e}"))?;
    *guard = Some(child);
    drop(guard);

    println!("[hearit] engine waking");
    let _ = app.emit("engine_waking", ());
    tauri::async_runtime::spawn(warmup(app.clone(), std::time::Instant::now()));
    Ok(())
}

/// Put the engine to sleep: kill the process, free the VRAM. Only the
/// tray calls this; the next press wakes it (the coordinator calls
/// engine_start on every take, and synth_waiting absorbs the warmup).
pub fn sleep(app: &AppHandle) {
    if let Some(mut child) = app.state::<Sidecar>().0.lock().unwrap().take() {
        let _ = child.kill();
        app.state::<Ready>().0.store(false, Ordering::Relaxed);
        println!("[hearit] engine sleeping — VRAM freed");
        let _ = app.emit("engine_sleeping", ());
    }
}

/// Warm the engine with one throwaway synthesis. Success doubles as the
/// readiness probe: `sidecar_ready` means "warm and listening", so the
/// first real press of the day speaks as fast as the hundredth.
async fn warmup(app: AppHandle, spawned: std::time::Instant) {
    for probe in 1..=60u32 {
        // The app may be shutting down mid-warmup; stop probing.
        if app.state::<Sidecar>().0.lock().unwrap().is_none() {
            return;
        }
        match synth::probe().await {
            Ok(()) => {
                println!(
                    "[timing] engine warm and ready in {:.1}s ({probe} probe{})",
                    spawned.elapsed().as_secs_f32(),
                    if probe == 1 { "" } else { "s" }
                );
                app.state::<Ready>().0.store(true, Ordering::Relaxed);
                let _ = app.emit("sidecar_ready", ());
                return;
            }
            Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    }
    eprintln!("[hearit] engine never became ready");
    let _ = app.emit("pipeline_error", "engine never became ready");
}
