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

    assert_eq!(dispatcher.translate_key(Key::Char(b'h')), Command::MoveLeft);
    assert_eq!(dispatcher.translate_key(Key::Char(b'j')), Command::MoveDown);
    assert_eq!(dispatcher.translate_key(Key::Char(b'k')), Command::MoveUp);
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'l')),
        Command::MoveRight
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'i')),
        Command::EnterInsertMode
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'a')),
        Command::EnterInsertModeAfter
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'x')),
        Command::DeleteForward
    );
    assert_eq!(dispatcher.translate_key(Key::Char(b'q')), Command::Quit);
    assert_eq!(
        dispatcher.translate_key(Key::Char(b':')),
        Command::EnterCommandMode
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'G')),
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
        dispatcher.translate_key(Key::Char(b'a')),
        Command::InsertByte(b'a')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b' ')),
        Command::InsertByte(b' ')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'\t')),
        Command::InsertByte(b'\t')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Backspace),
        Command::DeleteBackward
    );
    assert_eq!(
        dispatcher.translate_key(Key::Enter),
        Command::InsertByte(b'\n')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Tab),
        Command::InsertByte(b'\t')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Escape),
        Command::EnterInsertMode
    );
}

#[test]
fn test_translate_insert_mode_ctrl() {
    let mut dispatcher = Dispatcher::new(Mode::Insert);

    // Ctrl+A should map to 0x01, Ctrl+C should map to 0x03
    assert_eq!(
        dispatcher.translate_key(Key::Ctrl(b'a')),
        Command::InsertByte(1)
    );
    assert_eq!(
        dispatcher.translate_key(Key::Ctrl(b'c')),
        Command::InsertByte(3)
    );
}

#[test]
fn test_pending_key_sequence() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    // First 'd' should set pending key
    assert_eq!(dispatcher.translate_key(Key::Char(b'd')), Command::Noop);
    assert_eq!(dispatcher.pending_key(), Some(Key::Char(b'd')));

    // Second 'd' should trigger delete_line
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'd')),
        Command::DeleteLine
    );
    assert_eq!(dispatcher.pending_key(), None);
}

#[test]
fn test_pending_key_sequence_gg() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    // First 'g' should set pending key
    assert_eq!(dispatcher.translate_key(Key::Char(b'g')), Command::Noop);
    assert_eq!(dispatcher.pending_key(), Some(Key::Char(b'g')));

    // Second 'g' should trigger move_to_buffer_start
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'g')),
        Command::MoveToBufferStart
    );
    assert_eq!(dispatcher.pending_key(), None);
}

#[test]
fn test_pending_key_sequence_invalid() {
    let mut dispatcher = Dispatcher::new(Mode::Normal);

    // First 'd' should set pending key
    assert_eq!(dispatcher.translate_key(Key::Char(b'd')), Command::Noop);

    // Different key should clear pending and return noop
    assert_eq!(dispatcher.translate_key(Key::Char(b'x')), Command::Noop);
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
    dispatcher.translate_key(Key::Char(b'd'));
    assert_eq!(dispatcher.pending_key(), Some(Key::Char(b'd')));
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
        dispatcher.translate_key(Key::Char(b'a')),
        Command::AppendToCommandLine(b'a')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b'q')),
        Command::AppendToCommandLine(b'q')
    );
    assert_eq!(
        dispatcher.translate_key(Key::Char(b' ')),
        Command::AppendToCommandLine(b' ')
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
    assert_eq!(dispatcher.translate_key(Key::Char(0)), Command::Noop);
    assert_eq!(dispatcher.translate_key(Key::Char(127)), Command::Noop);
}
