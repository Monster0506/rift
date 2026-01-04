use crate::error::RiftError;
use crate::layer::LayerCompositor;
use crate::render::{CursorPosition, DrawContext, RenderCache};
use crate::term::TerminalBackend;
use crate::viewport::Viewport;

/// Persistent rendering system that holds all render-related state
pub struct RenderSystem {
    pub compositor: LayerCompositor,
    pub viewport: Viewport,
    pub render_cache: RenderCache,
}

impl RenderSystem {
    /// Create a new render system with specified dimensions
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            compositor: LayerCompositor::new(rows, cols),
            viewport: Viewport::new(rows, cols),
            render_cache: RenderCache::default(),
        }
    }

    /// Resize the render system
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.viewport.set_size(rows, cols);
        self.compositor.resize(rows, cols);
        self.render_cache.invalidate_all();
    }

    /// Render to terminal
    pub fn render<T: TerminalBackend>(
        &mut self,
        term: &mut T,
        ctx: DrawContext,
    ) -> Result<CursorPosition, RiftError> {
        crate::render::render(term, &mut self.compositor, ctx, &mut self.render_cache)
    }

    /// Force full redraw
    pub fn force_full_redraw<T: TerminalBackend>(
        &mut self,
        term: &mut T,
        ctx: DrawContext,
    ) -> Result<CursorPosition, RiftError> {
        crate::render::full_redraw(term, &mut self.compositor, ctx, &mut self.render_cache)
    }
}
