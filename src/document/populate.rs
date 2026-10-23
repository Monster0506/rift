use super::{
    DirEntry, DirectoryDiff, Document, BUFFER_LIST_STATE_KEY, CLIPBOARD_STATE_KEY,
    DIRECTORY_STATE_KEY, GIT_BLAME_STATE_KEY, GIT_REBASE_TODO_STATE_KEY, GIT_STATUS_STATE_KEY,
    MESSAGES_STATE_KEY, TERMINAL_STATE_KEY, UNDO_FILE_VIEW_STATE_KEY, UNDO_TREE_STATE_KEY,
};
use crate::buffer::TextBuffer;
use crate::character::Character;
use std::collections::HashSet;
use std::fmt::Write as _;

struct RebasePlanRender {
    text: String,
    highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    head_lines: Vec<(usize, String)>,
}

enum CursorTarget {
    HunkLine {
        path: String,
        staged_side: bool,
        hunk_index: usize,
        line_index: usize,
    },
    HunkHeader {
        path: String,
        staged_side: bool,
        hunk_index: usize,
    },
    Entry {
        path: String,
    },
}

#[derive(Default)]
struct GitStatusRender {
    chars: Vec<Character>,
    highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    byte_offset: usize,
    line_idx: usize,
    exact_restore_line: Option<usize>,
    hunk_restore_line: Option<usize>,
    entry_restore_line: Option<usize>,
}

impl GitStatusRender {
    fn push_line(&mut self, text: &str) -> std::ops::Range<usize> {
        let start = self.byte_offset;
        for c in text.chars() {
            self.chars.push(Character::from(c));
            self.byte_offset += c.len_utf8();
        }
        let end = self.byte_offset;
        self.chars.push(Character::Newline);
        self.byte_offset += 1;
        start..end
    }
}

struct GitStatusView<'a> {
    snapshot: &'a crate::git::status::StatusSnapshot,
    expanded:
        &'a std::collections::HashMap<(std::path::PathBuf, bool), Vec<crate::git::diff::Hunk>>,
    cursor: Option<&'a CursorTarget>,
}

impl Document {
    pub(crate) fn replace_buffer_content(&mut self, content: &str) {
        let old_revision = self.buffer.revision;
        if let Ok(mut new_buffer) = TextBuffer::new(content.len().max(64)) {
            let _ = new_buffer.insert_str(content);
            let _ = new_buffer.set_cursor(0);
            new_buffer.revision = old_revision + 1;
            self.buffer = new_buffer;
        }
    }

    pub(super) fn replace_buffer_content_chars(&mut self, chars: &[Character]) {
        let old_revision = self.buffer.revision;
        let byte_len: usize = chars
            .iter()
            .copied()
            .map(super::super::character::Character::len_utf8)
            .sum();
        if let Ok(mut new_buffer) = TextBuffer::new(byte_len.max(64)) {
            new_buffer.insert_chars(chars);
            let _ = new_buffer.set_cursor(0);
            new_buffer.revision = old_revision + 1;
            self.buffer = new_buffer;
        }
    }

    pub(crate) fn populate_directory_buffer(&mut self, mut entries: Vec<DirEntry>) {
        use crate::color::Color;

        let Some(state) = self.state.try_get(DIRECTORY_STATE_KEY) else {
            return;
        };
        let dir_path = state.path.clone();
        let show_hidden = state.show_hidden;

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
            let id = u16::try_from(i + 1).unwrap_or(u16::MAX);
            entry.id = id;
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let (display, color) = if entry.is_dir {
                (format!("{name}/"), Color::Blue)
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
        let Some(state) = self.state.try_get_mut(DIRECTORY_STATE_KEY) else {
            return;
        };
        state.path = dir_path;
        state.entries = entries;
        state.show_hidden = show_hidden;
        self.history.mark_saved();
    }

    pub(crate) fn recompute_directory_highlights(&mut self) {
        use crate::color::Color;

        let Some(state) = self.state.try_get(DIRECTORY_STATE_KEY) else {
            return;
        };
        let id_to_orig: std::collections::HashMap<u16, String> = state
            .entries
            .iter()
            .filter(|entry| entry.id != 0)
            .map(|entry| {
                let name = entry
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_string();
                (entry.id, name)
            })
            .collect();

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

    pub(crate) fn populate_undotree_buffer(
        &mut self,
        text: &str,
        sequences: Vec<crate::history::EditSeq>,
        highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    ) {
        let Some(state) = self.state.try_get_mut(UNDO_TREE_STATE_KEY) else {
            return;
        };
        state.sequences = sequences;

        self.replace_buffer_content(text);
        self.custom_highlights = highlights;
        self.history.mark_saved();
    }

    pub(crate) fn populate_undo_file_view(
        &mut self,
        parsed: &crate::history::persist::ParsedUndoFile,
    ) {
        let mut hash_hex = String::new();
        for b in &parsed.content_hash {
            let _ = write!(hash_hex, "{b:02x}");
        }
        let header = format!(
            "Undo history for {}\nSHA-256 {}\n{} edit(s), currently at #{}\n\n",
            parsed.source_path.display(),
            hash_hex,
            parsed.tree.nodes.len().saturating_sub(1),
            parsed.tree.current_seq(),
        );
        let (tree_text, sequences, highlights) =
            crate::undotree_view::render_tree_to_text(&parsed.tree);
        let offset = header.len();
        let mut content = header;
        content.push_str(&tree_text);

        let highlights = highlights
            .into_iter()
            .map(|(range, color)| (range.start + offset..range.end + offset, color))
            .collect();

        self.replace_buffer_content(&content);
        self.custom_highlights = highlights;
        if let Some(state) = self.state.try_get_mut(UNDO_FILE_VIEW_STATE_KEY) {
            state.sequences = sequences;
        }
        self.history.mark_saved();
    }

    pub(crate) fn populate_messages_buffer(&mut self, log: &[crate::notification::MessageEntry]) {
        use crate::color::Color;
        use crate::notification::{JobEventKind, MessageEntry, NotificationType};

        let Some(show_all) = self
            .state
            .try_get(MESSAGES_STATE_KEY)
            .map(|state| state.show_all)
        else {
            return;
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
                .duration_since(crate::time::UNIX_EPOCH)
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
                    let _ = write!(content, " #{job_id} ");
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

    pub(crate) fn populate_clipboard_buffer(
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
        let Some(state) = self.state.try_get_mut(CLIPBOARD_STATE_KEY) else {
            return;
        };
        state.entries = entries.iter().cloned().collect();
        self.history.mark_saved();
    }

    pub(crate) fn populate_buffer_list_buffer(
        &mut self,
        infos: &[crate::document::manager::BufferInfo],
    ) {
        use crate::color::Color;

        let mut content = String::new();
        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();
        let mut entries: Vec<super::DocumentId> = Vec::with_capacity(infos.len());
        self.annotations.clear();

        if infos.is_empty() {
            content.push_str("(no buffers)");
        } else {
            for (i, info) in infos.iter().enumerate() {
                let current = if info.is_current { "%" } else { " " };
                let dirty = if info.status.is_dirty { "+" } else { " " };
                let read_only = if info.status.is_read_only { "R" } else { " " };
                let special = if info.status.is_special { "~" } else { " " };
                let start = content.len();
                let _ = write!(
                    content,
                    "[{}] {}: {current}{dirty}{read_only}{special}",
                    info.index + 1,
                    info.name,
                );
                let color = if info.is_current {
                    Color::Cyan
                } else if info.status.is_dirty {
                    Color::Yellow
                } else {
                    Color::White
                };
                highlights.push((start..content.len(), color));
                content.push('\n');
                entries.push(info.id);
                self.annotations.create_buffer_entry(i, info.id);
            }
            if content.ends_with('\n') {
                content.pop();
            }
        }

        self.replace_buffer_content(&content);
        self.custom_highlights = highlights;
        let Some(state) = self.state.try_get_mut(BUFFER_LIST_STATE_KEY) else {
            return;
        };
        state.entries = entries;
        self.history.mark_saved();
    }

    pub(crate) fn populate_regions_buffer(
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
                    .map(super::super::character::Character::to_char_lossy)
                    .collect();
                let raw = raw.replace('\n', "\u{23ce}");
                let preview: String = if raw.chars().count() > 48 {
                    raw.chars().take(45).chain("...".chars()).collect()
                } else {
                    raw
                };
                let _ = writeln!(content, "{}: {}:{} \"{}\"", i + 1, row, col, preview);
            }
            if content.ends_with('\n') {
                content.pop();
            }
        }
        self.replace_buffer_content(&content);
        self.history.mark_saved();
    }

    pub(crate) fn parse_clipboard_order(&self) -> Vec<usize> {
        let Some(entries_len) = self
            .state
            .try_get(CLIPBOARD_STATE_KEY)
            .map(|state| state.entries.len())
        else {
            return Vec::new();
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

    pub(crate) fn parse_directory_diff(&self) -> DirectoryDiff {
        let Some(state) = self.state.try_get(DIRECTORY_STATE_KEY) else {
            return DirectoryDiff::default();
        };
        let entries = &state.entries;
        let dir_path = &state.path;

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

            let annotation_entry_id = self.annotations.directory_entry_id_at_line(line_idx);

            if let Some(entry_id) = annotation_entry_id {
                if entry_id == 0 {
                    continue;
                }

                let primary_name = line_text.trim_end_matches('/').trim().to_string();

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

    pub(crate) fn handle_terminal_data(&mut self, _data: &[u8]) {
        self.sync_terminal_buffer();
    }

    pub(crate) fn sync_terminal_buffer(&mut self) {
        let (content, cursor_line, cursor_col, cell_colors) = match self.terminal() {
            Some(terminal) => terminal.read_screen(),
            None => return,
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
            if let Some(state) = self.state.try_get_mut(TERMINAL_STATE_KEY) {
                state.terminal_cursor = Some((cursor_line, cursor_col));
                state.terminal_cell_colors = cell_colors;
            }
        }
    }

    pub(crate) fn populate_git_status_buffer(
        &mut self,
        snapshot: crate::git::status::StatusSnapshot,
        head_subject: Option<String>,
    ) {
        let Some(state) = self.state.try_get_mut(GIT_STATUS_STATE_KEY) else {
            return;
        };
        state.snapshot = snapshot;
        state.expanded_diffs.clear();
        state.head_subject = head_subject;
        self.render_git_status();
        self.history.mark_saved();
    }

    pub(crate) fn set_git_status_expanded(
        &mut self,
        path: std::path::PathBuf,
        staged_side: bool,
        hunks: Vec<crate::git::diff::Hunk>,
    ) {
        let Some(state) = self.state.try_get_mut(GIT_STATUS_STATE_KEY) else {
            return;
        };
        state.expanded_diffs.insert((path, staged_side), hunks);
        self.render_git_status();
    }

    pub(crate) fn collapse_git_status_entry(&mut self, path: &std::path::Path, staged_side: bool) {
        let Some(state) = self.state.try_get_mut(GIT_STATUS_STATE_KEY) else {
            return;
        };
        state
            .expanded_diffs
            .remove(&(path.to_path_buf(), staged_side));
        self.render_git_status();
    }

    pub(crate) fn is_git_status_expanded(&self, path: &std::path::Path, staged_side: bool) -> bool {
        self.state
            .try_get(GIT_STATUS_STATE_KEY)
            .is_some_and(|state| {
                state
                    .expanded_diffs
                    .contains_key(&(path.to_path_buf(), staged_side))
            })
    }

    pub(crate) fn is_git_status_entry_untracked(&self, path: &std::path::Path) -> bool {
        self.state
            .try_get(GIT_STATUS_STATE_KEY)
            .is_some_and(|state| {
                state
                    .snapshot
                    .entries
                    .iter()
                    .any(|entry| entry.path == path && entry.is_untracked())
            })
    }

    fn render_git_status(&mut self) {
        let Some(state) = self.state.try_get(GIT_STATUS_STATE_KEY) else {
            return;
        };
        let snapshot = state.snapshot.clone();
        let expanded_diffs = state.expanded_diffs.clone();
        let head_subject = state.head_subject.clone();

        let cursor_target = self.git_status_cursor_target();
        self.annotations.clear();

        let view = GitStatusView {
            snapshot: &snapshot,
            expanded: &expanded_diffs,
            cursor: cursor_target.as_ref(),
        };
        let mut r = GitStatusRender::default();
        self.push_git_status_header(&mut r, &snapshot, head_subject.as_deref());
        self.push_git_status_sections(&mut r, &view);

        if r.chars.last() == Some(&Character::Newline) {
            r.chars.pop();
        }

        self.replace_buffer_content_chars(&r.chars);
        self.custom_highlights = r.highlights;
        let restore_line = r
            .exact_restore_line
            .or(r.hunk_restore_line)
            .or(r.entry_restore_line);
        if let Some(line) = restore_line {
            if let Some(offset) = self.buffer.line_index.get_start(line) {
                let _ = self.buffer.set_cursor(offset);
            }
        }
    }

    fn git_status_cursor_target(&self) -> Option<CursorTarget> {
        let cursor = self.buffer.cursor();
        let line = self.buffer.line_index.get_line_at(cursor);
        if let Some((path, staged_side, hunk_index, line_index)) =
            self.annotations.git_hunk_line_at_line(line)
        {
            Some(CursorTarget::HunkLine {
                path,
                staged_side,
                hunk_index,
                line_index,
            })
        } else if let Some((path, staged_side, hunk_index)) =
            self.annotations.git_hunk_at_line(line)
        {
            Some(CursorTarget::HunkHeader {
                path,
                staged_side,
                hunk_index,
            })
        } else {
            self.annotations
                .git_status_entry_at_line(line)
                .map(|(path, ..)| CursorTarget::Entry { path })
        }
    }

    fn push_git_status_header(
        &mut self,
        r: &mut GitStatusRender,
        snapshot: &crate::git::status::StatusSnapshot,
        head_subject: Option<&str>,
    ) {
        use crate::color::Color;

        let branch_line = snapshot.branch.head.as_ref().map_or_else(
            || {
                snapshot.branch.oid.as_ref().map_or_else(
                    || "On an unborn branch".to_string(),
                    |oid| format!("HEAD detached at {}", oid.get(..12).unwrap_or(oid.as_str())),
                )
            },
            |name| format!("On branch {name}"),
        );
        let range = r.push_line(&branch_line);
        r.highlights.push((range, Color::Cyan));
        r.line_idx += 1;

        if let (Some(oid), Some(subject)) = (&snapshot.branch.oid, head_subject) {
            let head_line = format!("HEAD  {} {subject}", oid.get(..8).unwrap_or(oid.as_str()));
            let range = r.push_line(&head_line);
            r.highlights.push((range, Color::Yellow));
            self.annotations.create_git_status_head(r.line_idx);
            r.line_idx += 1;
        }

        if let Some(upstream) = &snapshot.branch.upstream {
            let (ahead, behind) = (snapshot.branch.ahead, snapshot.branch.behind);
            let msg = if ahead > 0 && behind > 0 {
                Some(format!(
                    "Your branch and '{upstream}' have diverged (ahead {ahead}, behind {behind})"
                ))
            } else if ahead > 0 {
                Some(format!(
                    "Your branch is ahead of '{upstream}' by {ahead} commit(s)"
                ))
            } else if behind > 0 {
                Some(format!(
                    "Your branch is behind '{upstream}' by {behind} commit(s)"
                ))
            } else {
                None
            };
            if let Some(msg) = msg {
                r.push_line(&msg);
                r.line_idx += 1;
            }
        }
        r.push_line("");
        r.line_idx += 1;
    }

    fn push_git_status_sections(&mut self, r: &mut GitStatusRender, view: &GitStatusView<'_>) {
        use crate::color::Color;

        let sections: [(&str, Color); 4] = [
            (git_status_sections::UNMERGED, Color::Magenta),
            (git_status_sections::STAGED, Color::Green),
            (git_status_sections::UNSTAGED, Color::Yellow),
            (git_status_sections::UNTRACKED, Color::Red),
        ];

        for (section, color) in sections {
            let entries: Vec<&crate::git::status::StatusEntry> = view
                .snapshot
                .entries
                .iter()
                .filter(|e| git_status_entry_matches_section(e, section))
                .collect();
            if entries.is_empty() {
                continue;
            }

            let header = git_status_sections::header_text(section, entries.len());
            let range = r.push_line(&header);
            r.highlights.push((range, Color::DarkGrey));
            r.line_idx += 1;

            for entry in entries {
                self.push_git_status_entry(r, view, entry, section, color);
            }

            r.push_line("");
            r.line_idx += 1;
        }
    }

    fn push_git_status_entry(
        &mut self,
        r: &mut GitStatusRender,
        view: &GitStatusView<'_>,
        entry: &crate::git::status::StatusEntry,
        section: &str,
        color: crate::color::Color,
    ) {
        let path_str = entry.path.to_string_lossy().to_string();
        let orig_str = entry
            .orig_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let display = orig_str
            .as_ref()
            .map_or_else(|| path_str.clone(), |orig| format!("{orig} -> {path_str}"));
        let code = git_status_entry_code(entry, section);
        let line_text = if code.is_empty() {
            format!("  {display}")
        } else {
            format!("  {code} {display}")
        };
        let range = r.push_line(&line_text);
        r.highlights.push((range, color));
        let this_line = r.line_idx;
        r.line_idx += 1;

        self.annotations.create_git_status_entry(
            this_line,
            &path_str,
            section,
            orig_str.as_deref(),
        );
        let target_path = view.cursor.map(|c| match c {
            CursorTarget::HunkLine { path, .. }
            | CursorTarget::HunkHeader { path, .. }
            | CursorTarget::Entry { path } => path.as_str(),
        });
        if target_path == Some(path_str.as_str()) {
            r.entry_restore_line = Some(this_line);
        }

        let staged_side = section == git_status_sections::STAGED;
        if let Some(hunks) = view.expanded.get(&(entry.path.clone(), staged_side)) {
            for (hunk_index, hunk) in hunks.iter().enumerate() {
                self.push_git_status_hunk(r, view, &path_str, staged_side, hunk_index, hunk);
            }
        }
    }

    fn push_git_status_hunk(
        &mut self,
        r: &mut GitStatusRender,
        view: &GitStatusView<'_>,
        path_str: &str,
        staged_side: bool,
        hunk_index: usize,
        hunk: &crate::git::diff::Hunk,
    ) {
        use crate::color::Color;

        let header_text = if hunk.header.is_empty() {
            format!(
                "      @@ -{},{} +{},{} @@",
                hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
            )
        } else {
            format!(
                "      @@ -{},{} +{},{} @@ {}",
                hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines, hunk.header
            )
        };
        let range = r.push_line(&header_text);
        r.highlights.push((range, Color::Cyan));
        let hunk_line_idx = r.line_idx;
        r.line_idx += 1;
        self.annotations
            .create_git_hunk(hunk_line_idx, path_str, staged_side, hunk_index);

        let same_hunk = matches!(
            view.cursor,
            Some(
                CursorTarget::HunkLine { path, staged_side: s, hunk_index: hi, .. }
                | CursorTarget::HunkHeader { path, staged_side: s, hunk_index: hi },
            ) if path == path_str && *s == staged_side && *hi == hunk_index
        );
        if same_hunk {
            r.hunk_restore_line = Some(hunk_line_idx);
        }
        let is_exact_header_target = matches!(
            view.cursor,
            Some(CursorTarget::HunkHeader { path, staged_side: s, hunk_index: hi })
            if path == path_str && *s == staged_side && *hi == hunk_index
        );
        if is_exact_header_target {
            r.exact_restore_line = Some(hunk_line_idx);
        }

        self.push_git_hunk_lines(r, view, path_str, staged_side, hunk_index, &hunk.lines);
    }

    fn push_git_hunk_lines(
        &mut self,
        r: &mut GitStatusRender,
        view: &GitStatusView<'_>,
        path_str: &str,
        staged_side: bool,
        hunk_index: usize,
        lines: &[crate::git::diff::DiffLine],
    ) {
        use crate::color::Color;

        for (dl_index, dl) in lines.iter().enumerate() {
            let (marker, color) = match dl.kind {
                crate::git::diff::DiffLineKind::Context => (' ', None),
                crate::git::diff::DiffLineKind::Addition => ('+', Some(Color::Green)),
                crate::git::diff::DiffLineKind::Deletion => ('-', Some(Color::Red)),
            };
            let text = format!("        {marker}{}", dl.content);
            let range = r.push_line(&text);
            if let Some(color) = color {
                r.highlights.push((range, color));
            }
            if dl.kind != crate::git::diff::DiffLineKind::Context {
                self.annotations.create_git_hunk_line(
                    r.line_idx,
                    path_str,
                    staged_side,
                    hunk_index,
                    dl_index,
                );
                let is_exact_line_target = matches!(
                    view.cursor,
                    Some(CursorTarget::HunkLine { path, staged_side: s, hunk_index: hi, line_index })
                    if path == path_str && *s == staged_side && *hi == hunk_index && *line_index == dl_index
                );
                if is_exact_line_target {
                    r.exact_restore_line = Some(r.line_idx);
                }
            }
            r.line_idx += 1;
        }
    }

    const fn git_rebase_verb_color(verb: crate::git::rebase::RebaseVerb) -> crate::color::Color {
        use crate::color::Color;
        use crate::git::rebase::RebaseVerb;
        match verb {
            RebaseVerb::Pick => Color::Green,
            RebaseVerb::Squash => Color::Yellow,
            RebaseVerb::Fixup => Color::DarkYellow,
            #[cfg(test)]
            RebaseVerb::Reword => Color::Blue,
            RebaseVerb::Edit => Color::Magenta,
            #[cfg(test)]
            RebaseVerb::Drop => Color::DarkGrey,
        }
    }

    fn build_rebase_plan_lines(
        steps: &[crate::git::rebase::RebaseStep],
        message_overrides: &std::collections::HashMap<String, String>,
        expanded_bodies: &std::collections::HashSet<String>,
        original_bodies: &std::collections::HashMap<String, String>,
        banner: Option<&str>,
    ) -> RebasePlanRender {
        use crate::color::Color;

        let mut text = String::new();
        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();
        let mut head_lines: Vec<(usize, String)> = Vec::new();
        let mut line_idx = 0usize;

        let push_line = |text: &mut String, s: &str| -> std::ops::Range<usize> {
            let start = text.len();
            text.push_str(s);
            let end = text.len();
            text.push('\n');
            start..end
        };

        if let Some(banner) = banner {
            let range = push_line(&mut text, &format!("# {banner}"));
            highlights.push((range, Color::Yellow));
            line_idx += 1;
        }

        for step in steps {
            let short_sha = step.sha.get(..8).unwrap_or(step.sha.as_str());
            let title = message_overrides
                .get(&step.sha)
                .map_or(step.subject.as_str(), |m| m.lines().next().unwrap_or(""));
            let head_text = format!("{} {short_sha} {title}", step.verb.as_str());
            let range = push_line(&mut text, &head_text);
            highlights.push((range, Self::git_rebase_verb_color(step.verb)));
            let this_line = line_idx;
            line_idx += 1;
            head_lines.push((this_line, step.sha.clone()));

            if expanded_bodies.contains(&step.sha) {
                let body = message_overrides.get(&step.sha).map_or_else(
                    || original_bodies.get(&step.sha).cloned().unwrap_or_default(),
                    |m| {
                        let mut lines = m.lines();
                        lines.next();
                        lines.collect::<Vec<_>>().join("\n")
                    },
                );
                let is_fold = matches!(
                    step.verb,
                    crate::git::rebase::RebaseVerb::Squash | crate::git::rebase::RebaseVerb::Fixup
                );
                if is_fold {
                    let range = push_line(&mut text, "    -> folds into previous");
                    highlights.push((range, Color::DarkGrey));
                    line_idx += 1;
                }
                for body_line in body.lines() {
                    let range = push_line(&mut text, &format!("    {body_line}"));
                    if is_fold {
                        highlights.push((range, Color::DarkGrey));
                    }
                    line_idx += 1;
                }
            }
        }
        if text.ends_with('\n') {
            text.pop();
        }
        RebasePlanRender {
            text,
            highlights,
            head_lines,
        }
    }

    pub(super) fn render_git_rebase_todo(&mut self, description: &str) {
        let Some(state) = self.state.try_get(GIT_REBASE_TODO_STATE_KEY) else {
            return;
        };
        let steps = state.steps.clone();
        let message_overrides = state.message_overrides.clone();
        let expanded_bodies = state.expanded_bodies.clone();
        let original_bodies = state.original_bodies.clone();

        let cursor_sha = {
            let cursor = self.buffer.cursor();
            let line = self.buffer.line_index.get_line_at(cursor);
            self.annotations.git_rebase_step_at_line(line)
        };

        let RebasePlanRender {
            text,
            highlights,
            head_lines,
        } = Self::build_rebase_plan_lines(
            &steps,
            &message_overrides,
            &expanded_bodies,
            &original_bodies,
            None,
        );
        let head_line_for_sha: std::collections::HashMap<String, usize> =
            head_lines.iter().cloned().map(|(l, s)| (s, l)).collect();
        let restore_line = cursor_sha.and_then(|sha| head_line_for_sha.get(&sha).copied());

        self.begin_transaction(description);
        let old_len = self.buffer.len();
        if old_len > 0 {
            let _ = self.delete_range(0, old_len);
        }
        if !text.is_empty() {
            let _ = self.insert_str(&text);
        }
        self.commit_transaction();

        self.annotations.clear();
        for (line, sha) in head_lines {
            self.annotations.create_git_rebase_step(line, &sha);
        }
        self.custom_highlights = highlights;
        if let Some(line) = restore_line.or(if steps.is_empty() { None } else { Some(0) }) {
            if let Some(offset) = self.buffer.line_index.get_start(line) {
                let _ = self.buffer.set_cursor(offset);
            }
        }
    }

    pub(crate) fn render_git_rebase_paused(
        &mut self,
        remaining: &[crate::git::rebase::RebaseStep],
        banner: &str,
    ) {
        let Some(state) = self.state.try_get_mut(GIT_REBASE_TODO_STATE_KEY) else {
            return;
        };
        let message_overrides = state.message_overrides.clone();
        let expanded_bodies = state.expanded_bodies.clone();
        let original_bodies = state.original_bodies.clone();
        state.steps = remaining.to_vec();
        let RebasePlanRender {
            text,
            highlights,
            head_lines,
        } = Self::build_rebase_plan_lines(
            remaining,
            &message_overrides,
            &expanded_bodies,
            &original_bodies,
            Some(banner),
        );
        self.replace_buffer_content(&text);
        self.annotations.clear();
        for (line, sha) in head_lines {
            self.annotations.create_git_rebase_step(line, &sha);
        }
        self.custom_highlights = highlights;
        self.history.mark_saved();
    }

    pub(crate) fn move_git_rebase_step(&mut self, sha: &str, down: bool) {
        let Some(state) = self.state.try_get_mut(GIT_REBASE_TODO_STATE_KEY) else {
            return;
        };
        let steps = &mut state.steps;
        let Some(idx) = steps.iter().position(|s| s.sha == sha) else {
            return;
        };
        let target = if down {
            if idx + 1 >= steps.len() {
                return;
            }
            idx + 1
        } else {
            if idx == 0 {
                return;
            }
            idx - 1
        };
        steps.swap(idx, target);
        let description = if down {
            "Move commit down"
        } else {
            "Move commit up"
        };
        self.render_git_rebase_todo(description);
    }

    pub(crate) fn set_git_rebase_verb(&mut self, sha: &str, verb: crate::git::rebase::RebaseVerb) {
        let Some(state) = self.state.try_get_mut(GIT_REBASE_TODO_STATE_KEY) else {
            return;
        };
        let steps = &mut state.steps;
        let Some(step) = steps.iter_mut().find(|s| s.sha == sha) else {
            return;
        };
        step.verb = verb;
        self.render_git_rebase_todo(&format!("Set commit to {}", verb.as_str()));
    }

    pub(crate) fn remove_git_rebase_step(&mut self, sha: &str) {
        let Some(state) = self.state.try_get_mut(GIT_REBASE_TODO_STATE_KEY) else {
            return;
        };
        let steps = &mut state.steps;
        let before = steps.len();
        steps.retain(|s| s.sha != sha);
        if steps.len() == before {
            return;
        }
        self.render_git_rebase_todo("Drop commit");
    }

    pub(crate) fn toggle_git_rebase_expand(&mut self, sha: &str, repo_root: &std::path::Path) {
        let Some(state) = self.state.try_get_mut(GIT_REBASE_TODO_STATE_KEY) else {
            return;
        };
        if state.expanded_bodies.remove(sha) {
            self.render_git_rebase_todo("Toggle commit preview");
            return;
        }

        let needs_fetch =
            !state.message_overrides.contains_key(sha) && !state.original_bodies.contains_key(sha);
        if needs_fetch {
            let full = crate::git::run_checked(repo_root, &["log", "-1", "--format=%B", sha])
                .unwrap_or_default();
            let mut lines = full.lines();
            lines.next();
            let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
            state.original_bodies.insert(sha.to_string(), body);
        }
        state.expanded_bodies.insert(sha.to_string());
        self.render_git_rebase_todo("Toggle commit preview");
    }

    pub(crate) fn git_rebase_current_message(
        &self,
        sha: &str,
        repo_root: &std::path::Path,
    ) -> String {
        let Some(state) = self.state.try_get(GIT_REBASE_TODO_STATE_KEY) else {
            return String::new();
        };
        if let Some(message) = state.message_overrides.get(sha) {
            return message.clone();
        }
        crate::git::run_checked(repo_root, &["log", "-1", "--format=%B", sha])
            .unwrap_or_default()
            .trim_end()
            .to_string()
    }

    pub(crate) fn set_git_rebase_message_override(&mut self, sha: &str, message: String) {
        let Some(state) = self.state.try_get_mut(GIT_REBASE_TODO_STATE_KEY) else {
            return;
        };
        let message_overrides = &mut state.message_overrides;
        let expanded_bodies = &mut state.expanded_bodies;
        if message.trim().is_empty() {
            message_overrides.remove(sha);
        } else {
            message_overrides.insert(sha.to_string(), message);
            expanded_bodies.insert(sha.to_string());
        }
        self.render_git_rebase_todo("Reword commit");
    }

    pub(crate) fn set_git_gutter_signs(
        &mut self,
        signs: &[(usize, crate::git::diff::GutterSignKind)],
    ) {
        self.annotations.replace_git_gutter_signs(signs);
    }

    pub(crate) fn populate_git_blame_buffer(&mut self, lines: Vec<crate::git::blame::BlameLine>) {
        let Some(state) = self.state.try_get_mut(GIT_BLAME_STATE_KEY) else {
            return;
        };
        state.wrap_rows = vec![1; lines.len()];
        state.lines = lines;
        state.wrap_key = None;
        self.render_git_blame();
        self.history.mark_saved();
    }

    pub(crate) fn set_git_blame_wrap_rows(
        &mut self,
        wrap_key: (super::DocumentId, usize, usize, u64),
        wrap_rows: Vec<usize>,
    ) {
        let Some(state) = self.state.try_get_mut(GIT_BLAME_STATE_KEY) else {
            return;
        };
        if state.wrap_key == Some(wrap_key) && state.wrap_rows == wrap_rows {
            return;
        }
        state.wrap_key = Some(wrap_key);
        state.wrap_rows = wrap_rows;
        self.render_git_blame();
    }

    fn render_git_blame(&mut self) {
        let selected_source_line = {
            let buffer_line = self.buffer.line_index.get_line_at(self.buffer.cursor());
            self.annotations
                .git_blame_source_line_at_line(buffer_line)
                .unwrap_or(buffer_line)
        };
        let Some(state) = self.state.try_get(GIT_BLAME_STATE_KEY) else {
            return;
        };
        let lines = state.lines.clone();
        let wrap_rows = state.wrap_rows.clone();

        let total_rows: usize = wrap_rows.iter().map(|rows| (*rows).max(1)).sum();
        let mut text = String::new();
        let mut highlights = Vec::with_capacity(lines.len() * 3);
        let mut annotation_lines = Vec::with_capacity(lines.len());
        let mut display_line = 0;
        for (source_line, line) in lines.iter().enumerate() {
            annotation_lines.push(display_line);
            let sha = &line.commit.sha;
            let short_sha = sha.get(..8).unwrap_or(sha.as_str());
            let date = crate::git::format_unix_datetime(line.commit.author_time);
            let author = truncate_display(&line.commit.author, 16);
            let sha_start = text.len();
            text.push_str(short_sha);
            highlights.push((sha_start..text.len(), crate::color::Color::Cyan));
            text.push_str(" (");
            let author_start = text.len();
            let _ = write!(text, "{author:<16}");
            highlights.push((author_start..text.len(), crate::color::Color::Blue));
            text.push(' ');
            let date_start = text.len();
            text.push_str(&date);
            highlights.push((date_start..text.len(), crate::color::Color::DarkGrey));
            text.push(')');

            let rows = wrap_rows.get(source_line).copied().unwrap_or(1).max(1);
            for _ in 0..rows {
                display_line += 1;
                if display_line < total_rows {
                    text.push('\n');
                }
            }
        }

        self.replace_buffer_content(&text);
        self.custom_highlights = highlights;
        self.annotations.clear();
        for (source_line, (line, display_line)) in
            lines.iter().zip(annotation_lines.iter()).enumerate()
        {
            self.annotations
                .create_git_blame_line(*display_line, source_line, &line.commit.sha);
        }
        let selected_source_line = selected_source_line.min(lines.len().saturating_sub(1));
        if let Some(display_line) = annotation_lines.get(selected_source_line) {
            if let Some(cursor) = self.buffer.line_index.get_start(*display_line) {
                let _ = self.buffer.set_cursor(cursor);
            }
        }
    }

    pub(crate) fn populate_git_log_buffer(&mut self, commits: Vec<crate::git::log::CommitSummary>) {
        let Some(state) = self.state.try_get_mut(super::GIT_LOG_STATE_KEY) else {
            return;
        };
        state.commits = commits;
        state.expanded = None;
        state.expanded_body = None;
        self.render_git_log();
        self.history.mark_saved();
    }

    pub(crate) fn set_git_log_expanded(&mut self, sha: Option<String>, body: Option<String>) {
        let Some(state) = self.state.try_get_mut(super::GIT_LOG_STATE_KEY) else {
            return;
        };
        state.expanded = sha;
        state.expanded_body = body;
        self.render_git_log();
    }

    fn render_git_log(&mut self) {
        let Some(state) = self.state.try_get(super::GIT_LOG_STATE_KEY) else {
            return;
        };
        let commits = state.commits.clone();
        let expanded = state.expanded.clone();
        let expanded_body = state.expanded_body.clone();

        let mut lines: Vec<String> = Vec::new();
        let mut commit_annotations: Vec<(usize, String)> = Vec::new();
        for commit in &commits {
            let line_idx = lines.len();
            let date = crate::git::format_unix_date(commit.author_time);
            lines.push(format!(
                "{} ({date}) {}: {}",
                commit.short_sha, commit.author_name, commit.subject
            ));
            commit_annotations.push((line_idx, commit.sha.clone()));
            if expanded.as_deref() == Some(commit.sha.as_str()) {
                if let Some(body) = &expanded_body {
                    for body_line in body.lines() {
                        lines.push(format!("    {body_line}"));
                    }
                }
            }
        }

        self.replace_buffer_content(&lines.join("\n"));
        self.annotations.clear();
        for (line, sha) in commit_annotations {
            self.annotations.create_git_log_commit(line, &sha);
        }
    }
}

fn truncate_display(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}
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

    annotation_entry_id.map_or(Color::Green, |eid| {
        let orig = id_to_orig.get(&eid).map_or("", |s| s.as_str());
        if trimmed == orig {
            if last_visible_char == '/' {
                Color::Blue
            } else {
                Color::White
            }
        } else {
            Color::Yellow
        }
    })
}

mod git_status_sections {
    pub(super) const UNMERGED: &str = "unmerged";
    pub(super) const STAGED: &str = "staged";
    pub(super) const UNSTAGED: &str = "unstaged";
    pub(super) const UNTRACKED: &str = "untracked";

    pub(super) fn header_text(section: &str, count: usize) -> String {
        match section {
            UNMERGED => format!("Unmerged paths ({count})"),
            STAGED => format!("Staged changes ({count})"),
            UNSTAGED => format!("Unstaged changes ({count})"),
            UNTRACKED => format!("Untracked files ({count})"),
            _ => String::new(),
        }
    }
}

fn git_status_entry_matches_section(
    entry: &crate::git::status::StatusEntry,
    section: &str,
) -> bool {
    match section {
        s if s == git_status_sections::UNMERGED => entry.is_unmerged(),
        s if s == git_status_sections::STAGED => entry.is_staged(),
        s if s == git_status_sections::UNSTAGED => entry.is_unstaged(),
        s if s == git_status_sections::UNTRACKED => entry.is_untracked(),
        _ => false,
    }
}

fn git_status_entry_code(entry: &crate::git::status::StatusEntry, section: &str) -> String {
    use crate::git::status::FileState;

    const fn state_char(state: FileState) -> char {
        match state {
            FileState::Unmodified => '.',
            FileState::Modified => 'M',
            FileState::TypeChanged => 'T',
            FileState::Added => 'A',
            FileState::Deleted => 'D',
            FileState::Renamed => 'R',
            FileState::Copied => 'C',
            FileState::UpdatedUnmerged => 'U',
        }
    }

    match section {
        s if s == git_status_sections::UNMERGED => format!(
            "{}{}",
            state_char(entry.index_state),
            state_char(entry.worktree_state)
        ),
        s if s == git_status_sections::STAGED => state_char(entry.index_state).to_string(),
        s if s == git_status_sections::UNSTAGED => state_char(entry.worktree_state).to_string(),
        _ => String::new(),
    }
}
