//! Top-level `assert_invariants`: meant to be called
//! after each keystroke by a fuzzer

use super::Editor;
use crate::invariants::{extend, InvariantTier, Violation};
use crate::mode::Mode;
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    pub(crate) fn assert_invariants(&self, tier: InvariantTier) -> Vec<Violation> {
        let mut out = Vec::new();

        self.check_pending_input_state(&mut out);
        self.check_command_line_state(&mut out);

        extend(
            &mut out,
            "document_manager",
            self.document_manager.check_invariants(),
        );
        extend(
            &mut out,
            "split_tree",
            self.split_tree
                .check_invariants(&self.document_manager, tier),
        );
        self.check_panel_layout(&mut out);
        extend(
            &mut out,
            "compositor",
            self.render_system.compositor.check_invariants(),
        );

        for doc in self.document_manager.documents_iter() {
            extend(
                &mut out,
                "document",
                doc.check_invariants(&self.buffer_kinds, tier),
            );
        }

        if tier >= InvariantTier::Standard {
            self.check_windows(&mut out);
            self.check_display_map_cache(&mut out);
        }

        out
    }

    fn check_pending_input_state(&self, out: &mut Vec<Violation>) {
        let mode = self.mode();

        if self.input.operator.is_some() != (mode == Mode::OperatorPending) {
            out.push(Violation::new(
                "editor.input.operator",
                format!(
                    "input.operator.is_some()={} but mode={mode:?}",
                    self.input.operator.is_some()
                ),
            ));
        }

        if self.visual_anchor().is_some() != mode.is_visual() {
            out.push(Violation::new(
                "editor.visual_anchor",
                format!(
                    "visual_anchor.is_some()={} but mode={mode:?}",
                    self.visual_anchor().is_some()
                ),
            ));
        }

        if !self.input.multi_insert_anchors.is_empty()
            && !matches!(mode, Mode::Insert | Mode::Replace)
        {
            out.push(Violation::new(
                "editor.input.multi_insert_anchors",
                format!(
                    "{} pending multi-insert anchors but mode={mode:?}, not Insert/Replace",
                    self.input.multi_insert_anchors.len()
                ),
            ));
        }

        if mode != Mode::Normal {
            return;
        }
        let Some(doc) = self
            .document_manager
            .get_document(self.active_document_id())
        else {
            return;
        };
        let cursor = doc.buffer().cursor();
        let last = doc.buffer().line_last_char();
        if cursor > last {
            out.push(Violation::new(
                "editor.normal_cursor",
                format!("Normal-mode cursor {cursor} past last char {last}"),
            ));
        }
    }

    fn check_command_line_state(&self, out: &mut Vec<Violation>) {
        let line = &self.state.command_line;
        let cursor = self.state.command_line_cursor;
        if cursor > line.len() {
            out.push(Violation::new(
                "editor.command_line",
                format!(
                    "command_line_cursor {cursor} > command_line.len() {}",
                    line.len()
                ),
            ));
        } else if !line.is_char_boundary(cursor) {
            out.push(Violation::new(
                "editor.command_line",
                format!("command_line_cursor {cursor} is not a char boundary in {line:?}"),
            ));
        }
    }

    fn check_panel_layout(&self, out: &mut Vec<Violation>) {
        let Some(layout) = &self.panel_layout else {
            return;
        };
        let area = "editor.panel_layout";
        if layout.dir_win_id == layout.preview_win_id {
            out.push(Violation::new(
                area,
                "panel list and preview share a window",
            ));
        }
        if !self.document_manager.is_private(layout.dir_doc_id) {
            out.push(Violation::new(
                area,
                format!("panel list buffer {} is not private", layout.dir_doc_id),
            ));
        }
        for id in [layout.dir_win_id, layout.preview_win_id] {
            if self.split_tree.get_window(id).is_none() {
                out.push(Violation::new(
                    area,
                    format!("panel window {id} is missing"),
                ));
            }
        }
        if self
            .document_manager
            .get_document(layout.original_doc_id)
            .is_none()
        {
            out.push(Violation::new(
                area,
                format!(
                    "panel original document {} is missing",
                    layout.original_doc_id
                ),
            ));
        }
        for (id, window) in self.split_tree.windows() {
            if !self.window_accepts_buffer(*id, window.document_id) {
                out.push(Violation::new(
                    area,
                    format!(
                        "window {id} is showing unexpected panel buffer {}",
                        window.document_id
                    ),
                ));
            }
        }
    }

    fn check_windows(&self, out: &mut Vec<Violation>) {
        for (id, window) in self.split_tree.windows() {
            let Some(doc) = self.document_manager.get_document(window.document_id) else {
                continue;
            };
            let total_lines = doc.buffer().get_total_lines();
            let viewport = &window.viewport;
            if viewport.visible_rows() == 0 || viewport.visible_cols() == 0 {
                out.push(Violation::new(
                    "editor.viewport",
                    format!(
                        "window {id}: viewport has zero visible size ({}x{})",
                        viewport.visible_rows(),
                        viewport.visible_cols()
                    ),
                ));
            }
            if viewport.top_line() > 0 && viewport.top_line() >= total_lines {
                out.push(Violation::new(
                    "editor.viewport",
                    format!(
                        "window {id}: top_line={} >= document total_lines={total_lines}",
                        viewport.top_line()
                    ),
                ));
            }
        }
    }

    fn check_display_map_cache(&self, out: &mut Vec<Violation>) {
        for entry in &self.display_map_cache {
            let Some(map) = entry.map.as_ref() else {
                continue;
            };
            let Some(doc) = self.document_manager.get_document(entry.doc_id) else {
                continue;
            };
            if entry.revision != doc.buffer().revision() || entry.buf_len != doc.buffer().len() {
                continue;
            }
            extend(
                out,
                "editor.display_map_cache",
                map.check_invariants(doc.buffer().len(), InvariantTier::Standard),
            );
        }
    }
}
