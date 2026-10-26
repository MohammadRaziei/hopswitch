# PAC benchmark (spike for ADR-0011)

Question: can per-domain rules be done with a PAC script when the rule list has ~127 000 domains?
This tool answers it with real numbers, in two places: a JavaScript engine on your machine
(correctness) and Windows' own PAC engine on a CI runner (does the real thing accept it).

```
# lists (public): Iran-hosted domains (MIT) and the FOD list of services that block Iran
curl -L -o data/iran.txt https://github.com/bootmortis/iran-hosted-domains/releases/latest/download/domains.txt
curl -L -o data/fod.txt  https://raw.githubusercontent.com/freedomofdevelopers/fod/master/domains

python tools/pac-bench/make_pac.py --iran data/iran.txt --fod data/fod.txt --out build
python tools/pac-bench/verify_local.py build            # V8 (node) and QuickJS (pip install quickjs)
pwsh   tools/pac-bench/bench.ps1 -PacDir build          # Windows only: the real WinHTTP engine
```

CI runs the same steps in `.github/workflows/pac-bench.yml` and prints a table in the job summary.

## Variants

| name | what it is |
| --- | --- |
| `object-full` | one object key per domain, whole list (simplest, biggest) |
| `object-ir` | the 69 k `.ir` entries replaced by one `*.ir` rule |
| `bucket-ir` | `*.ir` rule + domains grouped by TLD and first two letters, searched with `indexOf` |
| `fod` | the FOD list: PROXY for listed domains, DIRECT for the rest |
| `rust-bucket-ir`, `rust-fod` | the same two as built by the product's Rust generator (`cargo run -p hopswitch-core --example gen_pac -- --iran data/iran.txt --fod data/fod.txt --out build`, after `make_pac.py`, which writes the samples they reuse) |

The generated JavaScript is ES3 on purpose (no `let`, arrow functions or trailing commas).
The sample hosts include sub-domains, look-alikes, upper case, plain host names and names such as
`constructor.com` that collide with JavaScript object properties.
