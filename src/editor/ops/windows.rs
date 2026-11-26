#[path = "../../split/tree.rs"]
pub(crate) mod tree;

use super::super::Editor;
use crate::document::DocumentId;
use crate::plugin::events::EventPayload;
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
        let opened = self.split_tree.split(
            direction,
            win,
            doc_id,
            size.rows as usize,
            size.cols as usize,
        )?;
        self.raise(EventPayload::WinNew {
            win: opened.get(),
            buf: doc_id,
        });
        Some(opened)
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
        let opened = self.split_tree.split_before_with_ratio(
            direction,
            win,
            doc_id,
            size.rows as usize,
            size.cols as usize,
            ratio,
        )?;
        self.raise(EventPayload::WinNew {
            win: opened.get(),
            buf: doc_id,
        });
        Some(opened)
    }

    pub(crate) fn close_window(&mut self, id: WindowId) -> bool {
        let Some(buf) = self
            .split_tree
            .get_window(id)
            .map(|window| window.document_id)
        else {
            return false;
        };
        if id == self.split_tree.focused_window_id() {
            let Some(next_id) = self.split_tree.window_after_close(id) else {
                return false;
            };
            let next_buf = self
                .split_tree
                .get_window(next_id)
                .expect("window in layout")
                .document_id;
            self.raise(EventPayload::WinLeave { win: id.get(), buf });
            if self
                .focus_document_with(next_buf, |editor| {
                    editor.split_tree.close_window(id);
                    editor.restore_window_cursor(next_buf);
                })
                .is_err()
            {
                return false;
            }
            self.raise(EventPayload::WinEnter {
                win: next_id.get(),
                buf: next_buf,
            });
        } else if !self.split_tree.close_window(id) {
            return false;
        }
        self.render_system.viewport.mark_needs_full_redraw();
        self.raise(EventPayload::WinClosed { win: id.get(), buf });
        true
    }

    fn restore_window_cursor(&mut self, buf: DocumentId) {
        let cursor = self.split_tree.focused_window().cursor_position;
        if let Some(doc) = self.document_manager.get_document_mut(buf) {
            let _ = doc.set_cursor(cursor, crate::document::ops::CursorCause::User);
        }
    }

    pub(crate) fn move_window(&mut self, direction: Direction) -> bool {
        let layouts = self.window_layouts();
        let moved_id = self.split_tree.focused_window_id();
        if !self.split_tree.move_window(direction, &layouts) {
            return false;
        }
        self.render_system.viewport.mark_needs_full_redraw();
        self.raise(EventPayload::WinMoved {
            win: moved_id.get(),
        });
        true
    }

    pub(crate) fn swap_windows(&mut self) -> bool {
        let focused_id = self.split_tree.focused_window_id();
        let Some(prev_id) = self.split_tree.previous_window() else {
            return false;
        };
        let Some(next_buf) = self
            .split_tree
            .get_window(prev_id)
            .map(|window| window.document_id)
        else {
            return false;
        };
        if focused_id == prev_id
            || self
                .focus_document_with(next_buf, |editor| {
                    editor.split_tree.exchange_windows(focused_id, prev_id);
                    editor.restore_window_cursor(next_buf);
                })
                .is_err()
        {
            return false;
        }
        self.render_system.viewport.mark_needs_full_redraw();
        self.raise(EventPayload::WinSwapped {
            win1: focused_id.get(),
            win2: prev_id.get(),
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
        let proportional = self.state.settings.equalize_proportional();
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

    pub(crate) fn focus_window(&mut self, target: WindowId) {
        if let Some(window) = self.split_tree.get_window(target) {
            self.focus_window_showing(target, window.document_id);
        }
    }

    pub(crate) fn focus_window_showing(&mut self, target: WindowId, buf: DocumentId) {
        let Some(target_buf) = self
            .split_tree
            .get_window(target)
            .map(|window| window.document_id)
        else {
            return;
        };
        let old_win = self.split_tree.focused_window_id();
        let old_buf = self.active_document_id();
        if old_win == target {
            if old_buf != buf {
                let _ = self.focus_document(buf);
            }
            return;
        }
        if let Some(doc) = self.document_manager.get_document(old_buf) {
            self.set_window_cursor(old_win, doc.buffer().cursor());
        }
        self.raise(EventPayload::WinLeave {
            win: old_win.get(),
            buf: old_buf,
        });
        if self
            .focus_document_with(buf, |editor| {
                editor.split_tree.set_focus(target);
                if target_buf == buf {
                    editor.restore_window_cursor(buf);
                } else {
                    editor.show_in_window(target, buf);
                    editor.restore_view_state();
                }
            })
            .is_ok()
        {
            self.raise(EventPayload::WinEnter {
                win: target.get(),
                buf,
            });
        }
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
