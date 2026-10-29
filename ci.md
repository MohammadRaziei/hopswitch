# CI report

- commit: `85f778d69e3c908bcac53cf74a0014204ab8d12b` on `master` - feat: add release run report job for CI failure and asset summary tracking
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/40973565389
- result: **FAILED** (5 of 6 jobs ok)
- written: 2026-10-29 23:32:10 UTC

| job | result |
| --- | --- |
| test (ubuntu-latest) | success |
| fmt + clippy | success |
| test (Rust 1.75, the declared minimum) | success |
| test (macos-latest) | success |
| test (windows-latest) | cancelled |
| Windows proxy adapter (real registry) | success |

## FAILED: test (windows-latest)

```text
(could not download the log: Command '['gh', 'api', '-H', 'Accept: application/vnd.github+json', 'repos/MohammadRaziei/hopswitch/actions/jobs/123796007174/logs']' returned non-zero exit status 1.)
```
