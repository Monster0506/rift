#[path = "../../split/tree.rs"]
pub(crate) mod tree;
#[path = "../../split/window.rs"]
pub(crate) mod window;

use super::super::{Editor, PanelKind, PanelLayout};
use crate::document::manager::{Placement, RemovalIntent};
use crate::document::DocumentId;
#[cfg(feature = "plugins")]
use crate::plugin::events::EventPayload;
use crate::split::navigation::Direction;
use crate::split::tree::SplitDirection;
use crate::split::window::WindowId;
use crate::term::TerminalBackend;
#[cfg(feature = "plugins")]
use crate::viewport::ScrollPosition;
use crate::viewport::Viewport;

#[derive(Clone, Copy)]
pub(in crate::editor) enum PanelSplit {
    Beside(DocumentId),
    Below,
}

impl<T: TerminalBackend> Editor<T> {
    fn window_layouts(&self) -> Vec<crate::split::layout::WindowLayout> {
        let size = self
            .term
            .get_size()
            .unwrap_or(crate::term::Size { rows: 24, cols: 80 });
        self.split_tree
            .compute_layout(size.rows as usize, size.cols as usize)
    }

    pub(in crate::editor) fn open_split_panel(
        &mut self,
        kind: PanelKind,
        list: crate::document::Document,
        private_preview: Option<crate::document::Document>,
        placement: PanelSplit,
    ) -> bool {
        let original_win = self.split_tree.focused_window_id();
        let original_doc_id = self.active_document_id();
        let dir_doc_id = self.open_buffer(list, Placement::Private);
        let private_preview = private_preview.map(|doc| self.open_buffer(doc, Placement::Private));
        let (direction, target) = match placement {
            PanelSplit::Beside(target) => (SplitDirection::Vertical, target),
            PanelSplit::Below => (SplitDirection::Horizontal, dir_doc_id),
        };
        let Some(new_win) = self.split_window_beside(original_win, direction, target) else {
            for id in [Some(dir_doc_id), private_preview].into_iter().flatten() {
                let _ = self.close_buffer(id, RemovalIntent::Force);
            }
            return false;
        };
        let (dir_win_id, preview_win_id) = match placement {
            PanelSplit::Beside(_) => (original_win, new_win),
            PanelSplit::Below => (new_win, original_win),
        };
        self.focus_window_showing(dir_win_id, dir_doc_id);
        self.panel_layout = Some(PanelLayout {
            kind,
            dir_win_id,
            preview_win_id,
            dir_doc_id,
            preview_doc_id: private_preview.unwrap_or(original_doc_id),
            original_doc_id,
        });
        true
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
        self.initialize_split_view();
        #[cfg(feature = "plugins")]
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
        self.initialize_split_view();
        #[cfg(feature = "plugins")]
        self.raise(EventPayload::WinNew {
            win: opened.get(),
            buf: doc_id,
        });
        Some(opened)
    }

    fn initialize_split_view(&mut self) {
        if self.split_tree.window_count() == 2 {
            let scroll = self.render_system.viewport.scroll_position();
            let focused = self.split_tree.focused_window_id();
            if let Some(window) = self.split_tree.get_window_mut(focused) {
                window.viewport.set_scroll_position(scroll);
            }
        }
    }

    pub(crate) fn close_window(&mut self, id: WindowId) -> bool {
        #[cfg(feature = "plugins")]
        let Some(buf) = self
            .split_tree
            .get_window(id)
            .map(|window| window.document_id)
        else {
            return false;
        };
        #[cfg(not(feature = "plugins"))]
        if self.split_tree.get_window(id).is_none() {
            return false;
        }
        if id == self.split_tree.focused_window_id() {
            let Some(next_id) = self.split_tree.window_after_close(id) else {
                return false;
            };
            let next_buf = self
                .split_tree
                .get_window(next_id)
                .expect("window in layout")
                .document_id;
            #[cfg(feature = "plugins")]
            self.raise(EventPayload::WinLeave { win: id.get(), buf });
            if self
                .focus_document_with(next_buf, |editor| {
                    editor.save_focused_view();
                    editor.split_tree.close_window(id);
                    editor.restore_focused_view();
                })
                .is_err()
            {
                return false;
            }
            #[cfg(feature = "plugins")]
            self.raise(EventPayload::WinEnter {
                win: next_id.get(),
                buf: next_buf,
            });
        } else if !self.split_tree.close_window(id) {
            return false;
        } else if self.split_tree.window_count() == 1 {
            self.restore_focused_view();
        }
        self.render_system.viewport.mark_needs_full_redraw();
        #[cfg(feature = "plugins")]
        self.raise(EventPayload::WinClosed { win: id.get(), buf });
        true
    }

    fn restore_focused_view(&mut self) {
        if self.split_tree.window_count() == 1 {
            let scroll = self.split_tree.focused_window().viewport.scroll_position();
            self.render_system.viewport.set_scroll_position(scroll);
        }
        let buf = self.active_document_id();
        let id = self.split_tree.focused_window_id();
        let Some(cursor) = self
            .split_tree
            .get_window_mut(id)
            .and_then(|window| window.saved_cursor.take())
        else {
            return;
        };
        if let Some(doc) = self.document_manager.get_document_mut(buf) {
            let _ = doc.set_cursor(
                cursor.min(doc.buffer().len()),
                crate::document::ops::CursorCause::Programmatic,
            );
        }
    }

    pub(super) fn save_focused_view(&mut self) {
        let scroll = self.focused_viewport().scroll_position();
        self.active_document().save_view_state(scroll);
        if self.split_tree.window_count() == 1 {
            return;
        }
        let id = self.split_tree.focused_window_id();
        let buf = self.active_document_id();
        if !self
            .split_tree
            .windows()
            .any(|(other, window)| *other != id && window.document_id == buf)
        {
            return;
        }
        let cursor = self.active_document().buffer().cursor();
        if let Some(window) = self.split_tree.get_window_mut(id) {
            window.saved_cursor = Some(cursor);
        }
    }

    pub(crate) fn move_window(&mut self, direction: Direction) -> bool {
        let layouts = self.window_layouts();
        #[cfg(feature = "plugins")]
        let moved_id = self.split_tree.focused_window_id();
        if !self.split_tree.move_window(direction, &layouts) {
            return false;
        }
        self.render_system.viewport.mark_needs_full_redraw();
        #[cfg(feature = "plugins")]
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
                    editor.save_focused_view();
                    editor.split_tree.exchange_windows(focused_id, prev_id);
                    editor.restore_focused_view();
                })
                .is_err()
        {
            return false;
        }
        self.render_system.viewport.mark_needs_full_redraw();
        #[cfg(feature = "plugins")]
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
        #[cfg(feature = "plugins")]
        self.raise(EventPayload::WinLeave {
            win: old_win.get(),
            buf: old_buf,
        });
        if self
            .focus_document_with(buf, |editor| {
                editor.save_focused_view();
                editor.split_tree.set_focus(target);
                if target_buf == buf {
                    editor.restore_focused_view();
                } else {
                    editor.show_in_window(target, buf);
                }
            })
            .is_ok()
        {
            #[cfg(feature = "plugins")]
            self.raise(EventPayload::WinEnter {
                win: target.get(),
                buf,
            });
        }
    }

    pub(in crate::editor) fn focused_viewport(&self) -> &Viewport {
        if self.split_tree.window_count() == 1 {
            &self.render_system.viewport
        } else {
            &self.split_tree.focused_window().viewport
        }
    }

    #[cfg(feature = "plugins")]
    fn focused_viewport_mut(&mut self) -> &mut Viewport {
        if self.split_tree.window_count() == 1 {
            &mut self.render_system.viewport
        } else {
            let id = self.split_tree.focused_window_id();
            self.window_viewport_mut(id)
                .expect("focused window must exist")
        }
    }

    #[cfg(feature = "plugins")]
    pub(in crate::editor) fn scroll_focused_view(&mut self, top: usize, left: usize) {
        let viewport = self.focused_viewport_mut();
        viewport.set_scroll_position(ScrollPosition {
            top_line: top,
            left_col: left,
            ..viewport.scroll_position()
        });
    }

    #[cfg(feature = "plugins")]
    pub(in crate::editor) fn center_focused_view(&mut self, row: usize, total: usize) {
        self.focused_viewport_mut().center_on(row, total);
    }

    pub(in crate::editor) fn follow_document_cursor(&mut self, id: WindowId) {
        if let Some(window) = self.split_tree.get_window_mut(id) {
            window.saved_cursor = None;
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
        let Some(doc) = self.document_manager.get_document(doc_id) else {
            return;
        };
        let scroll = doc.saved_view_state();
        if self.split_tree.set_window_document(win, doc_id, scroll)
            && win == self.split_tree.focused_window_id()
        {
            self.restore_focused_view();
        }
    }
}
