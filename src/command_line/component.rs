use crate::component::{Component, EventResult};
use crate::editor::ComponentAction;
use crate::key::Key;
use crate::layer::Layer;
use crate::state::CommandLineWindowSettings;
/// Component header for command line input
pub struct CommandLineComponent {
    content: String,
    cursor: usize,
    prompt: char,
    settings: CommandLineWindowSettings,
    last_cursor_pos: Option<(u16, u16)>,
}

impl CommandLineComponent {
    pub fn new(prompt: char, settings: CommandLineWindowSettings) -> Self {
        Self {
            content: String::new(),
            cursor: 0,
            prompt,
            settings,
            last_cursor_pos: None,
        }
    }

    pub fn with_content(mut self, content: String) -> Self {
        let len = content.len();
        self.content = content;
        self.cursor = len;
        self
    }
}

impl Component for CommandLineComponent {
    fn handle_input(&mut self, key: Key) -> EventResult {
        match key {
            Key::Char(c) => {
                if self.cursor == self.content.len() {
                    self.content.push(c);
                } else {
                    self.content.insert(self.cursor, c);
                }
                self.cursor += 1;
                EventResult::Consumed
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    if self.cursor < self.content.len() {
                        self.content.remove(self.cursor);
                    }
                }
                EventResult::Consumed
            }
            Key::ArrowLeft => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                EventResult::Consumed
            }
            Key::ArrowRight => {
                if self.cursor < self.content.len() {
                    self.cursor += 1;
                }
                EventResult::Consumed
            }
            Key::Enter => {
                // Determine action based on prompt or context?
                // For now, we assume the creator of the component knows context,
                // but ComponentAction needs to distinguish.
                // Actually the prompt is a good indicator: ':' -> Command, '/' -> Search
                if self.prompt == '/' {
                    EventResult::Action(Box::new(ComponentAction::ExecuteSearch(
                        self.content.clone(),
                    )))
                } else {
                    EventResult::Action(Box::new(ComponentAction::ExecuteCommand(
                        self.content.clone(),
                    )))
                }
            }
            Key::Escape => EventResult::Action(Box::new(ComponentAction::CancelMode)),
            _ => EventResult::Ignored,
        }
    }

    fn render(&mut self, layer: &mut Layer) {
        use crate::command_line::{CommandLine, RenderOptions};
        use crate::viewport::Viewport; // We might need to construct a dummy viewport or modify render_to_layer signature?

        let viewport = Viewport::new(layer.rows(), layer.cols());

        let options = RenderOptions {
            default_border_chars: None, // Use default
            window_settings: &self.settings,
            fg: None, // Use default
            bg: None, // Use default
            prompt: self.prompt,
        };

        let (window_row, window_col, _, offset) =
            CommandLine::render_to_layer(layer, &viewport, &self.content, self.cursor, options);

        // Calculate and cache cursor position
        let (cursor_row, cursor_col) = CommandLine::calculate_cursor_position(
            (window_row, window_col),
            self.cursor,
            offset,
            self.settings.border,
        );
        self.last_cursor_pos = Some((cursor_row, cursor_col));
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        self.last_cursor_pos
    }
}
