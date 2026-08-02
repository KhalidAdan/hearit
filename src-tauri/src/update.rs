//! Self-update from GitHub Releases, the invisible way — more literally
//! than sayit manages it. On Windows, installing runs the NSIS installer,
//! which requires the app to close; install-at-boot therefore makes the
//! app vanish seconds after launch whenever a release shipped overnight
//! (observed on this machine, v0.1.x). So: DOWNLOAD at boot, INSTALL on
//! the way out. "Never restart out from under the user", kept to the
//! letter. Updates are signature-checked against the pubkey baked into
//! tauri.conf.json; the private key lives only in ~/.tauri and GitHub
//! secrets.

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

/// The downloaded-but-not-installed update, held until exit.
#[derive(Default)]
pub struct Staged(pub Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>);

pub async fn check_and_stage(app: AppHandle) {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("[update] updater unavailable: {e}");
            return;
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            println!("[update] v{version} available — downloading");
            match update.download(|_, _| {}, || {}).await {
                Ok(bytes) => {
                    println!("[update] v{version} downloaded — installs on quit");
                    let _ = app.emit("update_installed", version);
                    *app.state::<Staged>().0.lock().unwrap() = Some((update, bytes));
                }
                Err(e) => eprintln!("[update] download failed: {e}"),
            }
        }
        Ok(None) => println!("[update] up to date"),
        Err(e) => eprintln!("[update] check failed (offline is fine): {e}"),
    }
}

/// Called from RunEvent::Exit — the app is already on its way down, so
/// the installer's close-the-app requirement is satisfied for free.
pub fn install_if_staged(app: &AppHandle) {
    if let Some((update, bytes)) = app.state::<Staged>().0.lock().unwrap().take() {
        println!("[update] installing v{} on exit", update.version);
        if let Err(e) = update.install(bytes) {
            eprintln!("[update] install failed: {e}");
        }
    }
}
