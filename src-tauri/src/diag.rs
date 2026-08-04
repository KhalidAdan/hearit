//! The durable ledger for a console-less app. An installer-launched
//! hearit has no stdout: when the 2026-08-04 instance couldn't wake its
//! engine after the idle sleep, nothing anywhere recorded why — the only
//! surfaces were the tray tooltip and the pill's silence. Engine
//! lifecycle events now also land in engine.log, next to the dead-air
//! CSV, so the next silent failure leaves evidence instead of a mystery.
//! The console echo stays: dev runs read the same story live.

use std::io::Write;
use tauri::{AppHandle, Manager};

/// Where durable things live: the app config dir — settings.json and the
/// dead-air CSV are already there, so the whole story sits in one folder.
pub fn dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Lifecycle events are a handful of lines per boot; 256KB is years.
/// Shelve rather than truncate — the tail being investigated may be in
/// the file that just filled up.
const MAX_BYTES: u64 = 256 * 1024;

pub fn log(app: &AppHandle, line: &str) {
    println!("{line}");
    let Some(dir) = dir(app) else { return };
    append(&dir, line);
}

/// The file half of log(), split from the AppHandle so the rotation
/// contract — shelve, never truncate — is testable.
fn append(dir: &std::path::Path, line: &str) {
    let path = dir.join("engine.log");
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let shelf = dir.join("engine-old.log");
        let _ = std::fs::remove_file(&shelf); // Windows rename won't overwrite
        let _ = std::fs::rename(&path, &shelf);
    }
    // Unix seconds, same clock as the dead-air CSV — the two ledgers
    // correlate by subtraction.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{stamp} {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_ledger_is_shelved_not_truncated() {
        let dir = std::env::temp_dir().join("hearit-diag-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let over = (MAX_BYTES + 1) as usize;
        std::fs::write(dir.join("engine.log"), vec![b'x'; over]).unwrap();

        append(&dir, "[hearit] boot");

        // Every old byte survived, on the shelf — the tail being
        // investigated must never be the rotation's casualty.
        let old = std::fs::read(dir.join("engine-old.log")).unwrap();
        assert_eq!(old.len(), over);
        // And the fresh ledger starts over with just the new line.
        let new = std::fs::read_to_string(dir.join("engine.log")).unwrap();
        assert!(new.trim_end().ends_with("[hearit] boot"));
        assert_eq!(new.lines().count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
