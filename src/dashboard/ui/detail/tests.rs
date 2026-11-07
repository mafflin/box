use super::*;

#[test]
fn cuts_lines_with_an_ellipsis() {
    let line = Line::from(vec![Span::raw("image    "), Span::raw("acme/api:dev")]);
    assert_eq!(cut(&line, 21).to_string(), "image    acme/api:dev");
    assert_eq!(cut(&line, 14).to_string(), "image    acme…");
    assert_eq!(cut(&line, 9).to_string(), "image   …");
    assert_eq!(cut(&line, 1).to_string(), "…");
}
