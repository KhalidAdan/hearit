//! Stage 1: the key. Collapses the OS shortcut into one logical signal —
//! `speak_pressed` — stamped with wall-clock ms so the coordinator can
//! measure dead air from the OS event itself. Tap, not hold: sayit's key
//! listens while held; hearit's key acts on the press, and the release is
//! nothing. Knows nothing about clipboards, models, or app state.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::ShortcutState;

/// The speak key. One line to change, per the north star.
/// F9 belongs to sayit — the siblings must not fight over a key.
pub const SPEAK_KEY: &str = "F10";

/// Whether the key is currently down, so a repeat `Pressed` collapses
/// into the first. On Windows this is mostly insurance: the underlying
/// crate (global-hotkey 0.8.0) registers with MOD_NOREPEAT, so a genuine
/// hold delivers ONE `Pressed` and then silence until release — no
/// repeat burst ever reaches us. The flag stays because it is cheap and
/// because other platforms and future crate versions may not be so tidy.
static DOWN: AtomicBool = AtomicBool::new(false);

/// When the last `Pressed` arrived, in wall-clock ms. `DOWN` alone is a
/// trap, because `Released` is not an OS event: on each WM_HOTKEY the
/// crate sends `Pressed` and spawns a thread that polls
/// GetAsyncKeyState every 50ms, sending `Released` only when the async
/// state finally reads key-up. Seen in production (v0.2.4): the async
/// state for F10 wedged "down" at the OS level, the poll thread looped
/// forever, `Released` never came, `DOWN` stuck true — and every press
/// after that was silently swallowed while the app looked perfectly
/// healthy. (A synthetic key-up via SendInput cleared the async state,
/// the poll thread delivered its overdue `Released`, and the key
/// revived — which is how we know.) The heal deliberately does NOT
/// consult GetAsyncKeyState to double-check the flag: that is exactly
/// the signal that lied. Time is the honest witness — a `Pressed`
/// arriving after a long silence is a human pressing the key again,
/// whatever `DOWN` claims.
static LAST_PRESSED_MS: AtomicU64 = AtomicU64::new(0);

/// Silence longer than this between `Pressed` events means the earlier
/// press is over, even if we never saw its `Released`. Where repeats do
/// reach us they arrive at most ~1.1s apart (Windows' slowest initial
/// repeat delay is ~1s, then faster), so 1.5s clears every legitimate
/// repeat gap with margin while staying below any deliberate second
/// press.
const STUCK_AFTER_MS: u64 = 1500;

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

/// The whole debounce decision, pure so it can be tested without a key or
/// an OS: a `Pressed` is fresh when the key was up — or when it has been
/// so long since the last `Pressed` that the old press must have ended
/// without us hearing about it. In practice a `Pressed` with `DOWN` set
/// is either a leaked repeat (gap under ~1.1s, swallowed by the
/// threshold) or a stale wedge (healed). The one theoretical false
/// positive — a lone repeat somehow leaking through more than 1.5s into
/// a hold — would start a second take, and the key's own grammar
/// absorbs it: pressing on the same text stops the take (main.ts). A
/// self-correcting maybe does not justify defensive machinery.
fn is_fresh_press(was_down: bool, last_pressed_ms: u64, now: u64) -> bool {
    !was_down || now.saturating_sub(last_pressed_ms) > STUCK_AFTER_MS
}

pub fn on_shortcut(app: &AppHandle, state: ShortcutState) {
    match state {
        ShortcutState::Pressed => {
            let now = now_ms();
            let last = LAST_PRESSED_MS.swap(now, Ordering::SeqCst);
            let was_down = DOWN.swap(true, Ordering::SeqCst);
            if is_fresh_press(was_down, last, now) {
                if was_down {
                    // The flag said held but the silence said otherwise —
                    // a release went missing and we just recovered. Durable
                    // on purpose: this is the incident that once left the
                    // key dead while the app looked healthy, and an
                    // installed instance has no console to confess on.
                    crate::diag::log(
                        app,
                        &format!(
                            "[hearit] speak key healed — fresh press after {}ms of silence with the down-flag still set (a release went missing)",
                            now.saturating_sub(last)
                        ),
                    );
                }
                let _ = app.emit("speak_pressed", now);
            }
        }
        ShortcutState::Released => {
            DOWN.store(false, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What `on_shortcut` does to the state on `Pressed`, mirrored for
    /// tests: swap in the new stamp, swap the flag true, decide.
    fn press(down: &mut bool, last: &mut u64, now: u64) -> bool {
        let was_down = std::mem::replace(down, true);
        let prev = std::mem::replace(last, now);
        is_fresh_press(was_down, prev, now)
    }

    #[test]
    fn fresh_press_emits() {
        let (mut down, mut last) = (false, 0);
        assert!(press(&mut down, &mut last, 10_000));
    }

    #[test]
    fn auto_repeat_burst_collapses() {
        // The insurance case: a platform or crate version where holding
        // the key does deliver a repeat burst (MOD_NOREPEAT spares us on
        // Windows today). Initial press, then repeats every 30ms.
        let (mut down, mut last) = (false, 0);
        assert!(press(&mut down, &mut last, 10_000));
        for t in (10_030..11_000).step_by(30) {
            assert!(!press(&mut down, &mut last, t), "repeat at {t} leaked");
        }
    }

    #[test]
    fn missed_release_then_later_press_heals() {
        // The production incident: press, release lost, key sits idle,
        // user presses again. The flag still says held; time overrules it.
        let (mut down, mut last) = (false, 0);
        assert!(press(&mut down, &mut last, 10_000));
        // No Released ever arrives. Next human press, seconds later:
        assert!(press(&mut down, &mut last, 40_000));
    }

    #[test]
    fn long_hold_does_not_double_emit() {
        // Same insurance world: a hold lasting far past the heal
        // threshold stays one logical press, because each repeat
        // refreshes the stamp. Include the worst legitimate gap — a
        // slowest-setting initial repeat delay (~1s) before the burst
        // begins. (On Windows today a hold delivers no repeats at all,
        // which trivially cannot double-emit.)
        let (mut down, mut last) = (false, 0);
        assert!(press(&mut down, &mut last, 10_000));
        assert!(
            !press(&mut down, &mut last, 11_000),
            "slow first repeat leaked"
        );
        for t in (11_030..15_000).step_by(30) {
            assert!(!press(&mut down, &mut last, t), "repeat at {t} leaked");
        }
    }

    #[test]
    fn release_then_quick_press_emits() {
        // The ordinary double-tap: a well-behaved release clears the flag,
        // so the second tap emits even though it lands within the heal
        // threshold.
        let (mut down, mut last) = (false, 0);
        assert!(press(&mut down, &mut last, 10_000));
        down = false; // Released arrived
        assert!(press(&mut down, &mut last, 10_200));
    }
}
