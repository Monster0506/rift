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

    // Explorer Actions
    assert_eq!(
        Action::from_str("explorer:close").unwrap(),
        Action::Explorer(FileExplorerAction::Close)
    );

    // Undotree Actions
    assert_eq!(
        Action::from_str("undotree:close").unwrap(),
        Action::UndoTree(UndoTreeAction::Close)
    );

    // Noop
    assert_eq!(Action::from_str("unknown:command").unwrap(), Action::Noop);
}
