//! Persistence for the key's few switches. A hand-rolled settings.json in
//! the app config dir — a couple of fields don't need a plugin, and every
//! line of this is explainable. Load never fails (absent or corrupt file
//! = defaults); save never panics (a failed write costs one preference,
//! not a crash). sayit's settings.rs, fields swapped.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

fn default_speed() -> f32 {
    1.0
}

fn default_idle_minutes() -> u64 {
    5
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Listening speed, applied to every synthesis request. Kokoro
    /// time-scales in the duration model, so pitch survives — this is
    /// "speed under your thumb", not chipmunk mode. North star: speed is
    /// to listening what font size is to reading.
    #[serde(default = "default_speed")]
    pub speed: f32,
    /// Minutes of idle before the engine sleeps and its VRAM comes home.
    /// Edit settings.json to change it; 0 disables auto-sleep entirely.
    /// Kept small on purpose — an idle engine squatting ~2GB overnight
    /// starves every other GPU workload on the machine (the 2026-08-02
    /// incident), and a wake costs only seconds.
    #[serde(default = "default_idle_minutes")]
    pub idle_minutes: u64,
}

/// Hand-rolled (not derived) so a missing settings.json gets the same
/// defaults as a settings.json missing the fields.
impl Default for Settings {
    fn default() -> Self {
        Settings {
            speed: default_speed(),
            idle_minutes: default_idle_minutes(),
        }
    }
}

fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        // Hand-edited files (idle_minutes lives here) may carry a UTF-8
        // BOM, which serde_json rejects — and a silent fall-back to
        // defaults would look like the edit was ignored.
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) {
    let Some(p) = path(app) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(p, json);
    }
}
