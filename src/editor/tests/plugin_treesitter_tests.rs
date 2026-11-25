use super::common::*;
use super::*;
use crate::annotations::kind::well_known;
use crate::annotations::Anchor;
use crate::color::Color;
use crate::document::ops::{Edit, EditSource};
use crate::test_utils::MockTerminal;

const RUST: &str = "fn main() {\n    let answer = 42;\n    let b = answer;\n}\n";

fn editor_with(text: &str, lang: &str) -> Editor<MockTerminal> {
    let mut editor = create_editor();
    load_text(&mut editor, text);
    editor.plugin_host.mark_lua_used();
    lua(
        &mut editor,
        &format!("rift.treesitter.start(nil, '{lang}')"),
    );
    editor.update_and_render().unwrap();
    editor
}

fn lua(editor: &mut Editor<MockTerminal>, code: &str) {
    editor.update_lua_state();
    let err = editor.plugin_host.lua_exec(code);
    assert!(err.is_none(), "{err:?}");
    editor.apply_plugin_mutations();
}

fn lua_err(editor: &mut Editor<MockTerminal>, code: &str) -> String {
    editor.update_lua_state();
    editor
        .plugin_host
        .lua_exec(code)
        .expect("expected a Lua error")
}

fn set_cursor(editor: &mut Editor<MockTerminal>, needle: &str) {
    let text = editor.active_document().buffer().to_string();
    let char_idx = text[..text.find(needle).unwrap()].chars().count();
    editor
        .active_document()
        .buffer_mut()
        .set_cursor(char_idx)
        .unwrap();
}

fn content_row(editor: &mut Editor<MockTerminal>, row: usize) -> Vec<crate::layer::Cell> {
    editor.update_and_render().unwrap();
    let cols = editor.render_system.compositor.cols();
    editor.render_system.compositor.get_composited_slice()[row * cols..(row + 1) * cols].to_vec()
}

fn cell_of(editor: &mut Editor<MockTerminal>, row: usize, text: &str) -> crate::layer::Cell {
    let cells = content_row(editor, row);
    let line: String = cells.iter().map(|c| c.to_char()).collect();
    let col = line
        .find(text)
        .unwrap_or_else(|| panic!("{text:?} not on row {row}: {line:?}"));
    cells[line[..col].chars().count()]
}

#[test]
fn get_node_returns_the_node_under_the_cursor() {
    let mut editor = editor_with(RUST, "rust");
    set_cursor(&mut editor, "answer");
    lua(
        &mut editor,
        r#"
        local node = rift.treesitter.get_node()
        assert(node:type() == "identifier", node:type())
        assert(node:text() == "answer", node:text())
        local sr, sc, er, ec = node:range()
        assert(sr == 2 and sc == 8 and er == 2 and ec == 14, table.concat({ sr, sc, er, ec }, ","))
        assert(node:parent():type() == "let_declaration", node:parent():type())
        assert(rift.treesitter.get_language() == "rust")
        local root = node:tree():root()
        assert(root:type() == "source_file" and root:child_count() == 1)
        "#,
    );
}

#[test]
fn queries_apply_predicates_and_row_windows() {
    let mut editor = editor_with(RUST, "rust");
    lua(
        &mut editor,
        r#"
        local q = rift.treesitter.query("rust", '((identifier) @id (#eq? @id "answer"))')
        local root = rift.treesitter.get_tree():root()
        local rows = {}
        for name, node in q:iter_captures(root) do
          assert(name == "id" and node:text() == "answer")
          rows[#rows + 1] = (node:range())
        end
        assert(#rows == 2 and rows[1] == 2 and rows[2] == 3, table.concat(rows, ","))
        local in_window = 0
        for _ in q:iter_captures(root, 3, 3) do in_window = in_window + 1 end
        assert(in_window == 1, "row window should keep only row 3, got " .. in_window)
        for pattern, captures in q:iter_matches(root, 2, 2) do
          assert(pattern == 1 and captures.id[1]:text() == "answer")
        end
        "#,
    );
    let err = lua_err(
        &mut editor,
        "rift.treesitter.query('rust', '(not_a_node) @x')",
    );
    assert!(err.contains("invalid rust query"), "{err}");
}

#[test]
fn set_query_rehighlights_open_buffers_and_nil_restores_the_bundled_query() {
    let mut editor = editor_with(RUST, "rust");
    let magenta = Some(Color::Rgb {
        r: 255,
        g: 0,
        b: 255,
    });
    lua(
        &mut editor,
        "rift.highlight.set('keyword', { fg = '#ff00ff' })",
    );
    assert_eq!(cell_of(&mut editor, 1, "let").fg, magenta);

    lua(
        &mut editor,
        "rift.treesitter.set_query('rust', 'highlights', '(integer_literal) @keyword')",
    );
    editor.update_and_render().unwrap();
    let names = editor
        .active_document()
        .syntax
        .as_ref()
        .unwrap()
        .capture_names()
        .to_vec();
    assert_eq!(names, vec!["keyword"]);
    assert_eq!(cell_of(&mut editor, 1, "42").fg, magenta);
    assert_ne!(
        cell_of(&mut editor, 1, "let").fg,
        magenta,
        "let kept its old capture"
    );

    lua(
        &mut editor,
        "rift.treesitter.set_query('rust', 'highlights', nil)",
    );
    editor.update_and_render().unwrap();
    assert!(
        editor
            .active_document()
            .syntax
            .as_ref()
            .unwrap()
            .capture_names()
            .len()
            > 1
    );
    let err = lua_err(
        &mut editor,
        "rift.treesitter.set_query('rust', 'highlights', '(oops')",
    );
    assert!(err.contains("invalid rust query"), "{err}");
}

#[test]
fn set_query_accepts_a_grammar_registered_in_the_same_call() {
    let mut editor = editor_with(RUST, "rust");
    lua(
        &mut editor,
        "rift.treesitter.set_query('not_loaded_yet', 'highlights', '(anything) @x')",
    );
    assert_eq!(
        editor
            .language_loader
            .query_source(
                "not_loaded_yet",
                crate::syntax::loader::QueryKind::Highlights
            )
            .as_deref(),
        Some("(anything) @x")
    );
}

#[test]
fn highlight_groups_restyle_rendered_captures_and_survive_a_theme_switch() {
    let mut editor = editor_with(RUST, "rust");
    set_cursor(&mut editor, "answer");
    lua(
        &mut editor,
        r##"
        rift.highlight.set("keyword", { fg = "#ff0000", bg = "#000080", bold = true, italic = true })
        local s = rift.highlight.get("keyword.function")
        assert(s.fg == "#ff0000" and s.bg == "#000080" and s.bold and s.italic and not s.underline)
        "##,
    );
    let navy = Some(Color::Rgb { r: 0, g: 0, b: 128 });
    let cell = cell_of(&mut editor, 0, "fn");
    assert_eq!(cell.fg, Some(Color::Rgb { r: 255, g: 0, b: 0 }));
    assert_eq!(cell.bg, navy);
    assert!(cell.attrs.is_bold() && cell.attrs.is_italic());

    editor.execute_command_line("set colorscheme dark");
    assert_eq!(editor.state.settings.theme.as_deref(), Some("dark"));
    assert_eq!(
        cell_of(&mut editor, 0, "fn").bg,
        navy,
        "override lost on theme switch"
    );

    lua(&mut editor, "rift.highlight.reset('keyword')");
    let dark = crate::color::theme::Theme::dark().syntax_style("keyword");
    assert_eq!(
        editor
            .state
            .settings
            .syntax_colors
            .as_ref()
            .unwrap()
            .exact_style("keyword"),
        dark
    );
    assert_ne!(cell_of(&mut editor, 0, "fn").bg, navy);
}

fn select_fn_main(editor: &mut Editor<MockTerminal>) {
    use crate::action::{Action, EditorAction};
    set_cursor(editor, "fn");
    editor.handle_action(&Action::Editor(EditorAction::EnterVisualChar));
    set_cursor(editor, "n()");
}

#[test]
fn visual_selection_keeps_syntax_colors_under_the_ui_selection_tint() {
    let mut editor = editor_with(RUST, "rust");
    set_cursor(&mut editor, "answer");
    let colors = editor.state.settings.syntax_colors.clone().unwrap();
    let keyword = cell_of(&mut editor, 0, "fn").fg;
    let function = cell_of(&mut editor, 0, "main").fg;
    assert_eq!(keyword, colors.get_color("keyword"));
    assert_eq!(function, colors.get_color("function"));
    assert_ne!(
        keyword, function,
        "the check needs two distinct syntax colors"
    );

    select_fn_main(&mut editor);

    let tint = colors.exact_style("ui.selection").and_then(|s| s.bg);
    assert!(tint.is_some());
    for (text, fg) in [("fn", keyword), ("main", function)] {
        let cell = cell_of(&mut editor, 0, text);
        assert_eq!((cell.fg, cell.bg), (fg, tint), "{text}");
    }
}

#[test]
fn plugins_restyle_the_selection_through_the_ui_selection_group() {
    let mut editor = editor_with(RUST, "rust");
    set_cursor(&mut editor, "answer");
    let keyword = cell_of(&mut editor, 0, "fn").fg;
    lua(
        &mut editor,
        r##"rift.highlight.set("ui.selection", { bg = "#ff0000" })"##,
    );
    select_fn_main(&mut editor);

    let cell = cell_of(&mut editor, 0, "fn");
    assert_eq!(cell.bg, Some(Color::Rgb { r: 255, g: 0, b: 0 }));
    assert_eq!(cell.fg, keyword);
}

#[test]
fn namespaced_highlights_from_nodes_render_over_syntax() {
    let mut editor = editor_with(RUST, "rust");
    set_cursor(&mut editor, "answer");
    lua(
        &mut editor,
        r##"
        local node = rift.treesitter.get_node()
        rift.highlight.add("probe", node, { fg = "#00ff00", underline = true })
        "##,
    );
    let ranges: Vec<(usize, usize)> = editor
        .active_document()
        .annotations
        .query_kind(well_known::PLUGIN_HIGHLIGHT)
        .map(|a| match a.anchor {
            Anchor::Range(s, e) => (s.offset, e.offset),
            other => panic!("unexpected anchor {other:?}"),
        })
        .collect();
    let start = RUST.find("answer").unwrap();
    assert_eq!(ranges, vec![(start, start + "answer".len())]);
    let cell = cell_of(&mut editor, 1, "answer");
    assert_eq!(cell.fg, Some(Color::Rgb { r: 0, g: 255, b: 0 }));
    assert!(cell.attrs.is_underline());

    lua(&mut editor, "rift.highlight.clear('probe')");
    assert_eq!(
        editor
            .active_document()
            .annotations
            .query_kind(well_known::PLUGIN_HIGHLIGHT)
            .count(),
        0
    );
}

#[test]
fn syntax_updated_hands_handlers_the_reparsed_tree() {
    let mut editor = editor_with(RUST, "rust");
    lua(
        &mut editor,
        r#"
        SEEN = {}
        rift.on("SyntaxUpdated", function(ev)
          local tree = rift.treesitter.get_tree(ev.buf)
          SEEN[#SEEN + 1] = (ev.lang or "none") .. ":" .. (tree and tree:root():child_count() or 0)
        end)
        "#,
    );
    editor.active_document().buffer_mut().move_to_end();
    editor
        .active_document()
        .edit(Edit::type_text("fn extra() {}\n"), EditSource::User)
        .unwrap();
    editor.update_and_render().unwrap();
    lua(
        &mut editor,
        "assert(SEEN[#SEEN] == 'rust:2', table.concat(SEEN, ' '))",
    );

    lua(&mut editor, "rift.treesitter.stop()");
    editor.update_and_render().unwrap();
    assert!(editor.active_document().syntax.is_none());
    lua(
        &mut editor,
        "assert(SEEN[#SEEN] == 'none:0', table.concat(SEEN, ' ')); assert(rift.treesitter.get_tree() == nil)",
    );
}

#[test]
fn tree_is_stale_after_a_local_edit_until_the_next_syntax_update() {
    let mut editor = editor_with(RUST, "rust");
    lua(
        &mut editor,
        r#"
        STALE = {}
        rift.on("SyntaxUpdated", function()
          STALE[#STALE + 1] = rift.treesitter.get_tree():is_stale()
        end)
        "#,
    );
    lua(
        &mut editor,
        r#"
        local tree = rift.treesitter.get_tree()
        assert(not tree:is_stale())
        rift.insert("// note\n")
        assert(tree:is_stale())
        assert(rift.treesitter.get_tree():is_stale())
        "#,
    );
    editor.update_and_render().unwrap();
    lua(
        &mut editor,
        "assert(#STALE > 0 and STALE[#STALE] == false, tostring(STALE[#STALE])) \
         assert(not rift.treesitter.get_tree():is_stale())",
    );
}

#[test]
fn background_tree_is_stale_after_a_local_edit() {
    let mut editor = editor_with(RUST, "rust");
    let first = editor.active_document_id().get();
    lua(&mut editor, "rift.create_scratch_buf('other', { 'x' })");
    lua(
        &mut editor,
        &format!(
            r#"
            assert(rift.current_buf() ~= {first})
            assert(rift.get_filetype({{ buf = {first} }}) == 'rust')
            assert(rift.get_filetype() == nil)
            local tree = rift.treesitter.get_tree({first})
            assert(not tree:is_stale())
            rift.insert("// note\n", {{ buf = {first} }})
            assert(tree:is_stale())
            assert(rift.treesitter.get_tree() == nil)
            "#
        ),
    );
}

#[test]
fn parse_and_captures_at_cover_strings_and_injected_layers() {
    let mut editor = editor_with("# Title\n\n```rust\nlet x = 1;\n```\n", "markdown");
    set_cursor(&mut editor, "let");
    lua(
        &mut editor,
        r#"
        local trees = rift.treesitter.get_trees()
        assert(#trees >= 2 and trees[1]:lang() == "markdown", #trees)
        local node = rift.treesitter.get_node({ injections = true })
        assert(node:lang() == "rust", node:lang())
        local caps = rift.treesitter.captures_at()
        assert(#caps > 0 and caps[1].lang == "rust" and caps[1].capture == "keyword", caps[1] and caps[1].capture)

        local tree = rift.treesitter.parse("lua", "local t = { 1, 2 }")
        assert(tree:buf() == nil and tree:lang() == "lua" and tree:root():child_count() == 1)
        assert(tree:root():child(1):text() == "local t = { 1, 2 }")
        "#,
    );
}

#[test]
fn cursor_moved_handlers_read_the_moved_cursor() {
    let mut editor = editor_with(RUST, "rust");
    lua(
        &mut editor,
        "MOVES = {} rift.on('CursorMoved', function(ev) local r, c = rift.get_cursor(); MOVES[#MOVES + 1] = (r == ev.row and c == ev.col) end)",
    );
    for _ in 0..3 {
        editor.execute_buffer_command(crate::command::Command::Move(
            crate::action::Motion::Right,
            1,
        ));
        editor.update_and_render().unwrap();
    }
    lua(
        &mut editor,
        "assert(#MOVES == 3); for _, ok in ipairs(MOVES) do assert(ok) end",
    );
}
