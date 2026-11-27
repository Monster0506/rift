use super::super::Editor;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn open_file_from_input(&mut self, path: Option<String>, force: bool) -> bool {
        if let Err(e) = self.open_file(path, force) {
            self.notify_error(e);
        } else {
            self.state.clear_command_line();
            if let Err(e) = self.force_full_redraw() {
                self.notify_error(e);
            }
        }
        true
    }

    pub(crate) fn open_terminal_from_input(&mut self, cmd: Option<String>) -> bool {
        if let Err(e) = self.open_terminal(cmd) {
            self.notify_error(e);
        } else {
            self.state.clear_command_line();
            self.set_mode(Mode::Insert);
        }
        true
    }

    pub(crate) fn scroll_terminal(&mut self, delta: i32) -> bool {
        let doc_id = self.active_document_id();
        if let Some(doc) = self.document_manager.get_document(doc_id) {
            if let Some(term) = doc.terminal() {
                term.scroll_display(delta);
            }
        }
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            doc.sync_terminal_buffer();
        }
        let _ = self.update_and_render();
        true
    }
}
