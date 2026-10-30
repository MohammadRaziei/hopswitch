# CI report

- commit: `71213b88b27bae270a41f97b189311621eb4bbdd` on `master` - chore: prepare project structure and crates for publishing on crates.io
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/41011844079
- result: **FAILED** (6 of 7 jobs ok)
- written: 2026-10-30 06:06:06 UTC

| job | result |
| --- | --- |
| init | success |
| package / build the Windows installer (.msi) | failure |
| package / build x86_64-unknown-linux-musl | success |
| package / build x86_64-apple-darwin | success |
| package / build x86_64-pc-windows-msvc | success |
| package / build aarch64-apple-darwin | success |
| publish __beta__ | success |

## FAILED: package / build the Windows installer (.msi)

failed step: Build the .msi

```text
(could not download the log: Command '['gh', 'api', '-H', 'Accept: application/vnd.github+json', 'repos/MohammadRaziei/hopswitch/actions/jobs/123919281856/logs']' returned non-zero exit status 1.)
```
