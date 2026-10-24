<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="design/logo/logo-dark.svg">
    <img alt="Hopswitch" src="design/logo/logo-light.svg" height="72">
  </picture>
</p>

Switch your **system proxy** and **DNS** between saved profiles with one click —
from a clean desktop UI or straight from the system tray. Windows · macOS · Linux.

> Hopswitch is a *client-side* tool. It points your system at a proxy/DNS you already
> have; it does not provide or run one.

## Status

Early development; see [`CHECKLIST.md`](CHECKLIST.md) for the full plan and [`HANDOFF.md`](HANDOFF.md) to get started.

| Part | State |
| --- | --- |
| Domain model, proxy/DNS services, presets, persistence | done |
| Windows proxy adapter | done, verified on a real runner by CI (`windows-proxy-smoke`) |
| Command line (`hopswitch`) | done |
| Connectivity test (`hopswitch ping`) and auto connect / auto switch (`hopswitch auto`) | done |
| Windows DNS adapter (needs administrator rights) | planned |
| macOS and Linux adapters | planned |
| Desktop UI and tray (Tauri) | planned |

## Command line

```
hopswitch presets                        # built-in proxy and DNS presets
hopswitch proxy use v2rayn-socks         # apply a preset or a saved profile
hopswitch dns use cloudflare
hopswitch status                         # what is active right now
hopswitch ping                           # test every saved profile and show latency
hopswitch off                            # restore the original proxy and DNS

hopswitch auto proxy pool add work       # take part in auto mode
hopswitch auto proxy connect on          # connect when something answers, disconnect when not
hopswitch auto proxy switch on           # stay on the fastest profile of the pool
hopswitch auto run                       # apply the auto settings (Ctrl-C to stop)
```

`ping` is a real test, not ICMP: an HTTP request through each proxy and a DNS query to each server.

## Development

```bash
cargo test                                   # run the test-suite
cargo clippy --all-targets -- -D warnings    # lint
cargo fmt --all                              # format
```

Read [`AGENT.md`](AGENT.md) for architecture rules and the TDD workflow, and
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the big picture.

## License

MIT
