#[path = "../../split/tree.rs"]
pub(crate) mod tree;

use super::super::Editor;
use crate::document::DocumentId;
use crate::plugin::events::EditorEvent;
use crate::split::navigation::Direction;
use crate::split::tree::SplitDirection;
use crate::split::window::WindowId;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    fn window_layouts(&self) -> Vec<crate::split::layout::WindowLayout> {
        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        self.split_tree
            .compute_layout(size.rows as usize, size.cols as usize)
    }

    pub(crate) fn split_window(
        &mut self,
        direction: SplitDirection,
        doc_id: DocumentId,
    ) -> Option<WindowId> {
        let focused_id = self.split_tree.focused_window_id();
        let new_id = self.split_window_beside(focused_id, direction, doc_id)?;
        self.focus_window(new_id);
        Some(new_id)
    }

    pub(crate) fn split_window_beside(
        &mut self,
        win: WindowId,
        direction: SplitDirection,
        doc_id: DocumentId,
    ) -> Option<WindowId> {
        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        self.split_tree.split(
            direction,
            win,
            doc_id,
            size.rows as usize,
            size.cols as usize,
        )
    }

    pub(crate) fn split_window_before(
        &mut self,
        win: WindowId,
        direction: SplitDirection,
        doc_id: DocumentId,
        ratio: f64,
    ) -> Option<WindowId> {
        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        self.split_tree.split_before_with_ratio(
            direction,
            win,
            doc_id,
            size.rows as usize,
            size.cols as usize,
            ratio,
        )
    }

    pub(crate) fn remove_window(&mut self, id: WindowId) -> bool {
        self.split_tree.close_window(id)
    }

    fn adopt_focused_window(&mut self) -> DocumentId {
        let window = self.split_tree.focused_window();
        let (doc_id, cursor) = (window.document_id, window.cursor_position);
        let _ = self.select_buffer(doc_id);
        if let Some(doc) = self.document_manager.get_document_mut(doc_id) {
            let _ = doc.set_cursor(cursor, crate::document::ops::CursorCause::User);
        }
        self.sync_state_with_active_document();
        doc_id
    }

    pub(crate) fn close_window(&mut self, id: WindowId) -> bool {
        if !self.remove_window(id) {
            return false;
        }
        self.adopt_focused_window();
        self.render_system.viewport.mark_needs_full_redraw();
        true
    }

    pub(crate) fn move_window(&mut self, direction: Direction) -> bool {
        let layouts = self.window_layouts();
        let moved_id = self.split_tree.focused_window_id();
        if !self.split_tree.move_window(direction, &layouts) {
            return false;
        }
        self.render_system.viewport.mark_needs_full_redraw();
        self.call_plugins(|editor| {
            editor.plugin_host.dispatch(&EditorEvent::WinMoved {
                win: moved_id.get(),
            });
        });
        true
    }

    pub(crate) fn swap_windows(&mut self) -> bool {
        let focused_id = self.split_tree.focused_window_id();
        let Some(prev_id) = self.split_tree.previous_window() else {
            return false;
        };
        if !self.split_tree.exchange_windows(focused_id, prev_id) {
            return false;
        }
        self.adopt_focused_window();
        self.render_system.viewport.mark_needs_full_redraw();
        self.call_plugins(|editor| {
            editor.plugin_host.dispatch(&EditorEvent::WinSwapped {
                win1: focused_id.get(),
                win2: prev_id.get(),
            });
        });
        true
    }

    pub(crate) fn resize_window(&mut self, direction: SplitDirection, delta: i32) {
        let layouts = self.window_layouts();
        self.split_tree
            .resize_focused(direction, f64::from(delta), &layouts);
        self.render_system.viewport.mark_needs_full_redraw();
    }

    pub(crate) fn equalize_windows(&mut self) {
        let proportional = self.state.settings.equalize_proportional;
        self.split_tree.equalize(proportional);
        self.render_system.viewport.mark_needs_full_redraw();
    }

    pub(crate) fn navigate_window(&mut self, direction: Direction) {
        let layouts = self.window_layouts();
        if let Some(target_id) = self.split_tree.navigate(direction, &layouts) {
            self.focus_window(target_id);
        }
    }

    pub(crate) fn focus_previous_window(&mut self) {
        if let Some(target_id) = self.split_tree.previous_window() {
            self.focus_window(target_id);
        }
    }

    pub(crate) fn focus_window(&mut self, target_id: WindowId) {
        let old_win_id = self.split_tree.focused_window_id();
        let old_doc_id = self.split_tree.focused_window().document_id;
        if let Some(doc) = self.document_manager.get_document(old_doc_id) {
            let cursor = doc.buffer().cursor();
            self.set_window_cursor(old_win_id, cursor);
        }

        self.call_plugins(|editor| {
            editor.plugin_host.dispatch(&EditorEvent::WinLeave {
                win: old_win_id.get(),
                buf: old_doc_id,
            });
        });

        self.split_tree.set_focus(target_id);
        let new_doc_id = self.adopt_focused_window();

        self.call_plugins(|editor| {
            editor.plugin_host.dispatch(&EditorEvent::WinEnter {
                win: target_id.get(),
                buf: new_doc_id,
            });
        });
    }

    pub(crate) fn focus_window_showing(&mut self, target_id: WindowId, doc_id: DocumentId) {
        self.split_tree.set_focus(target_id);
        let _ = self.select_buffer(doc_id);
    }

    pub(crate) fn set_window_cursor(&mut self, id: WindowId, cursor: usize) {
        if let Some(window) = self.split_tree.get_window_mut(id) {
            window.cursor_position = cursor;
        }
    }

    pub(crate) fn window_viewport_mut(
        &mut self,
        id: WindowId,
    ) -> Option<&mut crate::viewport::Viewport> {
        self.split_tree
            .get_window_mut(id)
            .map(|window| &mut window.viewport)
    }

    pub(crate) fn show_in_window(&mut self, win: WindowId, doc_id: DocumentId) {
        self.split_tree.set_window_document(win, doc_id);
    }
}
