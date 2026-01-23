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

    fn close_active_modal(&mut self) {
        self.close_active_modal();
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
        // Sync legacy state for consistency
        self.state.command_line = cmd.clone();

        // Parse and execute the command
        let parsed_command = self.command_parser.parse(&cmd);
        let execution_result = crate::command_line::commands::CommandExecutor::execute(
            parsed_command.clone(),
            &mut self.state,
            self.document_manager
                .active_document_mut()
                .expect("active document missing"),
            &self.settings_registry,
            &self.document_settings_registry,
        );

        self.handle_execution_result(execution_result);
    }

    fn active_modal_component(&mut self) -> Option<&mut dyn crate::component::Component> {
        match self.modal.as_mut() {
            Some(m) => Some(m.component.as_mut()),
            None => None,
        }
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
