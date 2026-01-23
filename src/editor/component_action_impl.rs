use super::ComponentAction;
use crate::editor::actions::{EditorAction, EditorContext};
use crate::error::RiftError;

impl EditorAction for ComponentAction {
    fn execute(self: Box<Self>, ctx: &mut dyn EditorContext) -> Result<(), RiftError> {
        match *self {
            ComponentAction::UndoTreeGoto(seq) => {
                // Get doc_id first before mutable borrow
                let doc_id = ctx.active_document_id().expect("No active document");

                // Now do mutable operations
                let goto_result = {
                    let doc = ctx.active_document_mut().expect("No active document");
                    doc.goto_seq(seq)
                };

                if let Err(e) = goto_result {
                    ctx.notify(
                        crate::notification::NotificationType::Error,
                        format!("Failed to go to sequence {}: {}", seq, e),
                    );
                }

                ctx.trigger_syntax_highlighting(doc_id);
                ctx.close_active_modal();
            }
            ComponentAction::UndoTreeCancel => {
                ctx.close_active_modal();
            }
            ComponentAction::UndoTreePreview(seq) => {
                // Get preview text first
                let preview_content = {
                    let doc = ctx.active_document_mut().expect("No active document");
                    if let Ok(preview_text) = doc.preview_at_seq(seq) {
                        use crate::layer::Cell;
                        let mut content = Vec::new();
                        for line in preview_text.lines() {
                            let cells: Vec<Cell> = line.chars().map(Cell::from_char).collect();
                            content.push(cells);
                        }
                        Some(content)
                    } else {
                        None
                    }
                };

                // Then update modal
                if let Some(content) = preview_content {
                    if let Some(component) = ctx.active_modal_component() {
                        if let Some(view) = component
                            .as_any_mut()
                            .downcast_mut::<crate::select_view::SelectView>()
                        {
                            view.set_right_content(content);
                        }
                    }
                }
                ctx.force_redraw()?;
            }
            ComponentAction::ExecuteCommand(cmd) => {
                ctx.execute_command_line(cmd);
                ctx.clear_command_line();
                ctx.close_active_modal();
            }
            ComponentAction::ExecuteSearch(query) => {
                if !query.is_empty() {
                    ctx.perform_search(&query, crate::search::SearchDirection::Forward);
                }
                ctx.clear_command_line();
                ctx.close_active_modal();
            }
            ComponentAction::CancelMode => {
                ctx.clear_command_line();
                ctx.close_active_modal();
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
