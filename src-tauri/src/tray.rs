//! The tray: hearit's only permanent visible presence. v2's switches
//! live here now — listening speed (adjusted live, remembered forever)
//! and startup residence — plus the originals: free the GPU, quit. Still
//! no settings screen; these are the key's physical switches, not a
//! config. Speed takes effect from the next sentence, which for a
//! sentence-streamed reader is what "live" honestly means.

use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::{settings, Speed};

/// The shelf of speeds. Kokoro accepts a continuous value; a menu wants
/// steps. These cover "careful read" to "double time" — change the list,
/// nothing else knows.
const SPEEDS: [f32; 6] = [0.8, 1.0, 1.25, 1.5, 1.75, 2.0];

/// The icon handle must outlive build() — a dropped TrayIcon disappears
/// from the tray. Managed state is the app-lifetime shelf for it.
#[derive(Default)]
pub struct Tray(pub Mutex<Option<TrayIcon<Wry>>>);

pub fn build(app: &AppHandle, saved_speed: f32) -> tauri::Result<()> {
    // Speed picker, persisted choice pre-checked.
    let mut speed_items: Vec<CheckMenuItem<Wry>> = Vec::new();
    for s in SPEEDS {
        speed_items.push(CheckMenuItem::with_id(
            app,
            format!("speed:{s}"),
            format!("{s}×"),
            true,
            (s - saved_speed).abs() < 0.01,
            None::<&str>,
        )?);
    }
    let speed_refs: Vec<&dyn IsMenuItem<Wry>> =
        speed_items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let speeds = Submenu::with_id_and_items(app, "speeds", "Speed", true, &speed_refs)?;

    // Autostart only makes sense for the built app: enabling it from a
    // dev run would register the debug exe, which needs the vite server
    // to be useful. The toggle works either way; the caveat lives here
    // as a comment and in the docs. (sayit's caveat, same words.)
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with Windows",
        true,
        autostart_on,
        None::<&str>,
    )?;

    let free = MenuItem::with_id(
        app,
        "free-vram",
        "Free VRAM — engine wakes on next press",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit hearit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &speeds,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &free,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("hearit")
        .icon(
            app.default_window_icon()
                .expect("bundle always has an icon")
                .clone(),
        )
        .tooltip(format!(
            "hearit v{} — select text, press F10",
            app.package_info().version
        ))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            if id == "quit" {
                // RunEvent::Exit kills the sidecar on the way out.
                app.exit(0);
            } else if id == "free-vram" {
                crate::sidecar::sleep(app);
            } else if id == "autostart" {
                let launcher = app.autolaunch();
                let flip = match launcher.is_enabled() {
                    Ok(true) => launcher.disable(),
                    _ => launcher.enable(),
                };
                if let Err(e) = flip {
                    eprintln!("[hearit] autostart toggle failed: {e}");
                }
                let _ = autostart.set_checked(launcher.is_enabled().unwrap_or(false));
            } else if let Some(s) = id.strip_prefix("speed:") {
                let Ok(speed) = s.parse::<f32>() else { return };
                println!("[hearit] speed: {speed}×");
                // Live: the next sentence synthesizes at this speed.
                *app.state::<Speed>().0.lock().unwrap() = speed;
                for item in &speed_items {
                    let _ = item.set_checked(item.id().as_ref() == id);
                }
                // Remembered forever: load-and-mutate so future fields
                // survive the write (sayit's dictionary lesson).
                let mut saved = settings::load(app);
                saved.speed = speed;
                settings::save(app, &saved);
            }
        })
        .build(app)?;

    app.state::<Tray>().0.lock().unwrap().replace(tray);
    Ok(())
}

/// The tray tooltip is the app's only voice when something's wrong at
/// boot — a release build has no console, and hearit has no window.
pub fn set_tooltip(app: &AppHandle, text: &str) {
    if let Some(tray) = app.state::<Tray>().0.lock().unwrap().as_ref() {
        let _ = tray.set_tooltip(Some(text));
    }
}
