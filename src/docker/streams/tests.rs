use super::*;

#[test]
fn reads_lines_until_told_to_stop() {
    let mut seen = Vec::new();
    read_lines(&b"one\ntwo\r\nthree"[..], |line| {
        seen.push(String::from_utf8_lossy(line).into_owned());
        true
    });
    assert_eq!(seen, ["one", "two\r", "three"]);

    let mut count = 0;
    read_lines(&b"a\nb\nc\n"[..], |_| {
        count += 1;
        count < 2
    });
    assert_eq!(count, 2);
}
