//! What the bare `hopswitch` command does (ADR-0012): open the GUI when there is a graphical
//! session and the GUI is installed, otherwise explain why not.
//!
//! This is a pure decision: the environment and the file system are passed in, so it is tested on
//! every platform. Starting the program it names is the caller's one untested line.

use std::path::{Path, PathBuf};

use crate::cli::Platform;

/// What the bare command should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// Start this program (the GUI, installed next to `hopswitch`).
    Gui(PathBuf),
    /// Do not open a window; show the help and the reason.
    NoGui(NoGuiReason),
}

/// Why no window is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoGuiReason {
    /// There is no graphical session (a server, or an SSH login without a display).
    NoDisplay,
    /// There is a graphical session but the GUI program is not installed.
    GuiNotInstalled,
}

/// The name of the GUI program on `platform`.
fn gui_file_name(platform: Platform) -> &'static str {
    if platform == Platform::Windows {
        "hopswitch-gui.exe"
    } else {
        "hopswitch-gui"
    }
}

/// Whether a variable is set to something.
fn is_set(env: &dyn Fn(&str) -> Option<String>, key: &str) -> bool {
    env(key).is_some_and(|value| !value.is_empty())
}

/// Whether a graphical session is available, judged from the environment: on Linux a display
/// variable; on macOS and Windows a desktop unless the process runs inside an SSH login.
fn has_display(platform: Platform, env: &dyn Fn(&str) -> Option<String>) -> bool {
    match platform {
        Platform::Other => is_set(env, "DISPLAY") || is_set(env, "WAYLAND_DISPLAY"),
        Platform::Windows | Platform::MacOs => !is_set(env, "SSH_CONNECTION"),
    }
}

/// Decides what the bare command does. `exe_dir` is the folder of the running `hopswitch`, and
/// `exists` tells whether a file is there.
#[must_use]
pub fn decide_launch(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<String>,
    exe_dir: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Launch {
    if !has_display(platform, env) {
        return Launch::NoGui(NoGuiReason::NoDisplay);
    }
    let gui = exe_dir.join(gui_file_name(platform));
    if exists(&gui) {
        Launch::Gui(gui)
    } else {
        Launch::NoGui(NoGuiReason::GuiNotInstalled)
    }
}

/// One line for the person who typed `hopswitch` and got no window. (When `hopswitch serve`
/// exists, ADR-0012 step b, this is where it is mentioned; it must not point at a command that
/// does not exist.)
#[must_use]
pub fn no_gui_hint(reason: NoGuiReason) -> &'static str {
    match reason {
        NoGuiReason::NoDisplay => {
            "No graphical session was found, so no window was opened. Use the commands above."
        }
        NoGuiReason::GuiNotInstalled => {
            "The graphical app is not installed, so no window was opened. Use the commands above."
        }
    }
}
