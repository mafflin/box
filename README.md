# box

A small docker helper: `box` opens a live dashboard of containers, images and volumes, and `box list`, `box prune` and `box stop` work from the command line. Everything goes through the `docker` command, so docker contexts and `DOCKER_HOST` work as they do for docker.

## Install

```sh
rake install   # builds target/release/box, links it into ~/.local/bin and the bash completion into ~/.completions
```

The links point into this repo, so `rake install` again after pulling updates them.

## Commands

```sh
box                                   # the dashboard; prints the help when output isn't a terminal
box list containers|images|volumes    # -x / --extended adds IPs and mounts to containers
box prune containers|images|volumes|all ...   # containers first, then volumes, then images
box stop                              # every running container
```

## Dashboard

| Key | Does |
|---|---|
| `↑↓` `j k` `g G` PgUp/PgDn, mouse wheel | move |
| `tab` `1 2 3` `← →` | containers, images, volumes |
| `/` | filter, `enter` keeps it, `esc` clears it |
| `P M N S T` | sort by CPU, memory, name, size, created, again to reverse |
| `a` | running containers only, or all |
| `i` | details below, beside, or none |
| `s` `r` `d` | stop or start, restart, remove, after `y` |
| `l` `enter` | follow the container's logs, `space` pauses |
| `p` | prune stopped containers, dangling volumes and dangling images |
| `?` `q` | help, quit |

Containers run with `-t` or `-i` have no logs view, as their output is a terminal session. Log lines are shown without escape sequences or control characters, so a container can't change the terminal.

## Settings

`~/.config/box/settings.yml` (`$XDG_CONFIG_HOME/box/` if set) is optional and only the dashboard reads it. Unknown keys are an error.

```yaml
accent: yellow # optional: yellow, red, green, blue, magenta or cyan
```

The colors are the terminal's ANSI ones, so its theme decides how they look; the dashboard's backgrounds are grays 236–239 of the 256 and assume a dark terminal.

## Tests

`rake` runs rubocop, `cargo fmt --check`, clippy and the unit tests, then the Ruby tests in `test/`: the completion's, and the command line's against the debug build with a fake `docker`. The dashboard's screens are [insta](https://insta.rs) snapshots in `src/dashboard/ui/snapshots/`; after changing what it draws, review them with `INSTA_UPDATE=always cargo test` and `git diff`.
