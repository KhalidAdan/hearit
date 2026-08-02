//! Stage 1: the key. Collapses the OS shortcut into one logical signal —
//! `speak_pressed` — stamped with wall-clock ms so the coordinator can
//! measure dead air from the OS event itself. Tap, not hold: sayit's key
//! listens while held; hearit's key acts on the press, and the release is
//! nothing. Knows nothing about clipboards, models, or app state.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::ShortcutState;

/// The speak key. One line to change, per the north star.
/// F9 belongs to sayit — the siblings must not fight over a key.
pub const SPEAK_KEY: &str = "F10";

/// Whether the key is currently down. Windows fires auto-repeat `Pressed`
/// events for as long as a key is held; this flag collapses the burst into
/// one logical press.
static DOWN: AtomicBool = AtomicBool::new(false);

/// Wall-clock ms, stamped at the OS event and carried in the payload so
/// the coordinator can measure how long the event took to cross into the
/// webview. Both sides read the same system clock, so the subtraction is
/// honest to within a millisecond.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn on_shortcut(app: &AppHandle, state: ShortcutState) {
    match state {
        ShortcutState::Pressed => {
            if !DOWN.swap(true, Ordering::SeqCst) {
                let _ = app.emit("speak_pressed", now_ms());
            }
        }
        ShortcutState::Released => {
            DOWN.store(false, Ordering::SeqCst);
        }
    }
}
