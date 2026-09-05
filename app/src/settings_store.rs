//! Loads and saves [`Settings`] to a JSON file under Application Support.
//!
//! `prefs_model` owns the data and its (de)serialization; this module only owns the
//! file location and the I/O, so it is the one macOS-specific piece of settings.

use std::fs;
use std::path::PathBuf;

use crate::prefs_model::Settings;

/// `~/Library/Application Support/GlowKey/settings.json`.
#[cfg(target_os = "macos")]
fn settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push("Library/Application Support/GlowKey");
    Some(path.join("settings.json"))
}

/// `%APPDATA%\GlowKey\settings.json`.
///
/// The same schema and the same file name as macOS, deliberately: Phase 2 made
/// `HotkeyPreset` carry both platforms' key identity side by side, so a settings
/// file copied from one to the other loads rather than being reinterpreted.
#[cfg(target_os = "windows")]
fn settings_path() -> Option<PathBuf> {
    Some(crate::platform::windows::paths::settings_dir()?.join("settings.json"))
}

/// Loads settings, falling back to defaults if the file is missing or unreadable.
/// Never fails — a first run or a corrupt file both yield sensible defaults.
#[must_use]
pub fn load() -> Settings {
    let Some(path) = settings_path() else {
        return Settings::default();
    };
    match fs::read_to_string(&path) {
        Ok(json) => Settings::from_json(&json), // tolerant: corrupt → default
        Err(_) => Settings::default(),          // missing → default
    }
}

/// Saves settings atomically (write a temp file, then rename), creating the
/// directory if needed. Logs and continues on error — persistence failure must
/// never stop the app; it just means changes won't survive a restart.
pub fn save(settings: &Settings) {
    let Some(path) = settings_path() else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(dir) {
        eprintln!("GlowKey: could not create settings dir: {e}");
        return;
    }
    // Keep one backup of the previous file. `Settings::from_json` falls back to
    // FULL defaults on any parse error (e.g. an older build reading a newer
    // enum variant), and the next save would then overwrite the user's file with
    // defaults — the .bak preserves what was there for manual recovery.
    if path.exists() {
        let _ = fs::copy(&path, path.with_extension("json.bak"));
    }
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = fs::write(&tmp, settings.to_json()) {
        eprintln!("GlowKey: could not write settings: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        eprintln!("GlowKey: could not finalize settings: {e}");
    }
}
