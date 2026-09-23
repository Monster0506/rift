use super::*;

#[test]
fn system_clipboard_cache_starts_empty() {
    let cache = SystemClipboardCache::new();
    assert!(cache.text().is_none());
    assert!(cache.last_refreshed.is_none());
}

#[test]
fn capture_text_preserves_byte_and_control_chars() {
    let mut buf = TextBuffer::new(16).unwrap();
    let _ = buf.insert_chars(&[
        Character::Unicode('a'),
        Character::Byte(0xFF),
        Character::Control(0x0C),
        Character::Unicode('b'),
    ]);
    let range = MotionRange {
        anchor: 0,
        new_cursor: 4,
        kind: RangeKind::Charwise,
        inclusive: false,
    };
    let captured = capture_text(&buf, &range);
    assert_eq!(
        captured,
        vec![
            Character::Unicode('a'),
            Character::Byte(0xFF),
            Character::Control(0x0C),
            Character::Unicode('b'),
        ]
    );
}

#[test]
fn read_system_clipboard_text_never_blocks_past_timeout() {
    // Can't simulate a truly hung owner here, but a non-hung read should
    // return well within the bounded read deadline.
    let start = std::time::Instant::now();
    let _ = read_system_clipboard_text();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "clipboard read took {:?}, expected it to be bounded by the read timeout",
        start.elapsed()
    );
}

#[test]
fn refresh_if_stale_does_not_block_render_thread() {
    // refresh_if_stale must never perform the arboard read inline on the
    // calling thread for longer than the bounded worker read allows.
    let mut cache = SystemClipboardCache::new();
    let start = std::time::Instant::now();
    cache.refresh_if_stale();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "refresh_if_stale took {:?}, expected the read to be off-thread and bounded",
        start.elapsed()
    );
    assert_eq!(cache.read_count(), 1);
}

#[test]
fn system_clipboard_cache_skips_rereading_within_interval() {
    let mut cache = SystemClipboardCache::new();
    cache.refresh_if_stale();
    let first = cache.last_refreshed;
    assert!(first.is_some(), "first call must record a refresh time");

    // Calling again immediately must not touch the OS clipboard or the
    // timestamp -- that's the whole point of the cache.
    cache.refresh_if_stale();
    assert_eq!(
        cache.last_refreshed, first,
        "second call within the refresh interval must not re-read"
    );
}
