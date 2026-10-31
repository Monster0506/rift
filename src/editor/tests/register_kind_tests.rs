use super::*;
use crate::replay::backend::ReplayBackend;

enum GhostCut {
    On,
    Off,
}

fn run(text: &str, keys: &str, ghost_cut: GhostCut) -> (String, (usize, usize)) {
    let backend = ReplayBackend::new(std::io::sink(), 24, 80);
    let mut editor = Editor::new(backend).unwrap();
    editor.state.settings.editing.ghost_cut = matches!(ghost_cut, GhostCut::On);

    let doc = editor.active_document();
    doc.buffer.insert_str(text).unwrap();
    doc.buffer.move_to_start();

    let keys = crate::key::notation::parse_key_sequence(keys).expect("valid key sequence");
    editor.term.push_keys(keys);
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }

    let doc = editor.active_document();
    let offset = doc.buffer.cursor();
    let line = doc.buffer.line_index.get_line_at(offset);
    let start = doc.buffer.line_index.get_start(line).unwrap_or(0);
    (doc.buffer.to_string(), (line, offset - start))
}

#[track_caller]
fn assert_put(text: &str, keys: &str, ghost_cut: GhostCut, want: (&str, (usize, usize))) {
    let (got_text, got_cursor) = run(text, keys, ghost_cut);
    assert_eq!(
        (got_text.as_str(), got_cursor),
        want,
        "text={text:?} keys={keys:?}"
    );
}

#[track_caller]
fn assert_text(text: &str, keys: &str, ghost_cut: GhostCut, want: &str) {
    assert_eq!(
        run(text, keys, ghost_cut).0,
        want,
        "text={text:?} keys={keys:?}"
    );
}

#[test]
fn bug_linewise_last_line_no_eol_yank_put() {
    assert_put(
        "one\ntwo\nthree",
        "Gyyp",
        GhostCut::Off,
        ("one\ntwo\nthree\nthree", (3, 0)),
    );
}

#[test]
fn bug_linewise_dd_last_line_put_charwise() {
    assert_put(
        "one\ntwo\nthree",
        "Gddp",
        GhostCut::Off,
        ("one\ntwo\nthree", (2, 0)),
    );
}

#[test]
fn bug_linewise_visual_last_line_put_before() {
    assert_put(
        "one\ntwo\nthree",
        "GVyP",
        GhostCut::Off,
        ("one\ntwo\nthree\nthree", (2, 0)),
    );
}

#[test]
fn bug_charwise_newline_yank_put_linewise() {
    assert_put(
        "abc\nxyz",
        "v$yGp",
        GhostCut::Off,
        ("abc\nxabc\nyz", (1, 1)),
    );
}

#[test]
fn bug_yyp_empty_buffer_no_op() {
    assert_put("", "yyp", GhostCut::Off, ("\n", (1, 0)));
}

#[test]
fn bug_yjp_last_empty_line_dropped() {
    assert_put("abc\n", "yjp", GhostCut::Off, ("abc\nabc\n\n", (1, 0)));
}

#[test]
fn bug_yank_line_on_last_line_drops_newline() {
    assert_text("a", "yyp", GhostCut::On, "a\na");
    assert_text("a\nb", "jyyP", GhostCut::On, "a\nb\nb");
}

#[test]
fn bug_dd_last_line_register_is_charwise() {
    assert_text("a\nb\nc", "GddP", GhostCut::Off, "a\nc\nb");
    assert_text("a\nb\nc", "Gddkp", GhostCut::On, "a\nc\nb");
}
