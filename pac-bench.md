# CI report

- commit: `32ee927ee077b30acc7defad02d9a6b0ee3bcb2d` on `master` - chore: Improve Rust code quality and add FromStr impl for PacConfig
- event: push, run: https://github.com/MohammadRaziei/hopswitch/actions/runs/40400356334
- result: **all green** (1 of 1 jobs ok)
- written: 2026-10-26 20:59:40 UTC

| job | result |
| --- | --- |
| PAC through WinHTTP | success |

### PAC scripts through the real WinHTTP engine

535 sample hosts per variant. 'first' includes downloading and compiling the script.

| variant | bytes | first lookup (ms) | mean lookup (us) | wrong | status |
| --- | ---: | ---: | ---: | ---: | --- |
| object-full | 2,276,542 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| object-ir | 1,074,053 |  |  |  | FAILED: Exception calling "Resolve" with "3" argument(s): "Unknown error (0x2f87)" 12167 |
| bucket-ir | 649,466 | 440.8 | 251.7 | 0 | ok |
| fod | 10,728 | 22.4 | 241.7 | 0 | ok |
| rust-bucket-ir | 649,574 | 45.1 | 297.4 | 0 | ok |
| rust-fod | 9,778 | 23 | 252.3 | 0 | ok |

### What WinHTTP returns for each PAC keyword

| keyword in the script | WinHTTP returned |
| --- | --- |
| PROXY 127.0.0.1:9 | type=3 proxy=127.0.0.1:9 |
| HTTPS 127.0.0.1:9 | type=3 proxy=https://127.0.0.1:9 |
| SOCKS 127.0.0.1:9 | type=1 proxy= |
| SOCKS4 127.0.0.1:9 | type=1 proxy= |
| SOCKS5 127.0.0.1:9 | type=1 proxy= |
