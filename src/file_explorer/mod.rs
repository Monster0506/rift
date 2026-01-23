use crate::color::Color;
use crate::component::{Component, EventResult};
use crate::input_box::{InputBox, InputBoxConfig};
use crate::job_manager::jobs::explorer::{
    DirectoryListJob, DirectoryListing, FileEntry, FilePreview, FilePreviewJob,
};
use crate::job_manager::jobs::fs::{FsBatchDeleteJob, FsCopyJob, FsCreateJob, FsMoveJob};
use crate::job_manager::{Job, JobMessage};
use crate::key::Key;
use crate::layer::Cell;
use crate::layer::Layer;
use crate::select_view::SelectView;
use std::collections::HashSet;
use std::path::PathBuf;

pub enum ExplorerAction {
    SpawnJob(Box<dyn Job>),
    OpenFile(PathBuf),
    Notify(crate::notification::NotificationType, String),
    Close,
}

mod action_impl;

#[derive(Clone)]
enum InputMode {
    None,
    NewFile,
    NewDir,
    Rename(PathBuf),
    Copy(PathBuf),
    DeleteConfirm(Vec<PathBuf>),
}

pub struct FileExplorer {
    select_view: SelectView,
    current_path: PathBuf,
    entries: Vec<FileEntry>,
    selected_indices: HashSet<usize>,

    show_hidden: bool,
    show_metadata: bool,

    input_box: Option<InputBox>,
    input_mode: InputMode,

    fg: Option<Color>,
    bg: Option<Color>,
    preview_cache: std::collections::HashMap<PathBuf, Vec<Vec<Cell>>>,
}

impl FileExplorer {
    pub fn new(initial_path: PathBuf) -> Self {
        let mut explorer = Self {
            select_view: SelectView::new()
                .with_left_width(50)
                .with_colors(None, None),
            current_path: initial_path,
            entries: Vec::new(),
            selected_indices: HashSet::new(),
            show_hidden: false,
            show_metadata: true,
            input_box: None,
            input_mode: InputMode::None,
            fg: None,
            bg: None,
            preview_cache: std::collections::HashMap::new(),
        };

        // Initial setup
        explorer.update_view();
        explorer
    }

    pub fn with_colors(mut self, fg: Option<Color>, bg: Option<Color>) -> Self {
        self.fg = fg;
        self.bg = bg;
        self.select_view = self.select_view.with_colors(fg, bg);
        self
    }

    pub fn create_list_job(&self) -> Box<dyn Job> {
        Box::new(DirectoryListJob::new(
            self.current_path.clone(),
            self.show_hidden,
        ))
    }

    fn update_view(&mut self) {
        let mut content = Vec::new();

        // Header ..
        if self.current_path.parent().is_some() {
            let row = vec![
                Cell::from_char('.').with_fg(Color::Blue),
                Cell::from_char('.').with_fg(Color::Blue),
            ];
            content.push(row);
        }

        for (i, entry) in self.entries.iter().enumerate() {
            let mut row = Vec::new();

            // Selection indicator
            if self.selected_indices.contains(&i) {
                row.push(Cell::from_char('>').with_fg(Color::Yellow));
                row.push(Cell::from_char(' '));
            } else {
                row.push(Cell::from_char(' '));
                row.push(Cell::from_char(' '));
            }

            // Icon/Type
            if entry.is_dir {
                row.push(Cell::from_char('D').with_fg(Color::Blue));
            } else {
                row.push(Cell::from_char('F').with_fg(Color::White));
            }
            row.push(Cell::from_char(' '));

            // Name
            let name_color = if entry.is_dir {
                Color::Blue
            } else {
                Color::White
            };
            for c in entry.name.chars() {
                row.push(Cell::from_char(c).with_fg(name_color));
            }

            // Metadata
            if self.show_metadata {
                // Padding
                while row.len() < 40 {
                    row.push(Cell::from_char(' '));
                }
                let size_str = if entry.is_dir {
                    "<DIR>".to_string()
                } else {
                    format_size(entry.size)
                };

                for c in size_str.chars() {
                    row.push(Cell::from_char(c).with_fg(Color::DarkGrey));
                }
            }

            content.push(row);
        }

        self.select_view.set_left_content(content);
    }

    fn get_entry_index(&self, visual_index: usize) -> Option<usize> {
        if self.current_path.parent().is_some() {
            if visual_index == 0 {
                return None; // Header
            }
            Some(visual_index - 1)
        } else {
            Some(visual_index)
        }
    }

    fn create_preview_action(&mut self, visual_index: usize) -> EventResult {
        let entry_idx = match self.get_entry_index(visual_index) {
            Some(i) => i,
            None => return EventResult::Consumed, // Header
        };

        if let Some(entry) = self.entries.get(entry_idx) {
            // Check cache
            if let Some(cached) = self.preview_cache.get(&entry.path) {
                self.select_view.set_right_content(cached.clone());
                return EventResult::Consumed;
            }

            if entry.is_dir {
                // Spawn preview job for directory
                return EventResult::Action(Box::new(ExplorerAction::SpawnJob(Box::new(
                    DirectoryListJob::new(entry.path.clone(), self.show_hidden),
                ))));
            } else {
                return EventResult::Action(Box::new(ExplorerAction::SpawnJob(Box::new(
                    FilePreviewJob::new(entry.path.clone()),
                ))));
            }
        }
        EventResult::Ignored
    }

    fn open_input_box(&mut self, title: &str, placeholder: &str, mode: InputMode) {
        let config = InputBoxConfig {
            title: Some(title.to_string()),
            placeholder: Some(placeholder.to_string()),
            width: 50,
            ..Default::default()
        };
        self.input_box = Some(InputBox::with_config(config));
        self.input_mode = mode;
    }

    fn close_input_box(&mut self) {
        self.input_box = None;
        self.input_mode = InputMode::None;
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

impl Component for FileExplorer {
    fn handle_job_message(&mut self, message: JobMessage) -> EventResult {
        match message {
            JobMessage::Custom(_, payload) => {
                if let Some(listing) = payload.as_any().downcast_ref::<DirectoryListing>() {
                    // LEFT PANE: Directory update
                    // Check if this listing is for our current path (avoid race conditions from old jobs)
                    if listing.path == self.current_path {
                        self.entries = listing.entries.clone();
                        self.selected_indices.clear();
                        self.update_view();

                        // Trigger preview for first item if exists
                        if !self.entries.is_empty() {
                            let idx = 0; // Reset to top
                            self.select_view.set_selected_line(Some(idx));
                            self.select_view.set_left_scroll(0); // Reset scroll
                            return self.create_preview_action(idx);
                        } else {
                            // Log empty directory for debugging
                            return EventResult::Action(Box::new(ExplorerAction::Notify(
                                crate::notification::NotificationType::Warning,
                                format!("Directory is empty: {:?}", self.current_path),
                            )));
                        }
                    }
                    // RIGHT PANE: Directory Preview
                    else {
                        // Check if this listing matches our currently selected entry (directory preview)
                        if let Some(visual_idx) = self.select_view.selected_line() {
                            if let Some(entry_idx) = self.get_entry_index(visual_idx) {
                                if let Some(entry) = self.entries.get(entry_idx) {
                                    if entry.path == listing.path {
                                        // This job result is for the currently selected directory!
                                        let mut content = Vec::new();
                                        for entry in &listing.entries {
                                            let mut row = Vec::new();
                                            // Icon
                                            if entry.is_dir {
                                                row.push(Cell::from_char('D').with_fg(Color::Blue));
                                            } else {
                                                row.push(
                                                    Cell::from_char('F').with_fg(Color::White),
                                                );
                                            }
                                            row.push(Cell::from_char(' '));

                                            // Name
                                            let color = if entry.is_dir {
                                                Color::Blue
                                            } else {
                                                Color::White
                                            };
                                            for c in entry.name.chars() {
                                                row.push(Cell::from_char(c).with_fg(color));
                                            }
                                            content.push(row);
                                        }

                                        if content.is_empty() {
                                            content.push(vec![
                                                Cell::from_char('<')
                                                    .with_fg(self.fg.unwrap_or(Color::DarkGrey)),
                                                Cell::from_char('e')
                                                    .with_fg(self.fg.unwrap_or(Color::DarkGrey)),
                                                Cell::from_char('m')
                                                    .with_fg(self.fg.unwrap_or(Color::DarkGrey)),
                                                Cell::from_char('p')
                                                    .with_fg(self.fg.unwrap_or(Color::DarkGrey)),
                                                Cell::from_char('t')
                                                    .with_fg(self.fg.unwrap_or(Color::DarkGrey)),
                                                Cell::from_char('y')
                                                    .with_fg(self.fg.unwrap_or(Color::DarkGrey)),
                                                Cell::from_char('>')
                                                    .with_fg(self.fg.unwrap_or(Color::DarkGrey)),
                                            ]);
                                        }

                                        // Update Cache
                                        self.preview_cache
                                            .insert(listing.path.clone(), content.clone());

                                        self.select_view.set_right_content(content);
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(preview) = payload.as_any().downcast_ref::<FilePreview>() {
                    // Update right pane
                    if let Some(visual_idx) = self.select_view.selected_line() {
                        if let Some(entry_idx) = self.get_entry_index(visual_idx) {
                            if let Some(entry) = self.entries.get(entry_idx) {
                                if entry.path == preview.path {
                                    let content: Vec<Vec<Cell>> = preview
                                        .content
                                        .lines()
                                        .map(|line| line.chars().map(Cell::from_char).collect())
                                        .collect();

                                    // Update Cache
                                    self.preview_cache
                                        .insert(preview.path.clone(), content.clone());

                                    self.select_view.set_right_content(content);
                                }
                            }
                        }
                    }
                }
            }
            JobMessage::Finished(_, silent) => {
                if !silent {
                    return EventResult::Action(Box::new(ExplorerAction::SpawnJob(
                        self.create_list_job(),
                    )));
                }
            }
            _ => {}
        }
        EventResult::Consumed
    }

    fn handle_input(&mut self, key: Key) -> EventResult {
        if self.input_box.is_some() {
            let (result, submit_content) = if let Some(ib) = self.input_box.as_mut() {
                let res = ib.handle_input(key);
                let content = if key == Key::Enter {
                    Some(ib.content().to_string())
                } else {
                    None
                };
                (res, content)
            } else {
                (EventResult::Ignored, None)
            };

            if let Some(content) = submit_content {
                return self.handle_input_box_submit(content);
            }
            return result;
        }

        // Normal Mode
        match key {
            Key::Char('q') | Key::Escape => {
                return EventResult::Action(Box::new(ExplorerAction::Close));
            }
            Key::Char('j') | Key::ArrowDown => {
                let res = self.select_view.handle_input(key);
                if let Some(idx) = self.select_view.selected_line() {
                    return self.create_preview_action(idx);
                }
                return res;
            }
            Key::Char('k') | Key::ArrowUp => {
                let res = self.select_view.handle_input(key);
                if let Some(idx) = self.select_view.selected_line() {
                    return self.create_preview_action(idx);
                }
                return res;
            }
            Key::Enter => {
                if let Some(visual_idx) = self.select_view.selected_line() {
                    if let Some(entry_idx) = self.get_entry_index(visual_idx) {
                        if let Some(entry) = self.entries.get(entry_idx) {
                            if entry.is_dir {
                                self.current_path = entry.path.clone();
                                self.preview_cache.clear(); // Clear cache on directory change
                                return EventResult::Action(Box::new(ExplorerAction::SpawnJob(
                                    self.create_list_job(),
                                )));
                            } else {
                                return EventResult::Action(Box::new(ExplorerAction::OpenFile(
                                    entry.path.clone(),
                                )));
                            }
                        }
                    } else {
                        // Header selected (..)
                        if let Some(parent) = self.current_path.parent() {
                            self.current_path = parent.to_path_buf();
                            return EventResult::Action(Box::new(ExplorerAction::SpawnJob(
                                self.create_list_job(),
                            )));
                        }
                    }
                }
            }
            Key::Backspace | Key::Char('-') => {
                if let Some(parent) = self.current_path.parent() {
                    self.current_path = parent.to_path_buf();
                    return EventResult::Action(Box::new(ExplorerAction::SpawnJob(
                        self.create_list_job(),
                    )));
                }
            }
            Key::Char(' ') => {
                if let Some(visual_idx) = self.select_view.selected_line() {
                    if let Some(idx) = self.get_entry_index(visual_idx) {
                        if self.selected_indices.contains(&idx) {
                            self.selected_indices.remove(&idx);
                        } else {
                            self.selected_indices.insert(idx);
                        }
                        self.update_view();
                    }
                }
            }
            Key::Char('a') => {
                for i in 0..self.entries.len() {
                    self.selected_indices.insert(i);
                }
                self.update_view();
            }
            Key::Char('u') => {
                self.selected_indices.clear();
                self.update_view();
            }
            Key::Char('R') => {
                // Manual refresh
                self.preview_cache.clear(); // Clear cache on refresh
                return EventResult::Action(Box::new(ExplorerAction::SpawnJob(
                    self.create_list_job(),
                )));
            }
            Key::Char('.') => {
                self.show_hidden = !self.show_hidden;
                return EventResult::Action(Box::new(ExplorerAction::SpawnJob(
                    self.create_list_job(),
                )));
            }
            Key::Char('l') => {
                self.show_metadata = !self.show_metadata;
                self.update_view();
            }
            Key::Char('n') => {
                // New File
                self.open_input_box("New File", "Filename...", InputMode::NewFile);
            }
            Key::Char('N') => {
                // New Dir
                self.open_input_box("New Directory", "Directory name...", InputMode::NewDir);
            }
            Key::Char('d') => {
                // Delete
                let mut targets = Vec::new();
                for idx in &self.selected_indices {
                    if let Some(e) = self.entries.get(*idx) {
                        targets.push(e.path.clone());
                    }
                }
                if targets.is_empty() {
                    if let Some(visual_idx) = self.select_view.selected_line() {
                        if let Some(idx) = self.get_entry_index(visual_idx) {
                            if let Some(e) = self.entries.get(idx) {
                                targets.push(e.path.clone());
                            }
                        }
                    }
                }

                if !targets.is_empty() {
                    self.open_input_box("Delete? (y/n)", "", InputMode::DeleteConfirm(targets));
                }
            }
            Key::Char('r') => {
                // Rename
                if let Some(visual_idx) = self.select_view.selected_line() {
                    if let Some(idx) = self.get_entry_index(visual_idx) {
                        if let Some(e) = self.entries.get(idx) {
                            let name = e.name.clone();
                            let path = e.path.clone();
                            self.open_input_box("Rename to", &name, InputMode::Rename(path));
                        }
                    }
                }
            }
            Key::Char('c') => {
                // Copy
                if let Some(visual_idx) = self.select_view.selected_line() {
                    if let Some(idx) = self.get_entry_index(visual_idx) {
                        if let Some(e) = self.entries.get(idx) {
                            let name = e.name.clone();
                            let path = e.path.clone();
                            self.open_input_box("Copy to", &name, InputMode::Copy(path));
                        }
                    }
                }
            }

            _ => return EventResult::Ignored,
        }
        EventResult::Consumed
    }

    fn render(&mut self, layer: &mut Layer) {
        self.select_view.render(layer);

        if let Some(ib) = self.input_box.as_mut() {
            ib.render(layer);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl FileExplorer {
    pub fn handle_input_box_submit(&mut self, content: String) -> EventResult {
        let mode = self.input_mode.clone();
        self.close_input_box();

        match mode {
            InputMode::NewFile => {
                let is_dir = content.ends_with("/");
                let path = self.current_path.join(content);
                EventResult::Action(Box::new(ExplorerAction::SpawnJob(Box::new(
                    FsCreateJob::new(path, is_dir),
                ))))
            }
            InputMode::NewDir => {
                let path = self.current_path.join(content);
                EventResult::Action(Box::new(ExplorerAction::SpawnJob(Box::new(
                    FsCreateJob::new(path, true),
                ))))
            }
            InputMode::Rename(old_path) => {
                let new_path = old_path.parent().unwrap().join(content);
                EventResult::Action(Box::new(ExplorerAction::SpawnJob(Box::new(
                    FsMoveJob::new(old_path, new_path),
                ))))
            }
            InputMode::Copy(old_path) => {
                let new_path = old_path.parent().unwrap().join(content);

                EventResult::Action(Box::new(ExplorerAction::SpawnJob(Box::new(
                    FsCopyJob::new(old_path, new_path),
                ))))
            }
            InputMode::DeleteConfirm(targets) => {
                if content.to_lowercase() == "y" || content.to_lowercase() == "yes" {
                    if !targets.is_empty() {
                        EventResult::Action(Box::new(ExplorerAction::SpawnJob(Box::new(
                            FsBatchDeleteJob::new(targets),
                        ))))
                    } else {
                        EventResult::Consumed
                    }
                } else {
                    EventResult::Consumed
                }
            }
            InputMode::None => EventResult::Consumed,
        }
    }
}

#[cfg(test)]
mod tests;
