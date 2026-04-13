//! Buffer population methods — rendering special buffer kinds into text.

use super::{BufferKind, DirEntry, DirectoryDiff, Document};
use crate::buffer::TextBuffer;
use std::collections::HashSet;

impl Document {
    /// Replace this document's buffer with new content, resetting cursor to the top.
    pub(super) fn replace_buffer_content(&mut self, content: &str) {
        let old_revision = self.buffer.revision;
        if let Ok(mut new_buffer) = TextBuffer::new(content.len().max(64)) {
            let _ = new_buffer.insert_str(content);
            let _ = new_buffer.set_cursor(0);
            new_buffer.revision = old_revision + 1;
            self.buffer = new_buffer;
        }
    }

    /// Populate (or repopulate) this directory buffer from a fresh directory listing.
    pub fn populate_directory_buffer(&mut self, entries: Vec<DirEntry>) {
        use crate::color::Color;

        let (dir_path, show_hidden) = match &self.kind {
            BufferKind::Directory {
                path, show_hidden, ..
            } => (path.clone(), *show_hidden),
            _ => return,
        };

        let mut content = String::new();
        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();

        let push_colored =
            |content: &mut String, highlights: &mut Vec<_>, s: &str, color: Color| {
                let start = content.len();
                content.push_str(s);
                highlights.push((start..content.len(), color));
            };

        push_colored(&mut content, &mut highlights, "../", Color::Blue);
        content.push('\n');

        for entry in &entries {
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let (display, color) = if entry.is_dir {
                (format!("{}/", name), Color::Blue)
            } else {
                (name.to_string(), Color::White)
            };
            push_colored(&mut content, &mut highlights, &display, color);
            content.push('\n');
        }
        if content.ends_with('\n') {
            content.pop();
        }

        self.replace_buffer_content(&content);
        self.custom_highlights = highlights;
        self.kind = BufferKind::Directory {
            path: dir_path,
            entries,
            show_hidden,
        };
        self.history.mark_saved();
    }

    /// Populate this undo-tree buffer from the given history.
    pub fn populate_undotree_buffer(
        &mut self,
        text: String,
        sequences: Vec<crate::history::EditSeq>,
        highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    ) {
        let linked_doc_id = match self.kind {
            BufferKind::UndoTree { linked_doc_id, .. } => linked_doc_id,
            _ => return,
        };

        self.replace_buffer_content(&text);
        self.custom_highlights = highlights;
        self.kind = BufferKind::UndoTree {
            linked_doc_id,
            sequences,
        };
        self.history.mark_saved();
    }

    /// Populate this messages buffer from the notification log.
    pub fn populate_messages_buffer(&mut self, log: &[crate::notification::MessageEntry]) {
        use crate::color::Color;
        use crate::notification::{JobEventKind, MessageEntry, NotificationType};

        let show_all = match self.kind {
            BufferKind::Messages { show_all } => show_all,
            _ => return,
        };

        let mut content = String::new();
        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();

        let push_colored =
            |content: &mut String, highlights: &mut Vec<_>, s: &str, color: Color| {
                let start = content.len();
                content.push_str(s);
                highlights.push((start..content.len(), color));
            };

        for entry in log {
            let include = match entry {
                MessageEntry::Notification { .. } => true,
                MessageEntry::JobEvent { silent, .. } => show_all || !silent,
            };
            if !include {
                continue;
            }

            let time = entry.time();
            let secs = time
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let h = secs / 3600 % 24;
            let m = secs / 60 % 60;
            let s = secs % 60;
            let time_str = format!("[{h:02}:{m:02}:{s:02}]");

            match entry {
                MessageEntry::Notification { kind, message, .. } => {
                    let (kind_str, color) = match kind {
                        NotificationType::Info => ("[info]   ", Color::Cyan),
                        NotificationType::Warning => ("[warn]   ", Color::Yellow),
                        NotificationType::Error => ("[error]  ", Color::Red),
                        NotificationType::Success => ("[ok]     ", Color::Green),
                    };
                    push_colored(&mut content, &mut highlights, &time_str, Color::Grey);
                    content.push(' ');
                    push_colored(&mut content, &mut highlights, kind_str, color);
                    content.push(' ');
                    content.push_str(message);
                    content.push('\n');
                }
                MessageEntry::JobEvent {
                    job_id,
                    kind,
                    message,
                    ..
                } => {
                    let (kind_str, color) = match kind {
                        JobEventKind::Started => ("[job:start]  ", Color::DarkCyan),
                        JobEventKind::Progress(_) => ("[job:progress]", Color::DarkCyan),
                        JobEventKind::Finished => ("[job:done]   ", Color::DarkGreen),
                        JobEventKind::Error => ("[job:error]  ", Color::Red),
                        JobEventKind::Cancelled => ("[job:cancel] ", Color::DarkYellow),
                    };
                    push_colored(&mut content, &mut highlights, &time_str, Color::Grey);
                    content.push(' ');
                    push_colored(&mut content, &mut highlights, kind_str, color);
                    content.push_str(&format!(" #{job_id} "));
                    content.push_str(message);
                    content.push('\n');
                }
            }
        }

        if content.ends_with('\n') {
            content.pop();
        }
        if content.is_empty() {
            content = "(no messages)".to_string();
        }

        let old_revision = self.buffer.revision;
        if let Ok(mut new_buffer) = TextBuffer::new(content.len().max(64)) {
            let _ = new_buffer.insert_str(&content);
            new_buffer.revision = old_revision + 1;
            self.buffer = new_buffer;
        }
        self.custom_highlights = highlights;
        self.history.mark_saved();
    }

    /// Populate (or repopulate) this clipboard index buffer from the ring.
    pub fn populate_clipboard_buffer(&mut self, entries: &std::collections::VecDeque<String>) {
        use crate::color::Color;

        let mut content = String::new();
        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();

        if entries.is_empty() {
            content.push_str("(empty)");
        } else {
            for (i, _) in entries.iter().enumerate() {
                let label = format!("[{i}]");
                let start = content.len();
                content.push_str(&label);
                highlights.push((start..content.len(), Color::Cyan));
                content.push('\n');
            }
            if content.ends_with('\n') {
                content.pop();
            }
        }

        self.replace_buffer_content(&content);
        self.custom_highlights = highlights;
        self.kind = BufferKind::Clipboard {
            entries: entries.iter().cloned().collect(),
        };
        self.history.mark_saved();
    }

    /// Parse the current buffer content of a clipboard index buffer and return the
    /// ordered list of original entry indices.
    pub fn parse_clipboard_order(&self) -> Vec<usize> {
        let entries_len = match &self.kind {
            BufferKind::Clipboard { entries } => entries.len(),
            _ => return vec![],
        };

        let content = self.buffer.to_string();
        let mut order = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if let Some(inner) = line.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                if let Ok(idx) = inner.parse::<usize>() {
                    if idx < entries_len {
                        order.push(idx);
                    }
                }
            }
        }

        order
    }

    /// Parse the current buffer content of a directory buffer and produce a diff.
    pub fn parse_directory_diff(&self) -> DirectoryDiff {
        let entries = match &self.kind {
            BufferKind::Directory { entries, .. } => entries,
            _ => {
                return DirectoryDiff {
                    renames: vec![],
                    deletes: vec![],
                    creates: vec![],
                }
            }
        };

        let content = self.buffer.to_string();

        let buffer_lines: Vec<&str> = content
            .lines()
            .filter(|l| *l != "../" && !l.trim().is_empty())
            .collect();

        let entry_name_set: HashSet<&str> = entries
            .iter()
            .filter_map(|e| e.path.file_name().and_then(|n| n.to_str()))
            .collect();

        let buffer_name_set: HashSet<&str> = buffer_lines
            .iter()
            .map(|l| l.trim_end_matches('/'))
            .collect();

        let unmatched_new: Vec<&str> = buffer_lines
            .iter()
            .filter(|l| !entry_name_set.contains(l.trim_end_matches('/')))
            .copied()
            .collect();

        let unmatched_old: Vec<&DirEntry> = entries
            .iter()
            .filter(|e| {
                let name = e.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                !buffer_name_set.contains(name)
            })
            .collect();

        let pair_count = unmatched_old.len().min(unmatched_new.len());

        let renames = (0..pair_count)
            .map(|i| {
                (
                    unmatched_old[i].path.clone(),
                    unmatched_new[i].trim_end_matches('/').to_string(),
                )
            })
            .collect();

        let deletes = unmatched_old[pair_count..]
            .iter()
            .map(|e| e.path.clone())
            .collect();

        let creates = unmatched_new[pair_count..]
            .iter()
            .map(|n| n.to_string())
            .collect();

        DirectoryDiff {
            renames,
            deletes,
            creates,
        }
    }

    /// Return the current directory path if this is a Directory buffer.
    pub fn directory_path(&self) -> Option<&std::path::PathBuf> {
        match &self.kind {
            BufferKind::Directory { path, .. } => Some(path),
            _ => None,
        }
    }

    /// Update terminal buffer content from the emulator's screen.
    pub fn handle_terminal_data(&mut self, _data: &[u8]) {
        let (content, cursor_line, cursor_col, cell_colors) = if let Some(terminal) = &self.terminal
        {
            terminal.read_screen()
        } else {
            return;
        };

        let old_revision = self.buffer.revision;
        if let Ok(mut new_buffer) = TextBuffer::new(content.len().max(64)) {
            let _ = new_buffer.insert_str(&content);

            let total_lines = new_buffer.get_total_lines();
            if cursor_line < total_lines {
                let start = new_buffer.line_index.get_start(cursor_line).unwrap_or(0);
                let line_end = new_buffer
                    .line_index
                    .get_end(cursor_line, new_buffer.len())
                    .unwrap_or(start);
                let target = start + cursor_col;
                let pos = target.min(line_end);
                let _ = new_buffer.set_cursor(pos);
            }

            new_buffer.revision = old_revision + 1;
            self.buffer = new_buffer;
            self.terminal_cursor = Some((cursor_line, cursor_col));
            self.terminal_cell_colors = cell_colors;
            self.mark_dirty();
        }
    }
}
