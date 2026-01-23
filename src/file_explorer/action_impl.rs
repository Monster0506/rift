use super::ExplorerAction;
use crate::editor_api::{EditorAction, EditorContext};
use crate::error::RiftError;

impl EditorAction for ExplorerAction {
    fn execute(self: Box<Self>, ctx: &mut dyn EditorContext) -> Result<(), RiftError> {
        match *self {
            ExplorerAction::SpawnJob(job) => {
                ctx.spawn_job(job);
            }
            ExplorerAction::OpenFile(path) => {
                if let Err(e) = ctx.open_file(Some(path.to_string_lossy().to_string()), false) {
                    return Err(e);
                } else {
                    ctx.close_active_modal();
                    let _ = ctx.force_redraw();
                }
            }
            ExplorerAction::Notify(kind, msg) => {
                ctx.notify(kind, msg);
            }
            ExplorerAction::Close => {
                ctx.close_active_modal();
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
