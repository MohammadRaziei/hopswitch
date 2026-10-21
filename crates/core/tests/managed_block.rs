//! Behavioural tests for the managed block used to edit files such as `/etc/resolv.conf`.
//!
//! The central promise: **everything outside our block is preserved byte-for-byte**,
//! and removing our block restores the original file exactly.

use hopswitch_core::managed_block::{
    apply_block, contains_block, remove_block, BlockError, BEGIN_MARKER, END_MARKER,
};

const DNS: [&str; 2] = ["nameserver 1.1.1.1", "nameserver 8.8.8.8"];

fn block(lines: &[&str]) -> String {
    let mut s = format!("{BEGIN_MARKER}\n");
    for l in lines {
        s.push_str(l);
        s.push('\n');
    }
    s.push_str(END_MARKER);
    s.push('\n');
    s
}

#[test]
fn inserts_the_block_at_the_top_and_keeps_the_original_untouched() {
    let original = "nameserver 9.9.9.9\nsearch lan\n";

    let result = apply_block(original, &DNS).unwrap();

    assert_eq!(result, format!("{}{original}", block(&DNS)));
}

#[test]
fn never_drops_the_users_own_entries_comments_or_options() {
    let original = "# written by me, please keep\nnameserver 192.168.1.1\nnameserver 10.0.0.1\n\
                    options edns0 trust-ad\nsearch corp.example.com # دامنه‌ی شرکت";

    let result = apply_block(original, &DNS).unwrap();

    assert!(
        result.ends_with(original),
        "original text must follow the block unchanged"
    );
    for line in original.lines() {
        assert!(result.contains(line), "lost line: {line}");
    }
}

#[test]
fn works_on_an_empty_file() {
    assert_eq!(apply_block("", &DNS).unwrap(), block(&DNS));
}

#[test]
fn replaces_an_existing_block_in_place() {
    let before = "# my note\n";
    let after = "nameserver 9.9.9.9\n";
    let original = format!("{before}{}{after}", block(&["nameserver 4.4.4.4"]));

    let result = apply_block(&original, &DNS).unwrap();

    assert_eq!(result, format!("{before}{}{after}", block(&DNS)));
}

#[test]
fn applying_twice_gives_the_same_result() {
    let original = "nameserver 9.9.9.9\n";
    let once = apply_block(original, &DNS).unwrap();
    let twice = apply_block(&once, &DNS).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn removing_the_block_restores_the_original_exactly() {
    let samples = [
        "",
        "x",
        "x\n",
        "nameserver 9.9.9.9\nsearch lan",
        "\n\n# blank lines first\nnameserver 1.1.1.1\n\n",
        "# unicode: سلام دنیا\nnameserver ::1\n",
    ];
    for original in samples {
        let applied = apply_block(original, &DNS).unwrap();
        assert_eq!(
            remove_block(&applied).unwrap(),
            original,
            "sample: {original:?}"
        );
    }
}

#[test]
fn removing_when_there_is_no_block_changes_nothing() {
    let original = "nameserver 9.9.9.9\n";
    assert_eq!(remove_block(original).unwrap(), original);
}

#[test]
fn an_empty_line_list_behaves_like_remove() {
    let original = "nameserver 9.9.9.9\n";
    let applied = apply_block(original, &DNS).unwrap();
    let none: [&str; 0] = [];
    assert_eq!(apply_block(&applied, &none).unwrap(), original);
}

#[test]
fn reports_whether_a_block_is_present() {
    assert!(!contains_block("nameserver 9.9.9.9\n").unwrap());
    assert!(contains_block(&block(&DNS)).unwrap());
}

#[test]
fn refuses_to_touch_a_file_with_an_unterminated_block() {
    let broken = format!("{BEGIN_MARKER}\nnameserver 1.1.1.1\nnameserver 9.9.9.9\n");
    assert_eq!(
        apply_block(&broken, &DNS).unwrap_err(),
        BlockError::UnterminatedBlock
    );
    assert_eq!(
        remove_block(&broken).unwrap_err(),
        BlockError::UnterminatedBlock
    );
}

#[test]
fn refuses_to_touch_a_file_with_a_stray_end_marker() {
    let broken = format!("nameserver 9.9.9.9\n{END_MARKER}\n");
    assert_eq!(
        apply_block(&broken, &DNS).unwrap_err(),
        BlockError::StrayEndMarker
    );
    assert_eq!(
        remove_block(&broken).unwrap_err(),
        BlockError::StrayEndMarker
    );
}

#[test]
fn refuses_to_touch_a_file_with_two_blocks() {
    let broken = format!("{}{}", block(&DNS), block(&DNS));
    assert_eq!(
        apply_block(&broken, &DNS).unwrap_err(),
        BlockError::DuplicateBlock
    );
    assert_eq!(
        remove_block(&broken).unwrap_err(),
        BlockError::DuplicateBlock
    );
}

#[test]
fn rejects_lines_that_could_break_out_of_the_block() {
    for bad in [
        "nameserver 1.1.1.1\nnameserver 6.6.6.6",
        "a\rb",
        END_MARKER,
        BEGIN_MARKER,
    ] {
        let err = apply_block("", &[bad]).unwrap_err();
        assert_eq!(
            err,
            BlockError::InvalidLine(bad.to_owned()),
            "line: {bad:?}"
        );
    }
}
