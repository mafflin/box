//! `box list`: a table of containers, images or volumes

use std::collections::HashMap;

use anyhow::Context;

use super::{CONTAINER_IMAGE_FORMAT, all_containers};
use crate::Result;
use crate::docker::{Docker, Mount, reference, shown_user, shown_workdir};
use crate::format::{self, short_id};
use crate::table::{self, Cell, Column, cell, lines};
use crate::theme::Theme;

const FIELD_SEPARATOR: &str = "\t";

const CONTAINER_COLUMNS: [Column; 6] = [
    Column::left("NAME", |theme| theme.name),
    Column::left("IMAGE", |theme| theme.image),
    Column::left("USER", |theme| theme.user),
    Column::left("STATUS", |theme| theme.status),
    Column::left("PORTS", |theme| theme.ports),
    Column::left("WORKDIR", |theme| theme.workdir),
];

const EXTENDED_CONTAINER_COLUMNS: [Column; 7] = [
    Column::left("NAME", |theme| theme.name),
    Column::left("IMAGE", |theme| theme.image),
    Column::left("USER", |theme| theme.user),
    Column::left("STATUS", |theme| theme.status),
    Column::left("IP", |theme| theme.ip),
    Column::left("PORTS", |theme| theme.ports),
    Column::left("MOUNTS", |theme| theme.mounts),
];

const IMAGE_COLUMNS: [Column; 5] = [
    Column::left("IMAGE", |theme| theme.image),
    Column::left("ID", |theme| theme.id),
    Column::right("SIZE", |theme| theme.size),
    Column::right("CREATED", |theme| theme.created),
    Column::right("CONTAINERS", |theme| theme.count),
];

const VOLUME_COLUMNS: [Column; 4] = [
    Column::left("VOLUME", |theme| theme.name),
    Column::left("DRIVER", |theme| theme.driver),
    Column::right("CREATED", |theme| theme.created),
    Column::right("CONTAINERS", |theme| theme.count),
];

const CONTAINER_FIELDS: [&str; 6] = [
    "{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}",
    "{{.Config.Image}}",
    "{{.Config.User}}",
    "{{range $port, $bindings := .NetworkSettings.Ports}}{{if $bindings}}{{range $bindings}}{{.HostPort}}->{{$port}} {{end}}{{else}}{{$port}} {{end}}{{end}}",
    "{{json .Mounts}}",
    "{{.Config.WorkingDir}}",
];

const CONTAINER_STATUS_FIELDS: [&str; 2] = ["{{.Names}}", "{{.Status}}"];

const IMAGE_FIELDS: [&str; 5] = [
    "{{.Repository}}",
    "{{.Tag}}",
    "{{.ID}}",
    "{{.Size}}",
    "{{.CreatedSince}}",
];

const CONTAINER_VOLUMES_FORMAT: &str =
    r#"{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}} {{end}}{{end}}"#;

const VOLUME_FIELDS: [&str; 3] = ["{{.Name}}", "{{.Driver}}", "{{.CreatedAt}}"];

/// Running containers, with their IPs and mounts instead of the working directory when extended
pub fn containers(docker: &impl Docker, extended: bool, theme: &Theme) -> Result<String> {
    let containers = records(docker, &["ps"], &CONTAINER_STATUS_FIELDS, &[])?;
    if containers.is_empty() {
        return Ok(String::new());
    }

    let names: Vec<&str> = containers.iter().map(|fields| fields[0].as_str()).collect();
    let details = records(docker, &["inspect"], &CONTAINER_FIELDS, &names)?;
    let mut rows = Vec::new();
    for (container, fields) in containers.iter().zip(&details) {
        let [ips, image, user, ports, mounts, workdir] = &fields[..] else {
            unreachable!("records pads its fields");
        };
        let mounts: Vec<Mount> = serde_json::from_str::<Option<Vec<Mount>>>(mounts)
            .context("can't read docker inspect's mounts")?
            .unwrap_or_default();
        let mut shown_ports: Vec<&str> = Vec::new();
        for port in ports.split_whitespace() {
            if !shown_ports.contains(&port) {
                shown_ports.push(port);
            }
        }

        let name = cell(&container[0]);
        let image = cell(image);
        let user = cell(shown_user(user));
        let status = cell(&container[1]);
        let ports = lines(shown_ports);
        rows.push(if extended {
            let mounts: Cell = mounts
                .iter()
                .map(|mount| format::mount(mount, workdir))
                .collect();
            vec![
                name,
                image,
                user,
                status,
                lines(ips.split_whitespace()),
                ports,
                mounts,
            ]
        } else {
            let workdir = cell(shown_workdir(workdir));
            vec![name, image, user, status, ports, workdir]
        });
    }

    let columns: &[Column] = if extended {
        &EXTENDED_CONTAINER_COLUMNS
    } else {
        &CONTAINER_COLUMNS
    };
    Ok(table::render(columns, &rows, theme))
}

/// Images and how many containers use them
pub fn images(docker: &impl Docker, theme: &Theme) -> Result<String> {
    let containers_by_image = tally(all_containers(docker, CONTAINER_IMAGE_FORMAT)?);
    let rows: Vec<Vec<Cell>> = records(docker, &["image", "ls", "--no-trunc"], &IMAGE_FIELDS, &[])?
        .into_iter()
        .map(|fields| {
            let [repository, tag, id, size, created] = &fields[..] else {
                unreachable!("records pads its fields");
            };
            vec![
                cell(reference(repository, tag)),
                cell(short_id(id)),
                cell(size),
                cell(created),
                cell(count(&containers_by_image, id)),
            ]
        })
        .collect();
    Ok(table::render(&IMAGE_COLUMNS, &rows, theme))
}

/// Volumes and how many containers use them
pub fn volumes(docker: &impl Docker, theme: &Theme) -> Result<String> {
    let names = docker.lines(&["volume", "ls", "-q"])?;
    if names.is_empty() {
        return Ok(String::new());
    }

    let volume_lists = all_containers(docker, CONTAINER_VOLUMES_FORMAT)?;
    let containers_by_volume = tally(
        volume_lists
            .iter()
            .flat_map(|volumes| volumes.split_whitespace())
            .map(String::from),
    );
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let rows: Vec<Vec<Cell>> = records(docker, &["volume", "inspect"], &VOLUME_FIELDS, &names)?
        .into_iter()
        .map(|fields| {
            let [name, driver, created_at] = &fields[..] else {
                unreachable!("records pads its fields");
            };
            vec![
                cell(name),
                cell(driver),
                cell(format::local_date(created_at)),
                cell(count(&containers_by_volume, name)),
            ]
        })
        .collect();
    Ok(table::render(&VOLUME_COLUMNS, &rows, theme))
}

/// Each output line's fields, as many as asked for, the last one taking the rest
fn records(
    docker: &impl Docker,
    command: &[&str],
    fields: &[&str],
    targets: &[&str],
) -> Result<Vec<Vec<String>>> {
    let format = fields.join(FIELD_SEPARATOR);
    let mut args = command.to_vec();
    args.extend(["--format", &format]);
    args.extend(targets);
    Ok(docker
        .lines(&args)?
        .iter()
        .map(|line| {
            let mut values: Vec<String> = line
                .splitn(fields.len(), FIELD_SEPARATOR)
                .map(String::from)
                .collect();
            values.resize(fields.len(), String::new());
            values
        })
        .collect())
}

fn tally(values: impl IntoIterator<Item = String>) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

/// Empty for none
fn count(counts: &HashMap<String, usize>, key: &str) -> String {
    counts.get(key).map(ToString::to_string).unwrap_or_default()
}
