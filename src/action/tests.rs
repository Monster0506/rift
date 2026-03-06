//! Tests for the action module

use super::*;

#[test]
fn test_motion_equals() {
    assert_eq!(Motion::Left, Motion::Left);
    assert_ne!(Motion::Left, Motion::Right);
}

#[test]
fn test_action_equals() {
    assert_eq!(
        Action::Editor(EditorAction::Move(Motion::Left)),
        Action::Editor(EditorAction::Move(Motion::Left))
    );
    assert_ne!(
        Action::Editor(EditorAction::Move(Motion::Left)),
        Action::Editor(EditorAction::Move(Motion::Right))
    );
}

#[test]
fn test_all_motions_debug_and_clone() {
    let motions = vec![
        Motion::Left,
        Motion::Right,
        Motion::Up,
        Motion::Down,
        Motion::StartOfLine,
        Motion::EndOfLine,
        Motion::StartOfFile,
        Motion::EndOfFile,
        Motion::PageUp,
        Motion::PageDown,
        Motion::NextWord,
        Motion::PreviousWord,
        Motion::NextParagraph,
        Motion::PreviousParagraph,
        Motion::NextSentence,
        Motion::PreviousSentence,
    ];

    for motion in motions {
        let cloned = motion.clone();
        assert_eq!(motion, cloned);
        assert_eq!(format!("{:?}", motion), format!("{:?}", cloned));
    }
}

#[test]
fn test_action_from_str() {
    // Tests for legacy string to Action conversion

    // Editor Actions
    assert_eq!(
        Action::from_str("editor:move_left").unwrap(),
        Action::Editor(EditorAction::Move(Motion::Left))
    );
    assert_eq!(
        Action::from_str("editor:enter_insert_mode").unwrap(),
        Action::Editor(EditorAction::EnterInsertMode)
    );

    // New Editor Actions
    assert_eq!(
        Action::from_str("editor:buffer_next").unwrap(),
        Action::Editor(EditorAction::BufferNext)
    );
    assert_eq!(
        Action::from_str("editor:buffer_previous").unwrap(),
        Action::Editor(EditorAction::BufferPrevious)
    );
    assert_eq!(
        Action::from_str("editor:delete_line").unwrap(),
        Action::Editor(EditorAction::DeleteLine)
    );
    assert_eq!(
        Action::from_str("editor:delete_char").unwrap(),
        Action::Editor(EditorAction::Delete(Motion::Right))
    );
    assert_eq!(
        Action::from_str("editor:enter_insert_mode_after").unwrap(),
        Action::Editor(EditorAction::EnterInsertModeAfter)
    );
    assert_eq!(
        Action::from_str("editor:enter_search_mode").unwrap(),
        Action::Editor(EditorAction::EnterSearchMode)
    );
    assert_eq!(
        Action::from_str("editor:toggle_debug").unwrap(),
        Action::Editor(EditorAction::ToggleDebug)
    );
    assert_eq!(
        Action::from_str("editor:redraw").unwrap(),
        Action::Editor(EditorAction::Redraw)
    );
    assert_eq!(
        Action::from_str("editor:save").unwrap(),
        Action::Editor(EditorAction::Save)
    );
    assert_eq!(
        Action::from_str("editor:save_and_quit").unwrap(),
        Action::Editor(EditorAction::SaveAndQuit)
    );
    assert_eq!(
        Action::from_str("editor:open_explorer").unwrap(),
        Action::Editor(EditorAction::OpenExplorer)
    );
    assert_eq!(
        Action::from_str("editor:open_undotree").unwrap(),
        Action::Editor(EditorAction::OpenUndoTree)
    );
    assert_eq!(
        Action::from_str("editor:show_buffer_list").unwrap(),
        Action::Editor(EditorAction::ShowBufferList)
    );
    assert_eq!(
        Action::from_str("editor:clear_highlights").unwrap(),
        Action::Editor(EditorAction::ClearHighlights)
    );
    assert_eq!(
        Action::from_str("editor:clear_notifications").unwrap(),
        Action::Editor(EditorAction::ClearNotifications)
    );
    assert_eq!(
        Action::from_str("editor:clear_last_notification").unwrap(),
        Action::Editor(EditorAction::ClearLastNotification)
    );
    assert_eq!(
        Action::from_str("editor:checkpoint").unwrap(),
        Action::Editor(EditorAction::Checkpoint)
    );

    // Terminal Actions
    assert_eq!(
        Action::from_str("editor:open_terminal").unwrap(),
        Action::Editor(EditorAction::OpenTerminal(None))
    );

    // Explorer Actions
    assert_eq!(
        Action::from_str("explorer:close").unwrap(),
        Action::Buffer("explorer:close".to_string())
    );
    assert_eq!(
        Action::from_str("explorer:refresh").unwrap(),
        Action::Buffer("explorer:refresh".to_string())
    );

    // Undotree Actions
    assert_eq!(
        Action::from_str("undotree:close").unwrap(),
        Action::Buffer("undotree:close".to_string())
    );
    assert_eq!(
        Action::from_str("undotree:refresh").unwrap(),
        Action::Buffer("undotree:refresh".to_string())
    );

    // Buffer-specific actions via Buffer variant
    assert_eq!(
        Action::from_str("editor:explorer_select").unwrap(),
        Action::Noop
    );
    assert_eq!(
        Action::from_str("editor:explorer_parent").unwrap(),
        Action::Noop
    );
    assert_eq!(
        Action::from_str("editor:undotree_select").unwrap(),
        Action::Noop
    );

    // History navigation
    assert_eq!(
        Action::from_str("editor:history_up").unwrap(),
        Action::Editor(EditorAction::HistoryUp)
    );
    assert_eq!(
        Action::from_str("editor:history_down").unwrap(),
        Action::Editor(EditorAction::HistoryDown)
    );

    // Unknown namespaced command becomes a Buffer action
    assert_eq!(Action::from_str("unknown:command").unwrap(), Action::Buffer("unknown:command".to_string()));
}

#[test]
fn test_all_motion_string_mappings() {
    // Every Motion variant must have a corresponding "editor:move_*" string
    let cases: &[(&str, Motion)] = &[
        ("editor:move_left", Motion::Left),
        ("editor:move_right", Motion::Right),
        ("editor:move_up", Motion::Up),
        ("editor:move_down", Motion::Down),
        ("editor:move_start_of_line", Motion::StartOfLine),
        ("editor:move_end_of_line", Motion::EndOfLine),
        ("editor:move_start_of_file", Motion::StartOfFile),
        ("editor:move_end_of_file", Motion::EndOfFile),
        ("editor:move_page_up", Motion::PageUp),
        ("editor:move_page_down", Motion::PageDown),
        ("editor:move_next_word", Motion::NextWord),
        ("editor:move_prev_word", Motion::PreviousWord),
        ("editor:move_next_big_word", Motion::NextBigWord),
        ("editor:move_prev_big_word", Motion::PreviousBigWord),
        ("editor:move_next_paragraph", Motion::NextParagraph),
        ("editor:move_prev_paragraph", Motion::PreviousParagraph),
        ("editor:move_next_sentence", Motion::NextSentence),
        ("editor:move_prev_sentence", Motion::PreviousSentence),
        ("editor:move_next_match", Motion::NextMatch),
        ("editor:move_prev_match", Motion::PreviousMatch),
    ];

    for (key, expected) in cases {
        let action = Action::from_str(key).unwrap();
        assert_eq!(
            action,
            Action::Editor(EditorAction::Move(*expected)),
            "string '{}' should map to {:?}",
            key,
            expected
        );
    }
}
