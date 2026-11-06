//! What the dashboard reads from docker's JSON output

use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;

use crate::Result;
use crate::format::{self, parse_size, parse_size_pair};

pub const UNTAGGED: &str = "<none>";

/// A mount as `docker inspect` gives it
#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Mount {
    #[serde(rename = "Type", default)]
    pub kind: String,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Source", default)]
    pub source: String,
    #[serde(rename = "Destination", default)]
    pub destination: String,
    #[serde(rename = "RW", default)]
    pub rw: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub image_id: String,
    pub user: String,
    pub workdir: String,
    /// As docker ps shows it, like `Up 2 days (healthy)`
    pub status: String,
    /// `running`, `exited`, `created`, `paused`, `restarting`, `removing` or `dead`
    pub state: String,
    pub exit_code: i64,
    pub tty: bool,
    pub open_stdin: bool,
    pub ips: Vec<String>,
    pub ports: Vec<String>,
    pub mounts: Vec<Mount>,
}

impl Container {
    /// Running, paused or restarting: what stop stops
    pub fn is_up(&self) -> bool {
        matches!(self.state.as_str(), "running" | "paused" | "restarting")
    }

    /// Started with `-t` or `-i`: its output is a terminal session, not logs
    pub fn is_interactive(&self) -> bool {
        self.tty || self.open_stdin
    }

    /// `-it`, `-t` or `-i`
    pub fn interactive_flags(&self) -> String {
        format!(
            "-{}{}",
            if self.open_stdin { "i" } else { "" },
            if self.tty { "t" } else { "" }
        )
    }

    pub fn shown_user(&self) -> &str {
        shown_user(&self.user)
    }

    pub fn shown_workdir(&self) -> &str {
        shown_workdir(&self.workdir)
    }
}

/// Root when the image doesn't set a user
pub fn shown_user(user: &str) -> &str {
    if user.is_empty() { "root" } else { user }
}

/// `/` when the image doesn't set a working directory
pub fn shown_workdir(workdir: &str) -> &str {
    if workdir.is_empty() { "/" } else { workdir }
}

/// `repository:tag`, or `<none>` for an untagged image
pub fn reference(repository: &str, tag: &str) -> String {
    if repository == UNTAGGED {
        UNTAGGED.to_string()
    } else {
        format!("{repository}:{tag}")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub repository: String,
    pub tag: String,
    pub id: String,
    pub size: String,
    pub size_bytes: u64,
    pub created_since: String,
    /// Seconds since the epoch, 0 when unknown
    pub created_at: i64,
}

impl Image {
    pub fn reference(&self) -> String {
        reference(&self.repository, &self.tag)
    }

    pub fn is_dangling(&self) -> bool {
        self.repository == UNTAGGED
    }

    /// What `docker rmi` takes: the tag, or the ID for an untagged image
    pub fn removal_target(&self) -> String {
        if self.repository == UNTAGGED || self.tag == UNTAGGED {
            format::short_id(&self.id).to_string()
        } else {
            self.reference()
        }
    }

    /// Unique even for the same image under several tags
    pub fn key(&self) -> String {
        format!("{}@{}", self.reference(), self.id)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    /// Local, like `2026-10-15 09:30`
    pub created: String,
    /// Seconds since the epoch, 0 when unknown
    pub created_at: i64,
}

/// A container's usage from `docker stats`
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub id: String,
    pub cpu: f64,
    pub memory: u64,
    pub memory_limit: u64,
    pub network: (u64, u64),
    pub block: (u64, u64),
    pub pids: u64,
}

#[derive(Deserialize)]
struct PsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Status", default)]
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Inspect {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    config: Config,
    #[serde(default)]
    state: State,
    #[serde(default)]
    network_settings: NetworkSettings,
    #[serde(default)]
    mounts: Option<Vec<Mount>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct Config {
    image: String,
    user: String,
    working_dir: String,
    tty: bool,
    open_stdin: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct State {
    status: String,
    exit_code: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct NetworkSettings {
    networks: Option<BTreeMap<String, Network>>,
    ports: Option<BTreeMap<String, Option<Vec<Binding>>>>,
}

#[derive(Deserialize, Default)]
struct Network {
    #[serde(rename = "IPAddress", default)]
    ip_address: String,
}

#[derive(Deserialize, Default)]
struct Binding {
    #[serde(rename = "HostPort", default)]
    host_port: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageLine {
    #[serde(default)]
    repository: String,
    #[serde(default)]
    tag: String,
    #[serde(rename = "ID")]
    id: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    created_since: String,
    #[serde(default)]
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VolumeInspect {
    name: String,
    #[serde(default)]
    driver: String,
    #[serde(default)]
    mountpoint: String,
    #[serde(default)]
    created_at: String,
}

#[derive(Deserialize)]
struct StatsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "CPUPerc", default)]
    cpu: String,
    #[serde(rename = "MemUsage", default)]
    memory: String,
    #[serde(rename = "NetIO", default)]
    network: String,
    #[serde(rename = "BlockIO", default)]
    block: String,
    #[serde(rename = "PIDs", default)]
    pids: String,
}

#[derive(Deserialize)]
struct EventLine {
    #[serde(rename = "Action", default)]
    action: String,
}

/// The containers' IDs from `docker ps -a --no-trunc --format '{{json .}}'` lines
pub fn ps_ids(ps: &[String]) -> Vec<String> {
    ps.iter()
        .filter_map(|line| serde_json::from_str::<PsLine>(line).ok())
        .map(|line| line.id)
        .collect()
}

/// `docker ps -a --no-trunc --format '{{json .}}'` lines and `docker inspect` of them, in ps' order
pub fn containers(ps: &[String], inspect: &str) -> Result<Vec<Container>> {
    let lines: Vec<PsLine> = ps
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line))
        .collect::<std::result::Result<_, _>>()
        .context("can't read docker ps")?;
    let inspected: Vec<Inspect> = if inspect.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(inspect).context("can't read docker inspect")?
    };

    Ok(lines
        .into_iter()
        .filter_map(|line| {
            let details = inspected.iter().find(|details| details.id == line.id)?;
            Some(container(line, details))
        })
        .collect())
}

fn container(line: PsLine, details: &Inspect) -> Container {
    let ips = details
        .network_settings
        .networks
        .iter()
        .flatten()
        .map(|(_, network)| network.ip_address.clone())
        .filter(|ip| !ip.is_empty())
        .collect();
    let mut ports: Vec<String> = Vec::new();
    for (port, bindings) in details.network_settings.ports.iter().flatten() {
        let shown: Vec<String> = match bindings {
            Some(bindings) if !bindings.is_empty() => bindings
                .iter()
                .map(|binding| format!("{}->{port}", binding.host_port))
                .collect(),
            _ => vec![port.clone()],
        };
        for port in shown {
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
    }
    Container {
        id: line.id,
        name: details.name.trim_start_matches('/').to_string(),
        image: details.config.image.clone(),
        image_id: details.image.clone(),
        user: details.config.user.clone(),
        workdir: details.config.working_dir.clone(),
        status: line.status,
        state: details.state.status.clone(),
        exit_code: details.state.exit_code,
        tty: details.config.tty,
        open_stdin: details.config.open_stdin,
        ips,
        ports,
        mounts: details.mounts.clone().unwrap_or_default(),
    }
}

/// `docker image ls --no-trunc --format '{{json .}}'` lines
pub fn images(lines: &[String]) -> Result<Vec<Image>> {
    lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let line: ImageLine =
                serde_json::from_str(line).context("can't read docker image ls")?;
            Ok(Image {
                size_bytes: parse_size(&line.size).unwrap_or(0),
                created_at: image_created_at(&line.created_at),
                repository: line.repository,
                tag: line.tag,
                id: line.id,
                size: line.size,
                created_since: line.created_since,
            })
        })
        .collect()
}

/// Like `2026-10-12 10:00:00 +0200 CEST`
fn image_created_at(created_at: &str) -> i64 {
    let time: Vec<&str> = created_at.split_whitespace().take(3).collect();
    jiff::fmt::strtime::parse("%Y-%m-%d %H:%M:%S %z", time.join(" "))
        .and_then(|time| time.to_timestamp())
        .map_or(0, jiff::Timestamp::as_second)
}

/// `docker volume inspect` of every volume
pub fn volumes(inspect: &str) -> Result<Vec<Volume>> {
    if inspect.trim().is_empty() {
        return Ok(Vec::new());
    }
    let volumes: Vec<VolumeInspect> =
        serde_json::from_str(inspect).context("can't read docker volume inspect")?;
    Ok(volumes
        .into_iter()
        .map(|volume| Volume {
            created: format::local_date(&volume.created_at),
            created_at: volume
                .created_at
                .parse::<jiff::Timestamp>()
                .map_or(0, jiff::Timestamp::as_second),
            name: volume.name,
            driver: volume.driver,
            mountpoint: volume.mountpoint,
        })
        .collect())
}

/// A `docker stats --no-trunc --format '{{json .}}'` line, which can start with the codes that clear the screen
/// between updates; `None` for anything else
pub fn stats(line: &str) -> Option<Stats> {
    let json = line.get(line.find('{')?..=line.rfind('}')?)?;
    let line: StatsLine = serde_json::from_str(json).ok()?;
    let (memory, memory_limit) = parse_size_pair(&line.memory).unwrap_or_default();
    Some(Stats {
        id: line.id,
        cpu: line.cpu.trim_end_matches('%').parse().unwrap_or(0.0),
        memory,
        memory_limit,
        network: parse_size_pair(&line.network).unwrap_or_default(),
        block: parse_size_pair(&line.block).unwrap_or_default(),
        pids: line.pids.parse().unwrap_or(0),
    })
}

/// A `docker events --format '{{json .}}'` line that can change what the dashboard shows; exec events, like
/// health checks', don't
pub fn is_change(event: &str) -> bool {
    serde_json::from_str::<EventLine>(event).is_ok_and(|event| !event.action.starts_with("exec_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSPECT: &str = r#"[{
        "Id": "aaa", "Name": "/web", "Image": "sha256:img",
        "Config": {"Image": "nginx:1.27", "User": "", "WorkingDir": "/srv", "Tty": false, "OpenStdin": true},
        "State": {"Status": "running", "ExitCode": 0},
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
    fn reads_images() {
        let lines = vec![
            r#"{"Repository":"nginx","Tag":"1.27","ID":"sha256:abcdefabcdef00","Size":"193MB","CreatedSince":"5 minutes ago","CreatedAt":"2026-10-12 10:00:00 +0200 CEST"}"#.to_string(),
            r#"{"Repository":"<none>","Tag":"<none>","ID":"sha256:0123456789ab99","Size":"1kB","CreatedSince":"1 day ago","CreatedAt":"bad"}"#.to_string(),
        ];
        let images = super::images(&lines).unwrap();
        assert_eq!(images[0].reference(), "nginx:1.27");
        assert_eq!(images[0].size_bytes, 193_000_000);
        assert_eq!(images[0].created_at, 1_791_792_000);
        assert_eq!(images[0].removal_target(), "nginx:1.27");
        assert_eq!(images[1].reference(), "<none>");
        assert_eq!(images[1].removal_target(), "0123456789ab");
        assert_eq!(images[1].created_at, 0);
        assert!(images[1].is_dangling());
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
}
