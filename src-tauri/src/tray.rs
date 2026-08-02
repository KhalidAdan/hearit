//! The tray: hearit's only permanent visible presence, and deliberately
//! thin. This is v2's "startup residence and a tray icon" arriving early
//! because one need was earned in real use: freeing the GPU without
//! killing the key, and killing the key without hunting a process. Two
//! items. No status line, no settings — those wait for their own
//! friction-list entries.

use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

/// The icon handle must outlive build() — a dropped TrayIcon disappears
/// from the tray. Managed state is the app-lifetime shelf for it.
#[derive(Default)]
pub struct Tray(pub Mutex<Option<TrayIcon<Wry>>>);

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let free = MenuItem::with_id(
        app,
        "free-vram",
        "Free VRAM — engine wakes on next press",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit hearit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&free, &PredefinedMenuItem::separator(app)?, &quit])?;

    let tray = TrayIconBuilder::with_id("hearit")
        .icon(
            app.default_window_icon()
                .expect("bundle always has an icon")
                .clone(),
        )
        .tooltip("hearit — select text, press F10")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            // RunEvent::Exit kills the sidecar on the way out.
            "quit" => app.exit(0),
            "free-vram" => crate::sidecar::sleep(app),
            _ => {}
        })
        .build(app)?;

    app.state::<Tray>().0.lock().unwrap().replace(tray);
    Ok(())
}
