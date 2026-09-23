//! Editor-side annotation interactivity: activation dispatch and navigation.
//! Resolves the annotation under the cursor to a handler.

use super::Editor;
use crate::annotations::registry::{Builtin, Handler};
use crate::term::TerminalBackend;

impl<T: TerminalBackend> Editor<T> {
    /// Activate the interactive annotation under the cursor in the active document,
    /// using its default action. Returns true if an action was dispatched.
    pub fn activate_annotation_at_cursor(&mut self) -> bool {
        self.activate_annotation_verb(None)
    }

    /// Activate a specific verb on the annotation under the cursor (`None` = the
    /// default action), letting separate keybinds drive verbs. True if dispatched.
    pub fn activate_annotation_verb(&mut self, verb: Option<&str>) -> bool {
        let resolved = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);
            let cursor_byte = doc.buffer.char_to_byte(cursor);
            doc.annotations
                .interactive_at(cursor_byte)
                .or_else(|| doc.annotations.interactive_at_line(line))
                .and_then(|a| {
                    let act = match verb {
                        Some(v) => a.action_for_verb(v),
                        None => a.default_action(),
                    };
                    act.map(|act| (a.id, a.kind.clone(), act.verb.clone()))
                })
        };
        let Some((ann_id, kind, verb)) = resolved else {
            return false;
        };

        match self.dispatch_registry.resolve(&kind, &verb).cloned() {
            Some(Handler::Builtin(Builtin::ToggleChecked)) => {
                // Prefer flipping a literal "[ ]"/"[x]" in the buffer at the anchor;
                // fall back to a payload/overlay-only toggle when there is none.
                let anchor = self
                    .document_manager
                    .active_document_mut()
                    .and_then(|doc| doc.annotations.get(ann_id))
                    .map(|a| match a.anchor {
                        crate::annotations::Anchor::Point(p) => p.offset,
                        crate::annotations::Anchor::Range(s, _) => s.offset,
                        crate::annotations::Anchor::Line(_) => 0,
                    });
                let buffer_checked = anchor.and_then(|off| self.toggle_buffer_checkbox(off));
                if let Some(doc) = self.document_manager.active_document_mut() {
                    doc.annotations.update(ann_id, |a| {
                        let checked = match buffer_checked {
                            Some(c) => {
                                a.payload.set("checked", crate::annotations::Value::Bool(c));
                                c
                            }
                            None => {
                                crate::annotations::registry::toggle_checked(&mut a.payload);
                                a.payload
                                    .get("checked")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false)
                            }
                        };
                        // Keep any overlay glyph in sync with the new state.
                        if let Some(ad) = a.presentation.as_mut().and_then(|p| p.adornment.as_mut())
                        {
                            ad.text = if checked { "[x]".into() } else { "[ ]".into() };
                        }
                    });
                }
                let _ = self.update_and_render();
                true
            }
            Some(Handler::Builtin(Builtin::FollowLink)) => {
                let href = self
                    .document_manager
                    .active_document_mut()
                    .and_then(|doc| doc.annotations.get(ann_id))
                    .and_then(|a| a.payload.get("href").and_then(|v| v.as_str()))
                    .map(|s| s.to_string());
                if let Some(href) = href {
                    let _ = self.open_file(Some(href), false);
                    true
                } else {
                    false
                }
            }
            Some(Handler::Builtin(Builtin::OpenEntry)) => {
                // Descend into a directory entry or open a file entry, from the
                // active directory buffer's path + the fs.entry payload.
                let doc_id = self.active_document_id();
                let info = self.document_manager.active_document_mut().and_then(|doc| {
                    let dir = doc.directory_path()?.clone();
                    let ann = doc.annotations.get(ann_id)?;
                    let name = crate::annotations::payload::fs::name(&ann.payload)?.to_string();
                    let is_dir =
                        crate::annotations::payload::fs::is_dir(&ann.payload).unwrap_or(false);
                    Some((dir, name, is_dir))
                });
                let Some((dir, name, is_dir)) = info else {
                    return false;
                };
                let target = dir.join(&name);
                if is_dir {
                    self.reload_directory_buffer(doc_id, target);
                } else if let Err(e) = self.open_file(Some(target.display().to_string()), false) {
                    self.state.handle_error(e);
                } else {
                    self.state.clear_command_line();
                }
                let _ = self.force_full_redraw();
                true
            }
            Some(Handler::Lua) => {
                let ctx = {
                    let Some(doc) = self.document_manager.active_document_mut() else {
                        return false;
                    };
                    let position = doc.buffer.cursor();
                    let buffer = doc.id;
                    let Some(ann) = doc.annotations.get(ann_id) else {
                        return false;
                    };
                    let params = ann
                        .action_for_verb(&verb)
                        .map(|a| a.params.clone())
                        .unwrap_or(crate::annotations::Value::Null);
                    crate::plugin::AnnotationActionCtx {
                        annotation_id: ann_id,
                        kind: kind.as_str().to_string(),
                        verb,
                        payload: ann.payload.clone(),
                        params,
                        position,
                        buffer,
                    }
                };
                let ran = self.plugin_host.invoke_annotation_action(&ctx);
                if ran {
                    self.apply_plugin_mutations();
                    let _ = self.update_and_render();
                }
                ran
            }
            Some(Handler::Command(cmd)) => {
                if cmd.is_empty() {
                    return false;
                }
                self.execute_command_line(cmd);
                let _ = self.update_and_render();
                true
            }
            // Remote handler is reserved for IPC; nothing to do in-process.
            _ => false,
        }
    }

    /// Flip a literal "[ ]"/"[x]" at byte offset `byte_off` in place (length-
    /// preserving, so markers hold), returning the new state, or None if absent.
    fn toggle_buffer_checkbox(&mut self, byte_off: usize) -> Option<bool> {
        let doc = self.document_manager.active_document_mut()?;
        let c0 = doc.buffer.byte_to_char(byte_off);
        let open = doc.buffer.char_at(c0)?.to_char_lossy();
        let mid = doc.buffer.char_at(c0 + 1)?.to_char_lossy();
        let close = doc.buffer.char_at(c0 + 2)?.to_char_lossy();
        if open != '[' || close != ']' {
            return None;
        }
        let checked = match mid {
            ' ' => false,
            'x' | 'X' => true,
            _ => return None,
        };
        let new_ch = if checked { ' ' } else { 'x' };
        doc.replace_repeat(c0 + 1, 1, new_ch).ok()?;
        Some(!checked)
    }

    /// Detect cursor enter/leave transitions over annotations and fire the
    /// matching Lua hooks once per change. Called each frame.
    pub fn update_annotation_hover(&mut self) {
        // The annotation under the cursor (offset first, then line-anchored).
        let current = {
            let Some(doc) = self.document_manager.active_document() else {
                self.hovered_annotation = None;
                return;
            };
            let cursor = doc.buffer.cursor();
            let line = doc.buffer.line_index.get_line_at(cursor);
            let cursor_byte = doc.buffer.char_to_byte(cursor);
            doc.annotations
                .query_at(cursor_byte)
                .next()
                .or_else(|| {
                    doc.annotations
                        .iter()
                        .find(|a| a.visible && a.anchor == crate::annotations::Anchor::Line(line))
                })
                .map(|a| a.id)
        };
        if current == self.hovered_annotation {
            return;
        }
        let previous = self.hovered_annotation;
        self.hovered_annotation = current;

        // Build a hover ctx for an annotation id, if it still exists.
        let make_ctx = |s: &Self, id: u64| -> Option<crate::plugin::AnnotationHoverCtx> {
            let doc = s.document_manager.active_document()?;
            let ann = doc.annotations.get(id)?;
            Some(crate::plugin::AnnotationHoverCtx {
                annotation_id: id,
                kind: ann.kind.as_str().to_string(),
                payload: ann.payload.clone(),
                position: doc.buffer.cursor(),
                buffer: doc.id,
            })
        };

        let mut ran = false;
        if let Some(prev) = previous {
            if let Some(ctx) = make_ctx(self, prev) {
                ran |= self.plugin_host.invoke_annotation_hook(false, &ctx);
            }
        }
        if let Some(cur) = current {
            if let Some(ctx) = make_ctx(self, cur) {
                ran |= self.plugin_host.invoke_annotation_hook(true, &ctx);
            }
        }
        if ran {
            self.apply_plugin_mutations();
        }
    }

    /// Move the cursor to the start of the next interactive annotation.
    pub fn goto_next_interactive_annotation(&mut self) -> bool {
        self.jump_to_interactive(true)
    }

    /// Move the cursor to the start of the previous interactive annotation.
    pub fn goto_prev_interactive_annotation(&mut self) -> bool {
        self.jump_to_interactive(false)
    }

    /// In interface mode, snap the cursor to the next/prev actionable line, skipping
    /// inert ones. False (caller falls back to line motion) if there is none.
    pub fn snap_to_actionable_line(&mut self, forward: bool) -> bool {
        let Some(doc) = self.document_manager.active_document_mut() else {
            return false;
        };
        let cursor = doc.buffer.cursor();
        let cur_line = doc.buffer.line_index.get_line_at(cursor);
        let lines = doc.annotations.interactive_lines(|b| {
            doc.buffer
                .line_index
                .get_line_at(doc.buffer.byte_to_char(b))
        });
        let target = if forward {
            lines.into_iter().find(|&l| l > cur_line)
        } else {
            lines.into_iter().rev().find(|&l| l < cur_line)
        };
        let Some(line) = target else {
            return false;
        };
        if let Some(off) = doc.buffer.line_index.get_start(line) {
            let _ = doc.buffer.set_cursor(off);
            let _ = self.update_and_render();
            true
        } else {
            false
        }
    }

    fn jump_to_interactive(&mut self, forward: bool) -> bool {
        let target = {
            let Some(doc) = self.document_manager.active_document_mut() else {
                return false;
            };
            let cursor_byte = doc.buffer.char_to_byte(doc.buffer.cursor());
            let next = if forward {
                doc.annotations.next_interactive(cursor_byte)
            } else {
                doc.annotations.prev_interactive(cursor_byte)
            };
            next.and_then(|a| match a.anchor {
                crate::annotations::Anchor::Point(p) => Some(p.offset),
                crate::annotations::Anchor::Range(s, _) => Some(s.offset),
                crate::annotations::Anchor::Line(_) => None,
            })
            .map(|byte_offset| doc.buffer.byte_to_char(byte_offset))
        };
        if let Some(offset) = target {
            if let Some(doc) = self.document_manager.active_document_mut() {
                let _ = doc.buffer.set_cursor(offset);
            }
            let _ = self.update_and_render();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
#[path = "annotations_ops_tests.rs"]
mod annotations_ops_tests;
