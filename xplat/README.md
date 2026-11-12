# xplat

The helper and its library, in one Cargo workspace.

| Crate | What it is |
|---|---|
| `herdr-core` | The hc client, the snapshot, the conversation parsers (Claude Code, Codex, pi, Cursor, Grok, agy), the session resolver and log locator, status line records, question pickers, approval rules and the key gate |
| `flockbook-modfeed` | The mod's helper (`herdr-app/docs/protocol.md`). It embeds the mod (`herdr-app/`) for `install`, and the phone page (`flockbook-modfeed/web/`, built from `web/` by `scripts/web-build.mjs`) |

`fixtures/` holds the session logs and hub answers that the tests of both crates read.

```sh
cargo test --locked -p flockbook-modfeed -p herdr-core
cargo build -p flockbook-modfeed
FLOCKBOOK_MODFEED=$PWD/target/debug/flockbook-modfeed npm --prefix ../herdr-app test   # the mod against this binary
```

Linux and macOS: Unix socket. Windows builds, but herdr-core's Windows endpoint and the open-files scan are TODOs:
there it answers `no-core` unless `HC_SOCKET` is set, and it fails closed.
