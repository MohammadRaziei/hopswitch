//! Where Hopswitch keeps its files.

use std::path::PathBuf;

/// The operating-system family, as far as file locations are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux and everything else.
    Other,
}

impl Platform {
    /// The platform this build runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
}

/// The settings folder: `HOPSWITCH_DIR` if set, otherwise the platform's usual
/// per-user location. `None` when no usable environment variable exists.
pub fn default_dir(platform: Platform, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let get = |key: &str| env(key).filter(|value| !value.is_empty());

    if let Some(custom) = get("HOPSWITCH_DIR") {
        return Some(PathBuf::from(custom));
    }
    match platform {
        Platform::Windows => get("APPDATA").map(|dir| PathBuf::from(dir).join("Hopswitch")),
        Platform::MacOs => get("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Hopswitch")
        }),
        Platform::Other => get("XDG_CONFIG_HOME")
            .map(|dir| PathBuf::from(dir).join("hopswitch"))
            .or_else(|| {
                get("HOME").map(|home| PathBuf::from(home).join(".config").join("hopswitch"))
            }),
    }
}
