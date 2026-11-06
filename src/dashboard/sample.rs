//! A docker host for the dashboard's tests

use std::time::Instant;

use super::app::{Action, App, Snapshot};
use crate::docker::{Container, Image, Mount, Stats, Volume};
use crate::theme::Theme;

const MIB: u64 = 1024 * 1024;
/// When the sample's statuses were taken, in seconds since the epoch
const NOW: i64 = 1_794_000_000;
const HOUR: i64 = 3600;
const GIB: u64 = 1024 * MIB;

fn id(seed: char) -> String {
    seed.to_string().repeat(64)
}

fn volume_mount(name: &str, destination: &str) -> Mount {
    Mount {
        kind: "volume".into(),
        name: Some(name.into()),
        source: format!("/var/lib/docker/volumes/{name}/_data"),
        destination: destination.into(),
        rw: true,
    }
}

fn bind(source: &str, destination: &str, rw: bool) -> Mount {
    Mount {
        kind: "bind".into(),
        name: None,
        source: source.into(),
        destination: destination.into(),
        rw,
    }
}

fn running(name: &str, image: &str, image_seed: char, status: &str, up: i64) -> Container {
    Container {
        started_at: NOW - up,
        id: format!("{name}-id"),
        name: name.into(),
        image: image.into(),
        image_id: format!("sha256:{}", id(image_seed)),
        status: status.into(),
        state: "running".into(),
        ..Container::default()
    }
}

fn containers() -> Vec<Container> {
    let api = Container {
        user: "app".into(),
        workdir: "/app".into(),
        ips: vec!["172.18.0.3".into(), "172.19.0.4".into()],
        ports: vec!["3000->3000/tcp".into()],
        mounts: vec![
            bind("/home/me/work/api", "/app", true),
            volume_mount("bundle", "/usr/local/bundle"),
        ],
        ..running(
            "api",
            "acme/api:dev",
            'a',
            "Up 40 minutes (healthy)",
            40 * 60,
        )
    };
    let db = Container {
        ips: vec!["172.18.0.4".into()],
        ports: vec!["5432/tcp".into()],
        mounts: vec![volume_mount("pgdata", "/var/lib/postgresql/data")],
        ..running("db", "postgres:16", 'b', "Up 2 days", 48 * HOUR)
    };
    let web = Container {
        workdir: "/usr/share/nginx/html".into(),
        ips: vec!["172.18.0.2".into()],
        ports: vec!["8080->80/tcp".into(), "8443->443/tcp".into()],
        mounts: vec![
            bind("/home/me/site", "/usr/share/nginx/html", false),
            volume_mount("nginx-cache", "/var/cache/nginx"),
        ],
        ..running("web", "nginx:1.27-alpine", 'c', "Up 3 days", 72 * HOUR)
    };
    let console = Container {
        workdir: "/app".into(),
        tty: true,
        open_stdin: true,
        ips: vec!["172.18.0.8".into()],
        ..running("console", "ruby:3.3-slim", 'd', "Up 5 hours", 5 * HOUR)
    };
    let worker = Container {
        user: "app".into(),
        workdir: "/app".into(),
        state: "exited".into(),
        status: "Exited (0) 3 hours ago".into(),
        finished_at: NOW - 3 * HOUR,
        mounts: vec![bind("/home/me/work/api", "/app", true)],
        ..running("worker", "acme/api:dev", 'a', "", 7 * HOUR)
    };

    vec![api, db, web, console, worker]
}

/// For a linux/arm64 host: postgres is built for amd64, acme/api has two tags
fn images() -> Vec<Image> {
    let image = |repository: &str,
                 tag: &str,
                 seed: char,
                 size: &str,
                 size_bytes,
                 since: &str,
                 created_at| Image {
        repository: repository.into(),
        tag: tag.into(),
        id: format!("sha256:{}", id(seed)),
        size: size.into(),
        size_bytes,
        created_since: since.into(),
        created_at,
        platform: "linux/arm64".into(),
        configured: true,
        ..Image::default()
    };
    let args = |args: &[&str]| {
        crate::format::command(&args.iter().map(ToString::to_string).collect::<Vec<_>>())
    };
    let api = Image {
        command: args(&["/app/bin/docker-entrypoint", "./bin/rails", "server"]),
        user: "app".into(),
        workdir: "/app".into(),
        ports: vec!["3000/tcp".into()],
        source: "https://github.com/acme/api".into(),
        version: "2.4.0".into(),
        revision: "9f3c2a71be0d4c5e8a6b".into(),
        ..image(
            "acme/api",
            "dev",
            'a',
            "1.21GB",
            1_210_000_000,
            "3 hours ago",
            300,
        )
    };

    vec![
        Image {
            tag: "latest".into(),
            ..api.clone()
        },
        api,
        Image {
            platform: "linux/amd64".into(),
            command: args(&["docker-entrypoint.sh", "postgres"]),
            ports: vec!["5432/tcp".into()],
            ..image(
                "postgres",
                "16",
                'b',
                "438MB",
                438_000_000,
                "2 weeks ago",
                200,
            )
        },
        Image {
            platform: "linux/arm64/v8".into(),
            command: args(&["/docker-entrypoint.sh", "nginx", "-g", "daemon off;"]),
            ports: vec!["80/tcp".into()],
            ..image(
                "nginx",
                "1.27-alpine",
                'c',
                "48.2MB",
                48_200_000,
                "12 days ago",
                210,
            )
        },
        Image {
            command: args(&["irb"]),
            ..image(
                "ruby",
                "3.3-slim",
                'd',
                "216MB",
                216_000_000,
                "3 weeks ago",
                100,
            )
        },
        Image {
            id: format!("sha256:0123456789ab{}", "0".repeat(52)),
            ..image(
                "<none>",
                "<none>",
                'e',
                "412MB",
                412_000_000,
                "2 days ago",
                250,
            )
        },
    ]
}

pub fn snapshot() -> Snapshot {
    let volume = |name: &str, created: &str, created_at| Volume {
        name: name.into(),
        driver: "local".into(),
        mountpoint: format!("/var/lib/docker/volumes/{name}/_data"),
        created: created.into(),
        created_at,
    };

    Snapshot {
        containers: containers(),
        images: images(),
        volumes: vec![
            volume("pgdata", "2026-10-15 09:30", 100),
            volume("nginx-cache", "2026-10-24 18:02", 200),
            volume("bundle", "2026-10-29 11:45", 300),
            volume("old-data", "2026-03-02 08:00", 50),
        ],
        host: "linux/arm64".into(),
    }
}

/// Usage that ends at the given values, with a wavy history before
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn stats(app: &mut App, id: &str, cpu: f64, memory: u64, memory_limit: u64) {
    for step in 0..80_u32 {
        let wave = (f64::from(step) / 4.0).sin().abs();
        let last = step == 79;
        let stats = Stats {
            id: id.into(),
            cpu: if last { cpu } else { cpu * (0.3 + 1.4 * wave) },
            memory: if last {
                memory
            } else {
                (memory as f64 * (0.9 + 0.1 * wave)) as u64
            },
            memory_limit,
            network: (75 * MIB, 10 * MIB),
            block: (4 * MIB, 0),
            pids: 12,
        };
        app.update(Action::Stats(stats), Instant::now());
    }
}

pub fn app() -> App {
    let mut app = App::new(Theme::default(), Instant::now());
    app.update(Action::Snapshot(Ok(snapshot())), Instant::now());
    stats(&mut app, "api-id", 18.0, 649 * MIB, 2 * GIB);
    stats(&mut app, "db-id", 6.0, 311 * MIB, 16 * GIB);
    stats(&mut app, "web-id", 1.5, 14 * MIB, 512 * MIB);
    stats(&mut app, "console-id", 0.1, 35 * MIB, 16 * GIB);
    app
}
