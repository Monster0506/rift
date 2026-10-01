use super::common::*;

#[test]
fn new_cut_commits_the_previously_pending_ghost_first() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");
    assert_eq!(editor.active_document().pending_ghost.len(), 1);

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    assert_eq!(editor.active_document().buffer.to_string(), "cdef");
    assert_eq!(editor.active_document().pending_ghost.len(), 1);

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "cdef");
}

#[test]
fn escape_commits_every_entry_in_a_multi_region_ghost_list() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    assert_eq!(editor.active_document().buffer.to_string(), "234789");
    assert_eq!(editor.active_document().pending_ghost.len(), 2);

    editor.handle_action(&Action::Editor(EditorAction::EnterNormalMode));
    assert_eq!(editor.active_document().buffer.to_string(), "234789");
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn entering_insert_mode_resolves_a_pending_ghost_via_the_mutation_hook() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");
    assert_eq!(editor.active_document().pending_ghost.len(), 1);

    editor.handle_action(&Action::Editor(EditorAction::EnterInsertMode));

    assert!(editor.active_document().pending_ghost.is_empty());
    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");
}

#[test]
fn undo_with_a_pending_ghost_materializes_then_undoes_it_in_one_step() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");

    assert!(editor.active_document().undo());
    assert_eq!(editor.active_document().buffer.to_string(), "abcdef");
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn save_commits_pending_ghost_before_writing() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");
    assert_eq!(editor.active_document().pending_ghost.len(), 1);

    editor.do_save();

    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn same_location_paste_restores_the_cut_byte_identical() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    assert_eq!(editor.active_document().buffer.to_string(), "def");

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "abcdef",
        "p immediately after d{{motion}} restores byte-identical, not vim's normal after-cursor reflow"
    );
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "a true no-op restore must not move the cursor either"
    );
}

#[test]
fn put_drains_every_documents_pending_ghost_but_inserts_only_the_most_recent() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");
    let doc1_id = editor.active_document_id();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");

    let mut doc2 = crate::document::Document::new(editor.document_manager.next_id()).unwrap();
    let _ = doc2.buffer.insert_str("xyz");
    doc2.buffer.move_to_start();
    editor.document_manager.add_document(doc2);
    let doc2_id = editor.document_manager.active_document_id().unwrap();
    editor.split_tree.set_focused_document(doc2_id);
    assert_ne!(doc1_id, doc2_id);

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));
    assert_eq!(editor.active_document().buffer.to_string(), "yz");

    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    let doc1 = editor.document_manager.get_document(doc1_id).unwrap();
    assert_eq!(doc1.buffer.to_string(), "bcdef");
    assert!(doc1.pending_ghost.is_empty());

    let doc2 = editor.document_manager.get_document(doc2_id).unwrap();
    assert_eq!(doc2.buffer.to_string(), "xyz");
    assert!(doc2.pending_ghost.is_empty());
}

#[test]
fn ddp_with_no_intervening_motion_does_not_move_the_cursor() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "A\nB\nC\n");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Put { before: false }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "A\nB\nC\n",
        "ddp with no intervening motion is a true no-op"
    );
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "ddp must not move the cursor when nothing moved in between"
    );
}

#[test]
fn dd_paste_before_with_no_intervening_motion_does_not_move_the_cursor() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "A\nB\nC\n");
    editor.active_document().buffer.set_cursor(0).unwrap();

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Put { before: true }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "A\nB\nC\n",
        "ddP with no intervening motion is a true no-op too"
    );
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "ddP must not move the cursor either"
    );
}

#[test]
fn same_location_paste_before_does_not_move_the_cursor_either() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    editor.handle_action(&Action::Editor(EditorAction::Put { before: true }));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "abcdef",
        "P immediately after d{{motion}} restores byte-identical too"
    );
    assert_eq!(
        editor.active_document().buffer.cursor(),
        0,
        "same-location P must not move the cursor either"
    );
}

#[test]
fn set_noghostcut_makes_dw_delete_immediately() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");
    editor.execute_command_line("set noghostcut".to_string());
    assert!(!editor.state.settings.ghost_cut);

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.pending_count = 3;
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    assert_eq!(
        editor.active_document().buffer.to_string(),
        "def",
        "d{{motion}} deletes right away with ghostcut off"
    );
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn set_noghostcut_makes_dd_delete_immediately() {
    use crate::action::{Action, EditorAction, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "one\ntwo\nthree\n");
    editor.execute_command_line("set noghostcut".to_string());

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "two\nthree\n");
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn set_noghostcut_makes_banked_region_delete_immediate() {
    use crate::action::{Action, EditorAction, OperatorType};
    use crate::selection::Region;
    use crate::wrap::RangeKind;

    let mut editor = create_editor();
    load_text(&mut editor, "0123456789");
    editor.execute_command_line("set noghostcut".to_string());
    editor
        .active_document()
        .selection_set
        .bank(Region::new(0, 1, RangeKind::Charwise));
    editor
        .active_document()
        .selection_set
        .bank(Region::new(5, 6, RangeKind::Charwise));

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));

    assert_eq!(editor.active_document().buffer.to_string(), "234789");
    assert!(editor.active_document().pending_ghost.is_empty());
}

#[test]
fn set_ghostcut_can_be_re_enabled() {
    use crate::action::{Action, EditorAction, Motion, OperatorType};

    let mut editor = create_editor();
    load_text(&mut editor, "abcdef");
    editor.execute_command_line("set noghostcut".to_string());
    editor.execute_command_line("set ghostcut".to_string());
    assert!(editor.state.settings.ghost_cut);

    editor.handle_action(&Action::Editor(EditorAction::Operator(
        OperatorType::Delete,
    )));
    editor.handle_action(&Action::Editor(EditorAction::Move(Motion::Right)));

    assert_eq!(editor.active_document().buffer.to_string(), "bcdef");
    assert_eq!(editor.active_document().pending_ghost.len(), 1);
}
