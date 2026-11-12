# herdr-contr

contr for the [Herdr](https://herdr.dev) desktop app: a layout, a mod (JavaScript) and a helper program
(Rust). Each agent tab shows as a conversation, and cards answer an agent's prompt or question.

This repository starts from a copy of Flockbook (MIT); `NOTICE` names it. `AGENTS.md` has the goal, the
rules and the layout of the repository.

| Path | Holds |
|---|---|
| `herdr-app/` | the layout, the themes, the keys and the mod, with its tests ([README](herdr-app/README.md)) |
| `xplat/herdr-contr-feed` | the helper: reads the agents' session logs and talks to hc |
| `xplat/herdr-core` | the helper's hc client and conversation parsers |
| `web/` | the source of the phone page that the helper embeds |

## Checks

```sh
cd herdr-app && npm test
cargo test --locked --manifest-path xplat/Cargo.toml -p herdr-contr-feed -p herdr-core
```

To run the mod's tests against the real helper too:

```sh
cargo build --manifest-path xplat/Cargo.toml -p herdr-contr-feed
(cd herdr-app && HERDR_CONTR_FEED="$PWD/../xplat/target/debug/herdr-contr-feed" npm test)
```

## Install into a test home

Never install into your real Herdr home (`~/.config/herdr-app`) for development. Use a test home:

```sh
cargo build --manifest-path xplat/Cargo.toml -p herdr-contr-feed
mkdir -p tmp/test-home
herdr-app/install-isolated.sh tmp/test-home xplat/target/debug/herdr-contr-feed
HERDR_APP_HOME=tmp/test-home herdr-app check tmp/test-home/mods/herdr-contr
```

Then start the Herdr app with `HERDR_APP_HOME` set to that folder, and pick the layout with
`layout.switch {"name": "herdr-contr-own"}`.

## License

MIT. See [LICENSE](LICENSE), [NOTICE](NOTICE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
