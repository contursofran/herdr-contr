# xplat

The helper and its library, in one Cargo workspace.

| Crate | What it is |
|---|---|
| `herdr-core` | The hc client, the snapshot, the conversation parsers (Claude Code, Codex, pi, Cursor, Grok, agy), the session resolver and log locator, status line records, question pickers, approval rules and the key gate |
| `herdr-contr-feed` | The mod's helper (`herdr-app/docs/protocol.md`). It embeds the mod (`herdr-app/`) for `install`, and the phone page (`herdr-contr-feed/web/`, built from `web/` by `scripts/web-build.mjs`) |

`fixtures/` holds the session logs and hub answers that the tests of both crates read.

```sh
cargo test --locked -p herdr-contr-feed -p herdr-core -p herdr-contr-orch
cargo build -p herdr-contr-feed
HERDR_CONTR_FEED=$PWD/target/debug/herdr-contr-feed npm --prefix ../herdr-app test   # the mod against this binary
```

Linux and macOS: Unix socket. Windows builds, but herdr-core's Windows endpoint and the open-files scan are TODOs:
there it answers `no-core` unless `HC_SOCKET` is set, and it fails closed.
