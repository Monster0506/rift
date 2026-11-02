use super::common::{editor_with_text, keys, press, KeyQueue};
use super::*;

const TEXT: &str = "abc\ndef";

fn buffer_text(editor: &mut Editor<KeyQueue>) -> String {
    editor.active_document().buffer.to_string()
}

#[test]
fn operator_pending_runs_normal_command() {
    for seq in ["do", "d-", "dC", "dpj", "dxj", "duj", "d.j", "2do"] {
        let mut editor = editor_with_text(TEXT);

        press(&mut editor, &keys(seq));

        assert_eq!(buffer_text(&mut editor), TEXT, "{seq}");
        assert_eq!(editor.current_mode, Mode::Normal, "{seq}");
        assert_eq!(editor.pending_count, 0, "{seq}");
    }
}
