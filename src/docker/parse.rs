//! What the dashboard reads from docker's JSON output

use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::Result;
use crate::format::{self, parse_size, parse_size_pair};

pub const UNTAGGED: &str = "<none>";
/// The most of a label or command that's kept
const MAX_TEXT: usize = 1000;

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
    /// When it last started and stopped, in seconds since the epoch, 0 when it never did
    pub started_at: i64,
    pub finished_at: i64,
    pub tty: bool,
    pub open_stdin: bool,
    pub ips: Vec<String>,
    pub ports: Vec<String>,
    pub mounts: Vec<Mount>,
}

impl Container {
    /// Running, not paused or restarting: what a shell can open in
    pub fn is_running(&self) -> bool {
        self.state == "running"
    }

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
fn shown_user(user: &str) -> &str {
    if user.is_empty() { "root" } else { user }
}

/// `/` when the image doesn't set a working directory
fn shown_workdir(workdir: &str) -> &str {
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
    /// Like `linux/arm64` or `linux/arm/v7`, empty when unknown
    pub platform: String,
    /// Docker image inspect gave its config, which the fields below come from
    pub configured: bool,
    /// The entrypoint, then the command
    pub command: String,
    pub user: String,
    pub workdir: String,
    /// Like `80/tcp`
    pub ports: Vec<String>,
    /// The `org.opencontainers.image` labels
    pub source: String,
    pub version: String,
    pub revision: String,
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

    /// Has a name to run it by, unlike a dangling image or one pulled by digest
    pub fn is_tagged(&self) -> bool {
        self.repository != UNTAGGED && self.tag != UNTAGGED
    }

    /// Built for another OS or CPU than docker's host, `linux/arm64` say, which runs it emulated if at all;
    /// x86-64 runs 32-bit x86 natively, Apple's ARM CPUs don't run 32-bit ARM
    pub fn is_foreign(&self, host: &str) -> bool {
        let built_for = |platform: &str| {
            self.platform
                .strip_prefix(platform)
                .is_some_and(|variant| variant.is_empty() || variant.starts_with('/'))
        };
        if host.is_empty() || self.platform.is_empty() {
            return false;
        }
        let native = built_for(host) || (host == "linux/amd64" && built_for("linux/386"));
        !native
    }

    pub fn shown_user(&self) -> &str {
        shown_user(&self.user)
    }

    pub fn shown_workdir(&self) -> &str {
        shown_workdir(&self.workdir)
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
    started_at: String,
    finished_at: String,
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
struct ImageInspect {
    id: String,
    #[serde(default)]
    os: Option<String>,
    #[serde(default)]
    architecture: Option<String>,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default)]
    config: Option<ImageConfig>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ImageConfig {
    user: Option<String>,
    working_dir: Option<String>,
    entrypoint: Option<Vec<String>>,
    cmd: Option<Vec<String>>,
    exposed_ports: Option<BTreeMap<String, serde_json::Value>>,
    labels: Option<BTreeMap<String, String>>,
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

/// `docker ps --no-trunc --format '{{json .}}'` lines and `docker inspect` of them, in ps' order, without those
/// inspect no longer found
pub fn containers(ps: &[String], inspect: &str) -> Result<Vec<Container>> {
    let lines: Vec<PsLine> = json_lines(ps).context("can't read docker ps")?;
    let inspected: Vec<Inspect> = json_array(inspect).context("can't read docker inspect")?;
    Ok(lines
        .into_iter()
        .filter_map(|line| {
            let details = inspected.iter().find(|details| details.id == line.id)?;
            Some(container(line, details))
        })
        .collect())
}

/// What comes from the image, like its user, working directory and volumes, made plain text that can't change
/// the terminal
fn container(line: PsLine, details: &Inspect) -> Container {
    Container {
        id: line.id,
        name: details.name.trim_start_matches('/').to_string(),
        image: plain(&details.config.image),
        image_id: details.image.clone(),
        user: plain(&details.config.user),
        workdir: plain(&details.config.working_dir),
        status: line.status,
        state: details.state.status.clone(),
        exit_code: details.state.exit_code,
        started_at: epoch_seconds(&details.state.started_at),
        finished_at: epoch_seconds(&details.state.finished_at),
        tty: details.config.tty,
        open_stdin: details.config.open_stdin,
        ips: ips(&details.network_settings),
        ports: ports(&details.network_settings),
        mounts: details.mounts.iter().flatten().map(plain_mount).collect(),
    }
}

/// The container's address in each network it's on
fn ips(settings: &NetworkSettings) -> Vec<String> {
    settings
        .networks
        .iter()
        .flatten()
        .map(|(_, network)| network.ip_address.clone())
        .filter(|ip| !ip.is_empty())
        .collect()
}

/// `8080->80/tcp` for a published port, `80/tcp` for one that isn't; once when it's bound on both IPv4 and IPv6
fn ports(settings: &NetworkSettings) -> Vec<String> {
    let mut ports: Vec<String> = Vec::new();
    for (port, bindings) in settings.ports.iter().flatten() {
        let shown: Vec<String> = match bindings {
            Some(bindings) if !bindings.is_empty() => bindings
                .iter()
                .map(|binding| plain(&format!("{}->{port}", binding.host_port)))
                .collect(),
            _ => vec![plain(port)],
        };
        for port in shown {
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
    }
    ports
}

/// Its name and paths made plain text
fn plain_mount(mount: &Mount) -> Mount {
    Mount {
        name: mount.name.as_deref().map(plain),
        source: plain(&mount.source),
        destination: plain(&mount.destination),
        ..mount.clone()
    }
}

/// An ISO 8601 time like `2026-11-06T13:08:00.123456789Z` in seconds since the epoch; 0 for docker's
/// `0001-01-01T00:00:00Z`, which means never, or for what isn't one
fn epoch_seconds(timestamp: &str) -> i64 {
    timestamp
        .parse::<jiff::Timestamp>()
        .map_or(0, jiff::Timestamp::as_second)
        .max(0)
}

/// The images' IDs from `docker image ls --no-trunc --format '{{json .}}'` lines, once for several tags
pub fn image_ids(lines: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for line in lines {
        if let Ok(line) = serde_json::from_str::<ImageLine>(line)
            && !ids.contains(&line.id)
        {
            ids.push(line.id);
        }
    }
    ids
}

/// `docker image ls --no-trunc --format '{{json .}}'` lines and `docker image inspect` of them; an image
/// that's gone from the inspect, or all of them when it can't be read, keep only what ls says
pub fn images(lines: &[String], inspect: &str) -> Result<Vec<Image>> {
    let lines: Vec<ImageLine> = json_lines(lines).context("can't read docker image ls")?;
    let inspected: Vec<ImageInspect> = serde_json::from_str(inspect).unwrap_or_default();
    Ok(lines
        .into_iter()
        .map(|line| {
            let mut image = Image {
                size_bytes: parse_size(&line.size).unwrap_or(0),
                created_at: image_created_at(&line.created_at),
                repository: line.repository,
                tag: line.tag,
                id: line.id,
                size: line.size,
                created_since: line.created_since,
                ..Image::default()
            };
            if let Some(details) = inspected.iter().find(|details| details.id == image.id) {
                add_details(&mut image, details);
            }
            image
        })
        .collect())
}

/// What the image runs and was built for; its labels and command are its author's, so they're made plain
/// text that can't change the terminal
fn add_details(image: &mut Image, details: &ImageInspect) {
    image.platform = platform(details);
    let Some(config) = &details.config else {
        return;
    };
    image.configured = true;
    image.command = command(config);
    image.user = plain(config.user.as_deref().unwrap_or_default());
    image.workdir = plain(config.working_dir.as_deref().unwrap_or_default());
    image.ports = exposed_ports(config);
    image.source = oci_label(config, "source");
    image.version = oci_label(config, "version");
    image.revision = oci_label(config, "revision");
}

/// Like `linux/arm64` or `linux/arm/v7`, from the parts docker knows
fn platform(details: &ImageInspect) -> String {
    [&details.os, &details.architecture, &details.variant]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .map(|part| plain(part))
        .collect::<Vec<_>>()
        .join("/")
}

/// The entrypoint, then the command, as one line
fn command(config: &ImageConfig) -> String {
    let args: Vec<String> = [&config.entrypoint, &config.cmd]
        .into_iter()
        .flatten()
        .flatten()
        .cloned()
        .collect();
    plain(&format::command(&args))
}

/// Like `80/tcp`
fn exposed_ports(config: &ImageConfig) -> Vec<String> {
    config
        .exposed_ports
        .iter()
        .flatten()
        .map(|(port, _)| plain(port))
        .collect()
}

/// An `org.opencontainers.image` label, empty when the image doesn't set it
fn oci_label(config: &ImageConfig, name: &str) -> String {
    config
        .labels
        .as_ref()
        .and_then(|labels| labels.get(&format!("org.opencontainers.image.{name}")))
        .map_or_else(String::new, |value| plain(value))
}

/// Docker's `--format '{{json .}}'` output, an object per line, without blank lines
fn json_lines<T: DeserializeOwned>(lines: &[String]) -> serde_json::Result<Vec<T>> {
    lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line))
        .collect()
}

/// An array like `docker inspect` prints, empty when there's no output
fn json_array<T: DeserializeOwned>(json: &str) -> serde_json::Result<Vec<T>> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(json)
}

/// One line of text that can't change the terminal
fn plain(text: &str) -> String {
    format::sanitize(text.replace(['\r', '\n'], " ").as_bytes(), MAX_TEXT)
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
    let volumes: Vec<VolumeInspect> =
        json_array(inspect).context("can't read docker volume inspect")?;
    Ok(volumes
        .into_iter()
        .map(|volume| Volume {
            created: format::local_date(&volume.created_at),
            created_at: epoch_seconds(&volume.created_at),
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
mod tests;
