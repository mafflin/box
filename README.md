# box

A docker helper: a live dashboard of containers, images and volumes, plus `list`, `prune` and `stop` commands. It runs the `docker` CLI, so contexts and `DOCKER_HOST` apply.

## Install

```sh
cargo install --locked --path .                 # into ~/.cargo/bin
stow -d completions -t ~/.completions bash      # bash completion
stow -d completions -t ~/.zsh/completions zsh   # zsh completion
```

stow needs the target directories to exist. For zsh, add `fpath=(~/.zsh/completions $fpath)` before `compinit`.

## Commands

```sh
box                                       # dashboard (help when not in a terminal)
box list containers|images|volumes        # containers -x: IPs and mounts too
box prune containers|images|volumes|all   # containers first, then volumes, then images
box stop                                  # all running containers
box --version                             # or -V
```

Pruning removes every volume no container uses, named ones too, unlike `docker volume prune`, which since Docker 23 only removes anonymous ones. Containers go first, so a stopped container's volumes go in the same run, unless another container uses them. That holds for `box prune all` and for `p` in the dashboard, where all three are ticked to start with.

## Dashboard

| Key | Action |
|---|---|
| `↑↓` `jk` `gG` PgUp/PgDn, wheel | move |
| `tab` `123` `←→` | switch view |
| `/` | filter (`enter` keep, `esc` clear) |
| `P M N S T` | sort: CPU, memory, name, size, time (again reverses) |
| `a` | running / all containers |
| `i` | details: below, beside, off |
| `s` `r` `d` | start/stop, restart, remove (confirm with `y`) |
| `l` `enter` | follow logs (`space` pauses) |
| `e` | shell in a running container: bash, else sh |
| `p` | prune stopped containers, unused volumes (named ones too) and dangling images |
| `?` `q` | help, quit |

Containers are sorted most recent first: running ones by when they started, then stopped ones by when they stopped. `T` sorts by that time for containers and by creation time for images and volumes.

Containers started with `-t` or `-i` have no logs view, since their output is a terminal session. Log lines are stripped of escape sequences and control characters, so a container can't change your terminal.

A shell from `e` runs on the terminal's main screen, so its session stays in the scrollback after you quit box.

## Settings

Optional: `~/.config/box/settings.toml` (or under `$XDG_CONFIG_HOME`). Its `[theme]` sets the colors of `box list` and the dashboard, every key optional; [`src/settings/example.toml`](src/settings/example.toml) lists them with their defaults. Colors that mean something, like a container's state, stay as they are.

```toml
[theme]
image = "cyan"
bar = 254
```

A color is `"default"` (the terminal's text color), an ANSI name like `"cyan"`, `"bright-red"`, `"gray"` or `"dark-gray"`, a number of the 256, or `"#rrggbb"`. ANSI names follow the terminal's theme. By default it's the text color and grays with a yellow accent, and the backgrounds are grays 236–239, for a dark terminal.

## Tests

`rake` runs rubocop, rustfmt, clippy, the unit tests and the Ruby tests in `test/`: both completions' (zsh's are skipped without zsh), and the debug build's against a fake `docker`. Dashboard screens are [insta](https://insta.rs) snapshots in `src/dashboard/ui/snapshots/`: after changing the UI, run `INSTA_UPDATE=always cargo test` and review with `git diff`.

`rake dist` builds static Linux binaries into `dist/`, and a universal macOS binary too when run on a Mac.
