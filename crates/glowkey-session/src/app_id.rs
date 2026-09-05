//! The identity of an application, as its platform names it.

use std::fmt;

/// An application's identity: a bundle identifier on macOS
/// (`com.apple.Terminal`), an executable file name on Windows
/// (`windowsterminal.exe`), whatever a future platform uses.
///
/// Opaque here on purpose. The session compares identities and looks them up in
/// the exclusion list; it never parses one, so it cannot grow an opinion about
/// what an identity looks like on any one platform.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppId(String);

impl AppId {
    /// Wraps an identity string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identity as the platform spelled it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for AppId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&String> for AppId {
    fn from(id: &String) -> Self {
        Self(id.clone())
    }
}

impl From<&str> for AppId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

impl AsRef<str> for AppId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
