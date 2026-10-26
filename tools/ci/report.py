#!/usr/bin/env python3
"""Builds a short Markdown report of one GitHub Actions run: which jobs failed and the lines of their
logs that say why (failed tests, compiler and clippy errors, ##[error] messages).

    report.py --repo OWNER/NAME --run 123 --out report.md [--skip "failure report"] [--extra file.md]

Needs the `gh` CLI with GH_TOKEN set (the workflow token is enough: actions: read). For tests,
`--fixture DIR` reads run.json, jobs.json and logs/<job id>.txt from a folder instead of calling gh.
The report is meant to be read by a person or a tool without opening the Actions UI.
"""

import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

TIMESTAMP = re.compile(r"^\d{4}-\d\d-\d\dT[\d:.]+Z ?")
ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
FAILED_TEST = re.compile(r"^test (\S+) \.\.\. FAILED$")
TEST_STDOUT = re.compile(r"^---- .+ (stdout|stderr) ----$")
COMPILER_ERROR = re.compile(r"^error(\[[A-Za-z0-9]+\])?: ")
MAX_BLOCK = 14
MAX_LINES_PER_JOB = 90
FALLBACK_TAIL = 40


def clean(line: str) -> str:
    return ANSI.sub("", TIMESTAMP.sub("", line.rstrip("\r\n")))


def block(lines: list[str], start: int, stop_at_blank_after: int = 1) -> list[str]:
    """`lines[start:]` up to a blank line (after at least `stop_at_blank_after` lines) or MAX_BLOCK."""
    out = []
    for line in lines[start : start + MAX_BLOCK]:
        if line.strip() == "" and len(out) >= stop_at_blank_after:
            break
        if out and (line.startswith(("failures:", "stack backtrace:")) or "##[error]" in line):
            break
        out.append(line)
    return out


def extract(log: str) -> list[str]:
    """The lines of a job log that explain a failure, de-duplicated, in order."""
    lines = [clean(raw) for raw in log.splitlines()]
    picked: list[str] = []
    seen: set[tuple[str, ...]] = set()

    def add(chunk: list[str]) -> None:
        key = tuple(chunk)
        if chunk and key not in seen:
            seen.add(key)
            picked.extend(chunk + [""])

    failed_tests = [m.group(1) for line in lines if (m := FAILED_TEST.match(line))]
    if failed_tests:
        add(["FAILED tests: " + ", ".join(dict.fromkeys(failed_tests))])
    for i, line in enumerate(lines):
        if TEST_STDOUT.match(line):
            add(block(lines, i, stop_at_blank_after=2))
        elif COMPILER_ERROR.match(line):
            add(block(lines, i))
        elif "##[error]" in line:
            add([line.split("##[error]", 1)[1].strip()])
        elif line.startswith("test result: FAILED"):
            add([line])
    if not picked:
        add([line for line in lines[-FALLBACK_TAIL:] if line.strip()])
    while picked and picked[-1] == "":
        picked.pop()
    if len(picked) > MAX_LINES_PER_JOB:
        picked = picked[:MAX_LINES_PER_JOB] + [f"... ({len(picked) - MAX_LINES_PER_JOB} more lines cut)"]
    return picked


class Source:
    """Where run data comes from: the GitHub API through `gh`, or a folder of fixtures."""

    def __init__(self, repo: str, run: str, fixture: Path | None):
        self.repo, self.run, self.fixture = repo, run, fixture

    def run_info(self) -> dict:
        if self.fixture:
            return json.loads((self.fixture / "run.json").read_text())
        return json.loads(self.gh(f"repos/{self.repo}/actions/runs/{self.run}"))

    def jobs(self) -> list[dict]:
        if self.fixture:
            return json.loads((self.fixture / "jobs.json").read_text())["jobs"]
        return json.loads(self.gh(f"repos/{self.repo}/actions/runs/{self.run}/jobs?per_page=100"))["jobs"]

    def log(self, job_id: int) -> str:
        if self.fixture:
            path = self.fixture / "logs" / f"{job_id}.txt"
            return path.read_text() if path.exists() else ""
        try:
            return self.gh(f"repos/{self.repo}/actions/jobs/{job_id}/logs")
        except (subprocess.CalledProcessError, OSError) as error:
            return f"(could not download the log: {error})"

    @staticmethod
    def gh(path: str) -> str:
        """GET api.github.com/<path>: through `gh` when it is installed, else plain HTTPS (with
        GH_TOKEN when set). Job logs need a token even for public repositories."""
        if shutil.which("gh"):
            return subprocess.run(
                ["gh", "api", "-H", "Accept: application/vnd.github+json", path],
                check=True, capture_output=True, text=True,
            ).stdout
        request = urllib.request.Request(
            f"https://api.github.com/{path}", headers={"Accept": "application/vnd.github+json"}
        )
        if os.environ.get("GH_TOKEN"):
            request.add_header("Authorization", f"Bearer {os.environ['GH_TOKEN']}")
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.read().decode("utf-8", "replace")


def build(source: Source, skip: set[str], extra: str | None, now: datetime.datetime) -> str:
    run = source.run_info()
    jobs = [j for j in source.jobs() if j["name"] not in skip]
    bad = [j for j in jobs if j.get("conclusion") not in ("success", "skipped", None)]
    message = (run.get("head_commit") or {}).get("message", "").splitlines()
    out = [
        "# CI report",
        "",
        f"- commit: `{run.get('head_sha', '?')}` on `{run.get('head_branch', '?')}`"
        + (f" - {message[0]}" if message else ""),
        f"- event: {run.get('event', '?')}, run: {run.get('html_url', '?')}",
        f"- result: **{'FAILED' if bad else 'all green'}** ({len(jobs) - len(bad)} of {len(jobs)} jobs ok)",
        f"- written: {now.strftime('%Y-%m-%d %H:%M:%S')} UTC",
        "",
        "| job | result |",
        "| --- | --- |",
    ]
    out += [f"| {j['name']} | {j.get('conclusion') or j.get('status')} |" for j in jobs]
    for job in bad:
        steps = [s["name"] for s in job.get("steps", []) if s.get("conclusion") == "failure"]
        out += ["", f"## FAILED: {job['name']}", ""]
        if steps:
            out += [f"failed step: {', '.join(steps)}", ""]
        out += ["```text", *extract(source.log(job["id"])), "```"]
    if extra:
        out += ["", extra.strip()]
    return "\n".join(out) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default="")
    parser.add_argument("--run", default="")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--skip", action="append", default=[], help="job name to leave out (this report's own job)")
    parser.add_argument("--extra", type=Path, help="a Markdown file to append (for example a benchmark table)")
    parser.add_argument("--fixture", type=Path)
    args = parser.parse_args()
    extra = args.extra.read_text(encoding="utf-8") if args.extra and args.extra.exists() else None
    source = Source(args.repo, args.run, args.fixture)
    now = datetime.datetime.now(datetime.timezone.utc)
    args.out.write_text(build(source, set(args.skip), extra, now), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
