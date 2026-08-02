//! The engine's keeper: the Kokoro sidecar's lifecycle. Started at boot,
//! put to sleep when the coordinator says so (idle timer) or the tray
//! asks, woken by the next press, killed at exit — and, via a Windows job
//! object, killed by the OS itself if hearit dies any less politely.
//! Sleeping frees ~2GB of working set (the CUDA runtime is most of it);
//! waking costs seconds, absorbed by synth_waiting's patience. The user
//! never manages any of this — the key always works.

use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::synth;

/// The job object every spawned engine is assigned to. Its one limit is
/// KILL_ON_JOB_CLOSE: when hearit dies — cleanly, by crash, by Task
/// Manager, by a dev run torn down mid-flight — the OS closes our handle,
/// the job closes, and koko dies with us. This is the crash-safe backstop
/// behind the polite kills in sleep() and RunEvent::Exit; without it an
/// orphaned engine squats ~2GB until someone notices (2026-08-02: idle
/// sidecars starved a nightly Ollama run on this machine). The handle is
/// created once and deliberately never closed — it must live exactly as
/// long as the process.
fn job() -> windows_sys::Win32::Foundation::HANDLE {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    static JOB: OnceLock<isize> = OnceLock::new();
    *JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            eprintln!("[hearit] couldn't create job object — engine won't be crash-tied to us");
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            eprintln!("[hearit] couldn't set kill-on-close on the job object");
        }
        job as isize
    }) as windows_sys::Win32::Foundation::HANDLE
}

/// Kill any koko.exe left over from a previous hearit that died without
/// cleaning up (pre-job-object builds, or a failed job assign). Called
/// once at boot, before the first spawn. Without this, the orphan keeps
/// its VRAM AND its port — sayit's whisper-server proved a stale sidecar
/// can bind alongside a fresh one instead of failing, so "the spawn
/// worked" proves nothing about being alone.
///
/// Only processes whose image path is exactly OUR resolved sidecar exe
/// are touched — a koko belonging to some other tool is not ours to kill.
pub fn reap_stale() {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    // Compare canonicalized, lowercased paths: the snapshot reports plain
    // "C:\..." while our resolver may hold a relative or \\?\ form.
    let Some(ours) = crate::paths::sidecar_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(p).ok())
        .map(|p| p.to_string_lossy().to_lowercase())
    else {
        return;
    };

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry) != 0;
        while ok {
            let name_len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);
            if name.eq_ignore_ascii_case("koko.exe") {
                let proc = OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                    0,
                    entry.th32ProcessID,
                );
                if !proc.is_null() {
                    let mut buf = [0u16; 1024];
                    let mut len = buf.len() as u32;
                    if QueryFullProcessImageNameW(proc, 0, buf.as_mut_ptr(), &mut len) != 0 {
                        let path = String::from_utf16_lossy(&buf[..len as usize]);
                        let theirs = std::fs::canonicalize(&path)
                            .map(|p| p.to_string_lossy().to_lowercase())
                            .unwrap_or_else(|_| path.to_lowercase());
                        if theirs == ours {
                            println!(
                                "[hearit] reaping stale koko (pid {}) from a previous run",
                                entry.th32ProcessID
                            );
                            TerminateProcess(proc, 1);
                        }
                    }
                    CloseHandle(proc);
                }
            }
            ok = Process32NextW(snap, &mut entry) != 0;
        }
        CloseHandle(snap);
    }
}

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
    // Tie the engine's lifetime to ours BEFORE it can outlive a crash.
    // A failed assign is logged, not fatal: the engine still works, it's
    // just back to trusting the exit handler (and the next boot's reap).
    unsafe {
        use std::os::windows::io::AsRawHandle;
        if windows_sys::Win32::System::JobObjects::AssignProcessToJobObject(
            job(),
            child.as_raw_handle() as _,
        ) == 0
        {
            eprintln!("[hearit] couldn't assign koko to the job object");
        }
    }
    *guard = Some(child);
    drop(guard);

    println!("[hearit] engine waking");
    let _ = app.emit("engine_waking", ());
    tauri::async_runtime::spawn(warmup(app.clone(), std::time::Instant::now()));
    Ok(())
}

/// Put the engine to sleep: kill the process, free the VRAM. The tray
/// and the coordinator's idle timer call this; the next press wakes it
/// (the coordinator calls engine_start on every take, and synth_waiting
/// absorbs the warmup).
pub fn sleep(app: &AppHandle) {
    if let Some(mut child) = app.state::<Sidecar>().0.lock().unwrap().take() {
        let _ = child.kill();
        // Reap the process object so "VRAM freed" below is true, not hopeful.
        let _ = child.wait();
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
