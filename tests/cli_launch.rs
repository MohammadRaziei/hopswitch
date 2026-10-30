//! What the bare `hopswitch` command does (ADR-0012 decisions 1 and 2): open the GUI when there is a
//! graphical session and the GUI is installed, otherwise explain and show the help.

use std::path::{Path, PathBuf};

use hopswitch::cli::{decide_launch, no_gui_hint, Launch, NoGuiReason, Platform};

const DIR: &str = "/opt/hopswitch";

fn decide(platform: Platform, vars: &[(&str, &str)], gui_installed: bool) -> Launch {
    let env = |key: &str| {
        vars.iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_owned())
    };
    let gui_name = if platform == Platform::Windows {
        "hopswitch-gui.exe"
    } else {
        "hopswitch-gui"
    };
    let expected = Path::new(DIR).join(gui_name);
    decide_launch(platform, &env, Path::new(DIR), &|p: &Path| {
        gui_installed && p == expected
    })
}

fn gui(name: &str) -> Launch {
    Launch::Gui(PathBuf::from(DIR).join(name))
}

#[test]
fn linux_with_an_x_display_opens_the_gui() {
    assert_eq!(
        decide(Platform::Other, &[("DISPLAY", ":0")], true),
        gui("hopswitch-gui")
    );
}

#[test]
fn linux_with_a_wayland_display_opens_the_gui() {
    assert_eq!(
        decide(Platform::Other, &[("WAYLAND_DISPLAY", "wayland-0")], true),
        gui("hopswitch-gui")
    );
}

#[test]
fn linux_without_a_display_is_headless_even_when_the_gui_is_installed() {
    assert_eq!(
        decide(Platform::Other, &[], true),
        Launch::NoGui(NoGuiReason::NoDisplay)
    );
}

#[test]
fn an_empty_display_variable_counts_as_no_display() {
    assert_eq!(
        decide(
            Platform::Other,
            &[("DISPLAY", ""), ("WAYLAND_DISPLAY", "")],
            true
        ),
        Launch::NoGui(NoGuiReason::NoDisplay)
    );
}

#[test]
fn a_display_without_the_gui_installed_says_so() {
    assert_eq!(
        decide(Platform::Other, &[("DISPLAY", ":0")], false),
        Launch::NoGui(NoGuiReason::GuiNotInstalled)
    );
}

#[test]
fn windows_assumes_a_desktop_and_looks_for_the_exe() {
    assert_eq!(
        decide(Platform::Windows, &[], true),
        gui("hopswitch-gui.exe")
    );
    assert_eq!(
        decide(Platform::Windows, &[], false),
        Launch::NoGui(NoGuiReason::GuiNotInstalled)
    );
}

#[test]
fn an_ssh_session_is_headless_on_windows_and_macos() {
    for platform in [Platform::Windows, Platform::MacOs] {
        assert_eq!(
            decide(
                platform,
                &[("SSH_CONNECTION", "10.0.0.1 5000 10.0.0.2 22")],
                true
            ),
            Launch::NoGui(NoGuiReason::NoDisplay),
            "{platform:?}"
        );
    }
}

#[test]
fn macos_assumes_a_desktop() {
    assert_eq!(decide(Platform::MacOs, &[], true), gui("hopswitch-gui"));
}

#[test]
fn an_ssh_session_with_x_forwarding_on_linux_may_open_the_gui() {
    let vars = [("DISPLAY", "localhost:10.0"), ("SSH_CONNECTION", "a b c d")];
    assert_eq!(decide(Platform::Other, &vars, true), gui("hopswitch-gui"));
}

#[test]
fn the_hint_names_the_reason() {
    let headless = no_gui_hint(NoGuiReason::NoDisplay);
    assert!(headless.contains("graphical session"), "{headless}");

    let missing = no_gui_hint(NoGuiReason::GuiNotInstalled);
    assert!(missing.contains("not installed"), "{missing}");
}
