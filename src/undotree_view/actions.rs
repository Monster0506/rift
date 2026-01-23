use crate::editor_api::{EditorAction, EditorContext};
use crate::error::RiftError;
use crate::history::EditSeq;

#[derive(Debug, Clone)]
pub struct UndoTreeGotoAction {
    pub seq: EditSeq,
}

impl EditorAction for UndoTreeGotoAction {
    fn execute(self: Box<Self>, ctx: &mut dyn EditorContext) -> Result<(), RiftError> {
        let doc_id = ctx.active_document_id().ok_or_else(|| {
            RiftError::new(
                crate::error::ErrorType::Execution,
                "NO_DOCUMENT",
                "No active document",
            )
        })?;

        {
            let doc = ctx.active_document_mut().ok_or_else(|| {
                RiftError::new(
                    crate::error::ErrorType::Execution,
                    "NO_DOCUMENT",
                    "No active document",
                )
            })?;
            doc.goto_seq(self.seq).map_err(|e| {
                RiftError::new(
                    crate::error::ErrorType::Execution,
                    "UNDO_ERROR",
                    e.to_string(),
                )
            })?;
        }

        ctx.trigger_syntax_highlighting(doc_id);
        ctx.close_active_modal();
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone)]
pub struct UndoTreePreviewAction {
    pub seq: EditSeq,
}

impl EditorAction for UndoTreePreviewAction {
    fn execute(self: Box<Self>, ctx: &mut dyn EditorContext) -> Result<(), RiftError> {
        let content = {
            let doc = ctx.active_document_mut().ok_or_else(|| {
                RiftError::new(
                    crate::error::ErrorType::Execution,
                    "NO_DOCUMENT",
                    "No active document",
                )
            })?;

            if let Ok(preview_text) = doc.preview_at_seq(self.seq) {
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

        if let Some(content) = content {
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
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone)]
pub struct UndoTreeCancelAction;

impl EditorAction for UndoTreeCancelAction {
    fn execute(self: Box<Self>, ctx: &mut dyn EditorContext) -> Result<(), RiftError> {
        ctx.close_active_modal();
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
