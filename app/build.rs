//! Stamps the git commit into the binary so the About window can name the exact
//! build a user is running.
//!
//! The version alone does not identify a build. GlowKey ships from a tag but is
//! also installed straight from a working tree by `just install`, so "0.1.0" can
//! mean any of a dozen builds — and the questions that get asked about this app
//! ("does your copy have the freeze fix?") are answered by the commit, not the
//! version.
//!
//! Absent git — a source tarball, a vendored build — this is not an error. The
//! About window says the version and omits the commit, which is exactly as much
//! as is true.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    watch_git_head(&root);
    stamp_windows_resources(&root);

    let commit = describe(&root).unwrap_or_default();
    // Always set, possibly empty: `env!` is a compile error on a missing
    // variable, so the alternative is a conditional in every consumer.
    println!("cargo:rustc-env=GLOWKEY_COMMIT={commit}");
}

/// Stamps the application icon and version metadata into the Windows executable.
///
/// Without it the binary has no icon at all: a blank sheet in Explorer, in
/// Alt-Tab, on the taskbar and in the "Windows protected your PC" dialog an
/// unsigned download shows. For a keyboard hook that already asks for more trust
/// than most programs, looking anonymous is not a neutral choice.
///
/// The .ico is **committed**, exactly as the macOS .icns is, so an ordinary build
/// needs no image tooling.  regenerates it from the shared
/// vector master when the artwork changes.
///
/// Absent icon file: a warning, not an error. A source checkout missing the
/// resource should still produce a working input method.
#[allow(unused_variables)]
fn stamp_windows_resources(root: &Path) {
    #[cfg(target_os = "windows")]
    {
        let icon = root.join("Resources").join("AppIcon.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        if !icon.exists() {
            println!("cargo:warning=no AppIcon.ico — the executable will have no icon");
            return;
        }
        let mut res = winresource::WindowsResource::new();
        res.set_icon(&icon.to_string_lossy());
        res.set("ProductName", "GlowKey");
        res.set("FileDescription", "GlowKey — Vietnamese input method");
        res.set("LegalCopyright", "MIT licensed");
        if let Err(err) = res.compile() {
            println!("cargo:warning=could not stamp the icon: {err}");
        }
    }
}

/// Rebuild when the checked-out commit changes.
///
/// Without this the stamp is whatever it was on the first build of the session,
/// which is worse than no stamp: a version string that names the wrong commit
/// sends someone debugging the wrong code.
fn watch_git_head(root: &Path) {
    let Some(git_dir) = git_dir(root) else {
        return;
    };
    let head = git_dir.join("HEAD");
    if !head.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", head.display());

    // `HEAD` itself only changes when the *branch* changes, so committing on the
    // same branch would not be noticed. The ref it points at is what moves.
    let Ok(contents) = std::fs::read_to_string(&head) else {
        return;
    };
    if let Some(reference) = contents.strip_prefix("ref: ").map(str::trim) {
        let path = git_dir.join(reference);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        // A packed ref has no file of its own; the pack is what changes.
        let packed = git_dir.join("packed-refs");
        if packed.exists() {
            println!("cargo:rerun-if-changed={}", packed.display());
        }
    }
}

/// The repository's git directory, asked of git rather than assumed: `.git` is a
/// *file* in a worktree or submodule, and this repo is developed in worktrees.
fn git_dir(root: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args([
            "-C",
            &root.to_string_lossy(),
            "rev-parse",
            "--absolute-git-dir",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    path.exists().then_some(path)
}

/// The short commit, with a `+` when the working tree has uncommitted changes.
///
/// The marker is not decoration. A build made from a dirty tree is not the commit
/// it names, and a bug report quoting a bare hash would send the reader to source
/// that never produced the binary.
fn describe(root: &Path) -> Option<String> {
    let root = root.to_string_lossy().to_string();
    let output = Command::new("git")
        .args(["-C", &root, "rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if commit.is_empty() {
        return None;
    }
    let dirty = Command::new("git")
        .args(["-C", &root, "status", "--porcelain", "--untracked-files=no"])
        .output()
        .map(|out| out.status.success() && !out.stdout.is_empty())
        .unwrap_or(false);
    Some(if dirty { format!("{commit}+") } else { commit })
}
