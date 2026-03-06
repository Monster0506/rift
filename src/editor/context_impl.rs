use super::Editor;
use crate::error::RiftError;
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> crate::editor_api::EditorContext for Editor<T> {
    fn active_document_id(&self) -> Option<crate::document::DocumentId> {
        self.document_manager.active_document_id()
    }

    fn active_document_mut(&mut self) -> Option<&mut crate::document::Document> {
        self.document_manager.active_document_mut()
    }

    fn open_file(&mut self, path: Option<String>, force: bool) -> Result<(), RiftError> {
        self.open_file(path, force)
    }

    fn spawn_job(&mut self, job: Box<dyn crate::job_manager::Job + Send>) -> usize {
        self.job_manager.spawn(job)
    }

    fn set_mode(&mut self, mode: Mode) {
        self.set_mode(mode);
    }

    fn notify(&mut self, kind: crate::notification::NotificationType, message: String) {
        self.state.notify(kind, message);
    }

    fn force_redraw(&mut self) -> Result<(), RiftError> {
        self.force_full_redraw()
    }

    fn quit(&mut self) {
        self.should_quit = true;
    }

    fn execute_command_line(&mut self, cmd: String) {
        self.execute_command_line(cmd);
    }

    fn perform_search(&mut self, query: &str, direction: crate::search::SearchDirection) {
        // Update state logic if needed, or rely on caller?
        // Editor::perform_search updates highlights but doesn't set last_search_query in state.
        // ComponentAction::ExecuteSearch did that manually.
        // It's safer if perform_search does it if we want it to persist.
        // But let's stick to calling the method.
        self.perform_search(query, direction, false);
    }

    fn trigger_syntax_highlighting(&mut self, doc_id: crate::document::DocumentId) {
        self.spawn_syntax_parse_job(doc_id);
    }

    fn clear_command_line(&mut self) {
        self.state.clear_command_line();
    }
}
