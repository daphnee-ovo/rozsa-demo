use rozsa_tui::input::kill_ring::{KillRing, PushOpts};

#[test]
fn basic_push_peek() {
    let mut kr = KillRing::new();
    kr.push("hello", PushOpts { prepend: false, accumulate: false });
    assert_eq!(kr.peek(), Some("hello"));
}

#[test]
fn empty_push_ignored() {
    let mut kr = KillRing::new();
    kr.push("", PushOpts { prepend: false, accumulate: false });
    assert_eq!(kr.len(), 0);
}

#[test]
fn accumulate_append() {
    let mut kr = KillRing::new();
    kr.push("hello", PushOpts { prepend: false, accumulate: false });
    kr.push(" world", PushOpts { prepend: false, accumulate: true });
    assert_eq!(kr.peek(), Some("hello world"));
    assert_eq!(kr.len(), 1);
}

#[test]
fn accumulate_prepend() {
    let mut kr = KillRing::new();
    kr.push("world", PushOpts { prepend: true, accumulate: false });
    kr.push("hello ", PushOpts { prepend: true, accumulate: true });
    assert_eq!(kr.peek(), Some("hello world"));
}

#[test]
fn rotate() {
    let mut kr = KillRing::new();
    kr.push("first", PushOpts { prepend: false, accumulate: false });
    kr.push("second", PushOpts { prepend: false, accumulate: false });
    kr.push("third", PushOpts { prepend: false, accumulate: false });
    assert_eq!(kr.peek(), Some("third"));
    kr.rotate();
    assert_eq!(kr.peek(), Some("second"));
    kr.rotate();
    assert_eq!(kr.peek(), Some("first"));
}
