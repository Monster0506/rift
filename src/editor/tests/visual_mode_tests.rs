use super::*;
use crate::replay::backend::ReplayBackend;

fn replay_editor(text: &str) -> Editor<ReplayBackend<std::io::Sink>> {
    let backend = ReplayBackend::new(std::io::sink(), 24, 80);
    let mut editor = Editor::new(backend).unwrap();
    editor.active_document().buffer.insert_str(text).unwrap();
    editor.active_document().buffer.move_to_start();

    editor
}

fn press(editor: &mut Editor<ReplayBackend<std::io::Sink>>, keys: &str) {
    let keys = crate::key::notation::parse_key_sequence(keys).unwrap();
    editor.term.push_keys(keys);
    while editor.term.poll(std::time::Duration::ZERO).unwrap() {
        editor.tick().unwrap();
    }
}

fn run(text: &str, keys: &str) -> (String, Mode) {
    let mut editor = replay_editor(text);
    press(&mut editor, keys);
    (
        editor.active_document().buffer.to_string(),
        editor.current_mode,
    )
}

#[test]
fn visual_x_deletes_the_selection() {
    assert_eq!(run("abc", "vlx"), ("c".to_string(), Mode::Normal));
}

#[test]
fn bug_visual_r_ignores_selection() {
    assert_eq!(run("abc", "vllrX"), ("XXX".to_string(), Mode::Normal));
}

#[test]
fn visual_upper_delete_is_linewise() {
    for keys in ["vjD", "vjX", "jvkD", "jvkX"] {
        assert_eq!(
            run("abc\ndef\nghi", keys),
            ("ghi".to_string(), Mode::Normal),
            "{keys}"
        );
    }
}

#[test]
fn visual_change_keeps_line_break() {
    for keys in ["Vjc!<Esc>", "vjC!<Esc>"] {
        assert_eq!(
            run("abc\ndef\nghi", keys),
            ("!\nghi".to_string(), Mode::Normal),
            "{keys}"
        );
    }
}

#[test]
fn visual_change_updates_register() {
    assert_eq!(
        run("abc def", "ywwvec!<Esc>p"),
        ("abc !def".to_string(), Mode::Normal)
    );
}

#[test]
fn visual_upper_yank_is_linewise() {
    assert_eq!(
        run("abc\ndef", "vlYp"),
        ("abc\nabc\ndef".to_string(), Mode::Normal)
    );
}

#[test]
fn visual_line_delete_handles_final_and_empty_lines() {
    for (text, keys, expected) in [
        ("abc\ndef", "jVD", "abc"),
        ("abc\ndef", "jVX", "abc"),
        ("abc\n", "jVd", "abc"),
        ("abc", "VD", ""),
        ("", "Vd", ""),
        ("\nnext", "Vd", "next"),
        ("abc\n", "jvd", "abc\n"),
        ("", "vd", ""),
        ("", "vx", ""),
    ] {
        assert_eq!(
            run(text, keys),
            (expected.to_string(), Mode::Normal),
            "{text:?}: {keys}"
        );
    }
}

#[test]
fn visual_line_change_retains_one_editable_line_at_boundaries() {
    for (text, keys, expected) in [
        ("abc\ndef", "jVc!<Esc>", "abc\n!"),
        ("abc\ndef", "jvC!<Esc>", "abc\n!"),
        ("abc\n", "jVc!<Esc>", "abc\n!"),
        ("abc\n", "Vc!<Esc>", "!\n"),
        ("abc", "Vc!<Esc>", "!"),
        ("", "Vc!<Esc>", "!"),
        ("", "vc!<Esc>", "!"),
        ("\nnext", "Vc!<Esc>", "!\nnext"),
        ("abc\ndef\nghi", "jVkc!<Esc>", "!\nghi"),
        ("abc\ndef\nghi", "jvkC!<Esc>", "!\nghi"),
        ("abc\ndef\nghi", "Vjjc!<Esc>", "!"),
    ] {
        assert_eq!(
            run(text, keys),
            (expected.to_string(), Mode::Normal),
            "{text:?}: {keys}"
        );
    }
}

#[test]
fn visual_change_and_insert_are_one_undo_step() {
    for keys in ["Vjc!<Esc>u", "vjC!<Esc>u", "vjc!<Esc>u"] {
        assert_eq!(
            run("abc\ndef\nghi", keys),
            ("abc\ndef\nghi".to_string(), Mode::Normal),
            "{keys}",
        );
    }
}

#[test]
fn visual_change_captures_original_text_and_kind() {
    use crate::wrap::RangeKind;

    for (text, keys, expected, kind) in [
        ("abc def", "vec!<Esc>", "abc", RangeKind::Charwise),
        (
            "abc\ndef\nghi",
            "Vjc!<Esc>",
            "abc\ndef\n",
            RangeKind::Linewise,
        ),
        ("abc\ndef", "jvC!<Esc>", "def\n", RangeKind::Linewise),
        ("", "Vc!<Esc>", "\n", RangeKind::Linewise),
    ] {
        let mut editor = replay_editor(text);
        press(&mut editor, keys);
        let clip = editor.clipboard_ring.most_recent().unwrap();
        let captured: String = clip.text().iter().map(ToString::to_string).collect();
        assert_eq!(
            (captured, clip.kind()),
            (expected.to_string(), kind),
            "{keys}"
        );
    }
}

#[test]
fn visual_yank_returns_to_selection_start_in_both_directions() {
    for (keys, cursor) in [("vly", 0), ("lvhy", 0), ("jvkY", 0), ("ljvky", 1)] {
        let mut editor = replay_editor("abc\ndef\nghi");
        press(&mut editor, keys);
        assert_eq!(editor.active_document().buffer.cursor(), cursor, "{keys}");
        assert_eq!(editor.current_mode, Mode::Normal, "{keys}");
    }
}

#[test]
fn empty_visual_yank_exits_without_entering_operator_pending() {
    for keys in ["vy", "VY"] {
        assert_eq!(run("", keys), (String::new(), Mode::Normal), "{keys}");
    }
}

#[test]
fn mixed_banked_change_preserves_kinds_boundaries_and_one_undo_step() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let text = "abc\ndef\nghi";
    let mut editor = replay_editor(text);
    editor
        .active_document()
        .bank_region(Region::new(0, 0, RangeKind::Linewise));
    editor
        .active_document()
        .bank_region(Region::new(5, 4, RangeKind::Charwise));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Change,
    )));
    press(&mut editor, "!<Esc>");

    assert_eq!(editor.active_document().buffer.to_string(), "!\n!f\nghi");
    assert!(editor.pending_multi_insert_anchors.is_empty());
    for (index, expected, kind) in [
        (0, "abc\n", RangeKind::Linewise),
        (1, "de", RangeKind::Charwise),
    ] {
        let clip = editor.clipboard_ring.get(index).unwrap();
        let captured: String = clip.text().iter().map(ToString::to_string).collect();
        assert_eq!((captured, clip.kind()), (expected.to_string(), kind));
    }
    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), text);
}

#[test]
fn readonly_visual_mutations_do_not_capture_or_enter_insert_mode() {
    for keys in ["vd", "vx", "vX", "vD", "vc", "vC"] {
        let mut editor = replay_editor("abc\ndef");
        editor
            .clipboard_ring
            .push_str("saved", crate::wrap::RangeKind::Charwise);
        editor.active_document().set_read_only(true);
        press(&mut editor, keys);
        assert_eq!(
            editor.active_document().buffer.to_string(),
            "abc\ndef",
            "{keys}"
        );
        assert_eq!(editor.current_mode, Mode::Normal, "{keys}");
        let clip = editor.clipboard_ring.most_recent().unwrap();
        let captured: String = clip.text().iter().map(ToString::to_string).collect();
        assert_eq!(captured, "saved", "{keys}");
    }
}

#[test]
fn visual_change_dot_repeats_selection_deletion_and_inserted_text() {
    assert_eq!(
        run("abc def", "vlcX<Esc>w."),
        ("Xc Xf".to_string(), Mode::Normal),
    );
}

#[test]
fn visual_line_change_dot_repeats_with_line_boundaries() {
    for keys in ["VcX<Esc>j.", "vCX<Esc>j."] {
        assert_eq!(
            run("abc\ndef\nghi", keys),
            ("X\nX\nghi".to_string(), Mode::Normal),
            "{keys}",
        );
    }
}

#[test]
fn visual_change_without_inserted_text_remains_repeatable() {
    assert_eq!(
        run("abc def", "vlc<Esc>w."),
        ("c f".to_string(), Mode::Normal),
    );
    assert_eq!(
        run("abc\ndef\nghi", "Vc<Esc>j."),
        ("\n\nghi".to_string(), Mode::Normal),
    );
}

#[test]
fn repeated_visual_change_is_one_undo_step() {
    assert_eq!(
        run("abc def", "vlcX<Esc>w.u"),
        ("Xc def".to_string(), Mode::Normal),
    );
}

#[test]
fn normal_insert_dot_remains_insertion_only() {
    assert_eq!(
        run("abc def", "iX<Esc>w."),
        ("Xabc Xdef".to_string(), Mode::Normal),
    );
}

#[test]
fn banked_multi_insert_dot_reconstructs_selection_before_inserting() {
    assert_eq!(
        run("abc def", "vl<Esc>wiX<Esc>w."),
        ("Xabc Xdef".to_string(), Mode::Normal),
    );
}

#[test]
fn banked_multi_change_dot_replays_insert_at_every_rebuilt_anchor() {
    assert_eq!(
        run("ab ab cd cd", "vl<Esc>mcXY<Esc>ww."),
        ("XY XY XY XY".to_string(), Mode::Normal),
    );
    assert_eq!(
        run("ab ab cd cd", "vl<Esc>mcXY<Esc>ww.u"),
        ("XY XY cd cd".to_string(), Mode::Normal),
    );
}

#[test]
fn banked_multi_insert_dot_replays_insert_at_every_rebuilt_anchor() {
    assert_eq!(
        run("ab ab cd cd", "vl<Esc>miX<Esc>ww."),
        ("Xab Xab Xcd Xcd".to_string(), Mode::Normal),
    );
}

#[test]
fn visual_line_delete_lands_on_first_nonblank_of_remaining_line() {
    for (text, keys, expected, cursor) in [
        ("abc\ndef", "GVd", "abc", 0),
        ("  abc\ndef", "GVd", "  abc", 2),
        ("abc\n  def", "Vd", "  def", 2),
        ("abc\n  def\n  ghi", "jvkD", "  ghi", 2),
        ("abc\ndef", "GvX", "abc", 0),
        ("abc\ndef", "GvD", "abc", 0),
        ("abc\n", "GVd", "abc", 0),
        ("abc", "Vd", "", 0),
    ] {
        let mut editor = replay_editor(text);
        press(&mut editor, keys);
        assert_eq!(
            editor.active_document().buffer.to_string(),
            expected,
            "{keys}"
        );
        assert_eq!(editor.active_document().buffer.cursor(), cursor, "{keys}");
    }
}

#[test]
fn normal_banked_line_delete_retains_existing_cursor_convention() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;

    let mut editor = replay_editor("abc\ndef");
    editor
        .active_document()
        .bank_region(Region::new(4, 4, crate::wrap::RangeKind::Linewise));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.active_document().buffer.to_string(), "abc");
    assert_eq!(editor.active_document().buffer.cursor(), 2);
}
