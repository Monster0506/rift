use super::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Holds = Arc<Mutex<Vec<(usize, usize)>>>;

fn recording_host(delay: Duration) -> (PluginHost, Holds) {
    let mut host = PluginHost::new(delay);
    let holds = Holds::default();
    let seen = Arc::clone(&holds);
    host.on("CursorHold", move |event| {
        if let EditorEvent::CursorHold { row, col, .. } = event {
            seen.lock().unwrap().push((*row, *col));
        }
    });
    (host, holds)
}

fn move_cursor(host: &mut PluginHost, row: usize, col: usize) {
    host.dispatch(&EditorEvent::CursorMoved {
        buf: DocumentId::new(1),
        row,
        col,
    });
}

fn hold_deadline(host: &PluginHost) -> crate::time::Instant {
    host.next_timer_deadline().expect("CursorHold is armed")
}

#[test]
fn cursor_hold_fires_once_after_the_delay() {
    let (mut host, holds) = recording_host(Duration::from_millis(400));
    let before = crate::time::Instant::now();
    move_cursor(&mut host, 3, 4);
    let deadline = hold_deadline(&host);
    assert!(deadline >= before + Duration::from_millis(400));

    let just_before = deadline.checked_sub(Duration::from_nanos(1)).unwrap();
    assert!(!host.fire_due_timers(just_before));
    assert!(holds.lock().unwrap().is_empty());

    assert!(host.fire_due_timers(deadline));
    assert_eq!(*holds.lock().unwrap(), [(3, 4)]);
    assert!(!host.fire_due_timers(deadline + Duration::from_secs(60)));
    assert_eq!(holds.lock().unwrap().len(), 1);
    assert_eq!(host.next_timer_deadline(), None);
}

#[test]
fn cursor_move_restarts_the_hold() {
    let (mut host, holds) = recording_host(Duration::from_millis(400));
    move_cursor(&mut host, 1, 0);
    let first = hold_deadline(&host);
    move_cursor(&mut host, 2, 0);
    let second = hold_deadline(&host);
    assert!(second >= first);

    assert!(host.fire_due_timers(second));
    assert_eq!(*holds.lock().unwrap(), [(2, 0)]);

    move_cursor(&mut host, 2, 0);
    assert_eq!(host.next_timer_deadline(), None, "same position stays held");

    move_cursor(&mut host, 5, 1);
    assert!(host.fire_due_timers(hold_deadline(&host)));
    assert_eq!(*holds.lock().unwrap(), [(2, 0), (5, 1)]);
}
