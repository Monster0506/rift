use super::Editor;
use crate::action::Motion;
use crate::dot_repeat::DotRegister;
use crate::mode::Mode;
#[allow(unused_imports)]
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub fn term_mut(&mut self) -> &mut T {
        &mut self.term
    }

    pub(super) fn execute_operator(
        &mut self,
        op: crate::action::OperatorType,
        motion: Motion,
    ) -> bool {
        let count = if self.pending_count > 0 {
            self.pending_count
        } else {
            1
        };
        self.pending_operator = None;
        self.pending_count = 0;

        // Capture text to ring before any destructive operation, and for yank.
        let viewport_height = self.render_system.viewport.visible_rows();
        let last_search_query = self.state.last_search_query.clone();
        let captured = self.document_manager.active_document_mut().and_then(|doc| {
            let tab_width = doc.options.tab_width;
            crate::executor::compute_motion_range(
                motion,
                count,
                doc,
                viewport_height,
                last_search_query.as_deref(),
                tab_width,
            )
            .map(|range| crate::clipboard::capture_text(&doc.buffer, &range))
        });
        let in_clipboard = self
            .document_manager
            .active_document()
            .map(|d| d.is_any_clipboard())
            .unwrap_or(false);
        let is_directory = self
            .document_manager
            .active_document()
            .map(|d| d.is_directory())
            .unwrap_or(false);
        if let Some(text) = captured.filter(|s| !s.is_empty()) {
            if !in_clipboard {
                let text = if is_directory {
                    strip_dir_id_prefixes(&text)
                } else {
                    text
                };
                self.clipboard_ring.push(text);
                self.refresh_clipboard_buffer_if_open();
            }
        }

        match op {
            crate::action::OperatorType::Delete => {
                let command = crate::command::Command::Delete(motion, count);
                self.set_mode(Mode::Normal);
                let result = self.execute_buffer_command(command);
                if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            crate::action::OperatorType::Change => {
                let command = crate::command::Command::Change(motion, count);
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                self.set_mode(Mode::Normal);
                self.execute_buffer_command(command);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
                true
            }
            crate::action::OperatorType::Yank => {
                // Text already captured above; just return to Normal.
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

    pub(super) fn execute_operator_linewise(&mut self, op: crate::action::OperatorType) -> bool {
        self.pending_operator = None;

        // Capture current line text for all operators.
        let captured = self
            .document_manager
            .active_document()
            .map(|doc| crate::clipboard::capture_current_line(&doc.buffer));
        let in_clipboard = self
            .document_manager
            .active_document()
            .map(|d| d.is_any_clipboard())
            .unwrap_or(false);
        let is_directory = self
            .document_manager
            .active_document()
            .map(|d| d.is_directory())
            .unwrap_or(false);
        if let Some(text) = captured.filter(|s| !s.is_empty()) {
            if !in_clipboard {
                let text = if is_directory {
                    strip_dir_id_prefixes(&text)
                } else {
                    text
                };
                self.clipboard_ring.push(text);
                self.refresh_clipboard_buffer_if_open();
            }
        }

        match op {
            crate::action::OperatorType::Delete => {
                let command = crate::command::Command::DeleteLine;
                self.set_mode(Mode::Normal);
                let result = self.execute_buffer_command(command);
                if result && !self.dot_repeat.is_replaying() && command.is_repeatable() {
                    self.dot_repeat.record_single(command);
                }
                result
            }
            crate::action::OperatorType::Change => {
                let command = crate::command::Command::ChangeLine;
                self.document_manager
                    .active_document_mut()
                    .unwrap()
                    .begin_transaction("Change");
                self.set_mode(Mode::Normal);
                self.execute_buffer_command(command);
                if !self.dot_repeat.is_replaying() {
                    self.dot_repeat.start_insert_recording(command);
                }
                self.set_mode(Mode::Insert);
                true
            }
            crate::action::OperatorType::Yank => {
                // Text already captured above; just return to Normal.
                self.set_mode(Mode::Normal);
                true
            }
        }
    }

    /// Replay the last repeatable action (dot-repeat)
    pub(super) fn execute_dot_repeat(&mut self) -> bool {
        let register = match self.dot_repeat.register() {
            Some(reg) => reg.clone(),
            None => return false,
        };

        let count = if self.pending_count > 0 {
            self.pending_count
        } else {
            1
        };

        self.dot_repeat.set_replaying(true);

        match register {
            DotRegister::Single(cmd) => {
                for _ in 0..count {
                    self.execute_buffer_command(cmd);
                }
            }
            DotRegister::InsertSession { entry, commands } => {
                for _ in 0..count {
                    // Enter insert mode (handles cursor positioning for a/A/I)
                    self.handle_mode_management(entry);

                    // Replay all commands from the session
                    for &cmd in &commands {
                        self.execute_buffer_command(cmd);
                    }

                    // Exit insert mode: commit transaction
                    if let Some(doc) = self.document_manager.active_document_mut() {
                        doc.commit_transaction();
                    }
                    self.set_mode(Mode::Normal);
                }
            }
        }

        self.dot_repeat.set_replaying(false);
        true
    }
}

pub(super) fn strip_dir_id_prefixes(text: &str) -> String {
    text.lines()
        .map(crate::document::dir_entry_name_from_line)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::strip_dir_id_prefixes;

    #[test]
    fn strip_empty_string() {
        assert_eq!(strip_dir_id_prefixes(""), "");
    }

    #[test]
    fn strip_single_line_with_prefix() {
        assert_eq!(strip_dir_id_prefixes("/001 hello.txt"), "hello.txt");
    }

    #[test]
    fn strip_single_line_no_prefix_passes_through() {
        assert_eq!(strip_dir_id_prefixes("hello.txt"), "hello.txt");
    }

    #[test]
    fn strip_multiline_mixed_prefix_and_plain() {
        let input = "../\n/001 a.txt\n/002 b.rs\nno_prefix.txt";
        let got = strip_dir_id_prefixes(input);
        assert_eq!(got, "../\na.txt\nb.rs\nno_prefix.txt");
    }

    #[test]
    fn strip_prefix_only_line_yields_empty_line() {
        // "/001 " with nothing visible after — output line must be empty, not garbage.
        let got = strip_dir_id_prefixes("/001 ");
        assert_eq!(got, "");
    }

    #[test]
    fn strip_partial_prefix_not_stripped() {
        // Missing the trailing space — not a valid prefix, must pass through unchanged.
        assert_eq!(strip_dir_id_prefixes("/001file.txt"), "/001file.txt");
    }

    #[test]
    fn strip_preserves_trailing_slash_on_dirs() {
        assert_eq!(strip_dir_id_prefixes("/042 subdir/"), "subdir/");
    }

    #[test]
    fn strip_line_count_preserved() {
        // Input has 3 lines → output must also have 3 lines (join("\n") not join("\n\n")).
        let input = "/001 a.txt\n/002 b.txt\n/003 c.txt";
        let out = strip_dir_id_prefixes(input);
        assert_eq!(out.lines().count(), 3);
    }
}
