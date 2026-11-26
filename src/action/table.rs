use super::{EditorAction, Motion, OperatorType};
use crate::command_line::commands::types::SplitSubcommand;
use crate::split::navigation::Direction;
use crate::split::tree::SplitDirection;
use crate::text_objects::{self, TextObjectSpec};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgShape {
    Text,
    Path,
    Count,
    Int,
    Char,
    Motion,
    TextObject,
    SplitDirection,
    SplitSubcommand,
}

impl ArgShape {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Path => "path",
            Self::Count => "count",
            Self::Int => "int",
            Self::Char => "char",
            Self::Motion => "motion",
            Self::TextObject => "text_object",
            Self::SplitDirection => "split_direction",
            Self::SplitSubcommand => "split_subcommand",
        }
    }
}

#[derive(Debug)]
pub(crate) struct ActionSpec {
    pub(crate) id: &'static str,
    pub(crate) args: &'static [ArgShape],
    pub(crate) op: &'static str,
    pub(crate) doc: &'static str,
}

trait ActionArg: Sized {
    const SHAPE: ArgShape;
    fn parse_arg(s: &str) -> Option<Self>;
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl ActionArg for String {
    const SHAPE: ArgShape = ArgShape::Text;
    fn parse_arg(s: &str) -> Option<Self> {
        Some(s.to_string())
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl ActionArg for PathBuf {
    const SHAPE: ArgShape = ArgShape::Path;
    fn parse_arg(s: &str) -> Option<Self> {
        (!s.is_empty()).then(|| Self::from(s))
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

macro_rules! number_arg {
    ($($ty:ty => $shape:ident),+) => {
        $(
            impl ActionArg for $ty {
                const SHAPE: ArgShape = ArgShape::$shape;
                fn parse_arg(s: &str) -> Option<Self> {
                    s.parse().ok()
                }
                fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(f, "{self}")
                }
            }
        )+
    };
}

number_arg!(usize => Count, u64 => Count, i32 => Int);

impl ActionArg for char {
    const SHAPE: ArgShape = ArgShape::Char;
    fn parse_arg(s: &str) -> Option<Self> {
        let mut chars = s.chars();
        let ch = chars.next()?;
        chars.next().is_none().then_some(ch)
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

const MOVE_PREFIX: &str = "editor:move:";

impl ActionArg for Motion {
    const SHAPE: ArgShape = ArgShape::Motion;
    fn parse_arg(s: &str) -> Option<Self> {
        match parse_table(&format!("{MOVE_PREFIX}{s}")) {
            Some(EditorAction::Move(motion)) => Some(motion),
            _ => None,
        }
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = EditorAction::Move(*self).to_string();
        f.write_str(id.strip_prefix(MOVE_PREFIX).unwrap_or(&id))
    }
}

impl ActionArg for TextObjectSpec {
    const SHAPE: ArgShape = ArgShape::TextObject;
    fn parse_arg(s: &str) -> Option<Self> {
        let mut chars = s.chars().peekable();
        let modifier = text_objects::modifier_for_key(chars.next()?)?;
        let direction = match chars.peek() {
            Some('n') => text_objects::Direction::Next,
            Some('p') => text_objects::Direction::Last,
            _ => text_objects::Direction::Current,
        };
        if direction != text_objects::Direction::Current {
            chars.next();
        }
        let mut digits = String::new();
        while let Some(digit) = chars.next_if(char::is_ascii_digit) {
            digits.push(digit);
        }
        let nesting = digits.parse().ok()?;
        let kind = text_objects::object_kind_for_key(chars.next()?)?;
        chars.next().is_none().then_some(Self {
            modifier,
            direction,
            nesting,
            kind,
        })
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction = match self.direction {
            text_objects::Direction::Current => "",
            text_objects::Direction::Next => "n",
            text_objects::Direction::Last => "p",
        };
        let key = text_objects::object_key(self.kind).ok_or(fmt::Error)?;
        write!(
            f,
            "{}{direction}{}{key}",
            text_objects::modifier_key(self.modifier),
            self.nesting
        )
    }
}

impl ActionArg for Direction {
    const SHAPE: ArgShape = ArgShape::Text;
    fn parse_arg(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            _ => None,
        }
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        })
    }
}

impl ActionArg for SplitDirection {
    const SHAPE: ArgShape = ArgShape::SplitDirection;
    fn parse_arg(s: &str) -> Option<Self> {
        match s {
            "horizontal" => Some(Self::Horizontal),
            "vertical" => Some(Self::Vertical),
            _ => None,
        }
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        })
    }
}

impl ActionArg for SplitSubcommand {
    const SHAPE: ArgShape = ArgShape::SplitSubcommand;
    fn parse_arg(s: &str) -> Option<Self> {
        let (name, arg) = s.split_once(':').unwrap_or((s, ""));
        match (name, arg) {
            ("current", "") => Some(Self::Current),
            ("exchange", "") => Some(Self::Exchange),
            ("previous", "") => Some(Self::PreviousWindow),
            ("equalize", "") => Some(Self::Equalize),
            ("file", path) if !path.is_empty() => Some(Self::File(path.to_string())),
            ("navigate", dir) => Direction::parse_arg(dir).map(Self::Navigate),
            ("move", dir) => Direction::parse_arg(dir).map(Self::Move),
            ("resize", amount) => i32::parse_arg(amount).map(Self::Resize),
            _ => None,
        }
    }
    fn write_arg(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Current => f.write_str("current"),
            Self::Exchange => f.write_str("exchange"),
            Self::PreviousWindow => f.write_str("previous"),
            Self::Equalize => f.write_str("equalize"),
            Self::File(path) => write!(f, "file:{path}"),
            Self::Navigate(dir) => {
                f.write_str("navigate:")?;
                dir.write_arg(f)
            }
            Self::Move(dir) => {
                f.write_str("move:")?;
                dir.write_arg(f)
            }
            Self::Resize(amount) => write!(f, "resize:{amount}"),
        }
    }
}

macro_rules! action_table {
    ($( $id:literal $( ( $($arg:ident : $ty:ty),+ ) )? => [$($variant:tt)+] $op:literal $doc:literal ; )*) => {
        pub(crate) const ACTIONS: &[ActionSpec] = &[
            $(
                ActionSpec {
                    id: $id,
                    args: &[$($(<$ty as ActionArg>::SHAPE),+)?],
                    op: $op,
                    doc: $doc,
                },
            )*
        ];

        pub(super) fn parse_table(s: &str) -> Option<EditorAction> {
            $(
                if let Some(action) =
                    action_table!(@parse s, $id, [$($variant)+] $(, $($arg : $ty),+)?)
                {
                    return Some(action);
                }
            )*
            None
        }

        impl fmt::Display for EditorAction {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(
                        EditorAction::$($variant)+ => {
                            f.write_str($id)?;
                            $($(
                                f.write_str(":")?;
                                ActionArg::write_arg($arg, f)?;
                            )+)?
                            Ok(())
                        }
                    )*
                }
            }
        }
    };
    (@parse $s:ident, $id:literal, [$($variant:tt)+]) => {
        ($s == $id).then(|| EditorAction::$($variant)+)
    };
    (@parse $s:ident, $id:literal, [$($variant:tt)+], $($arg:ident : $ty:ty),+) => {
        $s.strip_prefix(concat!($id, ":")).and_then(|rest| {
            let mut parts = rest.splitn([$(stringify!($arg)),+].len(), ':');
            $( let $arg = <$ty as ActionArg>::parse_arg(parts.next()?)?; )+
            Some(EditorAction::$($variant)+)
        })
    };
}

action_table! {
    "editor:move:left" => [Move(Motion::Left)] "Document::move_cursor" "Move the cursor one character left";
    "editor:move:right" => [Move(Motion::Right)] "Document::move_cursor" "Move the cursor one character right";
    "editor:move:up" => [Move(Motion::Up)] "Document::move_cursor" "Move the cursor one line up";
    "editor:move:down" => [Move(Motion::Down)] "Document::move_cursor" "Move the cursor one line down";
    "editor:move:line:start" => [Move(Motion::StartOfLine)] "Document::move_cursor" "Move to the start of the line";
    "editor:move:line:end" => [Move(Motion::EndOfLine)] "Document::move_cursor" "Move to the end of the line";
    "editor:move:file:start" => [Move(Motion::StartOfFile)] "Document::move_cursor" "Move to the start of the file";
    "editor:move:file:end" => [Move(Motion::EndOfFile)] "Document::move_cursor" "Move to the end of the file";
    "editor:move:page:up" => [Move(Motion::PageUp)] "Document::move_cursor" "Move one page up";
    "editor:move:page:down" => [Move(Motion::PageDown)] "Document::move_cursor" "Move one page down";
    "editor:move:word:next" => [Move(Motion::NextWord)] "Document::move_cursor" "Move to the next word";
    "editor:move:word:prev" => [Move(Motion::PreviousWord)] "Document::move_cursor" "Move to the previous word";
    "editor:move:bigword:next" => [Move(Motion::NextBigWord)] "Document::move_cursor" "Move to the next whitespace-delimited word";
    "editor:move:bigword:prev" => [Move(Motion::PreviousBigWord)] "Document::move_cursor" "Move to the previous whitespace-delimited word";
    "editor:move:paragraph:next" => [Move(Motion::NextParagraph)] "Document::move_cursor" "Move to the next paragraph";
    "editor:move:paragraph:prev" => [Move(Motion::PreviousParagraph)] "Document::move_cursor" "Move to the previous paragraph";
    "editor:move:sentence:next" => [Move(Motion::NextSentence)] "Document::move_cursor" "Move to the next sentence";
    "editor:move:sentence:prev" => [Move(Motion::PreviousSentence)] "Document::move_cursor" "Move to the previous sentence";
    "editor:move:match:next" => [Move(Motion::NextMatch)] "Document::move_cursor" "Move to the next search match";
    "editor:move:match:prev" => [Move(Motion::PreviousMatch)] "Document::move_cursor" "Move to the previous search match";
    "editor:move:word:end" => [Move(Motion::WordEnd)] "Document::move_cursor" "Move to the end of the current or next word";
    "editor:move:find_char" (ch: char) => [Move(Motion::FindCharForward(ch))] "Document::move_cursor" "Move to the next occurrence of a character on the line";
    "editor:move:find_char_back" (ch: char) => [Move(Motion::FindCharBackward(ch))] "Document::move_cursor" "Move to the previous occurrence of a character on the line";
    "editor:move:till_char" (ch: char) => [Move(Motion::TillCharForward(ch))] "Document::move_cursor" "Move to just before the next occurrence of a character on the line";
    "editor:move:till_char_back" (ch: char) => [Move(Motion::TillCharBackward(ch))] "Document::move_cursor" "Move to just after the previous occurrence of a character on the line";
    "editor:move:find:repeat" => [Move(Motion::RepeatFindForward)] "Document::move_cursor" "Repeat the last character find in the same direction";
    "editor:move:find:repeat_back" => [Move(Motion::RepeatFindBackward)] "Document::move_cursor" "Repeat the last character find in the opposite direction";
    "editor:move:to_line" (line: usize) => [Move(Motion::ToLine(line))] "Document::move_cursor" "Move linewise to a 0-indexed line";
    "editor:move:text_object" (object: TextObjectSpec) => [Move(Motion::TextObject(object))] "Document::move_cursor" "Select a text object for the pending operator";

    "mode:normal" => [EnterNormalMode] "Editor::set_mode" "Enter Normal mode";
    "mode:insert" => [EnterInsertMode] "Editor::set_mode" "Enter Insert mode before the cursor";
    "mode:insert_after" => [EnterInsertModeAfter] "Editor::set_mode" "Enter Insert mode after the cursor";
    "mode:insert_line_start" => [EnterInsertModeAtLineStart] "Editor::set_mode" "Enter Insert mode at the first non-blank of the line";
    "mode:insert_line_end" => [EnterInsertModeAtLineEnd] "Editor::set_mode" "Enter Insert mode at the end of the line";
    "mode:open_line_below" => [OpenLineBelow] "Editor::set_mode" "Open a line below and enter Insert mode";
    "mode:open_line_above" => [OpenLineAbove] "Editor::set_mode" "Open a line above and enter Insert mode";
    "mode:command" => [EnterCommandMode] "Editor::set_mode" "Enter Command mode";
    "mode:search" => [EnterSearchMode] "Editor::set_mode" "Enter Search mode";
    "mode:visual" => [EnterVisualChar] "Editor::set_mode" "Enter charwise Visual mode";
    "mode:visual_line" => [EnterVisualLine] "Editor::set_mode" "Enter linewise Visual mode";
    "mode:visual_block" => [EnterVisualBlock] "Editor::set_mode" "Enter blockwise Visual mode";

    "buffer:next" => [BufferNext] "Editor::focus_document" "Switch to the next buffer";
    "buffer:prev" => [BufferPrevious] "Editor::focus_document" "Switch to the previous buffer";
    "buffer:list" => [ShowBufferList] "Editor::show_buffer_list" "Show the buffer list";

    "search:clear" => [ClearHighlights] "Editor::clear_search" "Clear search highlights";

    "notifications:clear" => [ClearNotifications] "Editor::clear_notifications" "Clear all notifications";
    "notifications:clear_last" => [ClearLastNotification] "Editor::clear_last_notification" "Clear the most recent notification";

    "history:undo" => [Undo] "Document::undo" "Undo the last change";
    "history:redo" => [Redo] "Document::redo" "Redo the last undone change";
    "history:checkpoint" => [Checkpoint] "Document::checkpoint" "Record an undo checkpoint";

    "explorer:open" => [OpenExplorer] "Editor::open_explorer" "Open the file explorer";
    "explorer:toggle_hidden" => [ExplorerToggleHidden] "Editor::explorer_toggle_hidden" "Toggle hidden files in the file explorer";
    "terminal:exit" => [ExitTerminalMode] "Editor::set_mode" "Leave terminal Insert mode";
    "terminal:scroll_up" => [TerminalScrollback(5)] "Editor::terminal_scroll" "Scroll the terminal up 5 lines";
    "terminal:scroll_down" => [TerminalScrollback(-5)] "Editor::terminal_scroll" "Scroll the terminal down 5 lines";
    "terminal:scroll_page_up" => [TerminalScrollback(20)] "Editor::terminal_scroll" "Scroll the terminal up 20 lines";
    "terminal:scroll_page_down" => [TerminalScrollback(-20)] "Editor::terminal_scroll" "Scroll the terminal down 20 lines";
    "undotree:open" => [OpenUndoTree] "Editor::open_undotree" "Open the undo tree";
    "terminal:open" => [OpenTerminal(None)] "Editor::open_terminal" "Open a terminal with the default shell";
    "messages:open" => [OpenMessages] "Editor::open_messages" "Open the messages buffer";
    "clipboard:open" => [OpenClipboard] "Editor::open_clipboard" "Open the clipboard ring";

    "editor:delete_line" => [DeleteLine] "Document::edit" "Delete the current line";
    "editor:delete_char" => [Delete(Motion::Right)] "Document::edit" "Delete the character under the cursor";
    "editor:delete_back" => [Delete(Motion::Left)] "Document::edit" "Delete the character before the cursor";
    "editor:toggle_debug" => [ToggleDebug] "Editor::toggle_debug" "Toggle the debug overlay";
    "editor:redraw" => [Redraw] "Editor::redraw" "Redraw the screen";
    "editor:reload" => [Reload] "Editor::reload_buffer" "Reload the current buffer from disk";
    "editor:save" => [Save] "Editor::save_buffer" "Save the current buffer";
    "editor:save_and_quit" => [SaveAndQuit] "Editor::save_buffer" "Save the current buffer and quit";
    "editor:quit" => [Quit] "Editor::quit" "Quit the editor";
    "editor:submit" => [Submit] "Editor::submit" "Submit the command line or search";
    "editor:history_up" => [HistoryUp] "Editor::history_up" "Recall the previous command-line entry";
    "editor:history_down" => [HistoryDown] "Editor::history_down" "Recall the next command-line entry";
    "editor:dot_repeat" => [DotRepeat] "Editor::dot_repeat" "Repeat the last change";

    "plugin" (id: String) => [PluginAction(id)] "Editor::run_plugin_action" "Run a plugin-registered action";
    "editor:run" (command: String) => [RunCommand(command)] "Editor::run_ex" "Run an ex command line";
    "editor:goto:last" => [GotoLine(0)] "Document::move_cursor" "Go to the last line";
    "editor:goto:line" (line: usize) => [GotoLine(line)] "Document::move_cursor" "Go to a 1-indexed line";
    "editor:search" (pattern: String) => [Search(pattern)] "Editor::search" "Search forward for a pattern";

    "lsp:definition" => [LspGotoDefinition] "Editor::lsp_goto_definition" "Go to the definition under the cursor";
    "lsp:references" => [LspReferences] "Editor::lsp_references" "List references to the symbol under the cursor";
    "lsp:hover" => [LspHover] "Editor::lsp_hover" "Show hover documentation";
    "lsp:rename" => [LspRename] "Editor::lsp_rename" "Rename the symbol under the cursor";
    "lsp:code_action" => [LspCodeAction] "Editor::lsp_code_action" "Show code actions";
    "lsp:format" => [LspFormat] "Editor::lsp_format" "Format the current buffer";
    "lsp:diagnostic:next" => [LspDiagnosticNext] "Editor::lsp_diagnostic_next" "Jump to the next diagnostic";
    "lsp:diagnostic:prev" => [LspDiagnosticPrev] "Editor::lsp_diagnostic_prev" "Jump to the previous diagnostic";
    "lsp:diagnostics_panel" => [LspDiagnosticsPanel] "Editor::lsp_diagnostics_panel" "Open the diagnostics panel";
    "annotation:activate" => [ActivateAnnotation] "Editor::activate_annotation" "Activate the annotation under the cursor";
    "annotation:activate" (verb: String) => [ActivateAnnotationVerb(verb)] "Editor::activate_annotation" "Run a named verb on the annotation under the cursor";
    "annotation:next" => [NextInteractiveAnnotation] "Editor::next_annotation" "Move to the next interactive annotation";
    "annotation:prev" => [PrevInteractiveAnnotation] "Editor::prev_annotation" "Move to the previous interactive annotation";

    "mode:replace" => [EnterReplaceMode] "Editor::set_mode" "Enter Replace mode";

    "editor:delete_word_back" => [Delete(Motion::PreviousWord)] "Document::edit" "Delete the word before the cursor";
    "editor:delete" (motion: Motion) => [Delete(motion)] "Document::edit" "Delete over a motion";
    "editor:change_line_end" => [ChangeToLineEnd] "Document::edit" "Change to the end of the line";
    "editor:delete_line_end" => [DeleteToLineEnd] "Document::edit" "Delete to the end of the line";
    "editor:insert" (ch: char) => [InsertChar(ch)] "Document::edit" "Insert a character";
    "editor:quit_key" => [QuitKey] "Editor::quit" "Quit as the q key does";
    "editor:find_char" => [FindCharPending { forward: true, till: false }] "Editor::await_char" "Find the next typed character on the line";
    "editor:find_char_back" => [FindCharPending { forward: false, till: false }] "Editor::await_char" "Find the previous typed character on the line";
    "editor:till_char" => [FindCharPending { forward: true, till: true }] "Editor::await_char" "Move to just before the next typed character on the line";
    "editor:till_char_back" => [FindCharPending { forward: false, till: true }] "Editor::await_char" "Move to just after the previous typed character on the line";
    "editor:replace_char" => [ReplaceCharPending] "Editor::await_char" "Replace characters with the next typed character";
    "editor:surround" => [SurroundStart] "Editor::surround" "Start a surround (sd, sc, sg)";
    "editor:surround_line" => [SurroundGiveLine] "Editor::surround" "Surround the whole line in a pending sg";

    "operator:delete" => [Operator(OperatorType::Delete)] "Editor::operator" "Delete over the next motion or the selection";
    "operator:change" => [Operator(OperatorType::Change)] "Editor::operator" "Change over the next motion or the selection";
    "operator:yank" => [Operator(OperatorType::Yank)] "Editor::operator" "Yank over the next motion or the selection";
    "operator:delete_lines" => [VisualLineOp(OperatorType::Delete)] "Editor::operator" "Delete the selected lines";
    "operator:change_lines" => [VisualLineOp(OperatorType::Change)] "Editor::operator" "Change the selected lines";
    "operator:yank_lines" => [VisualLineOp(OperatorType::Yank)] "Editor::operator" "Yank the selected lines";

    "selection:replace" (ch: char) => [ReplaceSelection(ch)] "Document::edit" "Replace every selected character";
    "selection:swap_ends" => [VisualSwapEnds] "Document::set_selection" "Swap the active end of the selection";
    "selection:occurrence_next" => [RegionBankOccurrenceNext] "Document::set_selection" "Add the next occurrence of the last region";
    "selection:occurrence_prev" => [RegionBankOccurrencePrev] "Document::set_selection" "Add the previous occurrence of the last region";
    "selection:add_surround" (ch: char, delim_count: usize) => [AddSurroundToSet { ch, delim_count }] "Document::edit" "Surround every region with a delimiter";
    "selection:expand" => [ExpandRegion] "Document::set_selection" "Grow the region to the enclosing text object";
    "selection:shrink" => [ShrinkRegion] "Document::set_selection" "Undo the last region expansion";

    "regions:toggle" => [ToggleRegionsWindow] "Editor::toggle_regions_window" "Toggle the regions list";
    "regions:down" => [RegionsListDown] "Editor::regions_list_move" "Move down the regions list";
    "regions:up" => [RegionsListUp] "Editor::regions_list_move" "Move up the regions list";
    "regions:select" => [RegionsListSelect] "Editor::regions_list_select" "Jump to the region under the cursor";
    "regions:drop" => [RegionsListDrop] "Editor::regions_list_drop" "Drop the region under the cursor";

    "clipboard:put_after" => [Put { before: false }] "Document::put" "Paste from the clipboard ring after the cursor";
    "clipboard:put_before" => [Put { before: true }] "Document::put" "Paste from the clipboard ring before the cursor";
    "clipboard:cycle_older" => [CyclePaste { forward: true }] "Document::put" "Replace the last paste with the next older entry";
    "clipboard:cycle_newer" => [CyclePaste { forward: false }] "Document::put" "Replace the last paste with the next newer entry";
    "clipboard:put_system_after" => [PutSystemClipboard { before: false }] "Document::put" "Paste from the system clipboard after the cursor";
    "clipboard:put_system_before" => [PutSystemClipboard { before: true }] "Document::put" "Paste from the system clipboard before the cursor";

    "buffer:edit" => [OpenFile { path: None, force: false }] "Editor::open_buffer" "Reopen the current file";
    "buffer:edit_force" => [OpenFile { path: None, force: true }] "Editor::open_buffer" "Reopen the current file, discarding changes";
    "buffer:open" (path: String) => [OpenFile { path: Some(path), force: false }] "Editor::open_buffer" "Open a file";
    "buffer:open_force" (path: String) => [OpenFile { path: Some(path), force: true }] "Editor::open_buffer" "Open a file, discarding changes";
    "explorer:open_dir" (path: PathBuf) => [OpenDirectory(path)] "Editor::open_explorer" "Open the file explorer on a directory";
    "terminal:run" (command: String) => [OpenTerminal(Some(command))] "Editor::open_terminal" "Open a terminal running a command";
    "terminal:scroll" (lines: i32) => [TerminalScrollback(lines)] "Editor::terminal_scroll" "Scroll the terminal (positive is up)";
    "window:split" (direction: SplitDirection, subcommand: SplitSubcommand) => [SplitWindow { direction, subcommand }] "Editor::split_window" "Split, focus, move or resize windows";

    "history:undo_count" => [UndoCount(None)] "Document::undo" "Undo the last change";
    "history:undo_count" (count: u64) => [UndoCount(Some(count))] "Document::undo" "Undo a number of changes";
    "history:redo_count" => [RedoCount(None)] "Document::redo" "Redo the last undone change";
    "history:redo_count" (count: u64) => [RedoCount(Some(count))] "Document::redo" "Redo a number of changes";
    "history:goto" (seq: u64) => [UndoGoto(seq)] "Document::goto_undo" "Go to an undo sequence number";
    "notifications:clear_all" => [NotificationClearAll] "Editor::clear_notifications" "Clear all notifications and the command line";

    "git:help" => [GitHelp] "Editor::git_help" "Show the key reference for the git buffer";
    "git_status:open" => [GitStatus] "Editor::git_status" "Open the git status buffer";
    "git_status:commit" => [GitCommitNew] "Editor::git_commit" "Open a new commit message";
    "git_status:commit_amend" => [GitCommitAmend] "Editor::git_commit" "Amend the HEAD commit message";
    "git_status:commit_fixup" => [GitCommitFixup] "Editor::git_commit" "Commit staged changes as a fixup of HEAD";
    "git_log:rebase" => [GitRebaseFromLogCommit] "Editor::git_rebase" "Rebase from the commit under the cursor";
    "git_rebase:abort" => [GitRebaseAbort] "Editor::git_rebase" "Abort the rebase";
    "git_rebase:move_up" => [GitRebaseMoveUp] "Editor::git_rebase" "Move the commit up";
    "git_rebase:move_down" => [GitRebaseMoveDown] "Editor::git_rebase" "Move the commit down";
    "git_rebase:pick" => [GitRebaseSetPick] "Editor::git_rebase" "Pick the commit";
    "git_rebase:squash" => [GitRebaseSetSquash] "Editor::git_rebase" "Squash the commit";
    "git_rebase:fixup" => [GitRebaseSetFixup] "Editor::git_rebase" "Fix up the commit";
    "git_rebase:edit" => [GitRebaseSetEdit] "Editor::git_rebase" "Stop to edit the commit";
    "git_rebase:drop" => [GitRebaseDrop] "Editor::git_rebase" "Drop the commit";
    "git_rebase:toggle_fold" => [GitRebaseToggleFold] "Editor::git_rebase" "Toggle the commit body preview";
    "git_rebase:reword" => [GitRebaseOpenMessage] "Editor::git_rebase" "Reword the commit message";
}
