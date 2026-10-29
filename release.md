# CI report

- commit: `e21258e7cfa96042394c05094b2c736292134dd4` on `master` - refactor: simplify release report generation and update documentation
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/40973739964
- result: **FAILED** (5 of 7 jobs ok)
- written: 2026-10-29 23:34:24 UTC

| job | result |
| --- | --- |
| init | success |
| package / build the Windows installer (.msi) | failure |
| package / build x86_64-unknown-linux-musl | failure |
| package / build x86_64-apple-darwin | success |
| package / build aarch64-apple-darwin | success |
| package / build x86_64-pc-windows-msvc | success |
| publish ${{ needs.init.outputs.release_name }} | skipped |

## FAILED: package / build the Windows installer (.msi)

failed step: Build the .msi

```text
(could not download the log: Command '['gh', 'api', '-H', 'Accept: application/vnd.github+json', 'repos/MohammadRaziei/hopswitch/actions/jobs/123797012312/logs']' returned non-zero exit status 1.)
```

## FAILED: package / build x86_64-unknown-linux-musl

failed step: Smoke test

```text
(could not download the log: Command '['gh', 'api', '-H', 'Accept: application/vnd.github+json', 'repos/MohammadRaziei/hopswitch/actions/jobs/123797012359/logs']' returned non-zero exit status 1.)
```
