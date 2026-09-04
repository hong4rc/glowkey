//! Resolving applications: the frontmost one for the menu bar's
//! "Enable/Disable for <App>" label, and any bundle identifier for the excluded
//! list's icon and name.
//!
//! Every call here is a window-server or Launch Services round-trip, so none of it
//! may run from the tap callback (`docs/decisions/0008`). The frontmost lookup is
//! called once at startup and then from the idle health tick; the resolution below
//! runs only while a Settings window is being built.

use objc2::rc::Retained;
use objc2_app_kit::{NSImage, NSWorkspace};
use objc2_foundation::{NSFileManager, NSString};

/// The frontmost application's `(display name, bundle id)`, if available.
pub fn frontmost() -> Option<(String, String)> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle_id = app.bundleIdentifier()?.to_string();
    let name = app
        .localizedName()
        .map(|n| n.to_string())
        .unwrap_or_else(|| bundle_id.clone());
    Some((name, bundle_id))
}

/// How an excluded bundle identifier should be shown.
pub struct AppDisplay {
    /// The app's own name, localized — "Visual Studio Code", not "VSCode".
    pub name: String,
    /// Its icon at whatever size the caller asks for, or `None` when the app is
    /// not installed.
    pub icon: Option<Retained<NSImage>>,
    /// Whether the app is installed on this machine.
    ///
    /// An uninstalled app is shown greyed rather than dropped: the exclusion is
    /// still real and still the user's, and silently removing entries for an app
    /// that is temporarily missing — an external disk, a reinstall in progress —
    /// would quietly undo a choice they made.
    pub installed: bool,
}

/// Resolves a bundle identifier to the name and icon to show for it.
///
/// Falls back to a readable form of the identifier itself when Launch Services
/// knows nothing about it, which is what happens for an app that has been
/// uninstalled since it was excluded.
pub fn describe(bundle_id: &str) -> AppDisplay {
    let workspace = NSWorkspace::sharedWorkspace();
    let url = workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id));
    let Some(url) = url else {
        return AppDisplay {
            name: readable_identifier(bundle_id),
            icon: None,
            installed: false,
        };
    };
    let Some(path) = url.path() else {
        return AppDisplay {
            name: readable_identifier(bundle_id),
            icon: None,
            installed: false,
        };
    };
    // `displayNameAtPath:` is what Finder shows: localized, and with the ".app"
    // suffix already removed under the user's settings rather than by us guessing.
    let name = NSFileManager::defaultManager()
        .displayNameAtPath(&path)
        .to_string();
    let icon = Some(workspace.iconForFile(&path));
    AppDisplay {
        name: if name.is_empty() {
            readable_identifier(bundle_id)
        } else {
            name
        },
        icon,
        installed: true,
    }
}

/// The last segment of a bundle identifier, capitalized — `com.apple.Terminal` →
/// `Terminal`.
///
/// Only a fallback now. It used to be what the excluded list always showed, which
/// is why that list read `Wezterm`, `Iterm2` and `Intellij`: the segment is a
/// programmer's identifier, not the app's name, and capitalizing its first letter
/// does not make it one.
fn readable_identifier(bundle_id: &str) -> String {
    let leaf = bundle_id.rsplit('.').next().unwrap_or(bundle_id);
    if leaf.is_empty() {
        return bundle_id.to_string();
    }
    let mut chars = leaf.chars();
    let first = chars.next().unwrap_or_default();
    format!("{}{}", first.to_uppercase(), chars.as_str())
}
