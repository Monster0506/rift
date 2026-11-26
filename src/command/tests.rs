use crate::action::Motion;
use crate::command::Command;

#[test]
fn test_command_is_mutating() {
    // Mutating commands
    assert!(Command::TypeChar('a').is_mutating());
    assert!(Command::DeleteRows(1).is_mutating());

    // Non-mutating commands
    assert!(!Command::MoveBy(Motion::Left, 1).is_mutating());
    assert!(!Command::MoveBy(Motion::Right, 1).is_mutating());
    assert!(!Command::MoveBy(Motion::Up, 1).is_mutating());
    assert!(!Command::MoveBy(Motion::Down, 1).is_mutating());
    assert!(!Command::MoveBy(Motion::StartOfLine, 1).is_mutating());
    assert!(!Command::MoveBy(Motion::EndOfLine, 1).is_mutating());
    assert!(!Command::MoveBy(Motion::StartOfFile, 1).is_mutating());
    assert!(!Command::MoveBy(Motion::EndOfFile, 1).is_mutating());
    assert!(!Command::BeginInsert(crate::command::InsertEntry::AtCursor).is_mutating());
    assert!(!Command::BeginInsert(crate::command::InsertEntry::AfterCursor).is_mutating());
    assert!(!Command::ReadExLine.is_mutating());
    assert!(!Command::AppendToCommandLine('a').is_mutating());
    assert!(!Command::DeleteFromCommandLine.is_mutating());
    assert!(!Command::ExecuteCommandLine.is_mutating());
    assert!(!Command::Noop.is_mutating());
}
