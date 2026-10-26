# CI report

- commit: `f63cf59a34f2597e8d633a294ea6eb149e5281e7` on `master` - feat(ci): add CI failure report job and enhance workflow robustness
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/40395894386
- result: **all green** (1 of 1 jobs ok)
- written: 2026-10-26 20:29:22 UTC

| job | result |
| --- | --- |
| PAC through WinHTTP | success |

### PAC scripts through the real WinHTTP engine

535 sample hosts per variant. 'first' includes downloading and compiling the script.

| variant | bytes | first lookup (ms) | mean lookup (us) | wrong | status |
| --- | ---: | ---: | ---: | ---: | --- |
| object-full | 2,276,542 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| object-ir | 1,074,053 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| bucket-ir | 649,466 | 403.5 | 262.4 | 0 | ok |
| fod | 10,728 | 22.2 | 277.4 | 0 | ok |
| rust-bucket-ir | 649,574 | 40.7 | 270.5 | 0 | ok |
| rust-fod | 9,778 | 26.1 | 295.9 | 0 | ok |

### What WinHTTP returns for each PAC keyword

| keyword in the script | WinHTTP returned |
| --- | --- |
| PROXY 127.0.0.1:9 | type=3 proxy=127.0.0.1:9 |
| HTTPS 127.0.0.1:9 | type=3 proxy=https://127.0.0.1:9 |
| SOCKS 127.0.0.1:9 | type=1 proxy= |
| SOCKS4 127.0.0.1:9 | type=1 proxy= |
| SOCKS5 127.0.0.1:9 | type=1 proxy= |
