use crate::action::{Action, EditorAction, Motion};
use crate::command::Command;
use crate::document::BufferKindId;
use crate::key::{Key, Mods};
use crate::keymap::{KeyContext, KeyMap};
/// Register default keybindings
pub fn register_defaults(keymap: &mut KeyMap) {
    // Terminal mode Defaults
    keymap.register(
        KeyContext::Terminal,
        Key::ctrl('\\'),
        Action::Editor(EditorAction::ExitTerminalMode),
    );
    keymap.register(
        KeyContext::Terminal,
        Key::ctrl('u'),
        Action::Editor(EditorAction::TerminalScrollback(10)),
    );
    keymap.register(
        KeyContext::Terminal,
        Key::ctrl('d'),
        Action::Editor(EditorAction::TerminalScrollback(-10)),
    );
    keymap.register(
        KeyContext::TerminalNormal,
        Key::ctrl('u'),
        Action::Editor(EditorAction::TerminalScrollback(10)),
    );
    keymap.register(
        KeyContext::TerminalNormal,
        Key::ctrl('d'),
        Action::Editor(EditorAction::TerminalScrollback(-10)),
    );

    // FileExplorer (Directory buffer) Defaults. Normal motions fall through to KeyContext::Normal
    // via the fallback chain; only directory-specific bindings are registered here.
    keymap.register(
        KeyContext::Buffer(BufferKindId::DIRECTORY),
        Key::ENTER,
        Action::Buffer("explorer:select".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::DIRECTORY),
        Key::char('-'),
        Action::Buffer("explorer:parent".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::DIRECTORY),
        Key::BACKSPACE,
        Action::Buffer("explorer:parent".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::DIRECTORY),
        Key::ESC,
        Action::Buffer("explorer:close".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::DIRECTORY),
        Key::char('H'),
        Action::Editor(EditorAction::ExplorerToggleHidden),
    );

    // GitStatus buffer Defaults;  fugitive's in-buffer vocabulary.
    // Normal motions fall through to KeyContext::Normal via the fallback chain.
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::ENTER,
        Action::Buffer("git_status:select".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::char('s'),
        Action::Buffer("git_status:stage".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::char('u'),
        Action::Buffer("git_status:unstage".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::char('-'),
        Action::Buffer("git_status:toggle_stage".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::char('X'),
        Action::Buffer("git_status:discard".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::char('='),
        Action::Buffer("git_status:toggle_expand".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::char('b'),
        Action::Buffer("git_status:blame".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        Key::char('r'),
        Action::Buffer("git_status:rebase".to_string()),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        vec![Key::char(']'), Key::char('c')],
        Action::Buffer("git_status:next_hunk".to_string()),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        vec![Key::char('['), Key::char('c')],
        Action::Buffer("git_status:prev_hunk".to_string()),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        vec![Key::char('c'), Key::char('c')],
        Action::Editor(EditorAction::GitCommitNew),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        vec![Key::char('c'), Key::char('a')],
        Action::Editor(EditorAction::GitCommitAmend),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        vec![Key::char('c'), Key::char('w')],
        Action::Editor(EditorAction::GitCommitAmend),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        vec![Key::char('c'), Key::char('f')],
        Action::Editor(EditorAction::GitCommitFixup),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_STATUS),
        vec![Key::char('g'), Key::char('?')],
        Action::Editor(EditorAction::GitHelp),
    );
    // UndoTree buffer Defaults
    // j/k are overridden to skip connector lines; <CR> selects the node.
    keymap.register(
        KeyContext::Buffer(BufferKindId::UNDO_TREE),
        Key::char('j'),
        Action::Buffer("undotree:next".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::UNDO_TREE),
        Key::char('k'),
        Action::Buffer("undotree:prev".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::UNDO_TREE),
        Key::ENTER,
        Action::Buffer("undotree:select".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::UNDO_TREE),
        Key::ESC,
        Action::Buffer("undotree:close".to_string()),
    );

    // Clipboard index buffer Defaults
    keymap.register(
        KeyContext::Buffer(BufferKindId::CLIPBOARD),
        Key::ENTER,
        Action::Buffer("clipboard:select".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::CLIPBOARD),
        Key::ESC,
        Action::Buffer("clipboard:close".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::CLIPBOARD),
        Key::char('n'),
        Action::Buffer("clipboard:new".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::CLIPBOARD),
        Key::char('r'),
        Action::Buffer("clipboard:refresh".to_string()),
    );
    // ClipboardEntry scratch buffer: Escape returns focus to index pane
    keymap.register(
        KeyContext::Buffer(BufferKindId::CLIPBOARD_ENTRY),
        Key::ESC,
        Action::Buffer("clipboard:entry:close".to_string()),
    );
    // Normal Mode Defaults
    // '-' opens the file-explorer buffer for the current file's parent directory
    keymap.register(
        KeyContext::Normal,
        Key::char('-'),
        Action::Editor(EditorAction::OpenExplorer),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('h'),
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('j'),
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('k'),
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('l'),
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('q'),
        Action::Editor(EditorAction::QuitForce),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('n'),
        Action::Editor(EditorAction::Move(Motion::RepeatFindForward)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('N'),
        Action::Editor(EditorAction::Move(Motion::RepeatFindBackward)),
    );
    // Find-char motion (f / F) and till motion (t / T)
    keymap.register(
        KeyContext::Normal,
        Key::char('f'),
        Action::Editor(EditorAction::FindCharPending {
            forward: true,
            till: false,
        }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('F'),
        Action::Editor(EditorAction::FindCharPending {
            forward: false,
            till: false,
        }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('t'),
        Action::Editor(EditorAction::FindCharPending {
            forward: true,
            till: true,
        }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('T'),
        Action::Editor(EditorAction::FindCharPending {
            forward: false,
            till: true,
        }),
    );
    // Replace (r = single char pending, R = stream replace mode)
    keymap.register(
        KeyContext::Normal,
        Key::char('r'),
        Action::Editor(EditorAction::ReplaceCharPending),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('R'),
        Action::Editor(EditorAction::EnterReplaceMode),
    );
    // Word Motion
    keymap.register(
        KeyContext::Normal,
        Key::char('w'),
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('b'),
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('e'),
        Action::Editor(EditorAction::Move(Motion::WordEnd)),
    );
    // Line Motion
    keymap.register(
        KeyContext::Normal,
        Key::char('0'),
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('$'),
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    // Paragraph Motion
    keymap.register(
        KeyContext::Normal,
        Key::char('}'),
        Action::Editor(EditorAction::Move(Motion::NextParagraph)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('{'),
        Action::Editor(EditorAction::Move(Motion::PreviousParagraph)),
    );
    // Sentence Motion
    keymap.register(
        KeyContext::Normal,
        Key::char(')'),
        Action::Editor(EditorAction::Move(Motion::NextSentence)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('('),
        Action::Editor(EditorAction::Move(Motion::PreviousSentence)),
    );
    // Page Motion
    keymap.register(
        KeyContext::Normal,
        Key::PAGE_UP,
        Action::Editor(EditorAction::Move(Motion::PageUp)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::PAGE_DOWN,
        Action::Editor(EditorAction::Move(Motion::PageDown)),
    );
    // Undo/Redo
    keymap.register(
        KeyContext::Normal,
        Key::char('u'),
        Action::Editor(EditorAction::Undo),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ctrl('r'),
        Action::Editor(EditorAction::Redo),
    );
    // Modes
    keymap.register(
        KeyContext::Normal,
        Key::char('i'),
        Action::Editor(EditorAction::EnterInsertMode),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char(':'),
        Action::Editor(EditorAction::EnterCommandMode),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('/'),
        Action::Editor(EditorAction::EnterSearchMode),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('a'),
        Action::Editor(EditorAction::EnterInsertModeAfter),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('I'),
        Action::Editor(EditorAction::EnterInsertModeAtLineStart),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('A'),
        Action::Editor(EditorAction::EnterInsertModeAtLineEnd),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('o'),
        Action::Editor(EditorAction::OpenLineBelow),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('O'),
        Action::Editor(EditorAction::OpenLineAbove),
    );
    // Normal Mode Arrows/Home/End
    keymap.register(
        KeyContext::Normal,
        Key::LEFT,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::RIGHT,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::UP,
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::DOWN,
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::HOME,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::END,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::LEFT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::RIGHT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::UP.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::PreviousParagraph)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::DOWN.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::NextParagraph)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::HOME.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::END.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::EndOfFile)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('x'),
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('.'),
        Action::Editor(EditorAction::DotRepeat),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('?'),
        Action::Editor(EditorAction::ToggleDebug),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ESC,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    keymap.register(
        KeyContext::Insert,
        Key::ESC,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    keymap.register(
        KeyContext::Insert,
        Key::LEFT,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::RIGHT,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::UP,
        Action::Editor(EditorAction::Move(Motion::Up)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::DOWN,
        Action::Editor(EditorAction::Move(Motion::Down)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::HOME,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::END,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::BACKSPACE,
        Action::Editor(EditorAction::Delete(Motion::Left)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::DELETE,
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::ENTER,
        Action::Editor(EditorAction::InsertChar('\n')),
    );
    keymap.register(
        KeyContext::Insert,
        Key::TAB,
        Action::Editor(EditorAction::InsertChar('\t')),
    );
    keymap.register(
        KeyContext::Insert,
        Key::PAGE_UP,
        Action::Editor(EditorAction::Move(Motion::PageUp)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::PAGE_DOWN,
        Action::Editor(EditorAction::Move(Motion::PageDown)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::LEFT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::RIGHT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Insert,
        Key::ctrl('w'),
        Action::Editor(EditorAction::Delete(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Command,
        Key::ESC,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    keymap.register(
        KeyContext::Search,
        Key::ESC,
        Action::Editor(EditorAction::EnterNormalMode),
    );
    // Submit
    keymap.register(
        KeyContext::Command,
        Key::ENTER,
        Action::Editor(EditorAction::Submit),
    );
    keymap.register(
        KeyContext::Search,
        Key::ENTER,
        Action::Editor(EditorAction::Submit),
    );
    // Backspace
    keymap.register(
        KeyContext::Command,
        Key::BACKSPACE,
        Action::Editor(EditorAction::Delete(Motion::Left)),
    );
    keymap.register(
        KeyContext::Search,
        Key::BACKSPACE,
        Action::Editor(EditorAction::Delete(Motion::Left)),
    );
    keymap.register(
        KeyContext::Command,
        Key::DELETE,
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Search,
        Key::DELETE,
        Action::Editor(EditorAction::Delete(Motion::Right)),
    );
    keymap.register(
        KeyContext::Command,
        Key::LEFT,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Search,
        Key::LEFT,
        Action::Editor(EditorAction::Move(Motion::Left)),
    );
    keymap.register(
        KeyContext::Command,
        Key::RIGHT,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Search,
        Key::RIGHT,
        Action::Editor(EditorAction::Move(Motion::Right)),
    );
    keymap.register(
        KeyContext::Command,
        Key::HOME,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Search,
        Key::HOME,
        Action::Editor(EditorAction::Move(Motion::StartOfLine)),
    );
    keymap.register(
        KeyContext::Command,
        Key::END,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Search,
        Key::END,
        Action::Editor(EditorAction::Move(Motion::EndOfLine)),
    );
    keymap.register(
        KeyContext::Command,
        Key::LEFT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Search,
        Key::LEFT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Command,
        Key::RIGHT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Search,
        Key::RIGHT.with(Mods::CTRL),
        Action::Editor(EditorAction::Move(Motion::NextWord)),
    );
    keymap.register(
        KeyContext::Command,
        Key::ctrl('w'),
        Action::Editor(EditorAction::Delete(Motion::PreviousWord)),
    );
    keymap.register(
        KeyContext::Search,
        Key::ctrl('w'),
        Action::Editor(EditorAction::Delete(Motion::PreviousWord)),
    );
    // History navigation
    keymap.register(
        KeyContext::Command,
        Key::UP,
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Search,
        Key::UP,
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Command,
        Key::DOWN,
        Action::Editor(EditorAction::HistoryDown),
    );
    keymap.register(
        KeyContext::Search,
        Key::DOWN,
        Action::Editor(EditorAction::HistoryDown),
    );
    keymap.register(
        KeyContext::Command,
        Key::ctrl('p'),
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Search,
        Key::ctrl('p'),
        Action::Editor(EditorAction::HistoryUp),
    );
    keymap.register(
        KeyContext::Command,
        Key::ctrl('n'),
        Action::Editor(EditorAction::HistoryDown),
    );
    keymap.register(
        KeyContext::Search,
        Key::ctrl('n'),
        Action::Editor(EditorAction::HistoryDown),
    );

    // Operators
    keymap.register(
        KeyContext::Normal,
        Key::char('d'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Delete)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('c'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Change)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('y'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Yank)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('s'),
        Action::Editor(EditorAction::SurroundStart),
    );
    keymap.register(
        KeyContext::OperatorPending,
        Key::char('g'),
        Action::Editor(EditorAction::SurroundGiveLine),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('v'),
        Action::Editor(EditorAction::EnterVisualChar),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('V'),
        Action::Editor(EditorAction::EnterVisualLine),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ctrl('v'),
        Action::Editor(EditorAction::EnterVisualBlock),
    );
    keymap.register(
        KeyContext::Visual,
        Key::char('o'),
        Action::Editor(EditorAction::VisualSwapEnds),
    );
    keymap.register(
        KeyContext::Visual,
        Key::char('O'),
        Action::Editor(EditorAction::VisualSwapEnds),
    );
    keymap.register(
        KeyContext::Visual,
        Key::char('d'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Delete)),
    );
    keymap.register(
        KeyContext::Visual,
        Key::char('c'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Change)),
    );
    keymap.register(
        KeyContext::Visual,
        Key::char('y'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Yank)),
    );
    keymap.register(
        KeyContext::Visual,
        Key::char(' '),
        Action::Editor(EditorAction::ExpandRegion),
    );
    keymap.register(
        KeyContext::Visual,
        Key::SHIFT_SPACE,
        Action::Editor(EditorAction::ShrinkRegion),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('m'),
        Action::Editor(EditorAction::RegionBankOccurrenceNext),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('M'),
        Action::Editor(EditorAction::RegionBankOccurrencePrev),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('C'),
        Action::Editor(EditorAction::Command(Box::new(Command::Change(
            Motion::EndOfLine,
            1,
        )))),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('D'),
        Action::Editor(EditorAction::Command(Box::new(Command::Delete(
            Motion::EndOfLine,
            1,
        )))),
    );

    let ww = Key::ctrl('w');
    for (ch, cmd) in [
        ('h', ":split :l"),
        ('j', ":split :d"),
        ('k', ":split :u"),
        ('l', ":split :r"),
        ('<', ":vsplit :resize -5"),
        ('>', ":vsplit :resize 5"),
        (',', ":split :resize -5"),
        ('.', ":split :resize 5"),
        ('H', ":split :H"),
        ('J', ":split :J"),
        ('K', ":split :K"),
        ('L', ":split :L"),
        ('x', ":split :x"),
        ('W', ":split :w"),
        ('=', ":split :="),
    ] {
        keymap.register_sequence(
            KeyContext::Normal,
            vec![ww, Key::char(ch)],
            Action::Editor(EditorAction::RunCommand(cmd.to_string())),
        );
    }
    keymap.register_sequence(
        KeyContext::Normal,
        vec![ww, Key::ctrl('w')],
        Action::Editor(EditorAction::RunCommand(":split :w".to_string())),
    );
    for (ch, cmd) in [
        ('h', ":split :l"),
        ('j', ":split :d"),
        ('k', ":split :u"),
        ('l', ":split :r"),
    ] {
        keymap.register_sequence(
            KeyContext::Normal,
            vec![ww, Key::ctrl(ch)],
            Action::Editor(EditorAction::RunCommand(cmd.to_string())),
        );
    }
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('g'), Key::char('g')],
        Action::Editor(EditorAction::Move(Motion::StartOfFile)),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('G'),
        Action::Editor(EditorAction::GotoLine(0)),
    );
    // <Space>pd: open the demo floating window
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(' '), Key::char('p'), Key::char('d')],
        Action::Editor(EditorAction::PluginAction("demo:window".to_string())),
    );
    // <Space>pi: insert demo text into the buffer
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(' '), Key::char('p'), Key::char('i')],
        Action::Editor(EditorAction::PluginAction("demo:insert".to_string())),
    );

    // Clipboard ring
    keymap.register(
        KeyContext::Normal,
        Key::char('p'),
        Action::Editor(EditorAction::Put { before: false }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('P'),
        Action::Editor(EditorAction::Put { before: true }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ctrl('n'),
        Action::Editor(EditorAction::CyclePaste { forward: true }),
    );
    keymap.register(
        KeyContext::Normal,
        Key::ctrl('p'),
        Action::Editor(EditorAction::CyclePaste { forward: false }),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('g'), Key::char('p')],
        Action::Editor(EditorAction::PutSystemClipboard { before: false }),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('g'), Key::char('P')],
        Action::Editor(EditorAction::PutSystemClipboard { before: true }),
    );

    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('g'), Key::char('d')],
        Action::Editor(EditorAction::LspGotoDefinition),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('g'), Key::char('r')],
        Action::Editor(EditorAction::LspReferences),
    );
    keymap.register(
        KeyContext::Normal,
        Key::char('K'),
        Action::Editor(EditorAction::LspHover),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(']'), Key::char('d')],
        Action::Editor(EditorAction::LspDiagnosticNext),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('['), Key::char('d')],
        Action::Editor(EditorAction::LspDiagnosticPrev),
    );
    // Interactive annotation navigation + activation.
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(']'), Key::char('a')],
        Action::Editor(EditorAction::NextInteractiveAnnotation),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('['), Key::char('a')],
        Action::Editor(EditorAction::PrevInteractiveAnnotation),
    );
    // Enter activates the interactive annotation under the cursor (rebindable
    // via `annotation:activate`; it is otherwise unbound in Normal mode).
    keymap.register(
        KeyContext::Normal,
        Key::ENTER,
        Action::Editor(EditorAction::ActivateAnnotation),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(' '), Key::char('f')],
        Action::Editor(EditorAction::LspFormat),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(' '), Key::char('c'), Key::char('a')],
        Action::Editor(EditorAction::LspCodeAction),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(' '), Key::char('r'), Key::char('n')],
        Action::Editor(EditorAction::LspRename),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(' '), Key::char('e')],
        Action::Editor(EditorAction::LspDiagnosticsPanel),
    );
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char(' '), Key::char('g')],
        Action::Editor(EditorAction::GitStatus),
    );

    // GitRebaseTodo buffer Defaults
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('X'),
        Action::Editor(EditorAction::GitRebaseAbort),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('K'),
        Action::Editor(EditorAction::GitRebaseMoveUp),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('J'),
        Action::Editor(EditorAction::GitRebaseMoveDown),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('p'),
        Action::Editor(EditorAction::GitRebaseSetPick),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('s'),
        Action::Editor(EditorAction::GitRebaseSetSquash),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('f'),
        Action::Editor(EditorAction::GitRebaseSetFixup),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('e'),
        Action::Editor(EditorAction::GitRebaseSetEdit),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        vec![Key::char('d'), Key::char('d')],
        Action::Editor(EditorAction::GitRebaseDrop),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::ENTER,
        Action::Editor(EditorAction::GitRebaseToggleFold),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('='),
        Action::Editor(EditorAction::GitRebaseToggleFold),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('c'),
        Action::Editor(EditorAction::GitRebaseOpenMessage),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        Key::char('r'),
        Action::Editor(EditorAction::GitRebaseOpenMessage),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_REBASE_TODO),
        vec![Key::char('g'), Key::char('?')],
        Action::Editor(EditorAction::GitHelp),
    );

    // GitBlame buffer Defaults
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_BLAME),
        Key::ENTER,
        Action::Buffer("git_blame:walk_back".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_BLAME),
        Key::BACKSPACE,
        Action::Buffer("git_blame:walk_forward".to_string()),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_BLAME),
        vec![Key::char('g'), Key::char('?')],
        Action::Editor(EditorAction::GitHelp),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_BLAME),
        Key::ESC,
        Action::Buffer("git_blame:close".to_string()),
    );

    // GitLog buffer Defaults
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_LOG),
        Key::ENTER,
        Action::Buffer("git_log:select".to_string()),
    );
    keymap.register_sequence(
        KeyContext::Buffer(BufferKindId::GIT_LOG),
        vec![Key::char('g'), Key::char('?')],
        Action::Editor(EditorAction::GitHelp),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_LOG),
        Key::char('='),
        Action::Buffer("git_log:toggle_expand".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::GIT_LOG),
        Key::char('r'),
        Action::Editor(EditorAction::GitRebaseFromLogCommit),
    );

    // Location list panel bindings (diagnostics / references)
    keymap.register(
        KeyContext::Buffer(BufferKindId::LOCATION_LIST),
        Key::ENTER,
        Action::Buffer("location_list:select".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::LOCATION_LIST),
        Key::char('q'),
        Action::Buffer("location_list:close".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::LOCATION_LIST),
        Key::char(' '),
        Action::Buffer("location_list:code_action".to_string()),
    );

    // `gv` regions list window.
    keymap.register_sequence(
        KeyContext::Normal,
        vec![Key::char('g'), Key::char('v')],
        Action::Editor(EditorAction::ToggleRegionsWindow),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::char('j'),
        Action::Editor(EditorAction::RegionsListDown),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::char('k'),
        Action::Editor(EditorAction::RegionsListUp),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::ENTER,
        Action::Editor(EditorAction::RegionsListSelect),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::char('x'),
        Action::Editor(EditorAction::RegionsListDrop),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::char('q'),
        Action::Buffer("regions:close".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::char('d'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Delete)),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::char('c'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Change)),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::REGIONS),
        Key::char('y'),
        Action::Editor(EditorAction::Operator(crate::action::OperatorType::Yank)),
    );

    // Buffer-list panel: j/k inherit Normal's motion; only selection and close
    // are bound here.
    keymap.register(
        KeyContext::Buffer(BufferKindId::BUFFER_LIST),
        Key::ENTER,
        Action::Buffer("buffer_list:select".to_string()),
    );
    keymap.register(
        KeyContext::Buffer(BufferKindId::BUFFER_LIST),
        Key::ESC,
        Action::Buffer("buffer_list:close".to_string()),
    );

    // Text objects are handled by run_loop.rs's pending-grammar state machine, not the trie.
}
