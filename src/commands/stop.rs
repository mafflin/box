//! `box stop`

use crate::Result;
use crate::docker::Docker;

/// Stops each running container, stopping at the first failure
pub fn stop(docker: &impl Docker) -> Result<()> {
    for id in docker.lines(&["ps", "-q", "--no-trunc"])? {
        docker.run(&["stop", &id])?;
    }
    Ok(())
}
