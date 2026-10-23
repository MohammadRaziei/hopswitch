//! Tests for choosing the folder where Hopswitch keeps its files.

use std::path::PathBuf;

use hopswitch_cli::{default_dir, Platform};

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_owned())
    }
}

#[test]
fn an_explicit_override_wins_everywhere() {
    for platform in [Platform::Windows, Platform::MacOs, Platform::Other] {
        let dir = default_dir(
            platform,
            &env(&[("HOPSWITCH_DIR", "/custom"), ("HOME", "/h")]),
        );
        assert_eq!(dir, Some(PathBuf::from("/custom")));
    }
}

#[test]
fn an_empty_override_is_ignored() {
    let dir = default_dir(
        Platform::Other,
        &env(&[("HOPSWITCH_DIR", ""), ("HOME", "/h")]),
    );

    assert_eq!(dir, Some(PathBuf::from("/h/.config/hopswitch")));
}

#[test]
fn windows_uses_appdata() {
    let dir = default_dir(
        Platform::Windows,
        &env(&[("APPDATA", r"C:\Users\me\AppData\Roaming")]),
    );

    assert_eq!(
        dir,
        Some(PathBuf::from(r"C:\Users\me\AppData\Roaming").join("Hopswitch"))
    );
}

#[test]
fn macos_uses_application_support() {
    let dir = default_dir(Platform::MacOs, &env(&[("HOME", "/Users/me")]));

    assert_eq!(
        dir,
        Some(PathBuf::from(
            "/Users/me/Library/Application Support/Hopswitch"
        ))
    );
}

#[test]
fn linux_prefers_xdg_then_home() {
    let xdg = default_dir(
        Platform::Other,
        &env(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]),
    );
    assert_eq!(xdg, Some(PathBuf::from("/x/hopswitch")));

    let home = default_dir(Platform::Other, &env(&[("HOME", "/h")]));
    assert_eq!(home, Some(PathBuf::from("/h/.config/hopswitch")));
}

#[test]
fn without_any_usable_variable_there_is_no_directory() {
    assert_eq!(default_dir(Platform::Windows, &env(&[])), None);
    assert_eq!(default_dir(Platform::Other, &env(&[])), None);
}
