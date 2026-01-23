use super::ComponentAction;
use crate::editor::actions::{EditorAction, EditorContext};
use crate::error::RiftError;

impl EditorAction for ComponentAction {
    fn execute(self: Box<Self>, ctx: &mut dyn EditorContext) -> Result<(), RiftError> {
        match *self {
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
