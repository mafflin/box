use super::*;
use crate::docker::fake::Fake;

#[test]
fn prunes_in_order_and_stops_at_a_failure() {
    let mut docker = Fake::default();
    docker.respond(&["ps", "--filter"], &["c1", "c2"]);
    docker.respond(&["volume", "ls"], &["v1"]);
    docker.respond(&["images"], &["i1"]);
    let removed = prune(&docker, &[PruneTarget::Images, PruneTarget::Containers]).unwrap();
    assert_eq!(
        removed,
        [(PruneTarget::Containers, 2), (PruneTarget::Images, 1)]
    );
    let changes: Vec<String> = docker
        .calls
        .borrow()
        .iter()
        .map(|call| call.join(" "))
        .collect();
    assert_eq!(
        changes,
        [
            "ps --filter status=exited --filter status=created -q",
            "rm -v c1",
            "rm -v c2",
            "images --filter dangling=true -q --no-trunc",
            "ps -aq",
            "rmi i1"
        ]
    );

    docker.fail(&["rm", "-v", "c1"]);
    let error = prune(&docker, &PruneTarget::ORDER).unwrap_err();
    assert_eq!(error.to_string(), "docker rm -v c1 failed");
    assert_eq!(PruneTarget::Volumes.count(1), "1 volume");
    assert_eq!(PruneTarget::Images.count(0), "0 images");
}

#[test]
fn leaves_images_that_containers_use() {
    let mut docker = Fake::default();
    docker.respond(&["images"], &["sha256:a", "sha256:b"]);
    docker.respond(&["ps", "-aq"], &["web"]);
    docker.respond(&["inspect", "--format", "{{.Image}}"], &["sha256:a"]);
    let removed = prune(&docker, &[PruneTarget::Images]).unwrap();
    assert_eq!(removed, [(PruneTarget::Images, 1)]);
    let calls = docker.calls.borrow();
    assert_eq!(calls.last().unwrap(), &["rmi", "sha256:b"]);
    assert!(!calls.iter().any(|call| call == &["rmi", "sha256:a"]));
}
