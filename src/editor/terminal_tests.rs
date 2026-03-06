use super::*;
use crate::test_utils::MockTerminal;

fn process_jobs(editor: &mut Editor<MockTerminal>) {
    let mut processed = 0;
    while let Ok(msg) = editor.job_manager.receiver().try_recv() {
        editor.handle_job_message(msg).unwrap();
        processed += 1;
        if processed > 100 {
            break;
        }
    }
}

#[test]
fn test_terminal_opens_in_normal_mode() {
    let term = MockTerminal::new(24, 80);
    let mut editor = Editor::new(term).unwrap();

    editor.open_terminal(None).unwrap();

    process_jobs(&mut editor);

    assert_eq!(
        editor.current_mode,
        Mode::Normal,
        "Terminal should start in Normal mode"
    );
}
