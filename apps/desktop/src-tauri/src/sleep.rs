// SPDX-License-Identifier: GPL-3.0-or-later
//! Locks the vault after the computer has slept (Settings → Security → "Lock when the
//! computer sleeps").
//!
//! A background thread wakes up every few seconds and compares how far the wall clock and the
//! monotonic clock moved. On macOS and Linux the monotonic clock stops while the system is
//! suspended, so the wall clock running ahead means the machine slept. Where the monotonic
//! clock keeps counting through suspend, a tick that arrives very late means the process was
//! frozen. Either way the vault is locked as soon as the app runs again. Moving the system
//! clock forward also triggers a lock, which errs on the safe side.

use crate::prefs::{self, SecurityPrefs};
use crate::state::AppState;
use std::time::{Duration, Instant, SystemTime};
use tauri::Manager;

/// How often the watcher thread wakes up.
const TICK: Duration = Duration::from_secs(5);
/// Wall-clock time that may pass beyond the monotonic clock before we call it a suspend.
const WALL_SLACK: Duration = Duration::from_secs(20);
/// How late a tick may be before we call it a suspend.
const LATE_TICK: Duration = Duration::from_secs(60);

/// Whether the time between two ticks covers a system suspend.
///
/// `wall` is the elapsed wall-clock time (`None` if the clock went backwards), `mono` the
/// elapsed monotonic time.
pub fn suspended(wall: Option<Duration>, mono: Duration, tick: Duration) -> bool {
    let wall_ahead = wall.is_some_and(|w| w.saturating_sub(mono) > WALL_SLACK);
    wall_ahead || mono > tick + LATE_TICK
}

/// Starts the watcher thread.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("sleep-watch".into())
        .spawn(move || {
            let mut wall = SystemTime::now();
            let mut mono = Instant::now();
            loop {
                std::thread::sleep(TICK);
                let (now_wall, now_mono) = (SystemTime::now(), Instant::now());
                let slept = suspended(
                    now_wall.duration_since(wall).ok(),
                    now_mono.duration_since(mono),
                    TICK,
                );
                (wall, mono) = (now_wall, now_mono);
                if slept {
                    on_wake(&app);
                }
            }
        });
    if let Err(e) = spawned {
        log::warn!("could not start the sleep watcher: {e}");
    }
}

fn on_wake(app: &tauri::AppHandle) {
    let unlocked = app
        .state::<AppState>()
        .vault
        .lock()
        .as_ref()
        .is_some_and(|open| !open.vault.is_locked());
    if !unlocked {
        return;
    }
    let security: SecurityPrefs = prefs::load(app, prefs::SECURITY);
    if !security.lock_on_sleep {
        return;
    }
    log::info!("system resumed from sleep; locking the vault");
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::commands::vault::lock_vault(&app).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn regular_ticks_are_not_a_suspend() {
        assert!(!suspended(Some(S(5)), S(5), TICK));
        assert!(!suspended(Some(S(7)), S(6), TICK));
        // A busy or throttled process: both clocks are late together.
        assert!(!suspended(Some(S(40)), S(40), TICK));
    }

    #[test]
    fn wall_clock_running_ahead_is_a_suspend() {
        assert!(suspended(Some(S(3605)), S(5), TICK));
        assert!(suspended(Some(S(30)), S(5), TICK));
    }

    #[test]
    fn very_late_tick_is_a_suspend() {
        // Monotonic clocks that count through suspend (e.g. on Windows).
        assert!(suspended(Some(S(600)), S(600), TICK));
    }

    #[test]
    fn clock_going_backwards_is_ignored() {
        assert!(!suspended(None, S(5), TICK));
    }
}
