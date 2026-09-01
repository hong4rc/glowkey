//! Resolving the frontmost application's display name and bundle identifier, for
//! the menu bar's "Enable/Disable for <App>" label.

use objc2_app_kit::NSWorkspace;

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
