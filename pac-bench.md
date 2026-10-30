# CI report

- commit: `c7dde812942b807c3d08bb5fdde68cd62e6c79af` on `master` - refactor: align workspace paths and imports in CI scripts
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/41037870187
- result: **all green** (1 of 1 jobs ok)
- written: 2026-10-30 10:04:33 UTC

| job | result |
| --- | --- |
| PAC through WinHTTP | success |

### PAC scripts through the real WinHTTP engine

535 sample hosts per variant. 'first' includes downloading and compiling the script.

| variant | bytes | first lookup (ms) | mean lookup (us) | wrong | status |
| --- | ---: | ---: | ---: | ---: | --- |
| object-full | 2,276,542 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| object-ir | 1,074,053 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| bucket-ir | 649,466 | 501.3 | 193.1 | 0 | ok |
| fod | 10,728 | 24.4 | 174.2 | 0 | ok |
| rust-bucket-ir | 649,574 | 47.1 | 221.4 | 0 | ok |
| rust-fod | 9,778 | 26.7 | 192.9 | 0 | ok |

### What WinHTTP returns for each PAC keyword, and for a dead PAC address

| keyword in the script | WinHTTP returned |
| --- | --- |
| PROXY 127.0.0.1:9 | type=3 proxy=127.0.0.1:9 |
| HTTPS 127.0.0.1:9 | type=3 proxy=https://127.0.0.1:9 |
| SOCKS 127.0.0.1:9 | type=1 proxy= |
| SOCKS4 127.0.0.1:9 | type=1 proxy= |
| SOCKS5 127.0.0.1:9 | type=1 proxy= |
| a PAC address nothing listens on | FAILED: Exception calling "ResolveRaw" with "3" argument(s): "Unknown error (0x2f87)" |
