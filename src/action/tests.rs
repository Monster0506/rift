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
