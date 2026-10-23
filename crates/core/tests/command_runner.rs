//! Tests for the `CommandRunner` fake used to test OS adapters on any machine.

use hopswitch_core::testing::FakeRunner;
use hopswitch_core::{CommandRunner, SystemError};

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn by_default_every_command_succeeds_with_empty_output() {
    let runner = FakeRunner::new();

    let out = runner.run("tool", &args(&["a"])).unwrap();

    assert!(out.success);
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, "");
}

#[test]
fn scripted_responses_are_returned_in_order_then_back_to_default() {
    let runner = FakeRunner::new();
    runner.push_stdout("first");
    runner.push_failure("boom");
    runner.push_error(SystemError::Unsupported("nope".into()));

    assert_eq!(runner.run("t", &[]).unwrap().stdout, "first");
    let failed = runner.run("t", &[]).unwrap();
    assert!(!failed.success);
    assert_eq!(failed.stderr, "boom");
    assert_eq!(
        runner.run("t", &[]).unwrap_err(),
        SystemError::Unsupported("nope".into())
    );
    assert!(runner.run("t", &[]).unwrap().success);
}

#[test]
fn every_call_is_recorded_with_program_and_arguments() {
    let runner = FakeRunner::new();

    runner
        .run("powershell", &args(&["-NoProfile", "x"]))
        .unwrap();
    runner.run("other", &[]).unwrap();

    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].program, "powershell");
    assert_eq!(calls[0].args, args(&["-NoProfile", "x"]));
    assert_eq!(calls[1].program, "other");
}
