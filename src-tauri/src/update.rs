//! Self-update from GitHub Releases, the invisible way: check at boot,
//! download and stage quietly, and NEVER restart out from under the user
//! — the new version takes effect whenever they next launch. Updates are
//! signature-checked against the pubkey baked into tauri.conf.json; the
//! private key lives only in ~/.tauri and GitHub secrets. Same file as
//! sayit's update.rs; hearit has no tray yet, so the coordinator just
//! logs the staged version.

use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

pub async fn check_and_install(app: AppHandle) {
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
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => {
                    println!("[update] v{version} staged; takes effect next launch");
                    let _ = app.emit("update_installed", version);
                }
                Err(e) => eprintln!("[update] install failed: {e}"),
            }
        }
        Ok(None) => println!("[update] up to date"),
        Err(e) => eprintln!("[update] check failed (offline is fine): {e}"),
    }
}
