# CI report

- commit: `00615eaa5859f73d8e6021636289e5ccf175332f` on `master` - fix CI musl static binary check using readelf instead of ldd
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/40975814476
- result: **FAILED** (6 of 7 jobs ok)
- written: 2026-10-29 23:53:21 UTC

| job | result |
| --- | --- |
| init | success |
| package / build aarch64-apple-darwin | success |
| package / build x86_64-apple-darwin | success |
| package / build x86_64-pc-windows-msvc | success |
| package / build x86_64-unknown-linux-musl | success |
| package / build the Windows installer (.msi) | failure |
| publish __beta__ | success |

## FAILED: package / build the Windows installer (.msi)

failed step: Build the .msi

```text
(could not download the log: Command '['gh', 'api', '-H', 'Accept: application/vnd.github+json', 'repos/MohammadRaziei/hopswitch/actions/jobs/123803486921/logs']' returned non-zero exit status 1.)
```
