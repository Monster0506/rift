use super::common::{editor_with_text, keys, press, KeyQueue};
use super::*;

const TEXT: &str = "abc\ndef";

fn buffer_text(editor: &mut Editor<KeyQueue>) -> String {
    editor.active_document().buffer().to_string()
}

#[test]
fn operator_pending_runs_normal_command() {
    for seq in ["do", "d-", "dC", "dpj", "dxj", "duj", "d.j", "2do"] {
        let mut editor = editor_with_text(TEXT);

        press(&mut editor, &keys(seq));

        assert_eq!(buffer_text(&mut editor), TEXT, "{seq}");
        assert_eq!(editor.current_mode, Mode::Normal, "{seq}");
        assert_eq!(editor.input.count, 0, "{seq}");
    }
}

#[test]
fn word_text_object_and_visual_word_transfer_the_same_target() {
    use super::common::ring_text;

    for (normal, visual, expected) in [("diw", "ved", " bravo"), ("yiw", "vey", "alpha bravo")] {
        for input in [normal, visual] {
            let mut editor = editor_with_text("alpha bravo");
            press(&mut editor, &keys(input));
            assert_eq!(buffer_text(&mut editor), expected, "{input}");
            assert_eq!(ring_text(&editor, 0).as_deref(), Some("alpha"), "{input}");
            assert_eq!(editor.mode(), crate::mode::Mode::Normal, "{input}");
        }
    }
}

#[test]
fn visual_delete_and_normal_aliases_cut_two_banks_in_one_undo() {
    use super::common::ring_text;

    for input in ["vl<Esc>wwvld", "vl<Esc>wwvl<Esc>x", "vl<Esc>wwvl<Esc>D"] {
        let mut editor = editor_with_text("ab keep cd");
        press(&mut editor, &keys(input));
        assert_eq!(buffer_text(&mut editor), " keep ", "{input}");
        assert_eq!(ring_text(&editor, 0).as_deref(), Some("ab"), "{input}");
        assert_eq!(ring_text(&editor, 1).as_deref(), Some("cd"), "{input}");
        press(&mut editor, &keys("u"));
        assert_eq!(buffer_text(&mut editor), "ab keep cd", "{input}");
    }
}
