# CI report

- commit: `85f778d69e3c908bcac53cf74a0014204ab8d12b` on `master` - feat: add release run report job for CI failure and asset summary tracking
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/40973565482
- result: **FAILED** (5 of 7 jobs ok)
- written: 2026-10-29 23:32:35 UTC

| job | result |
| --- | --- |
| init | success |
| package / build x86_64-pc-windows-msvc | success |
| package / build aarch64-apple-darwin | success |
| package / build x86_64-unknown-linux-musl | failure |
| package / build x86_64-apple-darwin | success |
| package / build the Windows installer (.msi) | failure |
| publish ${{ needs.init.outputs.release_name }} | skipped |

## FAILED: package / build x86_64-unknown-linux-musl

failed step: Smoke test

```text
(could not download the log: Command '['gh', 'api', '-H', 'Accept: application/vnd.github+json', 'repos/MohammadRaziei/hopswitch/actions/jobs/123796067418/logs']' returned non-zero exit status 1.)
```

## FAILED: package / build the Windows installer (.msi)

failed step: Build the .msi

```text
(could not download the log: Command '['gh', 'api', '-H', 'Accept: application/vnd.github+json', 'repos/MohammadRaziei/hopswitch/actions/jobs/123796067533/logs']' returned non-zero exit status 1.)
```
