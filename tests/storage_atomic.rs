//! Tests for the atomic file-write primitive.

use std::fs;

use hopswitch::storage::write_atomic;

#[test]
fn creates_the_file_with_the_given_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.json");

    write_atomic(&path, b"hello").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"hello");
}

#[test]
fn creates_missing_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("data.json");

    write_atomic(&path, b"x").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"x");
}

#[test]
fn replaces_existing_content_completely() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.json");
    write_atomic(&path, b"a much longer first version").unwrap();

    write_atomic(&path, b"short").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"short");
}

#[test]
fn leaves_no_temporary_files_behind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.json");

    write_atomic(&path, b"one").unwrap();
    write_atomic(&path, b"two").unwrap();

    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["data.json"]);
}

#[test]
fn a_stale_temp_file_from_a_crashed_run_does_not_get_in_the_way() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.json");
    fs::write(dir.path().join(".data.json.tmp"), b"half-written garbage").unwrap();

    write_atomic(&path, b"good").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"good");
}

#[test]
fn fails_cleanly_when_the_target_is_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.json");
    fs::create_dir(&path).unwrap();

    assert!(write_atomic(&path, b"x").is_err());
    assert!(path.is_dir(), "the existing entry is untouched");
}
