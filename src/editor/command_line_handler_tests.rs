use super::{Editor, Mode, ParsedCommand};
use crate::command_line::commands::parser::CommandParser;
use crate::command_line::commands::types::SplitSubcommand;
use crate::document::ops::{ContentKind, CursorCause};
use crate::editor::PanelKind;
use crate::notification::NotificationType;
use crate::selection::Region;
use crate::split::tree::{SplitDirection, SplitNode};
use crate::state::definitions::create_settings_registry;
use crate::test_utils::MockTerminal;
use crate::wrap::RangeKind;

fn editor() -> Editor<MockTerminal> {
    Editor::new(MockTerminal::new(24, 80)).unwrap()
}

fn editor_with_text(text: &str, cursor: usize) -> Editor<MockTerminal> {
    let mut editor = editor();
    let document = editor.active_document();
    document.replace_content(text, ContentKind::Render);
    document.set_cursor(cursor, CursorCause::User).unwrap();
    editor
}

fn submit(editor: &mut Editor<MockTerminal>, command: &str) {
    editor.set_mode(Mode::Command);
    editor.state.command_line = command.to_owned();
    editor.execute_command_line(command);
}

fn assert_notice(editor: &Editor<MockTerminal>, kind: NotificationType, message: &str) {
    assert!(editor
        .state
        .notifications
        .iter_active()
        .any(|notice| notice.kind == kind && notice.message.contains(message)));
}

fn assert_substitute_result(
    editor: &mut Editor<MockTerminal>,
    pattern: &str,
    count: usize,
    expected: &str,
) {
    assert_eq!(editor.active_document().buffer().to_string(), expected);
    assert_eq!(editor.state.last_search_query.as_deref(), Some(pattern));
    assert_notice(
        editor,
        NotificationType::Info,
        &format!("{count} substitutions"),
    );
    assert!(editor.state.command_line.is_empty());
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn test_execute_quit() {
    let mut editor = editor();
    submit(&mut editor, ":q");
    assert!(editor.should_quit);
}

#[cfg(feature = "plugins")]
#[test]
fn test_execute_unknown_command_passes_arguments_to_plugin() {
    let mut editor = editor();
    editor.plugin_host.mark_lua_used();
    editor.update_lua_state();
    let error = editor.plugin_host.lua_exec(
        "rift.register_command('zznonexistent', function(args) _G.seen = args end, 'probe')",
    );
    assert!(error.is_none(), "{error:?}");
    editor.apply_plugin_mutations();

    submit(&mut editor, ":zznonexistent arg1");

    let error = editor
        .plugin_host
        .lua_exec("assert(#_G.seen == 1 and _G.seen[1] == 'arg1')");
    assert!(error.is_none(), "{error:?}");
    assert_eq!(editor.current_mode, Mode::Normal);
}

#[test]
fn test_execute_unregistered_command() {
    let mut editor = editor();
    let command = ":zznonexistent arg1";
    submit(&mut editor, command);
    assert_notice(
        &editor,
        NotificationType::Error,
        "Unknown command: zznonexistent",
    );
    assert_eq!(editor.current_mode, Mode::Command);
    assert_eq!(editor.state.command_line, command);
}

#[test]
fn test_execute_ambiguous_command() {
    let mut editor = editor();
    assert!(!editor.apply_ex_command(
        ParsedCommand::Ambiguous {
            prefix: "se".to_owned(),
            matches: vec!["setup".to_owned(), "settings".to_owned()],
        },
        ":se",
    ));
    assert_notice(
        &editor,
        NotificationType::Error,
        "Ambiguous command 'se': matches setup, settings",
    );
}

#[test]
fn test_substitute_current_line() {
    let mut editor = editor_with_text("foo bar foo", 0);
    submit(&mut editor, ":s /foo/baz");
    assert_substitute_result(&mut editor, "foo", 1, "baz bar foo");
    editor.tick().unwrap();
    assert_eq!(editor.state.search_matches.len(), 1);
    assert_eq!(editor.state.search_matches[0].range, 8..11);
}

#[test]
fn test_substitute_global_line() {
    let mut editor = editor_with_text("foo bar foo", 0);
    submit(&mut editor, ":s /foo/baz/g");
    assert_substitute_result(&mut editor, "foo", 2, "baz bar baz");
}

#[test]
fn test_substitute_whole_file() {
    let mut editor = editor_with_text("foo\nbar\nfoo", 0);
    submit(&mut editor, ":s% /foo/baz/g");
    assert_substitute_result(&mut editor, "foo", 2, "baz\nbar\nbaz");
}

#[test]
fn test_substitute_first_match_on_each_line() {
    let mut editor = editor_with_text("foo foo\nfoo foo", 0);
    submit(&mut editor, ":s% /foo/baz");
    assert_substitute_result(&mut editor, "foo", 2, "baz foo\nbaz foo");
}

#[test]
fn test_substitute_case_insensitive() {
    let mut editor = editor_with_text("Foo bar foo", 0);
    submit(&mut editor, ":s /foo/baz");
    assert_substitute_result(&mut editor, "foo", 1, "baz bar foo");
}

#[test]
fn test_substitute_no_match() {
    let mut editor = editor_with_text("hello world", 0);
    let revision = editor.active_document().buffer().revision();
    let command = ":s /nothere/baz";
    submit(&mut editor, command);
    assert_eq!(editor.active_document().buffer().revision(), revision);
    assert_notice(&editor, NotificationType::Error, "Pattern not found");
    assert_eq!(editor.current_mode, Mode::Command);
    assert_eq!(editor.state.command_line, command);
}

#[test]
fn test_substitute_read_only_keeps_previous_search() {
    let mut editor = editor_with_text("foo bar", 0);
    editor.set_search_pattern(Some("bar"));
    editor.active_document().set_read_only(true);
    submit(&mut editor, ":s /foo/baz");
    assert_substitute_result(&mut editor, "bar", 0, "foo bar");
}

#[test]
fn test_substitute_parsing_requires_space() {
    let parser = CommandParser::new(create_settings_registry());
    match parser.parse("s/foo/bar") {
        ParsedCommand::Unknown { name, .. } => assert_eq!(name, "s/foo/bar"),
        other => panic!("Expected Unknown command, got {other:?}"),
    }
}

#[test]
fn test_substitute_parsing_with_space() {
    let parser = CommandParser::new(create_settings_registry());
    assert_eq!(
        parser.parse("s /foo/bar"),
        ParsedCommand::Substitute {
            pattern: "foo".to_owned(),
            replacement: "bar".to_owned(),
            flags: String::new(),
            range: None,
            bangs: 0,
        }
    );
}

#[test]
fn test_g_is_an_alias_for_the_git_command() {
    let parser = CommandParser::new(create_settings_registry());
    for (line, expected) in [("G log", "log"), ("G", "")] {
        match parser.parse(line) {
            ParsedCommand::Git { args, .. } => assert_eq!(args, expected),
            other => panic!("Expected Git command via the G alias, got {other:?}"),
        }
    }
}

#[test]
fn test_substitute_parsing_weird_behavior_percent() {
    let parser = CommandParser::new(create_settings_registry());
    assert_eq!(
        parser.parse("s % /ABC/XYZ"),
        ParsedCommand::Substitute {
            pattern: " /ABC/XYZ".to_owned(),
            replacement: String::new(),
            flags: String::new(),
            range: None,
            bangs: 0,
        }
    );
}

#[test]
fn test_substitute_undo_redo() {
    let original = "foo bar foo";
    let mut editor = editor_with_text(original, 0);
    submit(&mut editor, ":s /foo/baz/g");
    assert_substitute_result(&mut editor, "foo", 2, "baz bar baz");

    let document = editor.active_document();
    assert!(document.undo());
    assert_eq!(document.buffer().to_string(), original);
    assert!(document.redo());
    assert_eq!(document.buffer().to_string(), "baz bar baz");
    assert!(document.undo());
    assert_eq!(document.buffer().to_string(), original);
}

#[test]
fn test_substitute_whole_file_undo() {
    let original = "foo\nbar\nfoo\nbaz\nfoo";
    let mut editor = editor_with_text(original, 0);
    submit(&mut editor, ":s% /foo/qux/g");
    assert_substitute_result(&mut editor, "foo", 3, "qux\nbar\nqux\nbaz\nqux");

    let document = editor.active_document();
    assert!(document.undo());
    assert_eq!(document.buffer().to_string(), original);
    assert!(document.redo());
    assert_eq!(document.buffer().to_string(), "qux\nbar\nqux\nbaz\nqux");
}

#[test]
fn substitute_with_nonempty_set_scopes_to_banked_regions_only() {
    let mut editor = editor_with_text("foo bar foo baz foo", 0);
    let document = editor.active_document();
    document.bank_region(Region::new(0, 2, RangeKind::Charwise));
    document.bank_region(Region::new(16, 18, RangeKind::Charwise));

    submit(&mut editor, ":s% /foo/XXX/g");
    assert_substitute_result(&mut editor, "foo", 2, "XXX bar foo baz XXX");
    assert!(editor.active_document().selections().is_empty());
}

#[test]
fn substitute_end_anchor_appends_at_match() {
    let mut editor = editor_with_text("abc", 0);
    submit(&mut editor, ":s /$/;");
    assert_substitute_result(&mut editor, "$", 1, "abc;");
}

#[test]
fn substitute_start_anchor_prepends_at_match() {
    let mut editor = editor_with_text("abc", 2);
    submit(&mut editor, ":s /^/> /");
    assert_substitute_result(&mut editor, "^", 1, "> abc");
}

#[test]
fn test_execute_undotree() {
    let mut editor = editor();
    let document = editor.active_document_id();
    submit(&mut editor, ":undotree");
    let layout = editor.panel_layout.as_ref().unwrap();
    assert_eq!(layout.kind, PanelKind::UndoTree);
    assert_eq!(layout.original_doc_id, document);
}

#[test]
fn test_execute_file() {
    let mut editor = editor();
    submit(&mut editor, ":file");
    let layout = editor.panel_layout.as_ref().unwrap();
    assert_eq!(layout.kind, PanelKind::FileExplorer);
    assert_eq!(editor.active_document_id(), layout.dir_doc_id);
}

#[test]
fn test_execute_file_path_fails() {
    let mut editor = editor();
    let path = std::env::current_exe().unwrap();
    assert!(!editor.apply_ex_command(
        ParsedCommand::File {
            path: Some(path.to_string_lossy().into_owned()),
            bangs: 0,
        },
        ":file",
    ));
    assert_notice(&editor, NotificationType::Error, "Not a directory");
    assert!(editor.panel_layout.is_none());
}

#[test]
fn test_execute_split() {
    let mut editor = editor();
    let document = editor.active_document_id();
    submit(&mut editor, ":split");
    assert!(matches!(
        editor.split_tree.root(),
        SplitNode::Split {
            direction: SplitDirection::Horizontal,
            ..
        }
    ));
    assert_eq!(editor.active_document_id(), document);
    assert_eq!(editor.split_tree.window_count(), 2);
}

#[test]
fn test_execute_vsplit_file() {
    let mut editor = editor();
    let original = editor.active_document_id();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    assert!(editor.apply_ex_command(
        ParsedCommand::VSplit {
            subcommand: SplitSubcommand::File(path.to_string_lossy().into_owned()),
            bangs: 0,
        },
        ":vsplit",
    ));
    assert!(matches!(
        editor.split_tree.root(),
        SplitNode::Split {
            direction: SplitDirection::Vertical,
            ..
        }
    ));
    assert_ne!(editor.active_document_id(), original);
    assert_eq!(
        editor.active_document().path().unwrap().file_name(),
        path.file_name()
    );
}
