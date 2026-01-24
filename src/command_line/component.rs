use crate::color::Color;
use crate::component::{Component, EventResult};
use crate::key::Key;
use crate::layer::Layer;
use crate::message::{AppMessage, CommandLineMessage};
use crate::state::CommandLineWindowSettings;
/// Component header for command line input
pub struct CommandLineComponent {
    content: String,
    cursor: usize,
    prompt: char,
    settings: CommandLineWindowSettings,
    last_cursor_pos: Option<(u16, u16)>,
    fg: Option<Color>,
    bg: Option<Color>,
}

impl CommandLineComponent {
    pub fn new(
        prompt: char,
        settings: CommandLineWindowSettings,
        fg: Option<Color>,
        bg: Option<Color>,
    ) -> Self {
        Self {
            content: String::new(),
            cursor: 0,
            prompt,
            settings,
            last_cursor_pos: None,
            fg,
            bg,
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
            Key::CtrlArrowLeft => {
                self.cursor = crate::movement::boundaries::prev_word(&self.content, self.cursor);
                EventResult::Consumed
            }
            Key::CtrlArrowRight => {
                self.cursor = crate::movement::boundaries::next_word(&self.content, self.cursor);
                EventResult::Consumed
            }
            Key::Home => {
                self.cursor = 0;
                EventResult::Consumed
            }
            Key::End => {
                self.cursor = self.content.len();
                EventResult::Consumed
            }
            Key::Delete => {
                if self.cursor < self.content.len() {
                    self.content.remove(self.cursor);
                }
                EventResult::Consumed
            }
            Key::Enter => {
                if self.prompt == '/' {
                    EventResult::Message(AppMessage::CommandLine(
                        CommandLineMessage::ExecuteSearch(self.content.clone()),
                    ))
                } else {
                    EventResult::Message(AppMessage::CommandLine(
                        CommandLineMessage::ExecuteCommand(self.content.clone()),
                    ))
                }
            }
            Key::Escape => {
                EventResult::Message(AppMessage::CommandLine(CommandLineMessage::CancelMode))
            }
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
            fg: self.fg,
            bg: self.bg,
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
