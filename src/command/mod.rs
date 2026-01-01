//! Command dispatch and keybindings
//! Translates keys into editor commands based on current mode

/// ## command/ Invariants
///
/// - `Command` represents editor-level intent, not key-level input.
/// - Commands contain no terminal- or platform-specific concepts.
/// - All data required to apply a command is contained within the command.
/// - Commands are immutable once created.
/// - Adding a new command requires explicit executor support.
use crate::action::Motion;
use crate::key::Key;
use crate::mode::Mode;

pub mod input;

/// Editor commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    // Movement
    Move(Motion, usize),

    // Editing
    EnterInsertMode,
    EnterInsertModeAfter,
    Delete(Motion, usize),
    DeleteForward,
    DeleteBackward,
    DeleteLine,
    InsertChar(char),

    // Mode transitions
    EnterCommandMode,
    EnterSearchMode,

    // Search
    ExecuteSearch,
    NextMatch,
    PreviousMatch,

    // Command line editing
    AppendToCommandLine(char),
    DeleteFromCommandLine,
    ExecuteCommandLine,

    // Buffer/Tab management
    BufferNext,
    BufferPrevious,

    // Control
    Quit,
    Noop,
}

impl Command {
    /// Check if command mutates the buffer content
    #[must_use]
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Command::DeleteForward
                | Command::DeleteBackward
                | Command::DeleteLine
                | Command::Delete(_, _)
                | Command::InsertChar(_)
        )
    }
}

/// Command dispatcher state
pub struct Dispatcher {
    mode: Mode,
    pending_key: Option<(Key, usize)>,
    pending_count: usize,
}

impl Dispatcher {
    #[must_use]
    pub fn new(mode: Mode) -> Self {
        Dispatcher {
            mode,
            pending_key: None,
            pending_count: 0,
        }
    }

    fn key_to_motion(key: Key) -> Option<Motion> {
        match key {
            Key::Char('h') | Key::ArrowLeft => Some(Motion::Left),
            Key::Char('j') | Key::ArrowDown => Some(Motion::Down),
            Key::Char('k') | Key::ArrowUp => Some(Motion::Up),
            Key::Char('l') | Key::ArrowRight => Some(Motion::Right),
            Key::Char('w') | Key::CtrlArrowRight => Some(Motion::NextWord),
            Key::Char('b') | Key::CtrlArrowLeft => Some(Motion::PreviousWord),
            Key::Char('}') | Key::CtrlArrowDown => Some(Motion::NextParagraph),
            Key::Char('{') | Key::CtrlArrowUp => Some(Motion::PreviousParagraph),
            Key::Char(')') => Some(Motion::NextSentence),
            Key::Char('(') => Some(Motion::PreviousSentence),
            Key::Char('0') | Key::Home => Some(Motion::StartOfLine),
            Key::Char('$') | Key::End => Some(Motion::EndOfLine),
            Key::Char('G') | Key::CtrlEnd => Some(Motion::EndOfFile),
            Key::Char('n') => Some(Motion::NextMatch),
            Key::Char('N') => Some(Motion::PreviousMatch),
            Key::CtrlHome => Some(Motion::StartOfFile),
            Key::PageUp => Some(Motion::PageUp),
            Key::PageDown => Some(Motion::PageDown),
            _ => None,
        }
    }

    /// Translate a key into a command based on current mode
    pub fn translate_key(&mut self, key: Key) -> Command {
        match self.mode {
            Mode::Normal => self.translate_normal_mode(key),
            Mode::Insert => self.translate_insert_mode(key),
            Mode::Command => self.translate_command_mode(key),
            Mode::Search => self.translate_search_mode(key),
        }
    }

    fn translate_normal_mode(&mut self, key: Key) -> Command {
        // Handle digits for count
        if let Key::Char(ch) = key {
            if ch.is_ascii_digit() && (ch != '0' || self.pending_count > 0) {
                let digit = ch.to_digit(10).unwrap() as usize;
                self.pending_count = self.pending_count.saturating_mul(10).saturating_add(digit);
                return Command::Noop;
            }
        }

        let count = if self.pending_count == 0 {
            1
        } else {
            self.pending_count
        };

        // Handle multi-key sequences
        if let Some((pending, pending_count)) = self.pending_key.take() {
            self.pending_count = 0;
            return self.handle_normal_mode_sequence(pending, pending_count, key, count);
        }

        self.pending_count = 0;

        match key {
            Key::Char(ch) => match ch {
                'h' => Command::Move(Motion::Left, count),
                'j' => Command::Move(Motion::Down, count),
                'k' => Command::Move(Motion::Up, count),
                'l' => Command::Move(Motion::Right, count),
                '0' => Command::Move(Motion::StartOfLine, 1),
                '$' => Command::Move(Motion::EndOfLine, count),
                'i' => Command::EnterInsertMode,
                'a' => Command::EnterInsertModeAfter,
                'w' => Command::Move(Motion::NextWord, count),
                'b' => Command::Move(Motion::PreviousWord, count),
                '}' => Command::Move(Motion::NextParagraph, count),
                '{' => Command::Move(Motion::PreviousParagraph, count),
                ')' => Command::Move(Motion::NextSentence, count),
                '(' => Command::Move(Motion::PreviousSentence, count),
                'x' => Command::DeleteForward,
                'q' => Command::Quit,
                ':' => Command::EnterCommandMode,
                '/' => Command::EnterSearchMode,
                'n' => Command::Move(Motion::NextMatch, count),
                'N' => Command::Move(Motion::PreviousMatch, count),
                'd' => {
                    // Start sequence for 'dd' or 'd<motion>'
                    self.pending_key = Some((key, count));
                    Command::Noop
                }
                'g' => {
                    // Start sequence for 'gg'
                    self.pending_key = Some((key, count));
                    Command::Noop
                }
                'G' => Command::Move(Motion::EndOfFile, count),
                _ => Command::Noop,
            },
            Key::ArrowLeft => Command::Move(Motion::Left, count),
            Key::ArrowRight => Command::Move(Motion::Right, count),
            Key::ArrowUp => Command::Move(Motion::Up, count),
            Key::ArrowDown => Command::Move(Motion::Down, count),
            Key::Home => Command::Move(Motion::StartOfLine, 1),
            Key::End => Command::Move(Motion::EndOfLine, count),
            Key::PageUp => Command::Move(Motion::PageUp, count),
            Key::PageDown => Command::Move(Motion::PageDown, count),
            Key::CtrlArrowLeft => Command::Move(Motion::PreviousWord, count),
            Key::CtrlArrowRight => Command::Move(Motion::NextWord, count),
            Key::CtrlArrowUp => Command::Move(Motion::PreviousParagraph, count),
            Key::CtrlArrowDown => Command::Move(Motion::NextParagraph, count),
            Key::CtrlHome => Command::Move(Motion::StartOfFile, 1),
            Key::CtrlEnd => Command::Move(Motion::EndOfFile, 1),
            _ => Command::Noop,
        }
    }

    fn handle_normal_mode_sequence(
        &mut self,
        first: Key,
        first_count: usize,
        second: Key,
        second_count: usize,
    ) -> Command {
        let total_count = first_count * second_count;
        match (first, second) {
            (Key::Char('d'), Key::Char('d')) => Command::DeleteLine,
            (Key::Char('d'), key) => {
                if let Some(motion) = Self::key_to_motion(key) {
                    Command::Delete(motion, total_count)
                } else {
                    Command::Noop
                }
            }
            (Key::Char('g'), Key::Char('g')) => Command::Move(Motion::StartOfFile, 1),
            _ => Command::Noop,
        }
    }

    fn translate_insert_mode(&self, key: Key) -> Command {
        use self::input::{Direction, Granularity, InputIntent};

        // Resolve shared input intent
        if let Some(intent) = input::resolve_input(key) {
            match intent {
                InputIntent::Type(ch) => Command::InsertChar(ch),
                InputIntent::Move(dir, granularity) => match (dir, granularity) {
                    (Direction::Left, Granularity::Character) => Command::Move(Motion::Left, 1),
                    (Direction::Right, Granularity::Character) => Command::Move(Motion::Right, 1),
                    (Direction::Up, Granularity::Character) => Command::Move(Motion::Up, 1),
                    (Direction::Down, Granularity::Character) => Command::Move(Motion::Down, 1),

                    (Direction::Left, Granularity::Word) => Command::Move(Motion::PreviousWord, 1),
                    (Direction::Right, Granularity::Word) => Command::Move(Motion::NextWord, 1),
                    (Direction::Up, Granularity::Word) => {
                        Command::Move(Motion::PreviousParagraph, 1)
                    }
                    (Direction::Down, Granularity::Word) => Command::Move(Motion::NextParagraph, 1),

                    (Direction::Left, Granularity::Line) => Command::Move(Motion::StartOfLine, 1),
                    (Direction::Right, Granularity::Line) => Command::Move(Motion::EndOfLine, 1),

                    (Direction::Left, Granularity::Sentence) => {
                        Command::Move(Motion::PreviousSentence, 1)
                    }
                    (Direction::Right, Granularity::Sentence) => {
                        Command::Move(Motion::NextSentence, 1)
                    }

                    (Direction::Up, Granularity::Paragraph) => {
                        Command::Move(Motion::PreviousParagraph, 1)
                    }
                    (Direction::Down, Granularity::Paragraph) => {
                        Command::Move(Motion::NextParagraph, 1)
                    }

                    (Direction::Up, Granularity::Page) => Command::Move(Motion::PageUp, 1),
                    (Direction::Down, Granularity::Page) => Command::Move(Motion::PageDown, 1),

                    (Direction::Left, Granularity::Document) => {
                        Command::Move(Motion::StartOfFile, 1)
                    }
                    (Direction::Right, Granularity::Document) => {
                        Command::Move(Motion::EndOfFile, 1)
                    }

                    // Fallbacks
                    (Direction::Left, _) => Command::Move(Motion::Left, 1),
                    (Direction::Right, _) => Command::Move(Motion::Right, 1),
                    (Direction::Up, _) => Command::Move(Motion::Up, 1),
                    (Direction::Down, _) => Command::Move(Motion::Down, 1),
                },
                InputIntent::Delete(Direction::Left, _) => Command::DeleteBackward, // Backspace
                InputIntent::Delete(Direction::Right, _) => Command::DeleteForward, // Delete
                InputIntent::Delete(_, _) => Command::Noop, // Other deletes not supported yet
                InputIntent::Accept => Command::InsertChar('\n'),
                InputIntent::Cancel => Command::EnterInsertMode, // Toggle back to normal
            }
        } else {
            Command::Noop
        }
    }

    fn translate_command_mode(&self, key: Key) -> Command {
        use self::input::{Direction, Granularity, InputIntent};

        if let Some(intent) = input::resolve_input(key) {
            match intent {
                InputIntent::Type(ch) => Command::AppendToCommandLine(ch),
                InputIntent::Move(dir, Granularity::Line) => match dir {
                    Direction::Left => Command::Move(Motion::StartOfLine, 1),
                    Direction::Right => Command::Move(Motion::EndOfLine, 1),
                    _ => Command::Noop,
                },
                InputIntent::Move(dir, Granularity::Word) => {
                    // TODO: Implement word-wise movement for command line
                    // For now, fall back to character movement
                    match dir {
                        Direction::Left => Command::Move(Motion::Left, 1),
                        Direction::Right => Command::Move(Motion::Right, 1),
                        Direction::Up => Command::Move(Motion::Up, 1),
                        Direction::Down => Command::Move(Motion::Down, 1),
                    }
                }
                InputIntent::Move(dir, _) => match dir {
                    Direction::Left => Command::Move(Motion::Left, 1),
                    Direction::Right => Command::Move(Motion::Right, 1),
                    Direction::Up => Command::Move(Motion::Up, 1),
                    Direction::Down => Command::Move(Motion::Down, 1),
                },
                InputIntent::Delete(Direction::Left, _) => Command::DeleteFromCommandLine, // Backspace
                InputIntent::Delete(Direction::Right, _) => Command::DeleteForward, // Forward delete
                InputIntent::Delete(_, _) => Command::Noop,
                InputIntent::Accept => Command::ExecuteCommandLine,
                InputIntent::Cancel => Command::Noop, // Handled by KeyHandler
            }
        } else {
            Command::Noop
        }
    }

    fn translate_search_mode(&self, key: Key) -> Command {
        use self::input::{Direction, Granularity, InputIntent};

        if let Some(intent) = input::resolve_input(key) {
            match intent {
                InputIntent::Type(ch) => Command::AppendToCommandLine(ch),
                InputIntent::Move(dir, Granularity::Line) => match dir {
                    Direction::Left => Command::Move(Motion::StartOfLine, 1),
                    Direction::Right => Command::Move(Motion::EndOfLine, 1),
                    _ => Command::Noop,
                },
                InputIntent::Move(dir, _) => match dir {
                    Direction::Left => Command::Move(Motion::Left, 1),
                    Direction::Right => Command::Move(Motion::Right, 1),
                    _ => Command::Noop,
                },
                InputIntent::Delete(Direction::Left, _) => Command::DeleteFromCommandLine,
                InputIntent::Delete(Direction::Right, _) => Command::DeleteForward,
                InputIntent::Delete(_, _) => Command::Noop,
                InputIntent::Accept => Command::ExecuteSearch,
                InputIntent::Cancel => Command::Noop, // Handled by KeyHandler
            }
        } else {
            Command::Noop
        }
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        // Clear pending key when switching modes
        self.pending_key = None;
        self.pending_count = 0;
    }

    #[must_use]
    pub fn pending_key(&self) -> Option<Key> {
        self.pending_key.map(|(k, _)| k)
    }

    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending_count
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
