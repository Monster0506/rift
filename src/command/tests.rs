//! Tests for command system

use crate::command::{Command, Dispatcher};
use crate::key::Key;
use crate::mode::Mode;

#[test]
fn test_dispatcher_new() {
    let dispatcher = Dispatcher::new(Mode::Normal);
    assert_eq!(dispatcher.mode(), Mode::Normal);
    assert_eq!(dispatcher.pending_key(), None);
}

#[test]
fn test_translate_normal_mode_simple() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    assert_eq!(dispatcher.translate_key(Key::Char('h')), Command::MoveLeft);
    assert_eq!(dispatcher.translate_key(Key::Char('j')), Command::MoveDown);
    assert_eq!(dispatcher.translate_key(Key::Char('k')), Command::MoveUp);
    assert_eq!(dispatcher.translate_key(Key::Char('l')), Command::MoveRight);
    assert_eq!(
        dispatcher.translate_key(Key::Char('i')),
        Command::EnterInsertMode
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char('a')),
        Command::EnterInsertModeAfter
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char('x')),
        Command::DeleteForward
    );
    assert_eq!(dispatcher.translate_key(Key::Char('q')), Command::Quit);
    assert_eq!(
        dispatcher.translate_key(Key::Char(':')),
        Command::EnterCommandMode
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char('G')),
        Command::MoveToBufferEnd
    );
}

#[test]
fn test_translate_normal_mode_arrows() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    assert_eq!(dispatcher.translate_key(Key::ArrowLeft), Command::MoveLeft);
    assert_eq!(
        dispatcher.translate_key(Key::ArrowRight),
        Command::MoveRight
    );
    assert_eq!(dispatcher.translate_key(Key::ArrowUp), Command::MoveUp);
    assert_eq!(dispatcher.translate_key(Key::ArrowDown), Command::MoveDown);
    assert_eq!(
        dispatcher.translate_key(Key::Home),
        Command::MoveToLineStart
    );
    assert_eq!(dispatcher.translate_key(Key::End), Command::MoveToLineEnd);
}

#[test]
fn test_translate_insert_mode() {
    let mut dispatcher = Dispatcher::new(Mode::Insert);

    assert_eq!(
        dispatcher.translate_key(Key::Char('a')),
        Command::InsertChar('a')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(' ')),
        Command::InsertChar(' ')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char('\t')),
        Command::InsertChar('\t')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Backspace),
        Command::DeleteBackward
    );
    assert_eq!(
        dispatcher.translate_key(Key::Enter),
        Command::InsertChar('\n')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Tab),
        Command::InsertChar('\t')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Escape),
        Command::EnterInsertMode
    );
}

#[test]
fn test_translate_insert_mode_ctrl() {
    let mut dispatcher = Dispatcher::new(Mode::Insert);

    // Ctrl+A should map to \u{1}, Ctrl+C should map to \u{3}
    assert_eq!(
        dispatcher.translate_key(Key::Ctrl(b'a')),
        Command::InsertChar('\u{1}')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Ctrl(b'c')),
        Command::InsertChar('\u{3}')
    );
}

#[test]
fn test_pending_key_sequence() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    // First 'd' should set pending key
    assert_eq!(dispatcher.translate_key(Key::Char('d')), Command::Noop);
    assert_eq!(dispatcher.pending_key(), Some(Key::Char('d')));

    // Second 'd' should trigger delete_line
    assert_eq!(
        dispatcher.translate_key(Key::Char('d')),
        Command::DeleteLine
    );
    assert_eq!(dispatcher.pending_key(), None);
}

#[test]
fn test_pending_key_sequence_gg() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    // First 'g' should set pending key
    assert_eq!(dispatcher.translate_key(Key::Char('g')), Command::Noop);
    assert_eq!(dispatcher.pending_key(), Some(Key::Char('g')));

    // Second 'g' should trigger move_to_buffer_start
    assert_eq!(
        dispatcher.translate_key(Key::Char('g')),
        Command::MoveToBufferStart
    );
    assert_eq!(dispatcher.pending_key(), None);
}

#[test]
fn test_pending_key_sequence_invalid() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    // First 'd' should set pending key
    assert_eq!(dispatcher.translate_key(Key::Char('d')), Command::Noop);

    // Different key should clear pending and return noop
    assert_eq!(dispatcher.translate_key(Key::Char('x')), Command::Noop);
    assert_eq!(dispatcher.pending_key(), None);
}

#[test]
fn test_mode_switching() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);
    assert_eq!(dispatcher.mode(), Mode::Normal);

    dispatcher.set_mode(Mode::Insert);
    assert_eq!(dispatcher.mode(), Mode::Insert);

    // Pending key should be cleared when switching modes
    dispatcher.set_mode(Mode::Normal);
    dispatcher.translate_key(Key::Char('d'));
    assert_eq!(dispatcher.pending_key(), Some(Key::Char('d')));
    dispatcher.set_mode(Mode::Insert);
    // Pending key should be cleared after mode switch
    assert_eq!(dispatcher.pending_key(), None);
}

#[test]
fn test_translate_command_mode() {
    let mut dispatcher = Dispatcher::new(Mode::Command);

    // In command mode, Escape should be Noop (handled by key handler)
    assert_eq!(dispatcher.translate_key(Key::Escape), Command::Noop);

    // Printable characters should append to command line
    assert_eq!(
        dispatcher.translate_key(Key::Char('a')),
        Command::AppendToCommandLine('a')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char('q')),
        Command::AppendToCommandLine('q')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(' ')),
        Command::AppendToCommandLine(' ')
    );

    // Backspace should delete from command line
    assert_eq!(
        dispatcher.translate_key(Key::Backspace),
        Command::DeleteFromCommandLine
    );

    // Enter should execute command line
    assert_eq!(
        dispatcher.translate_key(Key::Enter),
        Command::ExecuteCommandLine
    );

    // Non-printable characters should be Noop
    assert_eq!(dispatcher.translate_key(Key::Char('\0')), Command::Noop);
}

#[test]
fn test_command_is_mutating() {
    // Mutating commands
    assert!(Command::InsertChar('a').is_mutating());
    assert!(Command::DeleteForward.is_mutating());
    assert!(Command::DeleteBackward.is_mutating());
    assert!(Command::DeleteLine.is_mutating());

    // Non-mutating commands
    assert!(!Command::MoveLeft.is_mutating());
    assert!(!Command::MoveRight.is_mutating());
    assert!(!Command::MoveUp.is_mutating());
    assert!(!Command::MoveDown.is_mutating());
    assert!(!Command::MoveToLineStart.is_mutating());
    assert!(!Command::MoveToLineEnd.is_mutating());
    assert!(!Command::MoveToBufferStart.is_mutating());
    assert!(!Command::MoveToBufferEnd.is_mutating());
    assert!(!Command::EnterInsertMode.is_mutating());
    assert!(!Command::EnterInsertModeAfter.is_mutating());
    assert!(!Command::EnterCommandMode.is_mutating());
    assert!(!Command::AppendToCommandLine('a').is_mutating());
    assert!(!Command::DeleteFromCommandLine.is_mutating());
    assert!(!Command::ExecuteCommandLine.is_mutating());
    assert!(!Command::Quit.is_mutating());
    assert!(!Command::Noop.is_mutating());
}
