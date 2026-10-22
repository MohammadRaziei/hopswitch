# Hopswitch

Switch your **system proxy** and **DNS** between saved profiles with one click —
from a clean desktop UI or straight from the system tray. Windows · macOS · Linux.

> Hopswitch is a *client-side* tool. It points your system at a proxy/DNS you already
> have; it does not provide or run one.

## Status

Early development, see [`CHECKLIST.md`](CHECKLIST.md). Steps 1 (domain model), 2 (system port + proxy/DNS services) and 3 (persistence) are complete. New to the project? Start with [`HANDOFF.md`](HANDOFF.md).

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
