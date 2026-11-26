use super::*;
use crate::job_manager::{AsyncOpDomain, CancellationSignal, Job, JobMessage};
use crate::test_utils::MockTerminal;
use std::collections::VecDeque;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

struct FrameTerminal {
    inner: MockTerminal,
    events: VecDeque<Option<InputEvent>>,
    polls: Vec<Duration>,
}

impl TerminalBackend for FrameTerminal {
    fn init(&mut self) -> Result<(), RiftError> {
        self.inner.init()
    }

    fn deinit(&mut self) {
        self.inner.deinit();
    }

    fn poll(&mut self, duration: Duration) -> Result<bool, RiftError> {
        self.polls.push(duration);
        Ok(!self.events.is_empty())
    }

    fn read_event(&mut self) -> Result<Option<InputEvent>, RiftError> {
        Ok(self.events.pop_front().flatten())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), RiftError> {
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> Result<(), RiftError> {
        self.inner.flush()
    }

    fn get_size(&self) -> Result<crate::term::Size, RiftError> {
        self.inner.get_size()
    }

    fn clear_screen(&mut self) -> Result<(), RiftError> {
        self.inner.clear_screen()
    }

    fn move_cursor(&mut self, row: u16, col: u16) -> Result<(), RiftError> {
        self.inner.move_cursor(row, col)
    }

    fn hide_cursor(&mut self) -> Result<(), RiftError> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), RiftError> {
        self.inner.show_cursor()
    }

    fn clear_to_end_of_line(&mut self) -> Result<(), RiftError> {
        self.inner.clear_to_end_of_line()
    }

    fn set_cursor_shape(&mut self, shape: crate::term::CursorShape) -> Result<(), RiftError> {
        self.inner.set_cursor_shape(shape)
    }
}

fn editor() -> Editor<FrameTerminal> {
    let terminal = FrameTerminal {
        inner: MockTerminal::new(24, 80),
        events: VecDeque::new(),
        polls: Vec::new(),
    };
    let mut editor = Editor::new(terminal).unwrap();
    editor.state.settings.poll_timeout_ms = 1000;
    editor.last_notification_generation = editor.state.notifications.generation;
    editor.term.inner.clear();
    editor.term.polls.clear();
    editor
}

#[test]
fn terminal_frame_delay_respects_exact_frame_boundary() {
    let mut editor = editor();
    let start = crate::time::Instant::now();
    editor.last_frame_at = Some(start);
    editor.terminal_frame_pending = true;

    assert_eq!(editor.terminal_frame_delay(start), Some(FRAME_INTERVAL));
    assert_eq!(
        editor.terminal_frame_delay(
            (start + FRAME_INTERVAL)
                .checked_sub(Duration::from_nanos(1))
                .unwrap()
        ),
        Some(Duration::from_nanos(1))
    );
    assert_eq!(
        editor.terminal_frame_delay(start + FRAME_INTERVAL),
        Some(Duration::ZERO)
    );
}

#[test]
fn terminal_frame_delay_requires_an_owed_frame() {
    let mut editor = editor();
    let now = crate::time::Instant::now();
    editor.last_frame_at = None;
    assert_eq!(editor.terminal_frame_delay(now), None);
    editor.terminal_frame_pending = true;
    assert_eq!(editor.terminal_frame_delay(now), Some(Duration::ZERO));
}

#[test]
fn pending_terminal_frame_waits_then_renders_without_new_messages() {
    let mut editor = editor();
    editor.terminal_frame_pending = true;
    editor.last_frame_at = Some(crate::time::Instant::now() + Duration::from_secs(60));
    editor.tick().unwrap();
    assert_eq!(editor.term.inner.flushes, 0);
    assert!(editor.terminal_frame_pending);
    assert!(editor.term.polls[0] <= FRAME_INTERVAL);

    editor.last_frame_at = crate::time::Instant::now().checked_sub(FRAME_INTERVAL);
    editor.tick().unwrap();
    assert_eq!(editor.term.inner.flushes, 1);
    assert!(!editor.terminal_frame_pending);

    // A new output notification immediately after the frame must wait again.
    editor.terminal_frame_pending = true;
    editor.last_frame_at = Some(crate::time::Instant::now() + Duration::from_secs(60));
    editor.tick().unwrap();
    assert_eq!(editor.term.inner.flushes, 1);
    assert!(editor.terminal_frame_pending);
}

#[test]
fn filtered_input_does_not_strand_a_due_terminal_frame() {
    let mut editor = editor();
    editor.term.events.push_back(None);
    editor.terminal_frame_pending = true;
    editor.last_frame_at = crate::time::Instant::now().checked_sub(FRAME_INTERVAL);
    editor.tick().unwrap();
    assert_eq!(editor.term.inner.flushes, 1);
    assert!(!editor.terminal_frame_pending);
}

#[test]
fn consumed_count_key_does_not_strand_a_due_terminal_frame() {
    let mut editor = editor();
    editor
        .term
        .events
        .extend([Some(InputEvent::Key(Key::char('2'))), None]);
    editor.terminal_frame_pending = true;
    editor.last_frame_at = crate::time::Instant::now().checked_sub(FRAME_INTERVAL);
    editor.tick().unwrap();
    assert_eq!(editor.pending_count, 2);
    assert_eq!(editor.term.inner.flushes, 1);
    assert!(!editor.terminal_frame_pending);
}

#[test]
fn key_render_does_not_wait_for_terminal_frame_deadline() {
    let mut editor = editor();
    editor
        .term
        .events
        .push_back(Some(InputEvent::Key(Key::char('i'))));
    editor.terminal_frame_pending = true;
    editor.last_frame_at = Some(crate::time::Instant::now() + Duration::from_secs(60));
    editor.tick().unwrap();
    assert_eq!(editor.current_mode, Mode::Insert);
    assert_eq!(editor.term.inner.flushes, 1);
    assert!(!editor.terminal_frame_pending);
}

#[derive(Debug)]
struct CaptureJobSender(Sender<Sender<JobMessage>>);

impl Job for CaptureJobSender {
    fn run(self: Box<Self>, _id: usize, sender: Sender<JobMessage>, _signal: CancellationSignal) {
        self.0.send(sender).unwrap();
    }

    fn is_silent(&self) -> bool {
        true
    }
}

#[test]
fn terminal_output_batch_drains_past_ten_without_bypassing_frame_deadline() {
    let mut editor = editor();
    let doc = editor.document_manager.active_document().unwrap();
    let handle = doc.handle();
    let kind = doc.buffer_kind_id();
    let token = editor
        .job_manager
        .next_token(handle, kind, AsyncOpDomain::Terminal);
    let (tx, rx) = mpsc::channel();
    editor.job_manager.spawn(CaptureJobSender(tx));
    let sender = rx.recv().unwrap();
    let started = editor.job_manager.receiver().recv().unwrap();
    editor.handle_job_message(started);
    editor.update_and_render().unwrap();
    editor.last_notification_generation = editor.state.notifications.generation;
    editor.term.inner.clear();
    editor.last_frame_at = Some(crate::time::Instant::now() + Duration::from_secs(60));

    for _ in 0..64 {
        sender
            .send(JobMessage::TerminalOutput(token, Vec::new()))
            .unwrap();
    }
    editor.tick().unwrap();

    assert!(editor.job_manager.receiver().try_recv().is_err());
    assert!(editor.terminal_frame_pending);
    assert_eq!(editor.term.inner.flushes, 0);

    editor.last_frame_at = crate::time::Instant::now().checked_sub(FRAME_INTERVAL);
    editor.tick().unwrap();
    assert_eq!(editor.term.inner.flushes, 1);
    assert!(!editor.terminal_frame_pending);
}

#[test]
fn hidden_terminal_output_does_not_schedule_a_frame() {
    let mut editor = editor();
    let hidden_id = editor.document_manager.next_id();
    let hidden = crate::document::Document::new(hidden_id).unwrap();
    editor.open_buffer(hidden, crate::document::manager::Placement::Focused);
    let doc = editor.document_manager.get_document(hidden_id).unwrap();
    let handle = doc.handle();
    let kind = doc.buffer_kind_id();
    let token = editor
        .job_manager
        .next_token(handle, kind, AsyncOpDomain::Terminal);

    editor.handle_job_message(JobMessage::TerminalOutput(token, Vec::new()));

    assert!(!editor.terminal_frame_pending);
    assert_eq!(editor.term.inner.flushes, 0);
}

#[cfg(feature = "plugins")]
#[test]
fn poll_timeout_shrinks_to_the_next_plugin_timer() {
    let mut editor = editor();
    assert!(editor
        .plugin_host
        .lua_exec("rift.timer(200, function() end)")
        .is_none());

    editor.tick().unwrap();

    assert!(editor.term.polls[0] <= Duration::from_millis(200));
    assert!(editor.term.polls[0] > Duration::from_millis(100));
}

#[test]
fn poll_timeout_shrinks_to_the_cursor_hold_deadline() {
    let mut editor = editor();

    editor.tick().unwrap();

    assert!(editor.term.polls[0] <= Duration::from_millis(400));
    assert!(editor.term.polls[0] > Duration::from_millis(300));
}

#[test]
fn poll_timeout_shrinks_to_the_search_refresh_deadline() {
    let mut editor = editor();
    editor.state.last_search_query = Some("x".to_string());
    editor.schedule_search_refresh();

    editor.tick().unwrap();

    assert!(editor.term.polls[0] <= super::super::scheduler::SEARCH_REFRESH_DEBOUNCE);
}

#[test]
fn poll_timeout_shrinks_to_the_syntax_reparse_deadline() {
    let mut editor = editor();
    let doc_id = editor.active_document_id();
    editor.debounce_syntax_reparse(doc_id);

    editor.tick().unwrap();

    assert!(editor.term.polls[0] <= super::super::scheduler::SYNTAX_REPARSE_DEBOUNCE);
}

#[test]
fn poll_timeout_shrinks_to_the_key_sequence_timeout() {
    let mut editor = editor();
    editor.pending_keys.push(Key::char('g'));
    editor.pending_keys_started_at = crate::time::Instant::now()
        .checked_sub(Duration::from_millis(KEY_SEQUENCE_TIMEOUT_MS - 50));

    editor.tick().unwrap();

    assert!(editor.term.polls[0] <= Duration::from_millis(50));
}

#[test]
fn poll_timeout_shrinks_to_the_next_notification_expiry() {
    let mut editor = editor();
    editor.state.settings.poll_timeout_ms = 60_000;
    editor.plugin_host.set_cursor_hold_delay_ms(60_000);
    editor.notify(crate::notification::NotificationType::Success, "saved");

    editor.tick().unwrap();

    assert!(editor.term.polls[0] <= Duration::from_secs(3));
    assert!(editor.term.polls[0] > Duration::from_secs(2));
}
