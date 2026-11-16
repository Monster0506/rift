use super::*;
use alacritty_terminal::grid::{Dimensions as _, Scroll};
use alacritty_terminal::term::Config;
use std::sync::mpsc;

fn make_term(rows: u16, cols: u16) -> Arc<FairMutex<Term<TerminalListener>>> {
    let (tx, _rx) = mpsc::channel();
    let listener = TerminalListener(tx);
    let dims = TermDims {
        rows: rows as usize,
        cols: cols as usize,
    };
    Arc::new(FairMutex::new(Term::new(
        Config::default(),
        &dims,
        listener,
    )))
}

fn feed(term: &Arc<FairMutex<Term<TerminalListener>>>, data: &[u8]) {
    let mut parser = alacritty_terminal::vte::ansi::Processor::<
        alacritty_terminal::vte::ansi::StdSyncHandler,
    >::new();
    parser.advance(&mut *term.lock(), data);
}

fn scroll(term: &Arc<FairMutex<Term<TerminalListener>>>, delta: i32) {
    term.lock().scroll_display(Scroll::Delta(delta));
}

fn scroll_bottom(term: &Arc<FairMutex<Term<TerminalListener>>>) {
    term.lock().scroll_display(Scroll::Bottom);
}

#[test]
fn test_display_offset_changes_after_scroll() {
    let term = make_term(5, 40);

    for i in 0..20u32 {
        feed(&term, format!("LINE{i}\r\n").as_bytes());
    }

    let history_before = term.lock().grid().history_size();
    let offset_before = term.lock().grid().display_offset();
    assert!(
        history_before > 0,
        "expected scrollback, got 0 (offset={offset_before})"
    );

    scroll(&term, 3);

    let offset_after = term.lock().grid().display_offset();
    assert_eq!(
        offset_after,
        3.min(history_before),
        "display_offset should be 3 after scroll(+3)"
    );
}

#[test]
fn test_scrollback_changes_visible_content() {
    let term = make_term(5, 40);

    for i in 0..20u32 {
        feed(&term, format!("LINE{i}\r\n").as_bytes());
    }

    let history = term.lock().grid().history_size();
    assert!(history > 0, "need scrollback history, got 0");

    let (bottom_screen, _, _, _, _) = snapshot(&term);

    scroll(&term, 3);
    let offset = term.lock().grid().display_offset();
    let (scrolled_screen, _, _, _, _) = snapshot(&term);

    assert_ne!(
        bottom_screen, scrolled_screen,
        "screen content should differ after scrolling up (display_offset={offset})"
    );

    scroll_bottom(&term);
    let (restored_screen, _, _, _, _) = snapshot(&term);
    assert_eq!(
        bottom_screen, restored_screen,
        "screen should match original after scrolling back"
    );
}

fn snapshot(
    term: &Arc<FairMutex<Term<TerminalListener>>>,
) -> (
    String,
    usize,
    usize,
    crate::color::CellColorSpans,
    ScrollPosition,
) {
    read_term_screen(term, Config::default().scrolling_history)
}

#[test]
fn snapshot_reports_output_scroll_and_scrollback_direction() {
    let term = make_term(3, 40);
    let previous = snapshot(&term).4;
    feed(&term, b"one\r\ntwo\r\nthree\r\nfour\r\n");
    let bottom = snapshot(&term).4;
    assert_eq!(bottom.scroll_since(previous), Some(2));

    scroll(&term, 1);
    let scrolled = snapshot(&term).4;
    assert_eq!(scrolled.scroll_since(bottom), Some(-1));
    scroll_bottom(&term);
    assert_eq!(snapshot(&term).4.scroll_since(scrolled), Some(1));
}

#[test]
fn snapshot_output_keeps_scrolled_view_stationary() {
    let term = make_term(3, 40);
    feed(&term, b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n");
    scroll(&term, 2);
    let previous = snapshot(&term).4;
    feed(&term, b"six\r\n");
    assert_eq!(snapshot(&term).4.scroll_since(previous), Some(0));
}

#[test]
fn snapshot_rejects_resize_and_alternate_screen_transitions() {
    let term = make_term(3, 40);
    feed(&term, b"one\r\ntwo\r\nthree\r\nfour\r\n");
    let previous = snapshot(&term).4;
    term.lock().resize(TermDims { rows: 4, cols: 40 });
    let resized = snapshot(&term).4;
    assert_eq!(resized.scroll_since(previous), None);
    feed(&term, b"\x1b[?1049h");
    let alternate = snapshot(&term).4;
    assert_eq!(alternate.scroll_since(resized), None);
    feed(&term, b"\n\n\n\n");
    assert_eq!(snapshot(&term).4.scroll_since(alternate), None);
    feed(&term, b"\x1b[?1049l");
    assert_eq!(snapshot(&term).4.scroll_since(alternate), None);
}

#[test]
fn snapshot_rejects_cleared_history() {
    let term = make_term(3, 40);
    feed(&term, b"one\r\ntwo\r\nthree\r\nfour\r\n");
    let previous = snapshot(&term).4;
    feed(&term, b"\x1b[3J");
    assert_eq!(snapshot(&term).4.scroll_since(previous), None);
}

#[test]
fn snapshot_rejects_scrollback_saturation_without_changing_history() {
    let term = make_term(3, 40);
    term.lock().grid_mut().update_history(2);
    feed(&term, b"one\r\ntwo\r\nthree\r\n");
    let previous = read_term_screen(&term, 2).4;
    feed(&term, b"four\r\nfive\r\nsix\r\n");
    let saturated = read_term_screen(&term, 2).4;
    assert_eq!(saturated.scroll_since(previous), None);
    assert_eq!(term.lock().grid().history_size(), 2);
    feed(&term, b"seven\r\n");
    assert_eq!(read_term_screen(&term, 2).4.scroll_since(saturated), None);
}
