import datetime
import json
import tempfile
import unittest
from pathlib import Path

import report

STAMP = "2026-10-26T10:11:12.1234567Z "

CARGO_FAILURE = """\
running 2 tests
test a_passing_test ... ok
test a_group_pointing_at_a_missing_proxy ... FAILED

failures:

---- a_group_pointing_at_a_missing_proxy stdout ----
thread 'a_group_pointing_at_a_missing_proxy' panicked at crates/core/tests/groups.rs:10:5:
assertion `left == right` failed
  left: 1
 right: 2
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    a_group_pointing_at_a_missing_proxy

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
error: test failed, to rerun pass `-p hopswitch-core --test groups`
##[error]Process completed with exit code 101.
"""

CLIPPY_FAILURE = """\
    Checking hopswitch-core v0.1.0 (/home/runner/work/x/crates/core)
error: used `assert!` to check that a value is empty
  --> crates/core/tests/profiles_rules.rs:90:5
   |
90 |     assert!(profiles.rules_profiles().is_empty());
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/

error: could not compile `hopswitch-core` (test "profiles_rules") due to 1 previous error
##[error]Process completed with exit code 101.
"""


def stamped(text: str) -> str:
    return "".join(STAMP + line + "\n" for line in text.splitlines())


class Extract(unittest.TestCase):
    def test_a_failed_test_is_named_and_its_panic_is_kept(self):
        got = "\n".join(report.extract(stamped(CARGO_FAILURE)))
        self.assertIn("FAILED tests: a_group_pointing_at_a_missing_proxy", got)
        self.assertIn("panicked at crates/core/tests/groups.rs:10:5", got)
        self.assertIn("left: 1", got)
        self.assertIn("Process completed with exit code 101.", got)
        self.assertNotIn("a_passing_test", got)
        self.assertNotIn("2026-10-26T", got)

    def test_the_backtrace_of_a_panic_is_left_out(self):
        log = "---- t stdout ----\nthread 't' panicked at a.rs:1:1:\nboom\nstack backtrace:\n   0: noise\n"
        got = "\n".join(report.extract(log))
        self.assertIn("boom", got)
        self.assertNotIn("noise", got)

    def test_a_clippy_error_keeps_its_location_and_snippet(self):
        got = "\n".join(report.extract(stamped(CLIPPY_FAILURE)))
        self.assertIn("used `assert!` to check that a value is empty", got)
        self.assertIn("--> crates/core/tests/profiles_rules.rs:90:5", got)
        self.assertIn("assert!(profiles.rules_profiles().is_empty());", got)

    def test_ansi_colours_are_removed(self):
        got = report.extract("\x1b[31merror\x1b[0m: boom\n\x1b[1m  --> a.rs:1:1\x1b[0m\n")
        self.assertEqual(got[0], "error: boom")

    def test_a_log_with_no_known_pattern_falls_back_to_its_tail(self):
        log = "\n".join(f"line {n}" for n in range(200))
        got = report.extract(log)
        self.assertEqual(got[-1], "line 199")
        self.assertLessEqual(len(got), report.FALLBACK_TAIL)

    def test_output_is_capped(self):
        log = "\n".join(f"error: problem number {n}\n  --> f.rs:{n}:1\n" for n in range(100))
        got = report.extract(log)
        self.assertLessEqual(len(got), report.MAX_LINES_PER_JOB + 1)
        self.assertIn("more lines cut", got[-1])

    def test_the_same_block_is_not_repeated(self):
        got = report.extract(CARGO_FAILURE + CARGO_FAILURE)
        self.assertEqual(sum("panicked at" in line for line in got), 1)


def fixture(jobs: list[dict], logs: dict[int, str], run: dict | None = None) -> Path:
    root = Path(tempfile.mkdtemp())
    (root / "logs").mkdir()
    base = {"head_sha": "abc1234", "head_branch": "main", "event": "push",
            "html_url": "https://example.test/run/1", "head_commit": {"message": "feat: x\n\nbody"}}
    (root / "run.json").write_text(json.dumps(run or base))
    (root / "jobs.json").write_text(json.dumps({"jobs": jobs}))
    for job_id, text in logs.items():
        (root / "logs" / f"{job_id}.txt").write_text(text)
    return root


NOW = datetime.datetime(2026, 10, 26, 12, 0, 0, tzinfo=datetime.timezone.utc)


class Build(unittest.TestCase):
    def test_all_green(self):
        jobs = [{"id": 1, "name": "fmt + clippy", "conclusion": "success"},
                {"id": 2, "name": "failure report", "conclusion": None, "status": "in_progress"}]
        text = report.build(report.Source("o/r", "1", fixture(jobs, {})), {"failure report"}, None, NOW)
        self.assertIn("**all green** (1 of 1 jobs ok)", text)
        self.assertIn("`abc1234` on `main` - feat: x", text)
        self.assertNotIn("failure report", text)
        self.assertNotIn("## FAILED", text)

    def test_a_failed_job_gets_its_step_and_its_errors(self):
        jobs = [
            {"id": 1, "name": "fmt + clippy", "conclusion": "failure",
             "steps": [{"name": "cargo fmt", "conclusion": "success"},
                       {"name": "cargo clippy", "conclusion": "failure"}]},
            {"id": 2, "name": "test (windows-latest)", "conclusion": "success"},
            {"id": 3, "name": "cancelled one", "conclusion": "cancelled"},
        ]
        text = report.build(report.Source("o/r", "1", fixture(jobs, {1: stamped(CLIPPY_FAILURE)})), set(), None, NOW)
        self.assertIn("**FAILED** (1 of 3 jobs ok)", text)
        self.assertIn("## FAILED: fmt + clippy", text)
        self.assertIn("failed step: cargo clippy", text)
        self.assertIn("--> crates/core/tests/profiles_rules.rs:90:5", text)
        self.assertIn("## FAILED: cancelled one", text)
        self.assertNotIn("## FAILED: test (windows-latest)", text)

    def test_an_extra_file_is_appended(self):
        jobs = [{"id": 1, "name": "j", "conclusion": "success"}]
        text = report.build(report.Source("o/r", "1", fixture(jobs, {})), set(), "### PAC table\n| a |", NOW)
        self.assertTrue(text.rstrip().endswith("| a |"))


if __name__ == "__main__":
    unittest.main()
