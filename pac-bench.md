# CI report

- commit: `e7ff0ccce8d2781fbac288d24de71e5022cbae75` on `master` - add hopswitch-pacserver crate and update Phase 2 checklist with local PAC server progress
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/40655962822
- result: **all green** (1 of 1 jobs ok)
- written: 2026-10-28 08:21:48 UTC

| job | result |
| --- | --- |
| PAC through WinHTTP | success |

### PAC scripts through the real WinHTTP engine

535 sample hosts per variant. 'first' includes downloading and compiling the script.

| variant | bytes | first lookup (ms) | mean lookup (us) | wrong | status |
| --- | ---: | ---: | ---: | ---: | --- |
| object-full | 2,276,542 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| object-ir | 1,074,053 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| bucket-ir | 649,466 | 750.4 | 160.7 | 0 | ok |
| fod | 10,728 | 18.3 | 142.2 | 0 | ok |
| rust-bucket-ir | 649,574 | 63.8 | 201.1 | 0 | ok |
| rust-fod | 9,778 | 18.3 | 138.3 | 0 | ok |

### What WinHTTP returns for each PAC keyword, and for a dead PAC address

| keyword in the script | WinHTTP returned |
| --- | --- |
| PROXY 127.0.0.1:9 | type=3 proxy=127.0.0.1:9 |
| HTTPS 127.0.0.1:9 | type=3 proxy=https://127.0.0.1:9 |
| SOCKS 127.0.0.1:9 | type=1 proxy= |
| SOCKS4 127.0.0.1:9 | type=1 proxy= |
| SOCKS5 127.0.0.1:9 | type=1 proxy= |
| a PAC address nothing listens on | FAILED: Exception calling "ResolveRaw" with "3" argument(s): "Unknown error (0x2f87)" |
