use super::{EditorAction, Motion};

pub(super) trait ActionArg: Sized {
    fn parse_arg(s: &str) -> Option<Self>;
}

impl ActionArg for String {
    fn parse_arg(s: &str) -> Option<Self> {
        Some(s.to_string())
    }
}

impl ActionArg for usize {
    fn parse_arg(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

macro_rules! action_table {
    ($( $id:literal $( ( $($arg:ident : $ty:ty),+ ) )? => [$($variant:tt)+] $op:literal $doc:literal ; )*) => {
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
}
