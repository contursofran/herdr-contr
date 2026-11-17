# xplat

> [!WARNING]
> The backend moved. The helper, the orchestration tool and the kit are in the repository contr-backend. Do not change `xplat/`, `kit/`, `web/` and the other paths of `scripts/backend-paths.txt` here. This repository will be archived.

The helper and its library, in one Cargo workspace.

| Crate | What it is |
|---|---|
| `herdr-core` | The hc client, the snapshot, the conversation parsers (Claude Code, Codex, pi, Cursor, Grok, agy), the session resolver and log locator, status line records, question pickers, approval rules and the key gate |
| `herdr-contr-feed` | The mod's helper (`docs/protocol.md`). It embeds the kit (`kit/`) and, with the feature `herdr-mod` (on by default), the mod (`herdr-app/`) for `install`, and the phone page (`herdr-contr-feed/web/`, built from `web/` by `scripts/web-build.mjs`) |

`fixtures/` holds the session logs and hub answers that the tests of both crates read.

```sh
cargo test --locked -p herdr-contr-feed -p herdr-core -p herdr-contr-orch
cargo build -p herdr-contr-feed
HERDR_CONTR_FEED=$PWD/target/debug/herdr-contr-feed npm --prefix ../herdr-app test   # the mod against this binary
cargo test --locked --no-default-features -p herdr-contr-feed -p herdr-core -p herdr-contr-orch   # the backend only
```

Without the feature `herdr-mod`, the build reads no file of `herdr-app/`, `install` is the backend install
(`install --backend`), and `health` gives only the checks `orch` and `kit`. The CI job `backend` builds it so,
with no `herdr-app/` folder.

Linux and macOS: Unix socket. Windows builds, but herdr-core's Windows endpoint and the open-files scan are TODOs:
there it answers `no-core` unless `HC_SOCKET` is set, and it fails closed.
