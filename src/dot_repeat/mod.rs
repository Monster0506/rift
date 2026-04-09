//! Dot-repeat state management
//!
//! Encapsulates the register and recording state needed to replay
//! the last repeatable action with the `.` key.

use crate::command::Command;

/// Tracks the last repeatable action for dot-repeat
#[derive(Debug, Clone)]
pub(crate) enum DotRegister {
    /// A single normal-mode command (e.g. `x`, `dd`, `dw`)
    Single(Command),
    /// An insert session: entry command + all commands typed during the session
    InsertSession {
        entry: Command,
        commands: Vec<Command>,
    },
}

/// Temporary state for recording an insert session in progress
struct InsertRecording {
    entry: Command,
    commands: Vec<Command>,
}

/// Owns all dot-repeat state: the saved register, any in-progress
/// insert recording, and the replaying flag.
pub struct DotRepeat {
    register: Option<DotRegister>,
    recording: Option<InsertRecording>,
    replaying: bool,
}

impl Default for DotRepeat {
    fn default() -> Self {
        Self::new()
    }
}

impl DotRepeat {
    pub fn new() -> Self {
        Self {
            register: None,
            recording: None,
            replaying: false,
        }
    }

    /// Whether we are currently replaying a dot-repeat sequence.
    pub fn is_replaying(&self) -> bool {
        self.replaying
    }

    /// Read the current register (for replay execution in the Editor).
    pub(crate) fn register(&self) -> Option<&DotRegister> {
        self.register.as_ref()
    }

    /// Store a single normal-mode command in the register.
    pub fn record_single(&mut self, cmd: Command) {
        self.register = Some(DotRegister::Single(cmd));
    }

    /// Begin recording an insert session with the given entry command.
    pub fn start_insert_recording(&mut self, entry: Command) {
        self.recording = Some(InsertRecording {
            entry,
            commands: Vec::new(),
        });
    }

    /// Push a command into the current insert recording.
    pub fn record_insert_command(&mut self, cmd: Command) {
        if let Some(ref mut rec) = self.recording {
            rec.commands.push(cmd);
        }
    }

    /// Finalize the in-progress insert recording into the register.
    /// Only stores if there were actual commands recorded.
    pub fn finish_insert_recording(&mut self) {
        if let Some(rec) = self.recording.take() {
            if !rec.commands.is_empty() {
                self.register = Some(DotRegister::InsertSession {
                    entry: rec.entry,
                    commands: rec.commands,
                });
            }
        }
    }

    /// Toggle the replaying flag.
    pub fn set_replaying(&mut self, replaying: bool) {
        self.replaying = replaying;
    }
}
