//! Everything box gets from docker goes through the docker command

mod parse;
mod run;
pub mod streams;

pub use parse::{
    Container, Image, Mount, Stats, Volume, containers, images, is_change, ps_ids, reference,
    shown_user, shown_workdir, stats, volumes,
};
pub use run::{Captured, Docker, Terminal, check, partial_output};

#[cfg(test)]
pub use run::fake;
