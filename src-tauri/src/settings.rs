//! Persistence for the key's few switches. A hand-rolled settings.json in
//! the app config dir — one field doesn't need a plugin, and every line
//! of this is explainable. Load never fails (absent or corrupt file =
//! defaults); save never panics (a failed write costs one preference,
//! not a crash). sayit's settings.rs, one field swapped.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

fn default_speed() -> f32 {
    1.0
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Listening speed, applied to every synthesis request. Kokoro
    /// time-scales in the duration model, so pitch survives — this is
    /// "speed under your thumb", not chipmunk mode. North star: speed is
    /// to listening what font size is to reading.
    #[serde(default = "default_speed")]
    pub speed: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { speed: 1.0 }
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
        .and_then(|text| serde_json::from_str(&text).ok())
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
