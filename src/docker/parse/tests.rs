use super::*;

const INSPECT: &str = r#"[{
        "Id": "aaa", "Name": "/web", "Image": "sha256:img",
        "Config": {"Image": "nginx:1.27", "User": "", "WorkingDir": "/srv", "Tty": false, "OpenStdin": true},
        "State": {"Status": "running", "ExitCode": 0, "StartedAt": "2026-10-15T09:30:00.123456789Z",
            "FinishedAt": "0001-01-01T00:00:00Z"},
        "NetworkSettings": {
            "Networks": {"b": {"IPAddress": "172.18.0.3"}, "a": {"IPAddress": ""}},
            "Ports": {
                "80/tcp": [{"HostIp": "0.0.0.0", "HostPort": "8080"}, {"HostIp": "::", "HostPort": "8080"}],
                "443/tcp": null
            }
        },
        "Mounts": [{"Type": "volume", "Name": "html", "Source": "/var/lib", "Destination": "/srv", "RW": false}]
    }, {"Id": "bbb", "Name": "/db", "Config": {"Image": "postgres"}, "State": {"Status": "exited", "ExitCode": 1},
        "NetworkSettings": {"Networks": null, "Ports": {}}, "Mounts": null}]"#;

#[test]
fn reads_containers() {
    let ps = vec![
        r#"{"ID":"bbb","Names":"db","Status":"Exited (1) 2 days ago"}"#.to_string(),
        r#"{"ID":"gone","Names":"x","Status":"Up"}"#.to_string(),
        r#"{"ID":"aaa","Names":"web","Status":"Up 2 days"}"#.to_string(),
    ];
    let containers = super::containers(&ps, INSPECT).unwrap();
    assert_eq!(containers.len(), 2);
    let (db, web) = (&containers[0], &containers[1]);
    assert_eq!(db.name, "db");
    assert_eq!(db.exit_code, 1);
    assert_eq!(db.started_at, 0);
    assert_eq!(web.started_at, 1_792_056_600);
    assert_eq!(web.finished_at, 0);
    assert!(!db.is_up());
    assert_eq!(web.status, "Up 2 days");
    assert_eq!(web.ips, ["172.18.0.3"]);
    assert_eq!(web.ports, ["443/tcp", "8080->80/tcp"]);
    assert_eq!(web.shown_user(), "root");
    assert!(web.is_interactive());
    assert_eq!(web.interactive_flags(), "-i");
    assert_eq!(
        format::mount(&web.mounts[0], &web.workdir).highlight,
        "/srv"
    );
    assert_eq!(super::containers(&[], "").unwrap(), Vec::<Container>::new());
    assert_eq!(ps_ids(&ps), ["bbb", "gone", "aaa"]);
}

#[test]
fn container_text_from_the_image_is_plain() {
    let ps = vec![r#"{"ID":"aaa","Status":"Up"}"#.to_string()];
    let inspect = r#"[{
            "Id": "aaa", "Name": "/web",
            "Config": {"Image": "app", "User": "\u001b[31mevil", "WorkingDir": "/srv\u001b]0;pwned\u0007"},
            "NetworkSettings": {"Ports": {"80/tcp\u001b[2J": null}},
            "Mounts": [{"Type": "volume", "Name": "data", "Source": "/var\u001b[H", "Destination": "/srv\u001b]0;x\u0007"}]
        }]"#;
    let web = super::containers(&ps, inspect).unwrap().remove(0);
    assert_eq!((web.user.as_str(), web.workdir.as_str()), ("evil", "/srv"));
    assert_eq!(web.ports, ["80/tcp"]);
    let mount = &web.mounts[0];
    assert_eq!(
        (mount.source.as_str(), mount.destination.as_str()),
        ("/var", "/srv")
    );
    assert_eq!(mount.name.as_deref(), Some("data"));
    // Still the working directory, to highlight
    assert_eq!(format::mount(mount, &web.workdir).highlight, "/srv");
}

#[test]
fn reads_images() {
    let lines = vec![
            r#"{"Repository":"nginx","Tag":"1.27","ID":"sha256:abcdefabcdef00","Size":"193MB","CreatedSince":"5 minutes ago","CreatedAt":"2026-10-12 10:00:00 +0200 CEST"}"#.to_string(),
            r#"{"Repository":"<none>","Tag":"<none>","ID":"sha256:0123456789ab99","Size":"1kB","CreatedSince":"1 day ago","CreatedAt":"bad"}"#.to_string(),
        ];
    let images = super::images(&lines, "").unwrap();
    assert_eq!(images[0].reference(), "nginx:1.27");
    assert_eq!(images[0].platform, "");
    assert_eq!(images[0].size_bytes, 193_000_000);
    assert_eq!(images[0].created_at, 1_791_792_000);
    assert_eq!(images[0].removal_target(), "nginx:1.27");
    assert_eq!(images[1].reference(), "<none>");
    assert_eq!(images[1].removal_target(), "0123456789ab");
    assert_eq!(images[1].created_at, 0);
    assert!(images[1].is_dangling());
}

#[test]
fn reads_image_details() {
    let lines = vec![
        r#"{"Repository":"nginx","Tag":"1.27","ID":"sha256:aaa","Size":"193MB"}"#.to_string(),
        r#"{"Repository":"nginx","Tag":"latest","ID":"sha256:aaa","Size":"193MB"}"#.to_string(),
        r#"{"Repository":"app","Tag":"dev","ID":"sha256:bbb","Size":"1GB"}"#.to_string(),
        r#"{"Repository":"gone","Tag":"1","ID":"sha256:ccc","Size":"1kB"}"#.to_string(),
    ];
    assert_eq!(
        image_ids(&lines),
        ["sha256:aaa", "sha256:bbb", "sha256:ccc"]
    );
    let inspect = r#"[{
            "Id": "sha256:aaa", "Os": "linux", "Architecture": "arm64", "Variant": "v8",
            "Config": {
                "User": "", "WorkingDir": "", "Entrypoint": ["/docker-entrypoint.sh"],
                "Cmd": ["nginx", "-g", "daemon off;"],
                "ExposedPorts": {"80/tcp": {}, "443/tcp": {}},
                "Labels": {
                    "org.opencontainers.image.source": "https://github.com/nginx/docker-nginx\u001b]0;pwned\u0007",
                    "org.opencontainers.image.version": "1.27.2",
                    "maintainer": "NGINX"
                }
            }
        }, {
            "Id": "sha256:bbb", "Os": "linux", "Architecture": "amd64", "Variant": null,
            "Config": {"User": "app", "WorkingDir": "/app", "Entrypoint": null, "Cmd": ["/bin/sh", "-c", "bin/rails server"],
                "ExposedPorts": null, "Labels": null}
        }]"#;
    let images = super::images(&lines, inspect).unwrap();
    let (nginx, app, gone) = (&images[0], &images[2], &images[3]);
    assert_eq!(nginx.platform, "linux/arm64/v8");
    assert_eq!(images[1].platform, "linux/arm64/v8");
    assert_eq!(
        nginx.command,
        "/docker-entrypoint.sh nginx -g 'daemon off;'"
    );
    assert_eq!(nginx.ports, ["443/tcp", "80/tcp"]);
    assert_eq!((nginx.shown_user(), nginx.shown_workdir()), ("root", "/"));
    assert_eq!(nginx.source, "https://github.com/nginx/docker-nginx");
    assert_eq!(
        (nginx.version.as_str(), nginx.revision.as_str()),
        ("1.27.2", "")
    );
    assert_eq!(app.platform, "linux/amd64");
    assert_eq!(app.command, "bin/rails server");
    assert_eq!((app.user.as_str(), app.workdir.as_str()), ("app", "/app"));
    assert!(app.ports.is_empty() && app.source.is_empty());
    assert_eq!((gone.platform.as_str(), gone.command.as_str()), ("", ""));

    assert!(nginx.configured && !gone.configured);
    assert!(!nginx.is_foreign("linux/arm64"));
    assert!(app.is_foreign("linux/arm64"));
    assert!(!app.is_foreign(""));
    assert!(!gone.is_foreign("linux/arm64"));

    // What can't be read leaves the images as ls has them
    let images = super::images(&lines, "not json").unwrap();
    assert_eq!(images.len(), 4);
    assert!(
        images
            .iter()
            .all(|image| image.platform.is_empty() && !image.configured)
    );
}

#[test]
fn image_platforms() {
    let lines = vec![r#"{"Repository":"x","Tag":"1","ID":"sha256:aaa"}"#.to_string()];
    let inspect = r#"[{"Id": "sha256:aaa", "Os": "linux", "Architecture": "amd64", "Variant": "v2\u001b[31m", "Config": null}]"#;
    let image = super::images(&lines, inspect).unwrap().remove(0);
    assert_eq!(image.platform, "linux/amd64/v2");
    assert!(!image.configured);

    let built_for = |platform: &str| Image {
        platform: platform.into(),
        ..Image::default()
    };
    assert!(!built_for("linux/386").is_foreign("linux/amd64"));
    assert!(built_for("linux/386").is_foreign("linux/arm64"));
    assert!(built_for("linux/arm/v7").is_foreign("linux/arm64"));
    assert!(built_for("linux/arm64x").is_foreign("linux/arm64"));
    assert!(!built_for("linux/arm/v7").is_foreign("linux/arm"));
}

#[test]
fn tags() {
    let image = |repository: &str, tag: &str| Image {
        repository: repository.into(),
        tag: tag.into(),
        ..Image::default()
    };
    assert!(image("nginx", "1.27").is_tagged());
    assert!(!image("nginx", UNTAGGED).is_tagged());
    assert!(!image(UNTAGGED, UNTAGGED).is_tagged());
}

#[test]
fn reads_volumes() {
    let volumes = super::volumes(
            r#"[{"Name":"pgdata","Driver":"local","Mountpoint":"/var/lib/docker/volumes/pgdata/_data","CreatedAt":"2026-10-15T09:30:00Z"}]"#,
        )
        .unwrap();
    assert_eq!(volumes[0].driver, "local");
    assert_eq!(volumes[0].created_at, 1_792_056_600);
    assert_eq!(super::volumes("").unwrap(), Vec::<Volume>::new());
}

#[test]
fn reads_stats() {
    let line = "\u{1b}[2J\u{1b}[H{\"BlockIO\":\"4.1MB / 0B\",\"CPUPerc\":\"12.34%\",\"Container\":\"web\",\"ID\":\"aaa\",\"MemPerc\":\"1%\",\"MemUsage\":\"42MiB / 512MiB\",\"Name\":\"web\",\"NetIO\":\"1.2kB / 648B\",\"PIDs\":\"5\"}";
    let stats = super::stats(line).unwrap();
    assert_eq!(stats.id, "aaa");
    assert!((stats.cpu - 12.34).abs() < 1e-9);
    assert_eq!(
        (stats.memory, stats.memory_limit),
        (42 * 1024 * 1024, 512 * 1024 * 1024)
    );
    assert_eq!(stats.network, (1200, 648));
    assert_eq!(stats.block, (4_100_000, 0));
    assert_eq!(stats.pids, 5);
    assert_eq!(super::stats("\u{1b}[2J\u{1b}[H"), None);
    assert_eq!(super::stats("} {"), None);
    assert_eq!(
        super::stats(r#"{"ID":"x","CPUPerc":"--","MemUsage":"-- / --"}"#)
            .unwrap()
            .cpu,
        0.0
    );
}

#[test]
fn events() {
    assert!(is_change(r#"{"Type":"container","Action":"start"}"#));
    assert!(!is_change(
        r#"{"Type":"container","Action":"exec_start: sh -c true"}"#
    ));
    assert!(!is_change("not json"));
}
