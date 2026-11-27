use super::super::Editor;
use crate::command_line::settings::resolver::OptionScope;
use crate::error::{ErrorType, RiftError};
use crate::notification::NotificationType;
use crate::term::TerminalBackend;
use std::path::PathBuf;

impl<T: TerminalBackend> Editor<T> {
    fn report_ex_error(&mut self, error: RiftError) -> bool {
        self.notify_error(error);
        false
    }

    pub(in crate::editor) fn reject_ex_argument(&mut self, message: &'static str) -> bool {
        self.report_ex_error(RiftError::new(
            ErrorType::Execution,
            "INVALID_ARGUMENT",
            message,
        ))
    }

    pub(in crate::editor) fn reject_ambiguous_ex_command(
        &mut self,
        prefix: &str,
        matches: &[String],
    ) -> bool {
        self.report_ex_error(RiftError::new(
            ErrorType::Parse,
            "AMBIGUOUS_COMMAND",
            format!(
                "Ambiguous command '{prefix}': matches {}",
                matches.join(", ")
            ),
        ))
    }

    pub(in crate::editor) fn apply_ex_option(
        &mut self,
        name: &str,
        value: Option<&str>,
        scope: OptionScope,
    ) -> bool {
        if let Err(error) = self.set_option(name, value, scope) {
            self.notify_error(error);
            return false;
        }
        true
    }

    pub(in crate::editor) fn notify_from_command(
        &mut self,
        kind: &str,
        message: String,
        clear_all: bool,
    ) -> bool {
        if kind.eq_ignore_ascii_case("clear") {
            self.do_notification_clear(clear_all);
            return true;
        }
        let kind = if kind.eq_ignore_ascii_case("info") {
            NotificationType::Info
        } else if kind.eq_ignore_ascii_case("warning") || kind.eq_ignore_ascii_case("warn") {
            NotificationType::Warning
        } else if kind.eq_ignore_ascii_case("error") {
            NotificationType::Error
        } else if kind.eq_ignore_ascii_case("success") {
            NotificationType::Success
        } else {
            self.notify_error(RiftError::new(
                ErrorType::Execution,
                "INVALID_NOTIFY_TYPE",
                format!("Unknown notification type: {kind}"),
            ));
            return false;
        };
        self.notify(kind, message);
        true
    }

    pub(in crate::editor) fn open_explorer_from_command(&mut self, path: Option<String>) -> bool {
        let dir = if let Some(path) = path {
            let path = PathBuf::from(path);
            if path.is_file() {
                self.notify_error(RiftError::new(
                    ErrorType::Execution,
                    crate::constants::errors::NOT_A_DIRECTORY,
                    crate::constants::errors::MSG_NOT_A_DIRECTORY,
                ));
                return false;
            }
            path
        } else if let Some(path) = self.active_document().path() {
            if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent().unwrap_or(path).to_path_buf()
            }
        } else {
            std::env::current_dir().unwrap_or_default()
        };
        self.open_explorer(dir);
        true
    }

    pub(in crate::editor) fn run_git_from_command(&mut self, args: String) {
        self.run_git_command(args);
        self.state.clear_command_line();
    }

    pub(in crate::editor) fn redraw_from_command(&mut self) {
        self.state.clear_command_line();
        if let Err(error) = self.force_full_redraw() {
            self.notify_error(error);
        }
    }

    fn reject_unknown_ex_command(&mut self, name: &str) -> bool {
        self.report_ex_error(RiftError::new(
            ErrorType::Parse,
            "UNKNOWN_COMMAND",
            format!("Unknown command: {name}"),
        ))
    }

    #[cfg(not(feature = "plugins"))]
    pub(in crate::editor) fn run_plugin_command(
        &mut self,
        name: &str,
        _args: &[String],
        _cmd: &str,
    ) -> bool {
        self.reject_unknown_ex_command(name)
    }

    #[cfg(feature = "plugins")]
    pub(in crate::editor) fn run_plugin_command(
        &mut self,
        name: &str,
        args: &[String],
        cmd: &str,
    ) -> bool {
        if name != "lua" {
            return self.call_plugins(|editor| editor.plugin_host.execute_command(name, args))
                || self.reject_unknown_ex_command(name);
        }
        let s = cmd.trim_start_matches(':').trim();
        let code = s.strip_prefix("lua").map_or("", |r| r.trim_start());
        self.plugin_host.mark_lua_used();
        self.call_plugins(|editor| {
            if let Some(err) = editor.plugin_host.lua_exec(code) {
                editor.notify(NotificationType::Error, err);
            }
        });
        true
    }
}
