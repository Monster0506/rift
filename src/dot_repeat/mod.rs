//! Dot-repeat state: the register and recording state needed to replay the
//! last repeatable action with the `.` key.

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
    /// Selection-building actions (`v`/motion/`Esc`, `m`/`M`) that built a `SelectionSet`,
    /// plus a follow-up: `Some` re-runs that action; `None` stops with the set banked.
    RegionBuildSession {
        actions: Vec<crate::action::Action>,
        follow_up: Option<crate::action::Action>,
    },
}

/// Temporary state for recording an insert session in progress
struct InsertRecording {
    entry: Command,
    commands: Vec<Command>,
}

/// Owns all dot-repeat state: the saved register, any in-progress
/// insert recording, and the replaying flag.
pub(crate) struct DotRepeat {
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
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            register: None,
            recording: None,
            replaying: false,
        }
    }

    /// Whether we are currently replaying a dot-repeat sequence.
    #[must_use]
    pub(crate) const fn is_replaying(&self) -> bool {
        self.replaying
    }

    /// Read the current register (for replay execution in the Editor).
    pub(crate) const fn register(&self) -> Option<&DotRegister> {
        self.register.as_ref()
    }

    /// Store a single normal-mode command in the register.
    pub(crate) fn record_single(&mut self, cmd: Command) {
        self.register = Some(DotRegister::Single(cmd));
    }

    pub(crate) fn record_region_build_session(
        &mut self,
        actions: Vec<crate::action::Action>,
        follow_up: Option<crate::action::Action>,
    ) {
        self.register = Some(DotRegister::RegionBuildSession { actions, follow_up });
    }

    pub(crate) fn start_insert_recording(&mut self, entry: Command) {
        self.recording = Some(InsertRecording {
            entry,
            commands: Vec::new(),
        });
    }

    pub(crate) fn record_insert_command(&mut self, cmd: Command) {
        if let Some(ref mut rec) = self.recording {
            rec.commands.push(cmd);
        }
    }

    pub(crate) fn pop_insert_command(&mut self) {
        if let Some(rec) = &mut self.recording {
            rec.commands.pop();
        }
    }

    pub(crate) fn finish_insert_recording(&mut self) {
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
    pub(crate) const fn set_replaying(&mut self, replaying: bool) {
        self.replaying = replaying;
    }
}
