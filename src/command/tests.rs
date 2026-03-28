use crate::action::Motion;
use crate::command::Command;

#[test]
fn test_command_is_mutating() {
    // Mutating commands
    assert!(Command::InsertChar('a').is_mutating());
    assert!(Command::DeleteForward.is_mutating());
    assert!(Command::DeleteBackward.is_mutating());
    assert!(Command::DeleteLine.is_mutating());

    // Non-mutating commands
    assert!(!Command::Move(Motion::Left, 1).is_mutating());
    assert!(!Command::Move(Motion::Right, 1).is_mutating());
    assert!(!Command::Move(Motion::Up, 1).is_mutating());
    assert!(!Command::Move(Motion::Down, 1).is_mutating());
    assert!(!Command::Move(Motion::StartOfLine, 1).is_mutating());
    assert!(!Command::Move(Motion::EndOfLine, 1).is_mutating());
    assert!(!Command::Move(Motion::StartOfFile, 1).is_mutating());
    assert!(!Command::Move(Motion::EndOfFile, 1).is_mutating());
    assert!(!Command::EnterInsertMode.is_mutating());
    assert!(!Command::EnterInsertModeAfter.is_mutating());
    assert!(!Command::EnterCommandMode.is_mutating());
    assert!(!Command::AppendToCommandLine('a').is_mutating());
    assert!(!Command::DeleteFromCommandLine.is_mutating());
    assert!(!Command::ExecuteCommandLine.is_mutating());
    assert!(!Command::Quit.is_mutating());
    assert!(!Command::Noop.is_mutating());
}
