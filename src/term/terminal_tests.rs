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

    let (bottom_screen, _, _, _) = read_term_screen(&term);

    scroll(&term, 3);
    let offset = term.lock().grid().display_offset();
    let (scrolled_screen, _, _, _) = read_term_screen(&term);

    assert_ne!(
        bottom_screen, scrolled_screen,
        "screen content should differ after scrolling up (display_offset={offset})"
    );

    scroll_bottom(&term);
    let (restored_screen, _, _, _) = read_term_screen(&term);
    assert_eq!(
        bottom_screen, restored_screen,
        "screen should match original after scrolling back"
    );
}
