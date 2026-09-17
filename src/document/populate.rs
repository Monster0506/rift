//! Buffer population methods: rendering special buffer kinds into text.

use super::{BufferKind, DirEntry, DirectoryDiff, Document};
use crate::buffer::TextBuffer;
use crate::character::Character;
use std::collections::HashSet;

struct RebasePlanRender {
    text: String,
    highlights: Vec<(std::ops::Range<usize>, crate::color::Color)>,
    head_lines: Vec<(usize, String)>,
}

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
    /// Entry IDs live in the annotation store, not the buffer; the buffer holds only filenames.
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

    /// Populate (or repopulate) the interactive buffer-list panel from the
    /// current buffer set, showing each entry's index, name, and status flags.
    pub fn populate_buffer_list_buffer(&mut self, infos: &[crate::document::manager::BufferInfo]) {
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
                let dirty = if info.is_dirty { "+" } else { " " };
                let read_only = if info.is_read_only { "R" } else { " " };
                let special = if info.is_special { "~" } else { " " };
                let start = content.len();
                content.push_str(&format!(
                    "[{}] {}: {current}{dirty}{read_only}{special}",
                    info.index + 1,
                    info.name,
                ));
                let color = if info.is_current {
                    Color::Cyan
                } else if info.is_dirty {
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
        self.kind = BufferKind::BufferList { entries };
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

    /// Parse the current buffer content of a directory buffer and produce a diff, by
    /// comparing each line's annotation-store entry ID against its visible buffer text.
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

                // A blank annotated line means the user erased the entry: treat as deleted.
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

    /// Populate (or repopulate) this git status buffer from a fresh `git status` snapshot. Collapses any previously-expanded hunks; callers that want to keep an expansion across a refresh must re-expand it after this call.
    pub fn populate_git_status_buffer(
        &mut self,
        snapshot: crate::git::status::StatusSnapshot,
        head_subject: Option<String>,
    ) {
        let repo_root = match &self.kind {
            BufferKind::GitStatus { repo_root, .. } => repo_root.clone(),
            _ => return,
        };
        self.kind = BufferKind::GitStatus {
            repo_root,
            snapshot,
            expanded_diffs: std::collections::HashMap::new(),
            head_subject,
        };
        self.render_git_status();
        self.history.mark_saved();
    }

    /// Record `hunks` as the expanded diff for `path` on the given side (`staged_side`: `true` = `git diff --cached`, `false` = `git diff`) and re-render. No-op if this isn't a git status buffer.
    pub fn set_git_status_expanded(
        &mut self,
        path: std::path::PathBuf,
        staged_side: bool,
        hunks: Vec<crate::git::diff::Hunk>,
    ) {
        match &mut self.kind {
            BufferKind::GitStatus { expanded_diffs, .. } => {
                expanded_diffs.insert((path, staged_side), hunks);
            }
            _ => return,
        }
        self.render_git_status();
    }

    /// Remove `path`'s expanded diff on the given side and re-render.
    pub fn collapse_git_status_entry(&mut self, path: &std::path::Path, staged_side: bool) {
        match &mut self.kind {
            BufferKind::GitStatus { expanded_diffs, .. } => {
                expanded_diffs.remove(&(path.to_path_buf(), staged_side));
            }
            _ => return,
        }
        self.render_git_status();
    }

    /// Whether `path`'s diff is currently expanded on the given side.
    pub fn is_git_status_expanded(&self, path: &std::path::Path, staged_side: bool) -> bool {
        match &self.kind {
            BufferKind::GitStatus { expanded_diffs, .. } => {
                expanded_diffs.contains_key(&(path.to_path_buf(), staged_side))
            }
            _ => false,
        }
    }

    /// Whether `path`'s snapshot entry is untracked (never in the index),
    /// so staging any part of it needs a "new file" patch header.
    pub fn is_git_status_entry_untracked(&self, path: &std::path::Path) -> bool {
        match &self.kind {
            BufferKind::GitStatus { snapshot, .. } => snapshot
                .entries
                .iter()
                .any(|e| e.path == path && e.is_untracked()),
            _ => false,
        }
    }

    /// Rebuild buffer text + annotations from this git status buffer's current `snapshot`/`expanded_diffs`. Does not touch `self.kind` itself (callers update the snapshot/expanded_diffs before calling this).
    fn render_git_status(&mut self) {
        use crate::color::Color;

        let (snapshot, expanded_diffs, head_subject) = match &self.kind {
            BufferKind::GitStatus {
                snapshot,
                expanded_diffs,
                head_subject,
                ..
            } => (
                snapshot.clone(),
                expanded_diffs.clone(),
                head_subject.clone(),
            ),
            _ => return,
        };

        // Capture what the cursor is currently "on" so it can be restored after the rebuild below; without this, every expand/collapse/ stage/unstage/discard silently snaps the cursor back to line 0, which then desyncs `j`'s next stop from where the user thinks they are (landing back on the file entry instead of.
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
        let cursor_target = {
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
        };
        let mut exact_restore_line: Option<usize> = None;
        let mut hunk_restore_line: Option<usize> = None;
        let mut entry_restore_line: Option<usize> = None;

        let mut chars: Vec<Character> = Vec::new();
        let mut highlights: Vec<(std::ops::Range<usize>, Color)> = Vec::new();
        let mut byte_offset = 0usize;
        let mut line_idx = 0usize;

        self.annotations.clear();

        let branch_line = match &snapshot.branch.head {
            Some(name) => format!("On branch {name}"),
            None => match &snapshot.branch.oid {
                Some(oid) => format!("HEAD detached at {}", &oid[..oid.len().min(12)]),
                None => "On an unborn branch".to_string(),
            },
        };
        let range = git_status_push_line(&mut chars, &mut byte_offset, &branch_line);
        highlights.push((range, Color::Cyan));
        line_idx += 1;

        if let (Some(oid), Some(subject)) = (&snapshot.branch.oid, &head_subject) {
            let head_line = format!("HEAD  {} {subject}", &oid[..oid.len().min(8)]);
            let range = git_status_push_line(&mut chars, &mut byte_offset, &head_line);
            highlights.push((range, Color::Yellow));
            self.annotations.create_git_status_head(line_idx);
            line_idx += 1;
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
                git_status_push_line(&mut chars, &mut byte_offset, &msg);
                line_idx += 1;
            }
        }
        git_status_push_line(&mut chars, &mut byte_offset, "");
        line_idx += 1;

        let sections: [(&str, Color); 4] = [
            (git_status_sections::UNMERGED, Color::Magenta),
            (git_status_sections::STAGED, Color::Green),
            (git_status_sections::UNSTAGED, Color::Yellow),
            (git_status_sections::UNTRACKED, Color::Red),
        ];

        for (section, color) in sections {
            let entries: Vec<&crate::git::status::StatusEntry> = snapshot
                .entries
                .iter()
                .filter(|e| git_status_entry_matches_section(e, section))
                .collect();
            if entries.is_empty() {
                continue;
            }

            let header = git_status_sections::header_text(section, entries.len());
            let range = git_status_push_line(&mut chars, &mut byte_offset, &header);
            highlights.push((range, Color::DarkGrey));
            line_idx += 1;

            for entry in entries {
                let path_str = entry.path.to_string_lossy().to_string();
                let orig_str = entry
                    .orig_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string());
                let display = match &orig_str {
                    Some(orig) => format!("{orig} -> {path_str}"),
                    None => path_str.clone(),
                };
                let code = git_status_entry_code(entry, section);
                let line_text = if code.is_empty() {
                    format!("  {display}")
                } else {
                    format!("  {code} {display}")
                };
                let range = git_status_push_line(&mut chars, &mut byte_offset, &line_text);
                highlights.push((range, color));
                let this_line = line_idx;
                line_idx += 1;

                self.annotations.create_git_status_entry(
                    this_line,
                    &path_str,
                    section,
                    orig_str.as_deref(),
                );
                let target_path = match &cursor_target {
                    Some(CursorTarget::HunkLine { path, .. })
                    | Some(CursorTarget::HunkHeader { path, .. })
                    | Some(CursorTarget::Entry { path }) => Some(path.as_str()),
                    None => None,
                };
                if target_path == Some(path_str.as_str()) {
                    entry_restore_line = Some(this_line);
                }

                let staged_side = section == git_status_sections::STAGED;
                if let Some(hunks) = expanded_diffs.get(&(entry.path.clone(), staged_side)) {
                    for (hunk_index, hunk) in hunks.iter().enumerate() {
                        let header_text = if hunk.header.is_empty() {
                            format!(
                                "      @@ -{},{} +{},{} @@",
                                hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
                            )
                        } else {
                            format!(
                                "      @@ -{},{} +{},{} @@ {}",
                                hunk.old_start,
                                hunk.old_lines,
                                hunk.new_start,
                                hunk.new_lines,
                                hunk.header
                            )
                        };
                        let range =
                            git_status_push_line(&mut chars, &mut byte_offset, &header_text);
                        highlights.push((range, Color::Cyan));
                        let hunk_line_idx = line_idx;
                        line_idx += 1;
                        self.annotations.create_git_hunk(
                            hunk_line_idx,
                            &path_str,
                            staged_side,
                            hunk_index,
                        );
                        let same_hunk = matches!(
                            &cursor_target,
                            Some(CursorTarget::HunkLine { path, staged_side: s, hunk_index: hi, .. })
                            | Some(CursorTarget::HunkHeader { path, staged_side: s, hunk_index: hi })
                            if path == &path_str && *s == staged_side && *hi == hunk_index
                        );
                        if same_hunk {
                            hunk_restore_line = Some(hunk_line_idx);
                        }
                        let is_exact_header_target = matches!(
                            &cursor_target,
                            Some(CursorTarget::HunkHeader { path, staged_side: s, hunk_index: hi })
                            if path == &path_str && *s == staged_side && *hi == hunk_index
                        );
                        if is_exact_header_target {
                            exact_restore_line = Some(hunk_line_idx);
                        }

                        for (dl_index, dl) in hunk.lines.iter().enumerate() {
                            let (marker, color) = match dl.kind {
                                crate::git::diff::DiffLineKind::Context => (' ', None),
                                crate::git::diff::DiffLineKind::Addition => {
                                    ('+', Some(Color::Green))
                                }
                                crate::git::diff::DiffLineKind::Deletion => ('-', Some(Color::Red)),
                            };
                            let text = format!("        {marker}{}", dl.content);
                            let range = git_status_push_line(&mut chars, &mut byte_offset, &text);
                            if let Some(color) = color {
                                highlights.push((range, color));
                            }
                            if dl.kind != crate::git::diff::DiffLineKind::Context {
                                self.annotations.create_git_hunk_line(
                                    line_idx,
                                    &path_str,
                                    staged_side,
                                    hunk_index,
                                    dl_index,
                                );
                                let is_exact_line_target = matches!(
                                    &cursor_target,
                                    Some(CursorTarget::HunkLine { path, staged_side: s, hunk_index: hi, line_index })
                                    if path == &path_str && *s == staged_side && *hi == hunk_index && *line_index == dl_index
                                );
                                if is_exact_line_target {
                                    exact_restore_line = Some(line_idx);
                                }
                            }
                            line_idx += 1;
                        }
                    }
                }
            }
            git_status_push_line(&mut chars, &mut byte_offset, "");
            line_idx += 1;
        }

        if chars.last() == Some(&Character::Newline) {
            chars.pop();
        }

        self.replace_buffer_content_chars(&chars);
        self.custom_highlights = highlights;
        let restore_line = exact_restore_line
            .or(hunk_restore_line)
            .or(entry_restore_line);
        if let Some(line) = restore_line {
            if let Some(offset) = self.buffer.line_index.get_start(line) {
                let _ = self.buffer.set_cursor(offset);
            }
        }
    }

    /// Verb-specific color for a rebase-todo head line.
    fn git_rebase_verb_color(verb: crate::git::rebase::RebaseVerb) -> crate::color::Color {
        use crate::color::Color;
        use crate::git::rebase::RebaseVerb;
        match verb {
            RebaseVerb::Pick => Color::Green,
            RebaseVerb::Squash => Color::Yellow,
            RebaseVerb::Fixup => Color::DarkYellow,
            RebaseVerb::Reword => Color::Blue,
            RebaseVerb::Edit => Color::Magenta,
            RebaseVerb::Drop => Color::DarkGrey,
        }
    }

    /// Pure computation: build the plan's rendered text, highlight spans, and `(line, sha)` head-line list for `steps`, optionally prefixed with a non-interactive `banner` line (e.g. a pause status). Shared by the normal interactive render and the paused-state render, which apply the result to the buffer.
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
            let short_sha = &step.sha[..step.sha.len().min(8)];
            let title = match message_overrides.get(&step.sha) {
                Some(m) => m.lines().next().unwrap_or(""),
                None => step.subject.as_str(),
            };
            let head_text = format!("{} {short_sha} {title}", step.verb.as_str());
            let range = push_line(&mut text, &head_text);
            highlights.push((range, Self::git_rebase_verb_color(step.verb)));
            let this_line = line_idx;
            line_idx += 1;
            head_lines.push((this_line, step.sha.clone()));

            if expanded_bodies.contains(&step.sha) {
                let body = match message_overrides.get(&step.sha) {
                    Some(m) => {
                        let mut lines = m.lines();
                        lines.next();
                        lines.collect::<Vec<_>>().join("\n")
                    }
                    None => original_bodies.get(&step.sha).cloned().unwrap_or_default(),
                };
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

    /// Rebuild this rebase-todo buffer's text + annotations from `steps`/ `message_overrides`/`expanded_bodies`/`original_bodies`, preserving which commit's head line the cursor was on. Unlike `render_git_status` (a read-only buffer, wholesale-swapped with no undo concerns), this buffer is genuinely editable, and.
    pub(super) fn render_git_rebase_todo(&mut self, description: &str) {
        let (steps, message_overrides, expanded_bodies, original_bodies) = match &self.kind {
            BufferKind::GitRebaseTodo {
                steps,
                message_overrides,
                expanded_bodies,
                original_bodies,
                ..
            } => (
                steps.clone(),
                message_overrides.clone(),
                expanded_bodies.clone(),
                original_bodies.clone(),
            ),
            _ => return,
        };

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

    /// Render a paused rebase-todo: `remaining` (already trimmed to what's left) with a status banner on top, in the same rich annotated/ colored view as the interactive plan; not a plain-text dump; so pausing doesn't jar into a visually different "other" buffer. K/J/ verb keys/fold/reword all keep working on the.
    pub fn render_git_rebase_paused(
        &mut self,
        remaining: &[crate::git::rebase::RebaseStep],
        banner: &str,
    ) {
        let (message_overrides, expanded_bodies, original_bodies) = match &self.kind {
            BufferKind::GitRebaseTodo {
                message_overrides,
                expanded_bodies,
                original_bodies,
                ..
            } => (
                message_overrides.clone(),
                expanded_bodies.clone(),
                original_bodies.clone(),
            ),
            _ => return,
        };
        if let BufferKind::GitRebaseTodo { steps, .. } = &mut self.kind {
            *steps = remaining.to_vec();
        }
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

    /// Swap the plan's step at `sha` with its neighbor (up or down).
    /// No-op at either edge or if `sha` isn't found.
    pub fn move_git_rebase_step(&mut self, sha: &str, down: bool) {
        let BufferKind::GitRebaseTodo { steps, .. } = &mut self.kind else {
            return;
        };
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

    /// Set the verb of the step at `sha` (never `Reword`/`Drop`;  those go
    /// through the message editor and `remove_git_rebase_step` respectively).
    pub fn set_git_rebase_verb(&mut self, sha: &str, verb: crate::git::rebase::RebaseVerb) {
        let BufferKind::GitRebaseTodo { steps, .. } = &mut self.kind else {
            return;
        };
        let Some(step) = steps.iter_mut().find(|s| s.sha == sha) else {
            return;
        };
        step.verb = verb;
        self.render_git_rebase_todo(&format!("Set commit to {}", verb.as_str()));
    }

    /// Remove the step at `sha` from the plan entirely (drop).
    pub fn remove_git_rebase_step(&mut self, sha: &str) {
        let BufferKind::GitRebaseTodo { steps, .. } = &mut self.kind else {
            return;
        };
        let before = steps.len();
        steps.retain(|s| s.sha != sha);
        if steps.len() == before {
            return;
        }
        self.render_git_rebase_todo("Drop commit");
    }

    /// Toggle whether `sha`'s body is previewed inline. Lazily fetches and
    /// caches its real body text on first expand if there's no override yet.
    pub fn toggle_git_rebase_expand(&mut self, sha: &str, repo_root: &std::path::Path) {
        let already_expanded = match &self.kind {
            BufferKind::GitRebaseTodo {
                expanded_bodies, ..
            } => expanded_bodies.contains(sha),
            _ => return,
        };
        if already_expanded {
            if let BufferKind::GitRebaseTodo {
                expanded_bodies, ..
            } = &mut self.kind
            {
                expanded_bodies.remove(sha);
            }
        } else {
            let needs_fetch = match &self.kind {
                BufferKind::GitRebaseTodo {
                    message_overrides,
                    original_bodies,
                    ..
                } => !message_overrides.contains_key(sha) && !original_bodies.contains_key(sha),
                _ => false,
            };
            if needs_fetch {
                let full = crate::git::run_checked(repo_root, &["log", "-1", "--format=%B", sha])
                    .unwrap_or_default();
                let mut lines = full.lines();
                lines.next(); // subject, already shown in the head line
                let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
                if let BufferKind::GitRebaseTodo {
                    original_bodies, ..
                } = &mut self.kind
                {
                    original_bodies.insert(sha.to_string(), body);
                }
            }
            if let BufferKind::GitRebaseTodo {
                expanded_bodies, ..
            } = &mut self.kind
            {
                expanded_bodies.insert(sha.to_string());
            }
        }
        self.render_git_rebase_todo("Toggle commit preview");
    }

    /// The current full message for `sha` (override if set, else its real
    /// current commit message), for prefilling the `c`/`r` sub-editor.
    pub fn git_rebase_current_message(&self, sha: &str, repo_root: &std::path::Path) -> String {
        match &self.kind {
            BufferKind::GitRebaseTodo {
                message_overrides, ..
            } => {
                if let Some(m) = message_overrides.get(sha) {
                    return m.clone();
                }
            }
            _ => return String::new(),
        }
        crate::git::run_checked(repo_root, &["log", "-1", "--format=%B", sha])
            .unwrap_or_default()
            .trim_end()
            .to_string()
    }

    /// Set (or clear, if `message` is empty) `sha`'s message override and
    /// re-render;  called when the `c`/`r` sub-editor is saved.
    pub fn set_git_rebase_message_override(&mut self, sha: &str, message: String) {
        let BufferKind::GitRebaseTodo {
            message_overrides,
            expanded_bodies,
            ..
        } = &mut self.kind
        else {
            return;
        };
        if message.trim().is_empty() {
            message_overrides.remove(sha);
        } else {
            message_overrides.insert(sha.to_string(), message);
            expanded_bodies.insert(sha.to_string());
        }
        self.render_git_rebase_todo("Reword commit");
    }

    /// Return the repo root for any git buffer kind.
    pub fn git_repo_root(&self) -> Option<&std::path::Path> {
        match &self.kind {
            BufferKind::GitStatus { repo_root, .. } => Some(repo_root),
            BufferKind::GitCommitMessage { repo_root, .. } => Some(repo_root),
            BufferKind::GitBlame { repo_root, .. } => Some(repo_root),
            BufferKind::GitLog { repo_root, .. } => Some(repo_root),
            BufferKind::GitRebaseTodo { repo_root, .. } => Some(repo_root),
            _ => None,
        }
    }

    /// Replace this file buffer's git-gutter signs (add/change/delete per line), computed by a `GitGutterDiffJob` against the buffer's live (possibly unsaved) content.
    pub fn set_git_gutter_signs(&mut self, signs: &[(usize, crate::git::diff::GutterSignKind)]) {
        self.annotations.replace_git_gutter_signs(signs);
    }

    /// Populate (or repopulate) this git blame buffer from a fresh
    /// `git blame --porcelain` listing.
    pub fn populate_git_blame_buffer(&mut self, lines: Vec<crate::git::blame::BlameLine>) {
        let (repo_root, linked_doc_id, linked_window_id, path, at_commit, history) =
            match &self.kind {
                BufferKind::GitBlame {
                    repo_root,
                    linked_doc_id,
                    linked_window_id,
                    path,
                    at_commit,
                    history,
                    ..
                } => (
                    repo_root.clone(),
                    *linked_doc_id,
                    *linked_window_id,
                    path.clone(),
                    at_commit.clone(),
                    history.clone(),
                ),
                _ => return,
            };
        let wrap_rows = vec![1; lines.len()];
        self.kind = BufferKind::GitBlame {
            repo_root,
            linked_doc_id,
            linked_window_id,
            path,
            at_commit,
            history,
            lines,
            wrap_rows,
            wrap_key: None,
        };
        self.render_git_blame();
        self.history.mark_saved();
    }

    /// Reflow blame metadata to match the linked source pane's visual rows.
    /// Continuation rows are blank and deliberately carry no annotation.
    pub fn set_git_blame_wrap_rows(
        &mut self,
        wrap_key: (super::DocumentId, usize, usize, u64),
        wrap_rows: Vec<usize>,
    ) {
        let unchanged = matches!(
            &self.kind,
            BufferKind::GitBlame {
                wrap_key: current_key,
                wrap_rows: current,
                ..
            } if *current_key == Some(wrap_key) && *current == wrap_rows
        );
        if unchanged {
            return;
        }
        if let BufferKind::GitBlame {
            wrap_key: current_key,
            wrap_rows: current,
            ..
        } = &mut self.kind
        {
            *current_key = Some(wrap_key);
            *current = wrap_rows;
        } else {
            return;
        }
        self.render_git_blame();
    }

    fn render_git_blame(&mut self) {
        let selected_source_line = {
            let buffer_line = self.buffer.line_index.get_line_at(self.buffer.cursor());
            self.annotations
                .git_blame_source_line_at_line(buffer_line)
                .unwrap_or(buffer_line)
        };
        let (lines, wrap_rows) = match &self.kind {
            BufferKind::GitBlame {
                lines, wrap_rows, ..
            } => (lines.clone(), wrap_rows.clone()),
            _ => return,
        };

        let total_rows: usize = wrap_rows.iter().map(|rows| (*rows).max(1)).sum();
        let mut text = String::new();
        let mut highlights = Vec::with_capacity(lines.len() * 3);
        let mut annotation_lines = Vec::with_capacity(lines.len());
        let mut display_line = 0;
        for (source_line, line) in lines.iter().enumerate() {
            annotation_lines.push(display_line);
            let sha = &line.commit.sha;
            let short_sha = &sha[..sha.len().min(8)];
            let date = crate::git::format_unix_datetime(line.commit.author_time);
            let author = truncate_display(&line.commit.author, 16);
            let sha_start = text.len();
            text.push_str(short_sha);
            highlights.push((sha_start..text.len(), crate::color::Color::Cyan));
            text.push_str(" (");
            let author_start = text.len();
            text.push_str(&format!("{author:<16}"));
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

    /// Populate (or repopulate) this git log buffer from a fresh commit
    /// listing. Collapses any previously-expanded `git show` body.
    pub fn populate_git_log_buffer(&mut self, commits: Vec<crate::git::log::CommitSummary>) {
        let (repo_root, path) = match &self.kind {
            BufferKind::GitLog {
                repo_root, path, ..
            } => (repo_root.clone(), path.clone()),
            _ => return,
        };
        self.kind = BufferKind::GitLog {
            repo_root,
            path,
            commits,
            expanded: None,
            expanded_body: None,
        };
        self.render_git_log();
        self.history.mark_saved();
    }

    /// Set (or clear, via `None`) the inline-expanded `git show` body for
    /// `sha` and re-render.
    pub fn set_git_log_expanded(&mut self, sha: Option<String>, body: Option<String>) {
        match &mut self.kind {
            BufferKind::GitLog {
                expanded,
                expanded_body,
                ..
            } => {
                *expanded = sha;
                *expanded_body = body;
            }
            _ => return,
        }
        self.render_git_log();
    }

    fn render_git_log(&mut self) {
        let (commits, expanded, expanded_body) = match &self.kind {
            BufferKind::GitLog {
                commits,
                expanded,
                expanded_body,
                ..
            } => (commits.clone(), expanded.clone(), expanded_body.clone()),
            _ => return,
        };

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

/// Truncate `s` to at most `max_chars` characters, for fixed-width columns
/// like a blame author name (never panics on multi-byte boundaries).
fn truncate_display(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
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

/// Status-buffer section identifiers and the header text they render as.
mod git_status_sections {
    pub const UNMERGED: &str = "unmerged";
    pub const STAGED: &str = "staged";
    pub const UNSTAGED: &str = "unstaged";
    pub const UNTRACKED: &str = "untracked";

    pub fn header_text(section: &str, count: usize) -> String {
        match section {
            UNMERGED => format!("Unmerged paths ({count})"),
            STAGED => format!("Staged changes ({count})"),
            UNSTAGED => format!("Unstaged changes ({count})"),
            UNTRACKED => format!("Untracked files ({count})"),
            _ => String::new(),
        }
    }
}

/// Whether `entry` belongs under `section` for status-buffer rendering.
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

/// The short status-code prefix shown for `entry` in `section` (e.g. `"M"`,
/// `"UU"`, or empty for untracked, which has no meaningful code).
fn git_status_entry_code(entry: &crate::git::status::StatusEntry, section: &str) -> String {
    use crate::git::status::FileState;

    fn state_char(state: FileState) -> char {
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

/// Append `text` plus a trailing newline to `chars`, advancing `byte_offset`.
/// Returns the byte range of `text` itself (excluding the newline), for highlights.
fn git_status_push_line(
    chars: &mut Vec<Character>,
    byte_offset: &mut usize,
    text: &str,
) -> std::ops::Range<usize> {
    let start = *byte_offset;
    for c in text.chars() {
        chars.push(Character::from(c));
        *byte_offset += c.len_utf8();
    }
    let end = *byte_offset;
    chars.push(Character::Newline);
    *byte_offset += 1;
    start..end
}
