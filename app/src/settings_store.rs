//! Loads and saves [`Settings`] to a JSON file under Application Support.
//!
//! `prefs_model` owns the data and its (de)serialization; this module only owns the
//! file location and the I/O, so it is the one macOS-specific piece of settings.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
///
/// A file that exists but does not parse is **moved aside** to
/// `settings.corrupt-<unix seconds>.json` before the defaults are returned. That
/// is what keeps the `.bak` in `save` meaningful: the defaults this returns are
/// saved again within seconds (macOS shows the welcome screen and saves at
/// launch whenever `welcome_shown` is false, which a lost file always is), and
/// if the unreadable file were still in place that save would copy it over the
/// last good backup — destroying the only recoverable copy in exactly the case
/// the backup exists for. Moving it here means `save` finds no file to back up,
/// so the previous `.bak` survives untouched and the unreadable original is
/// preserved under its own name as well.
#[must_use]
pub fn load() -> Settings {
    let Some(path) = settings_path() else {
        return Settings::default();
    };
    match fs::read_to_string(&path) {
        Ok(json) => match Settings::try_from_json(&json) {
            Ok(settings) => settings,
            Err(e) => {
                preserve_corrupt(&path, &e);
                Settings::default()
            }
        },
        Err(_) => Settings::default(), // missing → default
    }
}

/// Moves an unparseable settings file aside, so neither it nor the `.bak` is
/// lost. Best-effort: if the rename fails there is nothing further to try, and
/// the app still starts on defaults.
fn preserve_corrupt(path: &Path, error: &str) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let aside = path.with_file_name(format!("settings.corrupt-{stamp}.json"));
    match fs::rename(path, &aside) {
        Ok(()) => crate::log::log(&format!(
            "SETTINGS unreadable ({error}); kept as {} and started on defaults",
            aside.display()
        )),
        Err(e) => crate::log::log(&format!(
            "SETTINGS unreadable ({error}) and could not be preserved: {e}"
        )),
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
        crate::log::log(&format!("SETTINGS could not create settings dir: {e}"));
        return;
    }
    // Keep one backup of the previous file, for the case where a save writes
    // something the user did not intend to lose (a downgrade dropping unknown
    // keys, say). This is only safe because `load` moves an unparseable file
    // aside before returning defaults: without that, the first save after a
    // failed load would copy the unreadable file over the last good backup, and
    // the backup would be destroyed by precisely the event it exists for.
    if path.exists() {
        let _ = fs::copy(&path, path.with_extension("json.bak"));
    }
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = write_durably(&tmp, &settings.to_json()) {
        crate::log::log(&format!("SETTINGS could not write settings: {e}"));
        return;
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        crate::log::log(&format!("SETTINGS could not finalize settings: {e}"));
    }
}

/// Writes `contents` and flushes it to the storage device before returning.
///
/// The rename that follows is atomic with respect to the *directory*, which is
/// what stops a reader seeing a half-written file. It says nothing about whether
/// the bytes reached the disk: without the `sync_all`, a crash or power loss
/// moments after the rename can leave the settings file present, named correctly
/// and empty. `sync_all` is the difference between an atomic swap and a durable
/// one, and settings are written from a hotkey press, so the window is real.
fn write_durably(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unreadable settings file is moved aside rather than left where the
    /// next save would copy it over the backup.
    ///
    /// This is the whole point of `preserve_corrupt`, and it is asserted on the
    /// filesystem rather than through `load` because `load` reads a fixed
    /// per-user path that a test must not touch.
    #[test]
    fn an_unreadable_file_is_preserved_and_leaves_the_backup_alone() {
        let dir = std::env::temp_dir().join(format!(
            "glowkey-settings-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("settings.json");
        let bak = path.with_extension("json.bak");

        // The good backup, and a file that will not parse.
        fs::write(&bak, r#"{"auto_fix": false}"#).expect("write bak");
        fs::write(&path, "{ this is not json").expect("write corrupt");

        assert!(Settings::try_from_json("{ this is not json").is_err());
        preserve_corrupt(&path, "test");

        assert!(!path.exists(), "the unreadable file was moved aside");
        assert_eq!(
            fs::read_to_string(&bak).expect("bak survives"),
            r#"{"auto_fix": false}"#,
            "the backup is untouched"
        );
        let preserved: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("settings.corrupt-"))
            .collect();
        assert_eq!(preserved.len(), 1, "the original is kept under its own name");

        let _ = fs::remove_dir_all(&dir);
    }

    /// A durable write leaves exactly what it was given.
    #[test]
    fn a_durable_write_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "glowkey-durable-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        write_durably(&path, r#"{"auto_fix": true}"#).expect("write");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            r#"{"auto_fix": true}"#
        );
        let _ = fs::remove_file(&path);
    }
}
