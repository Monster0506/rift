use super::super::Editor;
use crate::document::DocumentId;
use crate::job_manager::jobs::file_operations::{FileLoadJob, FileLoadMode};
use crate::mode::Mode;
use crate::term::TerminalBackend;
use std::path::PathBuf;

impl<T: TerminalBackend> Editor<T> {
    pub(in crate::editor) fn start_file_load(
        &mut self,
        doc_id: DocumentId,
        path: PathBuf,
        mode: FileLoadMode,
    ) {
        let doc = self
            .document_manager
            .get_document(doc_id)
            .expect("file loads target a registered document");
        self.job_manager
            .register_document_handle(doc.handle(), doc.buffer_kind_id());
        let job = self.job_manager.spawn(FileLoadJob::new(doc_id, path, mode));
        self.file_load_jobs.insert(job, doc_id);
    }

    pub(crate) fn open_file_from_input(&mut self, path: Option<String>, force: bool) -> bool {
        if let Err(e) = self.open_file(path, force) {
            self.notify_error(e);
            return false;
        }
        self.state.clear_command_line();
        if let Err(e) = self.force_full_redraw() {
            self.notify_error(e);
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
