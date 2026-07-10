//! Buffer population methods — rendering special buffer kinds into text.

use super::{BufferKind, DirEntry, DirectoryDiff, Document};
use crate::buffer::TextBuffer;
use crate::character::Character;
use std::collections::HashSet;

impl Document {
    /// Replace this document's buffer with new content, resetting cursor to the top.
    pub fn replace_buffer_content(&mut self, content: &str) {
        let old_revision = self.buffer.revision;
        if let Ok(mut new_buffer) = TextBuffer::new(content.len().max(64)) {
            let _ = new_buffer.insert_str(content);
            let _ = new_buffer.set_cursor(0);
            new_buffer.revision = old_revision + 1;
            self.buffer = new_buffer;
        }
    }

    /// Replace this document's buffer with a sequence of Characters.
    pub(super) fn replace_buffer_content_chars(&mut self, chars: &[Character]) {
        let old_revision = self.buffer.revision;
        let byte_len: usize = chars.iter().map(|c| c.len_utf8()).sum();
        if let Ok(mut new_buffer) = TextBuffer::new(byte_len.max(64)) {
            let _ = new_buffer.insert_chars(chars);
            let _ = new_buffer.set_cursor(0);
            new_buffer.revision = old_revision + 1;
            self.buffer = new_buffer;
        }
    }

    /// Populate (or repopulate) this directory buffer from a fresh directory listing.
    ///
    /// Entry IDs are stored in the annotation store rather than embedded as bytes in the
    /// buffer. The buffer contains only the visible filenames.
    pub fn populate_directory_buffer(&mut self, mut entries: Vec<DirEntry>) {
        use crate::color::Color;

        let (dir_path, show_hidden) = match &self.kind {
            BufferKind::Directory {
                path, show_hidden, ..
            } => (path.clone(), *show_hidden),
            _ => return,
        };

        let mut chars: Vec<Character> = Vec::new();
        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();
        let mut byte_offset = 0usize;

        {
            let start = byte_offset;
            for c in "../".chars() {
                chars.push(Character::from(c));
            }
            highlights.push((start..start + 3, Color::Blue));
            byte_offset += 3;
        }
        chars.push(Character::Newline);
        byte_offset += 1;

        self.annotations.clear();

        for (i, entry) in entries.iter_mut().enumerate() {
            entry.id = (i + 1) as u16;
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

            let start = byte_offset;
            for c in display.chars() {
                chars.push(Character::from(c));
                byte_offset += c.len_utf8();
            }
            highlights.push((start..byte_offset, color));

            chars.push(Character::Newline);
            byte_offset += 1;

            self.annotations
                .create_fs_entry(i + 1, entry.id, name, entry.is_dir);
        }
        if chars.last() == Some(&Character::Newline) {
            chars.pop();
        }

        self.replace_buffer_content_chars(&chars);
        self.custom_highlights = highlights;
        self.kind = BufferKind::Directory {
            path: dir_path,
            entries,
            show_hidden,
        };
        self.history.mark_saved();
    }

    /// Recompute `custom_highlights` from the current buffer state for directory buffers.
    ///
    /// Called before each render so that highlights stay accurate after user edits.
    pub fn recompute_directory_highlights(&mut self) {
        use crate::color::Color;

        if !matches!(&self.kind, BufferKind::Directory { .. }) {
            return;
        }

        let id_to_orig: std::collections::HashMap<u16, String> = match &self.kind {
            BufferKind::Directory { entries, .. } => entries
                .iter()
                .filter(|e| e.id != 0)
                .map(|e| {
                    let name = e
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    (e.id, name)
                })
                .collect(),
            _ => std::collections::HashMap::new(),
        };

        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();
        let mut byte_pos = 0usize;
        let mut line_idx = 0usize;
        let mut line_start = 0usize;
        let mut line_has_content = false;
        let mut last_visible_char = '\0';
        let mut line_text = String::new();

        for ch in self.buffer.iter_at(0) {
            let char_len = ch.len_utf8();
            match ch {
                Character::Newline => {
                    if line_has_content {
                        let color = dir_entry_color(
                            &id_to_orig,
                            self.annotations.directory_entry_id_at_line(line_idx),
                            &line_text,
                            last_visible_char,
                        );
                        highlights.push((line_start..byte_pos, color));
                    }
                    line_start = byte_pos + 1;
                    line_has_content = false;
                    last_visible_char = '\0';
                    line_text.clear();
                    line_idx += 1;
                }
                c => {
                    if !line_has_content {
                        line_start = byte_pos;
                        line_has_content = true;
                    }
                    let ch = c.to_char_lossy();
                    last_visible_char = ch;
                    line_text.push(ch);
                }
            }
            byte_pos += char_len;
        }

        // Last line (no trailing newline)
        if line_has_content {
            let color = dir_entry_color(
                &id_to_orig,
                self.annotations.directory_entry_id_at_line(line_idx),
                &line_text,
                last_visible_char,
            );
            highlights.push((line_start..byte_pos, color));
        }

        self.custom_highlights = highlights;
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
    pub fn populate_clipboard_buffer(
        &mut self,
        entries: &std::collections::VecDeque<Vec<crate::character::Character>>,
    ) {
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

    /// Populate (or repopulate) the `gv` regions list from `regions`,
    /// computed against `source_buf` (the document the set belongs to).
    pub fn populate_regions_buffer(
        &mut self,
        source_buf: &TextBuffer,
        regions: &[crate::selection::Region],
    ) {
        use crate::buffer::api::BufferView;

        let mut content = String::new();
        if regions.is_empty() {
            content.push_str("(empty)");
        } else {
            for (i, region) in regions.iter().enumerate() {
                let (start, end) = region.buffer_span(source_buf);
                let row = source_buf.line_index.get_line_at(start);
                let line_start = source_buf.line_index.get_start(row).unwrap_or(0);
                let col = start.saturating_sub(line_start);
                let raw: String = source_buf
                    .chars(start..end)
                    .map(|c| c.to_char_lossy())
                    .collect();
                let raw = raw.replace('\n', "\u{23ce}");
                let preview: String = if raw.chars().count() > 48 {
                    raw.chars().take(45).chain("...".chars()).collect()
                } else {
                    raw
                };
                content.push_str(&format!("{}: {}:{} \"{}\"\n", i + 1, row, col, preview));
            }
            if content.ends_with('\n') {
                content.pop();
            }
        }
        self.replace_buffer_content(&content);
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
    ///
    /// Queries the annotation store for each line to find the entry ID, then compares
    /// the visible buffer text against the original entry path to detect renames,
    /// deletes, and creates.
    pub fn parse_directory_diff(&self) -> DirectoryDiff {
        let (entries, dir_path) = match &self.kind {
            BufferKind::Directory { entries, path, .. } => (entries, path),
            _ => return DirectoryDiff::default(),
        };

        let id_map: std::collections::HashMap<u16, &DirEntry> = entries
            .iter()
            .filter(|e| e.id != 0)
            .map(|e| (e.id, e))
            .collect();

        let total_lines = self.buffer.get_total_lines();
        let mut renames: Vec<(std::path::PathBuf, String)> = Vec::new();
        let mut creates: Vec<String> = Vec::new();
        let mut seen_ids: HashSet<u16> = HashSet::new();

        for line_idx in 0..total_lines {
            let line_start = self.buffer.line_index.get_start(line_idx).unwrap_or(0);
            let line_end = self
                .buffer
                .line_index
                .get_end(line_idx, self.buffer.len())
                .unwrap_or(self.buffer.len());

            // Collect the visible text of this line (no annotation bytes to strip).
            let line_text: String = self
                .buffer
                .iter_at(line_start)
                .take(line_end - line_start)
                .filter_map(|c| {
                    if c == Character::Newline {
                        None
                    } else {
                        Some(c.to_char_lossy())
                    }
                })
                .collect();

            // Look up the annotation for this line in the store.
            let annotation_entry_id = self.annotations.directory_entry_id_at_line(line_idx);

            if let Some(entry_id) = annotation_entry_id {
                // Line has a known annotation. entry_id=0 is the "no-id" sentinel -> skip silently.
                if entry_id == 0 {
                    continue;
                }

                // Primary entry name: visible line content with trailing slash and whitespace stripped.
                let primary_name = line_text.trim_end_matches('/').trim().to_string();

                // A blank annotated line means the user erased the entry — treat as deleted.
                if primary_name.is_empty() {
                    continue;
                }

                seen_ids.insert(entry_id);

                if let Some(entry) = id_map.get(&entry_id) {
                    let orig_name = entry
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    if entry.is_dir && primary_name.contains('/') {
                        creates.push(line_text.trim().to_string());
                    } else if primary_name != orig_name {
                        renames.push((entry.path.clone(), primary_name));
                    }
                }
            } else {
                let trimmed = line_text.trim();
                if !trimmed.is_empty() && trimmed != "../" {
                    creates.push(trimmed.to_string());
                }
            }
        }

        let protected_dirs: HashSet<std::path::PathBuf> = renames
            .iter()
            .filter_map(|(_, new_name)| {
                let p = std::path::Path::new(new_name.as_str());
                p.parent()
                    .filter(|parent| *parent != std::path::Path::new(""))
                    .map(|parent| dir_path.join(parent))
            })
            .collect();

        let deletes: Vec<std::path::PathBuf> = entries
            .iter()
            .filter(|e| e.id != 0 && !seen_ids.contains(&e.id))
            .map(|e| e.path.clone())
            .filter(|p| !protected_dirs.contains(p))
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
        self.sync_terminal_buffer();
    }

    /// Re-read the terminal emulator's current visible grid into the document buffer.
    pub fn sync_terminal_buffer(&mut self) {
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
        }
    }
}

/// Determine the highlight color for one directory buffer line.
fn dir_entry_color(
    id_to_orig: &std::collections::HashMap<u16, String>,
    annotation_entry_id: Option<u16>,
    line_text: &str,
    last_visible_char: char,
) -> crate::color::Color {
    use crate::color::Color;

    let trimmed = line_text.trim_end_matches('/').trim();

    if trimmed == ".." {
        return Color::Blue;
    }

    match annotation_entry_id {
        None => Color::Green,
        Some(eid) => {
            let orig = id_to_orig.get(&eid).map(|s| s.as_str()).unwrap_or("");
            if trimmed == orig {
                if last_visible_char == '/' {
                    Color::Blue
                } else {
                    Color::White
                }
            } else {
                Color::Yellow
            }
        }
    }
}
