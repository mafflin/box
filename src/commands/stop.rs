//! `box stop`

use crate::Result;
use crate::docker::Docker;

/// Stops each running container, stopping at the first failure
pub fn stop(docker: &impl Docker) -> Result<()> {
    for name in docker.lines(&["ps", "--format", "{{.Names}}"])? {
        docker.run(&["stop", &name])?;
    }
    Ok(())
}
