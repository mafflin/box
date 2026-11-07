use super::*;

fn keys(groups: &[Vec<Hint>]) -> Vec<Vec<&str>> {
    groups
        .iter()
        .map(|group| group.iter().map(|hint| hint.key.as_str()).collect())
        .collect()
}

#[test]
fn drops_the_hints_that_go_first() {
    // `s stop  r restart  │  / filter  i details`, 41 wide
    let mut groups = vec![
        vec![hint("s", "stop", 3), hint("r", "restart", 1)],
        vec![hint("/", "filter", 2), hint("i", "details", 1)],
    ];
    fit(&mut groups, 41);
    assert_eq!(keys(&groups), [vec!["s", "r"], vec!["/", "i"]]);
    // Of two that go as soon, the later one
    fit(&mut groups, 30);
    assert_eq!(keys(&groups), [vec!["s", "r"], vec!["/"]]);
    fit(&mut groups, 20);
    assert_eq!(keys(&groups), [vec!["s"], vec!["/"]]);
    // No separator for an empty group
    fit(&mut groups, 6);
    assert_eq!(keys(&groups), [vec!["s"], vec![]]);
    fit(&mut groups, 0);
    assert_eq!(keys(&groups), [Vec::<&str>::new(), vec![]]);
}
