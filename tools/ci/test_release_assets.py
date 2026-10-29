import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("release_assets.sh")
VERSION = "0.1.0"
URL = "https://github.com/x/y/releases/tag/__beta__"
REQUIRED = [
    f"hopswitch-{VERSION}-x86_64-unknown-linux-musl.tar.gz",
    f"hopswitch_{VERSION}-1_amd64.deb",
    f"hopswitch-{VERSION}-1.x86_64.rpm",
    f"hopswitch-{VERSION}-aarch64-apple-darwin.tar.gz",
    f"hopswitch-{VERSION}-x86_64-apple-darwin.tar.gz",
    f"hopswitch-{VERSION}-x86_64-pc-windows-msvc.zip",
    "SHA256SUMS",
]
MSI = f"hopswitch-{VERSION}-x86_64-pc-windows-msvc.msi"


def report(assets=None, failure=None):
    """Runs the script with a fake `gh` that answers like `gh release view --json ... --jq ...`."""
    with tempfile.TemporaryDirectory() as tmp:
        fake = Path(tmp) / "gh"
        if failure is not None:
            body = f"echo '{failure}' >&2\nexit 1\n"
        else:
            lines = [URL, "true"] + [f"{name}\t1234" for name in assets]
            body = "cat <<'EOF'\n" + "\n".join(lines) + "\nEOF\n"
        fake.write_text("#!/bin/bash\n" + body)
        fake.chmod(fake.stat().st_mode | stat.S_IEXEC)
        env = dict(os.environ, PATH=f"{tmp}:{os.environ['PATH']}", GITHUB_REPOSITORY="x/y")
        result = subprocess.run(
            ["bash", str(SCRIPT), "__beta__", VERSION],
            env=env, capture_output=True, text=True, check=False,
        )
    assert result.returncode == 0, result.stderr
    return result.stdout


class ReleaseAssets(unittest.TestCase):
    def test_a_complete_release_is_said_to_be_complete(self):
        text = report(REQUIRED + [MSI])
        self.assertIn("All expected files are there.", text)
        self.assertNotIn("Missing", text)
        self.assertNotIn("optional, not built", text)
        for name in REQUIRED + [MSI]:
            self.assertIn(f"| {name} | 1234 |", text)

    def test_missing_files_are_named(self):
        text = report([REQUIRED[0], "SHA256SUMS"])
        self.assertIn("**Missing files:**", text)
        for name in REQUIRED[1:-1]:
            self.assertIn(f"- `{name}`", text)
        self.assertNotIn(f"- `{REQUIRED[0]}`", text)
        self.assertNotIn("All expected files are there.", text)

    def test_the_installer_is_optional_and_only_noted(self):
        text = report(REQUIRED)
        self.assertIn("All expected files are there.", text)
        self.assertIn(f"- optional, not built: `{MSI}`", text)

    def test_an_empty_release_lists_every_required_file_as_missing(self):
        text = report([])
        for name in REQUIRED:
            self.assertIn(f"- `{name}`", text)

    def test_a_name_that_only_contains_an_expected_name_does_not_count(self):
        text = report([name + ".sig" for name in REQUIRED])
        self.assertIn("**Missing files:**", text)
        self.assertIn(f"- `{REQUIRED[0]}`", text)

    def test_a_release_that_cannot_be_read_is_reported_without_failing(self):
        text = report(failure="release not found")
        self.assertIn("The release could not be read: release not found", text)
        self.assertNotIn("Missing", text)


if __name__ == "__main__":
    unittest.main()
