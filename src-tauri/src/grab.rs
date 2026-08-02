//! Stage 2: the borrowed copy. sayit's inject.rs, mirrored — where inject
//! ends by putting text ON the clipboard and synthesizing Ctrl+V, grab
//! begins by synthesizing Ctrl+C and lifting text OFF it. Same trick, same
//! reason it works: hearit never has focus, so the app the user is reading
//! still owns the selection, and the copy lands here.
//!
//! The clipboard is cleared first so an empty selection is distinguishable
//! from a slow one: "still empty after the wait" means nothing was
//! selected, which the coordinator reads as "the press means stop".

use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grab {
    /// The selection, or empty if nothing was selected. Empty is a signal,
    /// not an error — the grammar needs it.
    pub text: String,
    /// Opening the clipboard + saving the user's old contents.
    pub clipboard_save_ms: u64,
    /// Synthesizing Ctrl+C.
    pub copy_ms: u64,
    /// Polling until the source app committed the copy (or gave us nothing).
    pub wait_ms: u64,
    pub total_ms: u64,
}

pub fn grab() -> Result<Grab, String> {
    let t_all = Instant::now();

    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    // Save what the user had. v1 preserves text only; an image on the
    // clipboard is lost to the grab. Same known limitation as sayit's
    // inject, same friction-list entry.
    let saved = clipboard.get_text().ok();
    let _ = clipboard.clear();
    let clipboard_save_ms = t_all.elapsed().as_millis() as u64;

    let t = Instant::now();
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Unicode('c'), Direction::Click)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|e| e.to_string())?;
    let copy_ms = t.elapsed().as_millis() as u64;

    // The source app writes the clipboard asynchronously. Poll instead of
    // a fixed sleep: a fast app costs ~20ms, a slow one gets 400ms of
    // patience before we conclude nothing was selected.
    let t = Instant::now();
    let mut text = String::new();
    while t.elapsed() < Duration::from_millis(400) {
        sleep(Duration::from_millis(20));
        if let Ok(current) = clipboard.get_text() {
            if !current.is_empty() {
                text = current;
                break;
            }
        }
    }
    let wait_ms = t.elapsed().as_millis() as u64;

    // Restore the user's clipboard on a detached thread, same as inject:
    // clipboard access can block indefinitely when another process holds
    // it, and a hung restore must cost at worst the old contents, never
    // the pipeline. The small grace delay lets a slow source app finish
    // writing its other clipboard formats before we stomp them.
    if let Some(saved) = saved {
        std::thread::spawn(move || {
            sleep(Duration::from_millis(150));
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(saved);
            }
        });
    }

    let timing = Grab {
        text,
        clipboard_save_ms,
        copy_ms,
        wait_ms,
        total_ms: t_all.elapsed().as_millis() as u64,
    };
    println!(
        "[timing] grab: save {clipboard_save_ms}ms · copy {copy_ms}ms · wait {wait_ms}ms · total {}ms — {} chars",
        timing.total_ms,
        timing.text.len()
    );
    Ok(timing)
}
