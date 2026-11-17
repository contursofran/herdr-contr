# herdr-contr

> [!WARNING]
> The backend moved. The helper, the orchestration tool and the kit are in the repository contr-core. Do not change `xplat/`, `kit/`, `web/` and the other paths of `scripts/backend-paths.txt` here. This repository will be archived.

contr for the [Herdr](https://herdr.dev) desktop app: a layout, a mod (JavaScript) and a helper program
(Rust). Each agent tab shows as a conversation, and cards answer an agent's prompt or question.

This repository starts from a copy of Flockbook (MIT); `NOTICE` names it. `AGENTS.md` has the goal, the
rules and the layout of the repository.

| Path | Holds |
|---|---|
| `herdr-app/` | the layout, the themes, the keys and the mod, with its tests ([README](herdr-app/README.md)) |
| `xplat/herdr-contr-feed` | the helper: reads the agents' session logs and talks to hc |
| `xplat/herdr-core` | the helper's hc client and conversation parsers |
| `xplat/herdr-contr-orch` | the orchestration tool: the profiles and the flow of the workers, the owner file |
| `web/` | the source of the phone page that the helper embeds |

## Checks

```sh
cd herdr-app && npm test
cargo test --locked --manifest-path xplat/Cargo.toml -p herdr-contr-feed -p herdr-core -p herdr-contr-orch
```

To run the mod's tests against the real helper too:

```sh
cargo build --manifest-path xplat/Cargo.toml -p herdr-contr-feed
(cd herdr-app && HERDR_CONTR_FEED="$PWD/../xplat/target/debug/herdr-contr-feed" npm test)
```

## Install

This installs merged `main` into your Herdr home (`~/.config/herdr-app`). Build the helper and the
orchestration tool, look at the plan, then install:

```sh
cargo build --release --locked --manifest-path xplat/Cargo.toml -p herdr-contr-feed -p herdr-contr-orch
H=xplat/target/release/herdr-contr-feed
O=xplat/target/release/herdr-contr-orch
"$H" install --dry-run --helper "$H" --orch "$O"
"$H" install --helper "$H" --orch "$O"
```

Give `--orch` each time. Without `herdr-contr-orch`, the settings screen shows "The settings could not
be read" and the kit's hooks do not run. The install never overwrites a layout, mod or theme that it
did not put there, and it merges `keys.json`.

Then pick a layout in the Herdr app:

```sh
herdr-app do layout.switch '{"name": "herdr-contr"}'
```

`herdr-contr` keeps Herdr's own sidebar. `herdr-contr-own` hides it. `ctrl-shift-l` switches back to
your own layout. [herdr-app/README.md](herdr-app/README.md) has the details.

The backend root (the kit, the programs and the orchestration tool's files) is `~/.config/contr-backend`
when that folder exists, else `~/.config/herdr-app/herdr-contr` when that exists, else
`~/.config/contr-backend`. `CONTR_HOME` and `HERDR_APP_HOME` come before this default. To move your
files to the neutral folder, run the move yourself, with no worker live and the phone page off:

```sh
"$H" home move --dry-run
"$H" home move
```

It moves `~/.config/herdr-app/herdr-contr` to `~/.config/contr-backend` and leaves a link at the old
path, so the mod, `--home` and Claude Code's marketplace entry continue to work. When the move is not
safe (the neutral folder has files, a lock is held, a worker is live, the phone service names the old
path, or the two folders are on different file systems), it stops and writes nothing.

## Install into a test home

Never install a branch that is not merged into your real Herdr home. For development, use a test home:

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
