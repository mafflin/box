use super::*;

#[test]
fn cuts_lines_with_an_ellipsis() {
    let line = Line::from(vec![Span::raw("image    "), Span::raw("acme/api:dev")]);
    assert_eq!(cut(&line, 21).to_string(), "image    acme/api:dev");
    assert_eq!(cut(&line, 14).to_string(), "image    acme…");
    assert_eq!(cut(&line, 9).to_string(), "image   …");
    assert_eq!(cut(&line, 1).to_string(), "…");
    // By the columns text takes, two for each of these, and nothing after one that doesn't fit
    let line = Line::from(vec![
        Span::raw("label    "),
        Span::raw("漢字漢字"),
        Span::raw("x"),
    ]);
    assert_eq!(cut(&line, 14).to_string(), "label    漢字…");
    assert_eq!(cut(&line, 13).to_string(), "label    漢…");
}

#[test]
fn eighths_cover_a_cell() {
    assert_eq!(EIGHTHS.len(), usize::from(EIGHTHS_PER_CELL) + 1);
}
