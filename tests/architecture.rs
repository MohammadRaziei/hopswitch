//! The layering of the package (ADR-0013). Before the crates were merged into one package, Cargo
//! enforced who may use whom; these tests do the same by reading the sources.
//!
//! `main.rs` is not checked: it is where the parts are put together (it gives the CLI the Windows
//! adapter and the prober).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Each module of `src/` and the other modules it may use. A new module must be added here, which
/// is the moment to decide where it belongs.
const ALLOWED: &[(&str, &[&str])] = &[
    ("core", &[]),
    ("storage", &["core"]),
    ("http", &[]),
    ("probe", &["core"]),
    ("platform_windows", &["core"]),
    ("pacserver", &["core", "http"]),
    ("cli", &["core", "storage", "http"]),
];

/// What the domain must never touch: the file system, processes, the environment, the operating
/// system, and real sockets. (Plain data types such as `std::net::IpAddr` are fine.)
const FORBIDDEN_IN_CORE: &[&str] = &[
    "std::fs",
    "std::process",
    "std::env",
    "std::os",
    "std::io::",
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "windows_sys",
];

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The `.rs` files of a module: `src/<name>.rs` or everything under `src/<name>/`.
fn files_of(module: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let single = src().join(format!("{module}.rs"));
    if single.is_file() {
        return vec![single];
    }
    let mut files = Vec::new();
    walk(&src().join(module), &mut files);
    files
}

/// The lines of code of a file, without comments (a doc link to another module is not a use).
fn code_lines(path: &Path) -> Vec<(usize, String)> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .map(|(n, line)| (n + 1, line.to_owned()))
        .collect()
}

/// The names that follow `crate::` on a line: `crate::core::X` gives `core`.
fn crate_paths(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("crate::") {
        rest = &rest[at + "crate::".len()..];
        if let Some(inner) = rest.strip_prefix('{') {
            let inside = inner.split('}').next().unwrap_or_default();
            found.extend(inside.split(',').map(|part| {
                let path = part.split("::").next().unwrap_or_default();
                // `core as other` names the module `core`.
                path.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            }));
        } else {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            found.push(name);
        }
    }
    found.retain(|name| !name.is_empty());
    found
}

#[test]
fn every_module_of_the_package_is_in_the_table() {
    let mut on_disk = BTreeSet::new();
    for entry in fs::read_dir(src()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if name != "lib" && name != "main" {
            on_disk.insert(name);
        }
    }
    let in_table: BTreeSet<String> = ALLOWED.iter().map(|(m, _)| (*m).to_owned()).collect();
    assert_eq!(
        on_disk, in_table,
        "src/ and ALLOWED in tests/architecture.rs must list the same modules"
    );
}

#[test]
fn a_module_uses_only_the_modules_it_is_allowed_to() {
    let mut violations = Vec::new();
    for (module, allowed) in ALLOWED {
        for file in files_of(module) {
            for (number, line) in code_lines(&file) {
                for used in crate_paths(&line) {
                    if used != *module && !allowed.contains(&used.as_str()) {
                        violations.push(format!(
                            "{}:{number}: `{module}` must not use `{used}`",
                            file.strip_prefix(src()).unwrap().display()
                        ));
                    }
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "layering broken:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_domain_does_not_touch_the_operating_system() {
    let mut violations = Vec::new();
    for file in files_of("core") {
        for (number, line) in code_lines(&file) {
            for word in FORBIDDEN_IN_CORE {
                if line.contains(word) {
                    violations.push(format!(
                        "{}:{number}: core must not use `{word}`",
                        file.strip_prefix(src()).unwrap().display()
                    ));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "core is not pure:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_package_has_no_dependency_that_would_tie_the_domain_to_an_operating_system() {
    // The manifest is shared by all modules, so this is the one place that would show an
    // operating-system crate arriving.
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .and_then(|rest| rest.split("\n[").next())
        .unwrap();
    for name in [
        "windows",
        "windows-sys",
        "winreg",
        "winapi",
        "libc",
        "nix",
        "tauri",
    ] {
        assert!(
            !dependencies
                .lines()
                .any(|l| l.trim_start().starts_with(&format!("{name} "))
                    || l.trim_start().starts_with(&format!("{name}="))),
            "`{name}` is a dependency; decide where it may be used before adding it"
        );
    }
}
