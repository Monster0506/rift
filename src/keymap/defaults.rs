use crate::action::{Action, EditorAction, Motion};
use crate::command::Command;
use crate::key::Key;
use crate::keymap::{KeyContext, KeyMap};

/// Register default keybindings
pub fn register_defaults(keymap: &mut KeyMap) {
    // Terminal mode Defaults
    keymap.register(
        KeyContext::Terminal,
        Key::Ctrl(92),
        Action::Editor(EditorAction::ExitTerminalMode),
    );
    keymap.register(
        KeyContext::Terminal,
        Key::Ctrl(b'u'),
        Action::Editor(EditorAction::TerminalScrollback(10)),
    );
    keymap.register(
        KeyContext::Terminal,
        Key::Ctrl(b'd'),
        Action::Editor(EditorAction::TerminalScrollback(-10)),
    );
    keymap.register(
        KeyContext::TerminalNormal,
        Key::Ctrl(b'u'),
        Action::Editor(EditorAction::TerminalScrollback(10)),
    );
    keymap.register(
        KeyContext::TerminalNormal,
        Key::Ctrl(b'd'),
        Action::Editor(EditorAction::TerminalScrollback(-10)),
    );

    // FileExplorer (Directory buffer) Defaults
    // Normal motions fall through to KeyContext::Normal via the fallback chain.
    // Only directory-specific bindings are registered here.
    keymap.register(
        KeyContext::FileExplorer,
        Key::Enter,
        Action::Buffer("explorer:select".to_string()),
    );
    keymap.register(
        KeyContext::FileExplorer,
        Key::Char('-'),
        Action::Buffer("explorer:parent".to_string()),
    );
    keymap.register(
        KeyContext::FileExplorer,
        Key::Backspace,
        Action::Buffer("explorer:parent".to_string()),
    );
    keymap.register(
        KeyContext::FileExplorer,
        Key::Escape,
        Action::Buffer("explorer:close".to_string()),
    );
    keymap.register(
        KeyContext::FileExplorer,
        Key::Char('H'),
        Action::Editor(EditorAction::ExplorerToggleHidden),
    );

    // UndoTree buffer Defaults
    // Normal motions fall through to KeyContext::Normal via the fallback chain.
    // Only <CR> is overridden to jump to the node under the cursor.
    keymap.register(
        KeyContext::UndoTree,
        Key::Enter,
        Action::Buffer("undotree:select".to_string()),
    );
    keymap.register(
        KeyContext::UndoTree,
        Key::Escape,
        Action::Buffer("undotree:close".to_string()),
    );

    // Clipboard index buffer Defaults
    keymap.register(
        KeyContext::Clipboard,
        Key::Enter,
        Action::Buffer("clipboard:select".to_string()),
    );
    keymap.register(
        KeyContext::Clipboard,
        Key::Escape,
        Action::Buffer("clipboard:close".to_string()),
    );
    keymap.register(
        KeyContext::Clipboard,
        Key::Char('n'),
        Action::Buffer("clipboard:new".to_string()),
    );
    keymap.register(
        KeyContext::Clipboard,
        Key::Char('r'),
        Action::Buffer("clipboard:refresh".to_string()),
    );

    // ClipboardEntry scratch buffer — Escape returns focus to index pane
    keymap.register(
        KeyContext::ClipboardEntry,
        Key::Escape,
        Action::Buffer("clipboard:entry:close".to_string()),
    );

    // Normal Mode Defaults
    // '-' opens the file-explorer buffer for the current file's parent directory
    keymap.register(
        KeyContext::Normal,
        Key::Char('-'),
        Action::Editor(EditorAction::OpenExplorer),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('h'),
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('k'),
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('l'),
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('q'),
        Action::Editor(EditorAction::QuitForce),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('n'),
        Action::Editor(EditorAction::Move(Motion::RepeatFindForward)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('N'),
        Action::Editor(EditorAction::Move(Motion::RepeatFindBackward)),
    );
    // Find-char motion (f / F) and till motion (t / T)
    keymap.register(
        KeyContext::Normal,
        Key::Char('f'),
        Action::Editor(EditorAction::FindCharPending {
            forward: true,
            till: false,
        }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('F'),
        Action::Editor(EditorAction::FindCharPending {
            forward: false,
            till: false,
        }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('t'),
        Action::Editor(EditorAction::FindCharPending {
            forward: true,
            till: true,
        }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('T'),
        Action::Editor(EditorAction::FindCharPending {
            forward: false,
            till: true,
        }),
    );
    // Word Motion
    keymap.register(
        KeyContext::Normal,
        Key::Char('w'),
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('b'),
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    // Line Motion
    keymap.register(
        KeyContext::Normal,
        Key::Char('0'),
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('$'),
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    // Paragraph Motion
    keymap.register(
        KeyContext::Normal,
        Key::Char('}'),
        Action::Editor(EditorAction::Move(Motion::NextParagraph)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('{'),
        Action::Editor(EditorAction::Move(Motion::PreviousParagraph)),
    );
    // Sentence Motion
    keymap.register(
        KeyContext::Normal,
        Key::Char(')'),
        Action::Editor(EditorAction::Move(Motion::NextSentence)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('('),
        Action::Editor(EditorAction::Move(Motion::PreviousSentence)),
    );
    // Page Motion
    keymap.register(
        KeyContext::Normal,
        Key::PageUp,
        Action::Editor(EditorAction::Move(Motion::PageUp)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::PageDown,
        Action::Editor(EditorAction::Move(Motion::PageDown)),
    );
    // Undo/Redo
    keymap.register(
        KeyContext::Normal,
        Key::Char('u'),
        Action::Editor(EditorAction::Undo),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Ctrl(b'r'),
        Action::Editor(EditorAction::Redo),
    );
    // Modes
    keymap.register(
        KeyContext::Normal,
        Key::Char('i'),
        Action::Editor(EditorAction::EnterInsertMode),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char(':'),
        Action::Editor(EditorAction::EnterCommandMode),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('/'),
        Action::Editor(EditorAction::EnterSearchMode),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('a'),
        Action::Editor(EditorAction::EnterInsertModeAfter),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('I'),
        Action::Editor(EditorAction::EnterInsertModeAtLineStart),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('A'),
        Action::Editor(EditorAction::EnterInsertModeAtLineEnd),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('o'),
        Action::Editor(EditorAction::OpenLineBelow),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('O'),
        Action::Editor(EditorAction::OpenLineAbove),
    );
    // Normal Mode Arrows/Home/End
    keymap.register(
        KeyContext::Normal,
        Key::ArrowLeft,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ArrowRight,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ArrowUp,
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ArrowDown,
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Home,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::End,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::CtrlArrowLeft,
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::CtrlArrowRight,
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::CtrlArrowUp,
        Action::Editor(EditorAction::Move(Motion::PreviousParagraph)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::CtrlArrowDown,
        Action::Editor(EditorAction::Move(Motion::NextParagraph)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::CtrlHome,
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::CtrlEnd,
        Action::Editor(EditorAction::Move(Motion::EndOfFile)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('x'),
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('.'),
        Action::Editor(EditorAction::DotRepeat),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('?'),
        Action::Editor(EditorAction::ToggleDebug),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Escape,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    keymap.register(
        KeyContext::Insert,
        Key::Escape,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    keymap.register(
        KeyContext::Insert,
        Key::ArrowLeft,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::ArrowRight,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::ArrowUp,
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::ArrowDown,
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::Home,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::End,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::Backspace,
        Action::Editor(EditorAction::Delete(Motion::Left)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::Delete,
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::Enter,
        Action::Editor(EditorAction::InsertChar('\n')),
    );
    keymap.register(
        KeyContext::Insert,
        Key::Tab,
        Action::Editor(EditorAction::InsertChar('\t')),
    );
    keymap.register(
        KeyContext::Insert,
        Key::PageUp,
        Action::Editor(EditorAction::Move(Motion::PageUp)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::PageDown,
        Action::Editor(EditorAction::Move(Motion::PageDown)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::CtrlArrowLeft,
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::CtrlArrowRight,
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::Ctrl(b'w'),
        Action::Editor(EditorAction::Delete(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Command,
        Key::Escape,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    keymap.register(
        KeyContext::Search,
        Key::Escape,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    // Submit
    keymap.register(
        KeyContext::Command,
        Key::Enter,
        Action::Editor(EditorAction::Submit),
    );
    keymap.register(
        KeyContext::Search,
        Key::Enter,
        Action::Editor(EditorAction::Submit),
    );
    // Backspace
    keymap.register(
        KeyContext::Command,
        Key::Backspace,
        Action::Editor(EditorAction::Delete(Motion::Left)),
    );
    keymap.register(
        KeyContext::Search,
        Key::Backspace,
        Action::Editor(EditorAction::Delete(Motion::Left)),
    );
    keymap.register(
        KeyContext::Command,
        Key::Delete,
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Search,
        Key::Delete,
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Command,
        Key::ArrowLeft,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Search,
        Key::ArrowLeft,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Command,
        Key::ArrowRight,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Search,
        Key::ArrowRight,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Command,
        Key::Home,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Search,
        Key::Home,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Command,
        Key::End,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Search,
        Key::End,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Command,
        Key::CtrlArrowLeft,
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Search,
        Key::CtrlArrowLeft,
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Command,
        Key::CtrlArrowRight,
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Search,
        Key::CtrlArrowRight,
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Command,
        Key::Ctrl(b'w'),
        Action::Editor(EditorAction::Delete(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Search,
        Key::Ctrl(b'w'),
        Action::Editor(EditorAction::Delete(Motion::PreviousWord)),
    );
    // History navigation
    keymap.register(
        KeyContext::Command,
        Key::ArrowUp,
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Search,
        Key::ArrowUp,
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Command,
        Key::ArrowDown,
        Action::Editor(EditorAction::HistoryDown),
    );
    keymap.register(
        KeyContext::Search,
        Key::ArrowDown,
        Action::Editor(EditorAction::HistoryDown),
    );
    keymap.register(
        KeyContext::Command,
        Key::Ctrl(b'p'),
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Search,
        Key::Ctrl(b'p'),
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Command,
        Key::Ctrl(b'n'),
        Action::Editor(EditorAction::HistoryDown),
    );
    keymap.register(
        KeyContext::Search,
        Key::Ctrl(b'n'),
        Action::Editor(EditorAction::HistoryDown),
    );

    // Operators
    keymap.register(
        KeyContext::Normal,
        Key::Char('d'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Delete)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('c'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Change)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('y'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Yank)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('C'),
        Action::Editor(EditorAction::Command(Box::new(Command::Change(
            Motion::EndOfLine,
            1,
        )))),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('D'),
        Action::Editor(EditorAction::Command(Box::new(Command::Delete(
            Motion::EndOfLine,
            1,
        )))),
    );

    // Sequences
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char('d'), Key::Char('d')],
        Action::Editor(EditorAction::DeleteLine),
    );

    let ww = Key::Ctrl(b'w');
    for (ch, cmd) in [
        ('h', ":split :l"),
        ('j', ":split :d"),
        ('k', ":split :u"),
        ('l', ":split :r"),
        ('<', ":vsplit :-5"),
        ('>', ":vsplit :+5"),
    ] {
        keymap.register_sequence(
            KeyContext::Normal,
            vec![ww, Key::Char(ch)],
            Action::Editor(EditorAction::RunCommand(cmd.to_string())),
        );
    }
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char('g'), Key::Char('g')],
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('G'),
        Action::Editor(EditorAction::GotoLine(0)),
    );
    // <Space>pd — open the demo floating window
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char(' '), Key::Char('p'), Key::Char('d')],
        Action::Editor(EditorAction::PluginAction("demo:window".to_string())),
    );
    // <Space>pi — insert demo text into the buffer
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char(' '), Key::Char('p'), Key::Char('i')],
        Action::Editor(EditorAction::PluginAction("demo:insert".to_string())),
    );

    // Clipboard ring
    keymap.register(
        KeyContext::Normal,
        Key::Char('p'),
        Action::Editor(EditorAction::Put { before: false }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Char('P'),
        Action::Editor(EditorAction::Put { before: true }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Ctrl(b'n'),
        Action::Editor(EditorAction::CyclePaste { forward: true }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::Ctrl(b'p'),
        Action::Editor(EditorAction::CyclePaste { forward: false }),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char('g'), Key::Char('p')],
        Action::Editor(EditorAction::PutSystemClipboard { before: false }),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::Char('g'), Key::Char('P')],
        Action::Editor(EditorAction::PutSystemClipboard { before: true }),
    );
}
