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
```

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
| `p` | prune stopped containers and dangling volumes and images |
| `?` `q` | help, quit |

Containers are sorted most recent first: running ones by when they started, then stopped ones by when they stopped. `T` sorts by that time for containers and by creation time for images and volumes.

Containers started with `-t` or `-i` have no logs view, since their output is a terminal session. Log lines are stripped of escape sequences and control characters, so a container can't change your terminal.

A shell from `e` runs on the terminal's main screen, so its session stays in the scrollback after you quit box.

## Settings

Optional: `~/.config/box/settings.yml` (or under `$XDG_CONFIG_HOME`).

```yaml
accent: yellow   # yellow, red, green, blue, magenta or cyan
```

Colors are the terminal's ANSI ones, so its theme decides them. Backgrounds are grays 236–239 of the 256-color palette and assume a dark terminal.

## Tests

`rake` runs rubocop, rustfmt, clippy, the unit tests and the Ruby tests in `test/`: both completions' (zsh's are skipped without zsh), and the debug build's against a fake `docker`. Dashboard screens are [insta](https://insta.rs) snapshots in `src/dashboard/ui/snapshots/`: after changing the UI, run `INSTA_UPDATE=always cargo test` and review with `git diff`.
