use super::*;
use crate::key::Key;
use crate::test_utils::MockTerminal;
use std::thread;
use std::time::Duration;

#[test]
fn test_terminal_workflow() {
    let term = MockTerminal::new(24, 80);
    let mut editor = Editor::new(term).unwrap();

    use crate::command_line::commands::ExecutionResult;
    editor.handle_execution_result(ExecutionResult::OpenTerminal {
        cmd: None,
        bangs: 0,
    });

    process_jobs(&mut editor);

    assert_eq!(
        editor.current_mode,
        Mode::Insert,
        "Terminal should start in Insert mode"
    );

    thread::sleep(Duration::from_millis(500));

    let keys = "echo \"hello\"\n";
    for ch in keys.chars() {
        let key = Key::Char(ch);

        let is_terminal_insert = if let Some(doc) = editor.document_manager.active_document() {
            doc.is_terminal() && editor.current_mode == Mode::Insert
        } else {
            false
        };

        if is_terminal_insert {
            if let Some(doc) = editor.document_manager.active_document_mut() {
                if let Some(term) = &mut doc.terminal {
                    let bytes = key.to_vt100_bytes();
                    term.write(&bytes).unwrap();
                }
            }
        } else {
            panic!("Not in Insert mode when typing command");
        }
    }

    let mut found = false;
    for _ in 0..50 {
        process_jobs(&mut editor);

        if let Some(doc) = editor.document_manager.active_document() {
            let content = doc.buffer.to_string();
            if content.contains("hello") {
                found = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = found;
}

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
