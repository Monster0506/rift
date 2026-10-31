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

fn vp_editor(text: &str) -> Editor<crate::replay::backend::ReplayBackend<std::io::Sink>> {
    let backend = crate::replay::backend::ReplayBackend::new(std::io::sink(), 24, 80);
    let mut editor = Editor::new(backend).unwrap();
    editor.active_document().buffer.insert_str(text).unwrap();
    editor.active_document().buffer.move_to_start();
    editor
}

fn vp_replay(
    editor: &mut Editor<crate::replay::backend::ReplayBackend<std::io::Sink>>,
    keys: &str,
) {
    let keys = crate::key::notation::parse_key_sequence(keys).unwrap();
    editor.term.push_keys(keys);
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }
}

fn vp_run(text: &str, keys: &str) -> (String, Mode) {
    let mut editor = vp_editor(text);
    vp_replay(&mut editor, keys);
    (
        editor.active_document().buffer.to_string(),
        editor.current_mode,
    )
}

#[test]
fn bug_visual_char_put_inserts_instead_of_replacing() {
    assert_eq!(
        vp_run("abc def", "ywwvep"),
        ("abc abc ".to_string(), Mode::Normal)
    );
}

#[test]
fn bug_visual_line_put_inserts_instead_of_replacing() {
    assert_eq!(
        vp_run("one\ntwo\nthree", "yyjVp"),
        ("one\none\nthree".to_string(), Mode::Normal)
    );
}

#[test]
fn visual_put_last_line_stays_last() {
    for keys in ["yyGVp", "yyGVP"] {
        assert_eq!(
            vp_run("one\ntwo\nthree", keys),
            ("one\ntwo\none".to_string(), Mode::Normal),
            "{keys}"
        );
    }
}

#[test]
fn visual_put_undo_restores_region() {
    for keys in ["ywwvepu", "ywwvePu"] {
        let mut editor = vp_editor("abc def");
        vp_replay(&mut editor, keys);
        let doc = editor.active_document();
        assert_eq!(
            (doc.buffer.to_string(), doc.buffer.cursor()),
            ("abc def".to_string(), 4),
            "{keys}"
        );
    }
}

#[test]
fn visual_put_char_into_whole_line() {
    assert_eq!(
        vp_run("one\ntwo\nthree", "yljVp"),
        ("one\no\nthree".to_string(), Mode::Normal)
    );
}

#[test]
fn visual_put_line_into_partial_line() {
    assert_eq!(
        vp_run("one\ntwo\nthree", "yyjvlp"),
        ("one\n\none\no\nthree".to_string(), Mode::Normal)
    );
}

#[test]
fn visual_put_swaps_deleted_register_with_selection_kind() {
    for (text, keys, deleted, kind) in [
        ("abc def", "ywwvep", "def", crate::wrap::RangeKind::Charwise),
        (
            "one\ntwo\nthree",
            "yljVp",
            "two\n",
            crate::wrap::RangeKind::Linewise,
        ),
        (
            "one\ntwo\nthree",
            "yyjvlp",
            "tw",
            crate::wrap::RangeKind::Charwise,
        ),
    ] {
        let mut editor = vp_editor(text);
        vp_replay(&mut editor, keys);
        let clip = editor.clipboard_ring.most_recent().unwrap();
        assert_eq!(
            (
                clip.text()
                    .iter()
                    .copied()
                    .map(crate::character::Character::to_char_lossy)
                    .collect::<String>(),
                clip.kind(),
            ),
            (deleted.to_string(), kind),
            "{keys}"
        );
    }
}

#[test]
fn visual_uppercase_put_preserves_register_and_kind() {
    for (text, keys, source, kind) in [
        (
            "abc def",
            "ywwveP",
            "abc ",
            crate::wrap::RangeKind::Charwise,
        ),
        (
            "one\ntwo\nthree",
            "yljVP",
            "o",
            crate::wrap::RangeKind::Charwise,
        ),
        (
            "one\ntwo\nthree",
            "yyjvlP",
            "one\n",
            crate::wrap::RangeKind::Linewise,
        ),
    ] {
        let mut editor = vp_editor(text);
        vp_replay(&mut editor, keys);
        let clip = editor.clipboard_ring.most_recent().unwrap();
        assert_eq!(
            (
                clip.text()
                    .iter()
                    .copied()
                    .map(crate::character::Character::to_char_lossy)
                    .collect::<String>(),
                clip.kind(),
            ),
            (source.to_string(), kind),
            "{keys}"
        );
        assert_eq!(editor.clipboard_ring.len(), 1, "{keys}");
    }
}

#[test]
fn visual_put_vim_boundaries_and_cursor() {
    use crate::wrap::RangeKind::{Charwise, Linewise};

    for (text, selection, source, kind, expected, cursor) in [
        ("abc", "lvh", "XY", Charwise, "XYc", 1),
        ("abc", "V", "  XY", Charwise, "  XY", 2),
        ("abc", "V", "   ", Charwise, "   ", 2),
        ("abc", "$v", "XY", Charwise, "abXY", 3),
        ("abc\ndef", "V", "X\n", Charwise, "X\n\ndef", 0),
        ("abc", "V", "X\n", Charwise, "X\n", 0),
        ("abc", "V", "X\n", Linewise, "X", 0),
        ("abc", "v$", "X\n", Linewise, "\nX\n", 1),
        ("abc", "vl", "X\n", Linewise, "\nX\nc", 1),
        ("abc\ndef", "vl", "A\nB", Charwise, "A\nBc\ndef", 0),
        ("abc\ndef", "jVk", "X", Charwise, "X", 0),
        ("abc\ndef", "jV", "X\n", Linewise, "abc\nX", 4),
        ("abc\ndef\n", "jV", "X\n", Linewise, "abc\nX\n", 4),
        ("", "v", "X", Charwise, "X", 0),
        ("", "V", "X", Charwise, "X", 0),
        ("", "V", "X\n", Linewise, "X", 0),
        ("", "v", "X\n", Linewise, "\nX\n", 1),
        ("abc", "v", "\n", Linewise, "\n\nbc", 1),
    ] {
        for put in ["p", "P"] {
            let mut editor = vp_editor(text);
            editor.clipboard_ring.push_str(source, kind);
            let keys = format!("{selection}{put}");
            vp_replay(&mut editor, &keys);
            assert_eq!(
                editor.active_document().buffer.to_string(),
                expected,
                "{keys}, source {source:?}"
            );
            assert_eq!(
                editor.active_document().buffer.cursor(),
                cursor,
                "{keys}, source {source:?}"
            );
            assert_eq!(editor.current_mode, Mode::Normal, "{keys}");
            assert_eq!(editor.visual_anchor, None, "{keys}");
            assert!(editor.active_document().selection_set.is_empty(), "{keys}");
            vp_replay(&mut editor, "u");
            assert_eq!(
                editor.active_document().buffer.to_string(),
                text,
                "{keys}: one undo"
            );
        }
    }
}

#[test]
fn visual_put_resolves_pending_ghost_before_register_swap() {
    use crate::wrap::RangeKind;

    let mut editor = vp_editor("one\ntwo\nthree");
    vp_replay(&mut editor, "ddV");
    editor.clipboard_ring.push_str("wrong", RangeKind::Charwise);
    vp_replay(&mut editor, "p");
    assert_eq!(editor.active_document().buffer.to_string(), "one\nthree");
    assert_eq!(
        editor.clipboard_ring.most_recent().unwrap().kind(),
        RangeKind::Linewise
    );
    assert!(editor.active_document().most_recent_ghost().is_none());
    assert!(editor.document_manager.most_recent_ghost_doc().is_none());
    vp_replay(&mut editor, "u");
    assert_eq!(editor.active_document().buffer.to_string(), "two\nthree");
}

#[test]
fn visual_put_undo_restores_annotation_anchors_in_same_transaction() {
    use crate::annotations::{Anchor, Annotation, AnnotationOwner};

    let mut editor = vp_editor("abc def");
    let anchor = Anchor::range(4, 7);
    let id = editor.active_document().annotations.add(Annotation::new(
        crate::annotations::kind::Kind::new("ui.visual_put_test"),
        anchor,
        AnnotationOwner::User,
    ));
    vp_replay(&mut editor, "ywwvepu");
    assert_eq!(
        editor.active_document().annotations.get(id).unwrap().anchor,
        anchor
    );
    assert_eq!(editor.active_document().buffer.to_string(), "abc def");
}

#[test]
fn visual_put_repeats_source_count() {
    for (text, keys, expected, cursor) in [
        ("abc def", "ywwve2p", "abc abc abc ", 11),
        ("abc def", "ywwve2P", "abc abc abc ", 11),
        ("one\ntwo\nthree", "yyGV2p", "one\ntwo\none\none", 8),
        ("one\ntwo\nthree", "yyjV2P", "one\none\none\nthree", 4),
    ] {
        let mut editor = vp_editor(text);
        vp_replay(&mut editor, keys);
        assert_eq!(
            editor.active_document().buffer.to_string(),
            expected,
            "{keys}"
        );
        assert_eq!(editor.active_document().buffer.cursor(), cursor, "{keys}");
        vp_replay(&mut editor, "u");
        assert_eq!(
            editor.active_document().buffer.to_string(),
            text,
            "{keys}: undo"
        );
    }
}
