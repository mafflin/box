//! Everything box gets from docker goes through the docker command

mod parse;
mod run;
pub mod streams;

pub use parse::{
    Container, Image, JSON_FORMAT, Mount, Stats, VOLUME_MOUNT, Volume, containers, image_ids,
    images, is_change, ps_ids, reference, state, stats, volumes,
};
pub use run::{
    Captured, DOCKER, Docker, FORCE, INSPECT_VOLUMES, LIST_VOLUMES, REMOVE_CONTAINER, REMOVE_IMAGE,
    REMOVE_VOLUME, Terminal, args, cant_run, check, command, inspect_all, without_daemon_prefix,
};

#[cfg(test)]
pub use run::fake;
