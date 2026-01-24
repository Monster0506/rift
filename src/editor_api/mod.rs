use crate::document::{Document, DocumentId};
use crate::error::RiftError;
use crate::job_manager::Job;

pub type JobId = usize;
use crate::mode::Mode;

/// Context provided to actions for manipulating the editor state
pub trait EditorContext {
    /// Get the ID of the active document
    fn active_document_id(&self) -> Option<DocumentId>;

    /// Access the active document mutably
    fn active_document_mut(&mut self) -> Option<&mut Document>;

    /// Open a file
    fn open_file(&mut self, path: Option<String>, force: bool) -> Result<(), RiftError>;

    /// Spawn a background job
    fn spawn_job(&mut self, job: Box<dyn Job + Send>) -> JobId;

    /// Switch mode
    fn set_mode(&mut self, mode: Mode);

    /// Close the active modal/overlay
    fn close_active_modal(&mut self);

    /// Send a notification
    fn notify(&mut self, kind: crate::notification::NotificationType, message: String);

    /// Force a full redraw
    fn force_redraw(&mut self) -> Result<(), RiftError>;

    /// Quit the editor
    fn quit(&mut self);

    /// Execute a command line string (e.g. ":w")
    fn execute_command_line(&mut self, cmd: String);

    /// Access the active modal component
    fn active_modal_component(&mut self) -> Option<&mut dyn crate::component::Component>;

    /// Perform a search
    fn perform_search(&mut self, query: &str, direction: crate::search::SearchDirection);

    /// Trigger syntax highlighting re-parse
    fn trigger_syntax_highlighting(&mut self, doc_id: DocumentId);

    /// Clear the command line state
    fn clear_command_line(&mut self);
}
