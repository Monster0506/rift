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
fn test_action_from_str_editor_only() {
    // Actions that remain under editor: because they have no domain namespace
    assert_eq!(
        Action::from_str("editor:delete_line").unwrap(),
        Action::Editor(EditorAction::DeleteLine)
    );
    assert_eq!(
        Action::from_str("editor:delete_char").unwrap(),
        Action::Editor(EditorAction::Delete(Motion::Right))
    );
    assert_eq!(
        Action::from_str("editor:delete_back").unwrap(),
        Action::Editor(EditorAction::Delete(Motion::Left))
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
        Action::from_str("editor:quit").unwrap(),
        Action::Editor(EditorAction::Quit)
    );
    assert_eq!(
        Action::from_str("editor:submit").unwrap(),
        Action::Editor(EditorAction::Submit)
    );
    assert_eq!(
        Action::from_str("editor:history_up").unwrap(),
        Action::Editor(EditorAction::HistoryUp)
    );
    assert_eq!(
        Action::from_str("editor:history_down").unwrap(),
        Action::Editor(EditorAction::HistoryDown)
    );
    assert_eq!(
        Action::from_str("editor:dot_repeat").unwrap(),
        Action::Editor(EditorAction::DotRepeat)
    );
}

#[test]
fn test_action_from_str_namespaced() {
    // Feature openers
    assert_eq!(
        Action::from_str("explorer:open").unwrap(),
        Action::Editor(EditorAction::OpenExplorer)
    );
    assert_eq!(
        Action::from_str("undotree:open").unwrap(),
        Action::Editor(EditorAction::OpenUndoTree)
    );
    assert_eq!(
        Action::from_str("terminal:open").unwrap(),
        Action::Editor(EditorAction::OpenTerminal(None))
    );
    assert_eq!(
        Action::from_str("messages:open").unwrap(),
        Action::Editor(EditorAction::OpenMessages)
    );

    // Mode transitions
    assert_eq!(
        Action::from_str("mode:normal").unwrap(),
        Action::Editor(EditorAction::EnterNormalMode)
    );
    assert_eq!(
        Action::from_str("mode:insert").unwrap(),
        Action::Editor(EditorAction::EnterInsertMode)
    );
    assert_eq!(
        Action::from_str("mode:insert_after").unwrap(),
        Action::Editor(EditorAction::EnterInsertModeAfter)
    );
    assert_eq!(
        Action::from_str("mode:insert_line_start").unwrap(),
        Action::Editor(EditorAction::EnterInsertModeAtLineStart)
    );
    assert_eq!(
        Action::from_str("mode:insert_line_end").unwrap(),
        Action::Editor(EditorAction::EnterInsertModeAtLineEnd)
    );
    assert_eq!(
        Action::from_str("mode:command").unwrap(),
        Action::Editor(EditorAction::EnterCommandMode)
    );
    assert_eq!(
        Action::from_str("mode:search").unwrap(),
        Action::Editor(EditorAction::EnterSearchMode)
    );

    // Buffer management
    assert_eq!(
        Action::from_str("buffer:next").unwrap(),
        Action::Editor(EditorAction::BufferNext)
    );
    assert_eq!(
        Action::from_str("buffer:prev").unwrap(),
        Action::Editor(EditorAction::BufferPrevious)
    );
    assert_eq!(
        Action::from_str("buffer:list").unwrap(),
        Action::Editor(EditorAction::ShowBufferList)
    );

    // Search
    assert_eq!(
        Action::from_str("search:clear").unwrap(),
        Action::Editor(EditorAction::ClearHighlights)
    );

    // Notifications
    assert_eq!(
        Action::from_str("notifications:clear").unwrap(),
        Action::Editor(EditorAction::ClearNotifications)
    );
    assert_eq!(
        Action::from_str("notifications:clear_last").unwrap(),
        Action::Editor(EditorAction::ClearLastNotification)
    );

    // History
    assert_eq!(
        Action::from_str("history:undo").unwrap(),
        Action::Editor(EditorAction::Undo)
    );
    assert_eq!(
        Action::from_str("history:redo").unwrap(),
        Action::Editor(EditorAction::Redo)
    );
    assert_eq!(
        Action::from_str("history:checkpoint").unwrap(),
        Action::Editor(EditorAction::Checkpoint)
    );
}

#[test]
fn test_action_from_str_buffer_forwarding() {
    // Buffer-kind-specific actions forward through Buffer variant
    assert_eq!(
        Action::from_str("explorer:close").unwrap(),
        Action::Buffer("explorer:close".to_string())
    );
    assert_eq!(
        Action::from_str("explorer:refresh").unwrap(),
        Action::Buffer("explorer:refresh".to_string())
    );
    assert_eq!(
        Action::from_str("undotree:close").unwrap(),
        Action::Buffer("undotree:close".to_string())
    );
    assert_eq!(
        Action::from_str("messages:refresh").unwrap(),
        Action::Buffer("messages:refresh".to_string())
    );
    // editor:X:Y forwards to Buffer("X:Y")
    assert_eq!(
        Action::from_str("editor:messages:refresh").unwrap(),
        Action::Buffer("messages:refresh".to_string())
    );
    assert_eq!(
        Action::from_str("editor:messages:open").unwrap(),
        Action::Buffer("messages:open".to_string())
    );
    // Unknown namespaced key becomes Buffer action
    assert_eq!(
        Action::from_str("unknown:command").unwrap(),
        Action::Buffer("unknown:command".to_string())
    );
}

#[test]
fn test_action_from_str_noop() {
    assert_eq!(Action::from_str("totally_unknown").unwrap(), Action::Noop);
    assert_eq!(
        Action::from_str("editor:unknown_flat").unwrap(),
        Action::Noop
    );
}

#[test]
fn test_action_from_str_explorer_toggle_hidden() {
    assert_eq!(
        Action::from_str("explorer:toggle_hidden").unwrap(),
        Action::Editor(EditorAction::ExplorerToggleHidden)
    );
}

#[test]
fn test_hierarchical_motion_string_mappings() {
    // New preferred hierarchical movement namespacing
    let cases: &[(&str, Motion)] = &[
        ("editor:move:left", Motion::Left),
        ("editor:move:right", Motion::Right),
        ("editor:move:up", Motion::Up),
        ("editor:move:down", Motion::Down),
        ("editor:move:line:start", Motion::StartOfLine),
        ("editor:move:line:end", Motion::EndOfLine),
        ("editor:move:file:start", Motion::StartOfFile),
        ("editor:move:file:end", Motion::EndOfFile),
        ("editor:move:page:up", Motion::PageUp),
        ("editor:move:page:down", Motion::PageDown),
        ("editor:move:word:next", Motion::NextWord),
        ("editor:move:word:prev", Motion::PreviousWord),
        ("editor:move:bigword:next", Motion::NextBigWord),
        ("editor:move:bigword:prev", Motion::PreviousBigWord),
        ("editor:move:paragraph:next", Motion::NextParagraph),
        ("editor:move:paragraph:prev", Motion::PreviousParagraph),
        ("editor:move:sentence:next", Motion::NextSentence),
        ("editor:move:sentence:prev", Motion::PreviousSentence),
        ("editor:move:match:next", Motion::NextMatch),
        ("editor:move:match:prev", Motion::PreviousMatch),
    ];

    for (key, expected) in cases {
        let action = Action::from_str(key).unwrap();
        assert_eq!(
            action,
            Action::Editor(EditorAction::Move(*expected)),
            "hierarchical string '{}' should map to {:?}",
            key,
            expected
        );
    }
}
