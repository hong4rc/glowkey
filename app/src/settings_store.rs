//! Loads and saves [`Settings`] to a JSON file under Application Support.
//!
//! The engine owns the data and its (de)serialization; this module only owns the
//! file location and the I/O, so it is the one macOS-specific piece of settings.

use std::fs;
use std::path::PathBuf;

use glowkey_engine::Settings;

/// Directory and file: `~/Library/Application Support/GlowKey/settings.json`.
fn settings_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push("Library/Application Support/GlowKey");
    Some(path.join("settings.json"))
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
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = fs::write(&tmp, settings.to_json()) {
        eprintln!("GlowKey: could not write settings: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        eprintln!("GlowKey: could not finalize settings: {e}");
    }
}
